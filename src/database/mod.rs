pub mod doc_ex;
pub mod error;

use self::error::{DatabaseError, DatabaseResult};
use automerge::{
    sync::{self, SyncDoc},
    transaction::Transactable,
    Automerge, ObjId, ObjType, ReadDoc, ROOT,
};
use doc_ex::ReadDocEx;
use uuid::Uuid;

pub struct SharedDatabase {
    doc: Automerge,
}
impl SharedDatabase {
    pub fn with_user(user: Uuid) -> DatabaseResult<Self> {
        Ok(SharedDatabase {
            doc: schema::create()?.with_actor(user.into()),
        })
    }

    pub fn add_contact(&mut self, contact: Contact) -> DatabaseResult<()> {
        let index = contact.uuid.to_string();
        let mut trans = self.doc.transaction();
        let obj = trans.put_object(schema::contacts_id(), &index, ObjType::Map)?;
        contact.reconcile(&mut trans, obj)?;
        trans.commit();

        Ok(())
    }

    pub fn add_message(&mut self, message: Message) -> DatabaseResult<()> {
        let mut trans = self.doc.transaction();
        let index = trans.length(schema::messages_id());
        let obj = trans.insert_object(schema::messages_id(), index, ObjType::Map)?;
        message.reconcile(&mut trans, obj)?;
        trans.commit();

        Ok(())
    }

    pub fn set_message(&mut self, index: usize, message: &Message) -> DatabaseResult<()> {
        let mut trans = self.doc.transaction();
        let obj = trans.get_map(schema::messages_id(), index)?;
        message.reconcile(&mut trans, obj)?;
        trans.commit();
        Ok(())
    }

    pub fn list_messages(&self) -> impl DoubleEndedIterator<Item = DatabaseResult<Message>> + '_ {
        let length = self.doc.length(schema::messages_id());

        (0..length).map(|idx| {
            let obj = self.doc.get_map(schema::messages_id(), idx)?;
            Message::hydrate(&self.doc, obj)
        })
    }

    pub fn list_contact(&self) -> impl DoubleEndedIterator<Item = DatabaseResult<Contact>> + '_ {
        let keys = self.doc.keys(schema::contacts_id());

        keys.map(|idx| {
            let obj = self.doc.get_map(schema::contacts_id(), idx)?;

            Contact::hydrate(&self.doc, obj)
        })
    }

    pub fn get_contact(&self, uuid: Uuid) -> DatabaseResult<Option<Contact>> {
        let obj = self
            .doc
            .get_opt_map(schema::contacts_id(), uuid.to_string())?;
        let Some(obj) = obj else { return Ok(None); };

        Ok(Some(Contact::hydrate(&self.doc, obj)?))
    }

    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    pub fn load_with_user(data: &[u8], user: Uuid) -> DatabaseResult<Self> {
        let doc = Automerge::load(data)?.with_actor(user.into());

        Ok(Self { doc })
    }

    pub fn start_sync(&self) -> AutomergeDbSync {
        AutomergeDbSync::new()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub uuid: Uuid,
    pub name: String,
}
impl Contact {
    pub fn hydrate<D: ReadDoc>(doc: &D, obj: ObjId) -> DatabaseResult<Self> {
        let uuid = doc
            .get_bytes(&obj, "uuid")?
            .try_into()
            .map_err(|_| DatabaseError::BadUuid)?;
        let name = doc.get_string(&obj, "name")?;

        Ok(Self {
            uuid: Uuid::from_bytes(uuid),
            name,
        })
    }

    pub fn reconcile<T: Transactable>(&self, trans: &mut T, obj: ObjId) -> DatabaseResult<()> {
        trans.put(&obj, "uuid", self.uuid.as_bytes().to_vec())?;
        trans.put(&obj, "name", self.name.to_string())?;

        Ok(())
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub from: Uuid,
    pub content: String,
    pub status: MessageStatus,
}
impl Message {
    pub fn hydrate<D: ReadDoc>(doc: &D, obj: ObjId) -> DatabaseResult<Self> {
        let from = doc
            .get_bytes(&obj, "from")?
            .try_into()
            .map_err(|_| DatabaseError::BadUuid)?;
        let content = doc.get_string(&obj, "content")?;
        let status = doc.get_u64(&obj, "status")?;

        Ok(Self {
            from: Uuid::from_bytes(from),
            content,
            status: status.into(),
        })
    }

