use crate::{
    crdt::{Author, CrdtWritable},
    entity::conversation,
    uuid::{SplitUuid, UuidValue},
};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
