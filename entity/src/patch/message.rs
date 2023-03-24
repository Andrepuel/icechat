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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageStatus {
    pub id: Uuid,
    pub conversation: Uuid,
    pub status: i32,
    pub crdt: CrdtWritable,
}
impl From<(message::Model, conversation::Model)> for MessageStatus {
    fn from((message, conversation): (message::Model, conversation::Model)) -> Self {
        let id = message.get_uuid();
        let conversation = conversation.get_uuid();

        MessageStatus {
            id: id.into(),
            conversation: conversation.into(),
            status: message.status,
            crdt: CrdtWritable {
                author: Author(message.status_crdt_author),
                generation: message.status_crdt_generation,
            },
        }
    }
}
