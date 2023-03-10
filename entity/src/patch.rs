use std::ops::Deref;

use crate::{
    crdt::{Author, CrdtValueTransaction},
    entity::{contact, conversation, key, message},
    uuid::{SplitUuid, UuidValue},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Contact {
    pub key: Key,
    pub name: String,
    pub crdt: CrdtWritable,
}
impl From<(key::Model, contact::Model)> for Contact {
    fn from((key, value): (key::Model, contact::Model)) -> Self {
        Contact {
            key: Key::new(key.public).expect("Inconsistent database"),
            name: value.name,
            crdt: CrdtWritable {
                author: Author(value.crdt_author),
                generation: value.crdt_generation,
            },
        }
    }
}
impl Contact {
    pub async fn get_or_create(
        key: Key,
        trans: &DatabaseTransaction,
    ) -> (key::Model, contact::Model) {
        let key = key.get_or_create(trans).await;

        let model = contact::Entity::find_by_id(key.id)
            .one(trans)
            .await
            .unwrap();

        let model = match model {
            Some(model) => model,
            None => contact::ActiveModel {
                key: ActiveValue::Set(key.id),
                name: ActiveValue::Set(Default::default()),
                crdt_generation: ActiveValue::Set(0),
                crdt_author: ActiveValue::Set(0),
            }
            .insert(trans)
            .await
            .unwrap(),
        };

        (key, model)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: Option<String>,
    pub crdt: CrdtWritable,
}
impl From<conversation::Model> for Conversation {
    fn from(value: conversation::Model) -> Self {
        let id = value.get_uuid();

        Conversation {
            id: id.into(),
            title: value.title,
            crdt: CrdtWritable {
                author: Author(value.crdt_author),
                generation: value.crdt_generation,
            },
        }
    }
}
impl Conversation {
    pub async fn get_or_create(uuid: Uuid, trans: &DatabaseTransaction) -> conversation::Model {
        let uuid = SplitUuid::from(uuid);
        let uuid_filter = uuid.to_filter::<conversation::Column>();
        let existent = conversation::Entity::find()
            .filter(uuid_filter.0)
            .filter(uuid_filter.1)
            .filter(uuid_filter.2)
            .filter(uuid_filter.3)
            .one(trans)
            .await
            .unwrap();

        let model = match existent {
            Some(existent) => existent,
            None => conversation::ActiveModel {
                id: ActiveValue::NotSet,
                uuid0: ActiveValue::Set(uuid.0),
                uuid1: ActiveValue::Set(uuid.1),
                uuid2: ActiveValue::Set(uuid.2),
                uuid3: ActiveValue::Set(uuid.3),
                title: ActiveValue::Set(Default::default()),
                crdt_generation: ActiveValue::Set(0),
                crdt_author: ActiveValue::Set(0),
            }
            .insert(trans)
            .await
            .unwrap(),
        };

        model
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Member {
    pub key: Key,
    pub conversation: Uuid,
    pub crdt: CrdtAddOnly,
}
impl From<(key::Model, conversation::Model)> for Member {
    fn from((key, conversation): (key::Model, conversation::Model)) -> Self {
        let conversation = conversation.get_uuid();

        Member {
            key: Key::new(key.public).expect("Inconsistent database"),
            conversation: conversation.into(),
            crdt: CrdtAddOnly,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewMessage {
    pub id: Uuid,
    pub from: Key,
    pub conversation: Uuid,
    pub text: String,
    pub crdt: CrdtWritable,
    pub sequence: CrdtSequence,
}
impl From<(message::Model, key::Model, conversation::Model)> for NewMessage {
    fn from(
        (message, fromm, conversation): (message::Model, key::Model, conversation::Model),
    ) -> Self {
        let id = message.get_uuid();

        let from = Key::new(fromm.public).expect("Inconsistent database");

        let conversation = conversation.get_uuid();

        NewMessage {
            id: id.into(),
            from,
            conversation: conversation.into(),
            text: message.text,
            crdt: CrdtWritable {
                author: Author(message.crdt_author),
                generation: message.crdt_generation,
            },
            sequence: CrdtSequence(message.crdt_sequence),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageStatus {
    pub id: Uuid,
    pub status: i32,
    pub crdt: CrdtWritable,
}
impl From<message::Model> for MessageStatus {
    fn from(value: message::Model) -> Self {
        let id = value.get_uuid();

        MessageStatus {
            id: id.into(),
            status: value.status,
            crdt: CrdtWritable {
                author: Author(value.crdt_author),
                generation: value.crdt_generation,
            },
        }
    }
}