    pub fn reconcile<T: Transactable>(&self, trans: &mut T, obj: ObjId) -> DatabaseResult<()> {
        trans.put(&obj, "from", self.from.as_bytes().to_vec())?;
        trans.put(&obj, "content", self.content.to_string())?;
        trans.put(&obj, "status", Into::<u64>::into(self.status))?;

        Ok(())
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    #[default]
    Sent,
    Delivered,
    Read,
}
impl From<u64> for MessageStatus {
    fn from(value: u64) -> Self {
        match value {
            0 => MessageStatus::Sent,
            1 => MessageStatus::Delivered,
            2 => MessageStatus::Read,
            _ => panic!(),
        }
    }
}
impl From<MessageStatus> for u64 {
    fn from(value: MessageStatus) -> Self {
        match value {
            MessageStatus::Sent => 0,
            MessageStatus::Delivered => 1,
            MessageStatus::Read => 2,
        }
    }
}

pub trait DbSync {
    type Database;

    fn tx(&mut self, database: &mut Self::Database) -> DatabaseResult<Option<Vec<u8>>>;
    fn rx(&mut self, database: &mut Self::Database, message: &[u8]) -> DatabaseResult<()>;
}

#[derive(Default)]
pub struct AutomergeDbSync {
    state: sync::State,
}
impl AutomergeDbSync {
    pub fn new() -> Self {
        Default::default()
    }
}
impl DbSync for AutomergeDbSync {
    type Database = SharedDatabase;

    fn tx(&mut self, database: &mut Self::Database) -> DatabaseResult<Option<Vec<u8>>> {
        let message = database.doc.generate_sync_message(&mut self.state);
        let Some(message) = message else { return Ok(None); };

        Ok(Some(message.encode()))
    }

    fn rx(&mut self, database: &mut Self::Database, message: &[u8]) -> DatabaseResult<()> {
        let message = sync::Message::decode(message)?;
        database
            .doc
            .receive_sync_message(&mut self.state, message)?;

        Ok(())
    }
}

pub struct LocalDatabase {
    doc: Automerge,
    user: Uuid,
}
impl LocalDatabase {
    pub fn with_user(user: Uuid) -> DatabaseResult<LocalDatabase> {
        let mut doc = Automerge::new().with_actor(Uuid::nil().into());
        let mut trans = doc.transaction();

        LocalDatabaseData {
            user,
            channels: Default::default(),
        }
        .reconcile(&mut trans, ROOT)?;

        trans.commit();

        Ok(LocalDatabase { doc, user })
    }

    pub fn user(&self) -> Uuid {
        self.user
    }

    pub fn set(&mut self, data: LocalDatabaseData) -> DatabaseResult<()> {
        let mut trans = self.doc.transaction();
        data.reconcile(&mut trans, ROOT)?;
        trans.commit();

        Ok(())
    }

