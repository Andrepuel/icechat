use automerge::{
    sync::{self, SyncDoc},
    transaction::Transactable,
    Automerge, ObjType, Prop, ReadDoc,
};
use autosurgeon::{hydrate, hydrate_prop, reconcile, reconcile_prop, Hydrate, Reconcile};
use uuid::Uuid;

pub struct SharedDatabase {
    doc: Automerge,
}
impl SharedDatabase {
    pub fn with_user(user: Uuid) -> SharedDatabase {
        SharedDatabase {
            doc: schema::create().with_actor(user.into()),
        }
    }

    pub fn add_contact(&mut self, contact: Contact) {
        let index = contact.uuid.to_string();
        let mut trans = self.doc.transaction();
        trans
            .put_object(schema::contacts_id(), &index, ObjType::Map)
            .unwrap();
        reconcile_prop(&mut trans, schema::contacts_id(), Prop::Map(index), contact).unwrap();
        trans.commit().unwrap();
    }

    pub fn add_message(&mut self, message: Message) {
        let index = self.doc.length(schema::messages_id());
        let mut trans = self.doc.transaction();
        trans
            .insert_object(schema::messages_id(), index, ObjType::Map)
            .unwrap();
        reconcile_prop(&mut trans, schema::messages_id(), index, message).unwrap();
        trans.commit().unwrap();
    }

    pub fn list_messages(&self) -> impl DoubleEndedIterator<Item = Message> + '_ {
        let length = self.doc.length(schema::messages_id());

        (0..length).map(|idx| hydrate_prop(&self.doc, schema::messages_id(), idx).unwrap())
    }

    pub fn list_contact(&self) -> impl DoubleEndedIterator<Item = Contact> + '_ {
        let length = self.doc.length(schema::contacts_id());

        (0..length).map(|idx| hydrate_prop(&self.doc, schema::contacts_id(), idx).unwrap())
    }

    pub fn get_contact(&self, uuid: Uuid) -> Option<Contact> {
        let prop: Prop = uuid.to_string().into();

        match hydrate_prop(&self.doc, schema::contacts_id(), prop) {
            Err(autosurgeon::HydrateError::Unexpected(autosurgeon::hydrate::Unexpected::None)) => {
                None
            }
            r => Some(r.unwrap()),
        }
    }

    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    pub fn load(data: &[u8]) -> Self {
        let doc = Automerge::load(data).unwrap();

        Self { doc }
    }

    pub fn start_sync(&mut self) -> AutomergeDbSync<'_> {
        AutomergeDbSync::new(&mut self.doc)
    }
}

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq, Eq)]
pub struct Contact {
    pub uuid: Uuid,
    pub name: String,
}

#[derive(Default, Debug, Clone, Reconcile, Hydrate, PartialEq, Eq)]
pub struct Message {
    pub from: Uuid,
    pub content: String,
    pub status: MessageStatus,
}

#[derive(Default, Debug, Clone, Reconcile, Hydrate, PartialEq, Eq)]
pub enum MessageStatus {
    #[default]
    Sent,
    Delivered,
    Read,
}

pub trait DbSync {
    fn message(&self) -> Option<&[u8]>;
    fn pop_message(&mut self);
    fn receive(&mut self, message: &[u8]);
    fn done(self);
}

pub struct AutomergeDbSync<'a> {
    doc: &'a mut Automerge,
    state: sync::State,
    msg: Option<Vec<u8>>,
}
impl<'a> AutomergeDbSync<'a> {
    pub fn new(doc: &'a mut Automerge) -> Self {
        let state = sync::State::new();
        let mut r = Self {
            doc,
            state,
            msg: Default::default(),
        };
        r.poll_send();

        r
    }

    fn poll_send(&mut self) {
        if self.msg.is_some() {
            return;
        }

        self.msg = self
            .doc
            .generate_sync_message(&mut self.state)
            .map(|message| message.encode());
    }
}
impl<'a> DbSync for AutomergeDbSync<'a> {
    fn message(&self) -> Option<&[u8]> {
        self.msg.as_deref()
    }

    fn pop_message(&mut self) {
        self.msg = None;
        self.poll_send();
    }

    fn receive(&mut self, message: &[u8]) {
        let message = sync::Message::decode(message).unwrap();
        self.doc
            .receive_sync_message(&mut self.state, message)
            .unwrap();
        self.poll_send();
    }

    fn done(self) {}
}

pub struct LocalDatabase {
    doc: Automerge,
}
impl LocalDatabase {
    pub fn with_user(uuid: Uuid) -> LocalDatabase {
        let mut doc = Automerge::new().with_actor(Uuid::nil().into());
        let mut trans = doc.transaction();
        reconcile(
            &mut trans,
            LocalDatabaseData {
                user: Contact {
                    uuid,
                    name: Default::default(),
                },
                channels: Default::default(),
            },
        )
        .unwrap();
        trans.commit();

        LocalDatabase { doc }
    }

    pub fn user(&self) -> Uuid {
        hydrate::<_, LocalDatabaseData>(&self.doc)
            .unwrap()
            .user
            .uuid
    }

    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    pub fn load(data: &[u8]) -> Self {
        let doc = Automerge::load(data).unwrap();

        LocalDatabase { doc }
    }
}

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq, Eq)]
pub struct LocalDatabaseData {
    user: Contact,
    channels: Vec<String>,
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

