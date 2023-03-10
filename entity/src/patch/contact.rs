use super::Key;
use crate::{
    crdt::{writable::CrdtWritable, Author},
    entity::{contact, key},
};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait};
use serde::{Deserialize, Serialize};

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