    pub fn get(&self) -> DatabaseResult<LocalDatabaseData> {
        LocalDatabaseData::hydrate(&self.doc, ROOT)
    }

    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    pub fn load(data: &[u8]) -> DatabaseResult<Self> {
        let doc = Automerge::load(data)?;
        let user = LocalDatabaseData::hydrate(&doc, ROOT)?.user;

        Ok(LocalDatabase { doc, user })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDatabaseData {
    pub user: Uuid,
    pub channels: Vec<String>,
}
impl LocalDatabaseData {
    pub fn hydrate<D: ReadDoc>(doc: &D, obj: ObjId) -> DatabaseResult<LocalDatabaseData> {
        let user = doc
            .get_bytes(&obj, "user")?
            .try_into()
            .map_err(|_| DatabaseError::BadUuid)?;

        let channels = doc.get_list(&obj, "channels")?;
        let channels_n = doc.length(&channels);
        let channels = (0..channels_n)
            .map(|idx| doc.get_string(&channels, idx))
            .collect::<Result<_, _>>()?;

        Ok(LocalDatabaseData {
            user: Uuid::from_bytes(user),
            channels,
        })
    }

    pub fn reconcile<T: Transactable>(&self, trans: &mut T, obj: ObjId) -> DatabaseResult<()> {
        trans.put(&obj, "user", self.user.as_bytes().to_vec())?;
        let channels = trans.put_object(&obj, "channels", ObjType::List)?;

        for (idx, str) in self.channels.iter().enumerate() {
            trans.insert(&channels, idx, str.to_string())?;
        }

        Ok(())
    }
}

mod schema {
    use automerge::{transaction::Transactable, ActorId, Automerge, ObjId, ObjType, ROOT};
    use uuid::Uuid;

    pub fn actor() -> ActorId {
        Uuid::nil().into()
    }

    pub fn contacts_id() -> ObjId {
        ObjId::Id(1, actor(), 0)
    }

    pub fn messages_id() -> ObjId {
        ObjId::Id(2, actor(), 0)
    }

    pub fn create() -> super::DatabaseResult<Automerge> {
        let mut doc = Automerge::new().with_actor(Uuid::nil().into());
        let mut trans = doc.transaction();
        let contacts = trans.put_object(ROOT, "contacts", ObjType::Map)?;
        assert_eq!(contacts, contacts_id());
        let messages = trans.put_object(ROOT, "messages", ObjType::List)?;
        assert_eq!(messages, messages_id());
        trans.commit();

        Ok(doc)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use rstest::*;

    const USER: Uuid = uuid::uuid!("a50ca24b-447e-487f-8f23-e9fbad19a452");

    struct DbEquals(SharedDatabase);
    impl DbEquals {
        fn comparer(&self) -> impl Eq + std::fmt::Debug {
            (
                self.0
                    .list_messages()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>(),
                self.0
                    .list_contact()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>(),
            )
        }
    }
    impl std::fmt::Debug for DbEquals {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Debug::fmt(&self.comparer(), f)
        }
    }
    impl PartialEq for DbEquals {
        fn eq(&self, other: &Self) -> bool {
            self.comparer().eq(&other.comparer())
        }
    }
    impl Eq for DbEquals {}

    mod given_an_empty_database {
        use super::*;

        type Given = (SharedDatabase,);
        #[fixture]
        fn given() -> Given {
            (SharedDatabase::with_user(USER).unwrap(),)
        }

        #[rstest]
        fn it_is_identical_to_other_empty_database(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let mut other = SharedDatabase::with_user(Uuid::new_v4())?;

            assert_eq!(database.save(), other.save());

            Ok(())
        }

        #[rstest]
        fn it_is_identical_if_the_user_is_the_same(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let mut other = SharedDatabase::with_user(USER)?;

            let same_message = Message {
                from: Uuid::new_v4(),
                ..Default::default()
            };

            database.add_message(same_message.clone())?;
            other.add_message(same_message)?;

            assert_eq!(database.save(), other.save());

            let mut third = SharedDatabase::load_with_user(&database.save(), USER)?;

            let same_message = Message {
                from: Uuid::new_v4(),
                ..Default::default()
            };

            database.add_message(same_message.clone())?;
            third.add_message(same_message)?;

            assert_eq!(database.save(), third.save());

            Ok(())
        }

        #[rstest]
        fn it_is_not_identical_if_the_user_is_not_the_same(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let mut other = SharedDatabase::with_user(Uuid::new_v4())?;

            let same_message = Message {
                from: Uuid::new_v4(),
                ..Default::default()
            };

            database.add_message(same_message.clone())?;
            other.add_message(same_message)?;

            assert_ne!(database.save(), other.save());

            Ok(())
        }

        #[rstest]
        fn it_gets_and_sets_contacts(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let contact = Contact {
                uuid: Uuid::new_v4(),
                name: "Puel".to_string(),
            };

            assert_eq!(database.get_contact(contact.uuid)?, None);

            database.add_contact(contact.clone())?;

            assert_eq!(
                database.list_contact().collect::<Result<Vec<_>, _>>()?,
                vec![contact.clone()]
            );
            assert_eq!(database.get_contact(contact.uuid)?, Some(contact.clone()));

            let contact = Contact {
                name: "André".to_string(),
                ..contact
            };

            database.add_contact(contact.clone())?;
            assert_eq!(database.get_contact(contact.uuid)?, Some(contact));

            let contact = Contact {
                uuid: Uuid::new_v4(),
                ..Default::default()
            };
            database.add_contact(contact)?;
            assert_eq!(database.list_contact().count(), 2);

            Ok(())
        }

        #[rstest]
        fn it_gets_and_sets_messages(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let message = Message {
                from: Uuid::new_v4(),
                content: "Hello".to_string(),
                status: MessageStatus::Sent,
            };

            assert_eq!(database.list_messages().count(), 0);
            database.add_message(message.clone())?;
            assert_eq!(
                database.list_messages().collect::<Result<Vec<_>, _>>()?,
                vec![message]
            );

            Ok(())
        }

        mod and_the_database_is_not_empty {
            use super::*;

            type Given = (SharedDatabase,);
            #[fixture]
            fn given() -> Given {
                let (mut database, ..) = super::given();

                database
                    .add_contact(Contact {
                        uuid: Uuid::new_v4(),
                        name: "name".to_string(),
                    })
                    .unwrap();
                database
                    .add_message(Message {
                        from: Uuid::new_v4(),
                        content: "Hello".to_string(),
                        ..Default::default()
                    })
                    .unwrap();

                (database,)
            }

            #[rstest]
            fn it_may_be_saved_and_then_load_back(given: Given) -> DatabaseResult<()> {
                let (mut database, ..) = given;

                let data = database.save();
                let back = SharedDatabase::load_with_user(&data, Uuid::new_v4())?;

                assert_eq!(DbEquals(database), DbEquals(back));

                Ok(())
            }

            #[rstest]
            fn it_syncs_with_another_empty_database(given: Given) -> DatabaseResult<()> {
                let (mut database, ..) = given;

                let mut other = SharedDatabase::with_user(Uuid::new_v4())?;
                other.add_message(Message {
                    from: Uuid::new_v4(),
                    ..Default::default()
                })?;

                let mut database_sync = database.start_sync();
                let mut other_sync = other.start_sync();

                loop {
                    if let Some(message) = database_sync.tx(&mut database)? {
                        other_sync.rx(&mut other, &message)?;
                    } else if let Some(message) = other_sync.tx(&mut other)? {
                        database_sync.rx(&mut database, &message)?;
                    } else {
                        break;
                    }
                }

                assert_eq!(DbEquals(database), DbEquals(other));

                Ok(())
            }

            #[rstest]
            fn it_edits_messages(given: Given) -> DatabaseResult<()> {
                let (mut database, ..) = given;

                let mut message = database.list_messages().next().unwrap()?;
                message.status = MessageStatus::Delivered;
                database.set_message(0, &message)?;

                let message_back = database.list_messages().next().unwrap()?;
                assert_eq!(message, message_back);
                assert_eq!(database.list_messages().count(), 1);

                Ok(())
            }
        }
    }

    mod given_an_empty_local_database {
        use super::*;

        type Given = (LocalDatabase,);
        #[fixture]
        fn given() -> Given {
            (LocalDatabase::with_user(USER).unwrap(),)
        }

        #[rstest]
        fn it_is_identical_to_other_database_with_same_user(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let mut other = LocalDatabase::with_user(USER)?;

            assert_eq!(database.save(), other.save());

            Ok(())
        }

        #[rstest]
        fn it_has_user(given: Given) {
            let (database, ..) = given;

            assert_eq!(database.user(), USER);
        }

        #[rstest]
        fn it_may_be_saved_and_then_load_back(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let data = database.save();
            let back = LocalDatabase::load(&data)?;

            assert_eq!(database.user(), back.user());

            Ok(())
        }

        #[rstest]
        fn it_may_add_channels(given: Given) -> DatabaseResult<()> {
            let (mut database, ..) = given;

            let mut data = database.get()?;
            data.channels.push("new channel".to_string());
            database.set(data.clone())?;

            let data_back = database.get()?;
            assert_eq!(data, data_back);

            Ok(())
        }
    }
}
