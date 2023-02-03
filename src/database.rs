use automerge::{transaction::Transactable, Automerge, ObjType, Prop, ReadDoc};
use autosurgeon::{hydrate, hydrate_prop, reconcile, reconcile_prop, Hydrate, Reconcile};
use uuid::Uuid;

pub struct SharedDatabase {
    doc: Automerge,
}
impl SharedDatabase {
    pub fn new() -> SharedDatabase {
        SharedDatabase {
            doc: schema::create().with_actor(Uuid::new_v4().into()),
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
}
impl Default for SharedDatabase {
    fn default() -> Self {
        Self::new()
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

    mod given_an_empty_database {
        use super::*;

        type Given = (SharedDatabase,);
        #[fixture]
        fn given() -> Given {
            (SharedDatabase::new(),)
        }

        #[rstest]
        fn it_is_identical_to_other_empty_database(given: Given) {
            let (mut database, ..) = given;

            let mut other = SharedDatabase::new();

            assert_eq!(database.save(), other.save());
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

        #[rstest]
        fn it_may_be_saved_and_then_load_back(given: Given) {
            let (mut database, ..) = given;

            database.add_message(Default::default());

            let data = database.save();
            let back = SharedDatabase::load(&data);

            assert_eq!(
                database.list_messages().collect::<Vec<_>>(),
                back.list_messages().collect::<Vec<_>>()
            );
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
