pub mod attachment;
pub mod contact;
pub mod conversation;
pub mod member;
pub mod message;

pub use self::{
    attachment::Attachment,
    contact::Contact,
    conversation::Conversation,
    member::Member,
    message::{MessageStatus, NewMessage},
};
use crate::{crdt::CrdtTransaction, entity::key};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Patch {
    Contact(Contact),
    Conversation(Conversation),
    Member(Member),
    NewMessage(NewMessage),
    MessageStatus(MessageStatus),
    Attachment(Attachment),
}
impl Patch {
    pub async fn merge(self, trans: &mut DatabaseTransaction) -> Option<Self> {
        match self {
            Patch::Contact(crdt) => trans.merge(crdt).await.map(Patch::Contact),
            Patch::Conversation(crdt) => trans.merge(crdt).await.map(Patch::Conversation),
            Patch::Member(crdt) => trans.merge(crdt).await.map(Patch::Member),
            Patch::NewMessage(crdt) => trans.merge(crdt).await.map(Patch::NewMessage),
            Patch::MessageStatus(crdt) => trans.merge(crdt).await.map(Patch::MessageStatus),
            Patch::Attachment(crdt) => trans.merge(crdt).await.map(Patch::Attachment),
        }
    }
}
impl From<Contact> for Patch {
    fn from(value: Contact) -> Patch {
        Patch::Contact(value)
    }
}
impl From<Conversation> for Patch {
    fn from(value: Conversation) -> Patch {
        Patch::Conversation(value)
    }
}
impl From<Member> for Patch {
    fn from(value: Member) -> Patch {
        Patch::Member(value)
    }
}
impl From<NewMessage> for Patch {
    fn from(value: NewMessage) -> Patch {
        Patch::NewMessage(value)
    }
}
impl From<MessageStatus> for Patch {
    fn from(value: MessageStatus) -> Patch {
        Patch::MessageStatus(value)
    }
}
impl From<Attachment> for Patch {
    fn from(value: Attachment) -> Self {
        Patch::Attachment(value)
    }
}

pub const KEY_LENGTH: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key(Vec<u8>);
impl Default for Key {
    fn default() -> Self {
        Self(vec![0; KEY_LENGTH])
    }
}
impl serde::Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.0, serializer)
    }
}
impl<'de> serde::Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec = serde::Deserialize::deserialize(deserializer)?;

        Key::new(vec).map_err(<D::Error as serde::de::Error>::custom)
    }
}
impl Deref for Key {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}
impl Key {
    pub fn new(key: Vec<u8>) -> Result<Self, BadKeyError> {
        if key.len() != KEY_LENGTH {
            return Err(BadKeyError);
        }

        Ok(Key(key))
    }

    pub fn new_exact(key: &[u8; KEY_LENGTH]) -> Self {
        Key::new(key.to_vec()).expect("Array is already correct size")
    }

    pub const fn zero() -> Key {
        Key(Vec::new())
    }

    pub async fn get_or_create(&self, trans: &DatabaseTransaction) -> key::Model {
        let existent = key::Entity::find()
            .filter(key::Column::Public.eq(self.0.clone()))
            .one(trans)
            .await
            .unwrap();

        match existent {
            Some(existent) => existent,
            None => key::ActiveModel {
                id: ActiveValue::NotSet,
                public: ActiveValue::Set(self.0.clone()),
            }
            .insert(trans)
            .await
            .unwrap(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Key should have 32 bytes")]
pub struct BadKeyError;
