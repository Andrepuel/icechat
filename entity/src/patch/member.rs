use super::Key;
use crate::{
    crdt::{Author, CrdtAddOnly},
    entity::{conversation, key, member},
    uuid::UuidValue,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Member {
    pub key: Key,
    pub conversation: Uuid,
    pub crdt: CrdtAddOnly,
}
impl From<(key::Model, member::Model, conversation::Model)> for Member {
    fn from((key, member, conversation): (key::Model, member::Model, conversation::Model)) -> Self {
        let conversation = conversation.get_uuid();

        Member {
            key: Key::new(key.public).expect("Inconsistent database"),
            conversation: conversation.into(),
            crdt: CrdtAddOnly(Author(member.crdt_author)),
        }
    }
}
