use super::{CrdtAddOnly, CrdtInstance, CrdtTransaction};
use crate::{
    entity::{attachment, conversation},
    patch::{Attachment, Conversation},
    uuid::SplitUuid,
};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, QueryFilter};
use uuid::Uuid;

impl CrdtInstance for Attachment {
    type Id = Uuid;
    type Crdt = CrdtAddOnly;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn crdt(&self) -> Self::Crdt {
        self.crdt
    }

    fn set_crdt(&mut self, crdt: Self::Crdt) {
        self.crdt = crdt
    }
}

impl CrdtTransaction<Attachment> for DatabaseTransaction {
    type RowId = i32;

    fn save(
        &mut self,
        value: Attachment,
        existent: Option<(Self::RowId, Attachment)>,
    ) -> LocalBoxFuture<'_, Attachment> {
        async move {
            let uuid = SplitUuid::from(value.id);
            let conversation = Conversation::get_or_create(value.conversation, self).await;

            let model = attachment::ActiveModel {
                id: match existent {
                    Some((id, _)) => ActiveValue::Set(id),
                    None => ActiveValue::NotSet,
                },
                uuid0: ActiveValue::Set(uuid.0),
                uuid1: ActiveValue::Set(uuid.1),
                uuid2: ActiveValue::Set(uuid.2),
                uuid3: ActiveValue::Set(uuid.3),
                conversation: ActiveValue::Set(conversation.id),
                payload: ActiveValue::Set(value.payload.clone()),
                crdt_author: ActiveValue::Set(value.crdt.0 .0),
            };

            model.save(self).await.unwrap();

            value
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <Attachment as CrdtInstance>::Id,
    ) -> LocalBoxFuture<'_, Option<(Self::RowId, Attachment)>> {
        async move {
            let uuid_filter = SplitUuid::from(id).to_filter::<attachment::Column>();

            let (attachment, conversation) = attachment::Entity::find()
                .find_also_related(conversation::Entity)
                .filter(uuid_filter.0)
                .filter(uuid_filter.1)
                .filter(uuid_filter.2)
                .filter(uuid_filter.3)
                .one(self)
                .await
                .unwrap()?;
            let conversation = conversation.unwrap();

            let rowid = attachment.id;

            Some((rowid, (conversation, attachment).into()))
        }
        .boxed_local()
    }
}
