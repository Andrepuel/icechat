use super::Key;
use crate::{
    crdt::{sequence::CrdtWritableSequence, writable::CrdtWritable, Author},
    entity::{conversation, key, message},
    uuid::UuidValue,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NewMessage {
    pub id: Uuid,
    pub from: Key,
    pub conversation: Uuid,
    pub text: String,
    pub crdt: CrdtWritableSequence,
}
impl From<(message::Model, key::Model, Uuid)> for NewMessage {
    fn from((message, from, conversation): (message::Model, key::Model, Uuid)) -> Self {
        let id = message.get_uuid();
        let from = Key::new(from.public).expect("Inconsistent database");

        NewMessage {
            id: id.into(),
            from,
            conversation,
            text: message.text,
            crdt: CrdtWritableSequence {
                writable: CrdtWritable {
                    author: Author(message.crdt_author),
                    generation: message.crdt_generation,
                },
                sequence: message.crdt_sequence,
            },
        }
    }
}
impl From<(message::Model, key::Model, conversation::Model)> for NewMessage {
    fn from(
        (message, from, conversation): (message::Model, key::Model, conversation::Model),
    ) -> Self {
        let conversation = Uuid::from(conversation.get_uuid());

        (message, from, conversation).into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageStatus {
    pub id: Uuid,
    pub conversation: Uuid,
    pub status: i32,
    pub crdt: CrdtWritable,
}
impl From<(message::Model, Uuid)> for MessageStatus {
    fn from((message, conversation): (message::Model, Uuid)) -> Self {
        let id = message.get_uuid();

        MessageStatus {
            id: id.into(),
            conversation,
            status: message.status,
            crdt: CrdtWritable {
                author: Author(message.status_crdt_author),
                generation: message.status_crdt_generation,
            },
        }
    }
}
impl From<(message::Model, conversation::Model)> for MessageStatus {
    fn from((message, conversation): (message::Model, conversation::Model)) -> Self {
        let conversation = Uuid::from(conversation.get_uuid());

        (message, conversation).into()
    }
}