    pub fn create() -> Automerge {
        let mut doc = Automerge::new().with_actor(Uuid::nil().into());
        let mut trans = doc.transaction();
        let contacts = trans.put_object(ROOT, "contacts", ObjType::Map).unwrap();
        assert_eq!(contacts, contacts_id());
        let messages = trans.put_object(ROOT, "messages", ObjType::List).unwrap();
        assert_eq!(messages, messages_id());
        trans.commit().unwrap();

        doc
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
                self.0.list_messages().collect::<Vec<_>>(),
                self.0.list_contact().collect::<Vec<_>>(),
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
            (SharedDatabase::with_user(USER),)
        }

        #[rstest]
        fn it_is_identical_to_other_empty_database(given: Given) {
            let (mut database, ..) = given;

            let mut other = SharedDatabase::with_user(Uuid::new_v4());

            assert_eq!(database.save(), other.save());
        }

        #[rstest]
        fn it_is_identical_if_the_user_is_the_same(given: Given) {
            let (mut database, ..) = given;

            let mut other = SharedDatabase::with_user(USER);

            let same_message = Message {
                from: Uuid::new_v4(),
                ..Default::default()
            };

            database.add_message(same_message.clone());
            other.add_message(same_message);

            assert_eq!(database.save(), other.save());
        }

        #[rstest]
        fn it_is_not_identical_if_the_user_is_not_the_same(given: Given) {
            let (mut database, ..) = given;

            let mut other = SharedDatabase::with_user(Uuid::new_v4());

            let same_message = Message {
                from: Uuid::new_v4(),
                ..Default::default()
            };

            database.add_message(same_message.clone());
            other.add_message(same_message);

            assert_ne!(database.save(), other.save());
        }

        #[rstest]
        fn it_gets_and_sets_contacts(given: Given) {
            let (mut database, ..) = given;

            let contact = Contact {
                uuid: Uuid::new_v4(),
                name: "Puel".to_string(),
            };

            assert_eq!(database.get_contact(contact.uuid), None);

            database.add_contact(contact.clone());

            assert_eq!(
                database.list_contact().collect::<Vec<_>>(),
                vec![contact.clone()]
            );
            assert_eq!(database.get_contact(contact.uuid), Some(contact));
        }

        #[rstest]
        fn it_gets_and_sets_messages(given: Given) {
            let (mut database, ..) = given;

            let message = Message {
                from: Uuid::new_v4(),
                content: "Hello".to_string(),
                status: MessageStatus::Sent,
            };

            assert_eq!(database.list_messages().count(), 0);
            database.add_message(message.clone());
            assert_eq!(database.list_messages().collect::<Vec<_>>(), vec![message]);
        }

        mod and_the_database_is_not_empty {
            use super::*;

            type Given = (SharedDatabase,);
            #[fixture]
            fn given() -> Given {
                let (mut database, ..) = super::given();

                database.add_contact(Contact {
                    uuid: Uuid::new_v4(),
                    name: "name".to_string(),
                });
                database.add_message(Message {
                    from: Uuid::new_v4(),
                    content: "Hello".to_string(),
                    ..Default::default()
                });

                (database,)
            }

            #[rstest]
            fn it_may_be_saved_and_then_load_back(given: Given) {
                let (mut database, ..) = given;

                let data = database.save();
                let back = SharedDatabase::load(&data);

                assert_eq!(DbEquals(database), DbEquals(back));
            }

            #[rstest]
            fn it_syncs_with_another_empty_database(given: Given) {
                let (mut database, ..) = given;

                let mut other = SharedDatabase::with_user(Uuid::new_v4());
                other.add_message(Message {
                    from: Uuid::new_v4(),
                    ..Default::default()
                });

                let mut send = database.start_sync();
                let mut recv = other.start_sync();

                loop {
                    if let Some(message) = send.message() {
                        println!("alice -> {message:?}");
                        recv.receive(message);
                        send.pop_message();
                    } else if let Some(message) = recv.message() {
                        println!("bob -> {message:?}");
                        send.receive(message);
                        recv.pop_message();
                    } else {
                        send.done();
                        recv.done();
                        break;
                    }
                }

                assert_eq!(DbEquals(database), DbEquals(other));
            }
        }
    }

    mod given_an_empty_local_database {
        use super::*;

        type Given = (LocalDatabase,);
        #[fixture]
        fn given() -> Given {
            (LocalDatabase::with_user(USER),)
        }

        #[rstest]
        fn it_is_identical_to_other_database_with_same_user(given: Given) {
            let (mut database, ..) = given;

            let mut other = LocalDatabase::with_user(USER);

            assert_eq!(database.save(), other.save())
        }

        #[rstest]
        fn it_has_user(given: Given) {
            let (database, ..) = given;

            assert_eq!(database.user(), USER);
        }

        #[rstest]
        fn it_may_be_saved_and_then_load_back(given: Given) {
            let (mut database, ..) = given;

            let data = database.save();
            let back = LocalDatabase::load(&data);

            assert_eq!(database.user(), back.user());
        }
    }
}
