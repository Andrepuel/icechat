use automerge::{transaction::Transactable, Automerge, ObjType, ReadDoc};
use autosurgeon::{hydrate_prop, reconcile_prop, Hydrate, Reconcile};
use uuid::Uuid;

pub struct DatabaseBase {
    doc: Automerge,
}
impl DatabaseBase {
    pub fn zero() -> DatabaseBase {
        DatabaseBase {
            doc: schema::create().with_actor(Uuid::new_v4().into()),
        }
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

    pub fn list_messages(&self) -> impl Iterator<Item = Message> + '_ {
        let length = self.doc.length(schema::messages_id());

        (0..length).map(|idx| hydrate_prop(&self.doc, schema::messages_id(), idx).unwrap())
    }
}

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
pub struct Contact {
    pub uuid: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Reconcile, Hydrate, PartialEq)]
pub struct Message {
    pub from: Uuid,
    pub content: String,
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
        let contacts = trans.put_object(ROOT, "contacts", ObjType::Table).unwrap();
        assert_eq!(contacts, contacts_id());
        let messages = trans.put_object(ROOT, "messages", ObjType::List).unwrap();
        assert_eq!(messages, messages_id());
        trans.commit().unwrap();

        doc
    }
}
