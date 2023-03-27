use super::Conversation;
use crate::{
    crdt::{Author, CrdtAddOnly},
    entity::{attachment, conversation},
    uuid::{SplitUuid, UuidValue},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, FromQueryResult, Iterable,
    QueryFilter, QuerySelect, SelectModel, Selector, TryIntoModel,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Attachment {
    pub id: Uuid,
    pub conversation: Uuid,
    pub payload: Option<Vec<u8>>,
    pub crdt: CrdtAddOnly,
}
impl From<(Uuid, attachment::Model)> for Attachment {
    fn from((conversation, attachment): (Uuid, attachment::Model)) -> Self {
        Attachment {
            id: attachment.get_uuid().into(),
            conversation,
            payload: attachment.payload,
            crdt: CrdtAddOnly(Author(attachment.crdt_author)),
        }
    }
}
impl From<(conversation::Model, attachment::Model)> for Attachment {
    fn from((conversation, attachment): (conversation::Model, attachment::Model)) -> Self {
        let conversation = Uuid::from(conversation.get_uuid());

        (conversation, attachment).into()
    }
}
impl Attachment {
    pub async fn get_or_create(uuid: Uuid, trans: &DatabaseTransaction) -> AttachmentMetaModel {
        let uuid = SplitUuid::from(uuid);
        let uuid_filter = uuid.to_filter::<attachment::Column>();

        let existent = attachment::Entity::find()
            .filter(uuid_filter.0)
            .filter(uuid_filter.1)
            .filter(uuid_filter.2)
            .filter(uuid_filter.3)
            .columns(
                attachment::Column::iter()
                    .filter(|col| !matches!(col, attachment::Column::Payload)),
            )
            .into_model::<AttachmentMetaModel>()
            .one(trans)
            .await
            .unwrap();

        match existent {
            Some(existent) => existent,
            None => {
                let conversation = Conversation::get_or_create(Default::default(), trans).await;

                attachment::ActiveModel {
                    id: ActiveValue::NotSet,
                    uuid0: ActiveValue::Set(uuid.0),
                    uuid1: ActiveValue::Set(uuid.1),
                    uuid2: ActiveValue::Set(uuid.2),
                    uuid3: ActiveValue::Set(uuid.3),
                    conversation: ActiveValue::Set(conversation.id),
                    payload: ActiveValue::Set(None),
                    crdt_author: ActiveValue::Set(0),
                }
                .save(trans)
                .await
                .unwrap()
                .try_into_model()
                .unwrap()
                .into()
            }
        }
    }
}

#[derive(FromQueryResult)]
pub struct AttachmentMetaModel {
    pub id: i32,
    pub uuid0: i32,
    pub uuid1: i32,
    pub uuid2: i32,
    pub uuid3: i32,
    pub conversation: i32,
    pub crdt_author: i32,
}
impl From<attachment::Model> for AttachmentMetaModel {
    fn from(value: attachment::Model) -> Self {
        AttachmentMetaModel {
            id: value.id,
            uuid0: value.uuid0,
            uuid1: value.uuid1,
            uuid2: value.uuid2,
            uuid3: value.uuid3,
            conversation: value.conversation,
            crdt_author: value.crdt_author,
        }
    }
}
impl AttachmentMetaModel {
    pub fn find_by_id(id: i32) -> Selector<SelectModel<AttachmentMetaModel>> {
        attachment::Entity::find_by_id(id)
            .columns(
                attachment::Column::iter()
                    .filter(|col| !matches!(col, attachment::Column::Payload)),
            )
            .into_model::<AttachmentMetaModel>()
    }
}
