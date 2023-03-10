use super::{CrdtAddOnly, Key};
use crate::{
    entity::{conversation, key},
    uuid::UuidValue,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
