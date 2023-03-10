pub mod contact;
pub mod conversation;
pub mod member;
pub mod message;

pub use self::{
    contact::Contact,
    conversation::Conversation,
    member::Member,
    message::{MessageStatus, NewMessage},
};
use crate::{
    crdt::{Author, CrdtValueTransaction},
    entity::key,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Serialize, Deserialize)]
pub enum Patch {
    Contact(Contact),
    Conversation(Conversation),
    Member(Member),
    NewMessage(NewMessage),
    MessageStatus(MessageStatus),
}
impl Patch {
    pub async fn merge(self, trans: &mut DatabaseTransaction) -> Option<Self> {
        match self {
            Patch::Contact(crdt) => trans.merge(crdt).await.map(Patch::Contact),
            Patch::Conversation(crdt) => trans.merge(crdt).await.map(Patch::Conversation),
            Patch::Member(crdt) => trans.merge(crdt).await.map(Patch::Member),
            Patch::NewMessage(crdt) => trans.merge(crdt).await.map(Patch::NewMessage),
            Patch::MessageStatus(crdt) => trans.merge(crdt).await.map(Patch::MessageStatus),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrdtWritable {
    pub author: Author,
    pub generation: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrdtSequence(pub i32);

#[derive(Debug, Serialize, Deserialize)]
pub struct CrdtAddOnly;

pub const KEY_LENGTH: usize = 32;
#[derive(Clone, Debug)]
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

    pub async fn get_or_create(&self, trans: &DatabaseTransaction) -> key::Model {
        let existent = key::Entity::find()
            .filter(key::Column::Public.eq(self.0.clone()))
            .one(trans)
            .await
            .unwrap();

        let model = match existent {
            Some(existent) => existent,
            None => key::ActiveModel {
                id: ActiveValue::NotSet,
                public: ActiveValue::Set(self.0.clone()),
            }
            .insert(trans)
            .await
            .unwrap(),
        };

        model
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Key should have 32 bytes")]
pub struct BadKeyError;
