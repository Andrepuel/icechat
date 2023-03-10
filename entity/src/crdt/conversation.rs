use super::{CrdtValue, CrdtValueTransaction, CrdtWritable};
use crate::{entity::conversation, patch::Conversation, uuid::SplitUuid};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, QueryFilter};
use uuid::Uuid;

impl CrdtValue for Conversation {
    type Id = Uuid;
    type Crdt = CrdtWritable;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn crdt(&self) -> Self::Crdt {
        self.crdt
    }

    fn set_crdt(&mut self, crdt: Self::Crdt) {
        self.crdt = crdt;
    }
}

impl CrdtValueTransaction<Conversation> for DatabaseTransaction {
    type RowId = i32;

    fn save(
        &mut self,
        conversation: Conversation,
        existent: Option<(i32, Conversation)>,
    ) -> LocalBoxFuture<'_, Conversation> {
        async move {
            let mut active = conversation::ActiveModel {
                id: ActiveValue::NotSet,
                uuid0: ActiveValue::NotSet,
                uuid1: ActiveValue::NotSet,
                uuid2: ActiveValue::NotSet,
                uuid3: ActiveValue::NotSet,
                title: ActiveValue::Set(conversation.title.clone()),
                crdt_generation: ActiveValue::Set(conversation.crdt.generation),
                crdt_author: ActiveValue::Set(conversation.crdt.author.0),
            };

            match existent {
                Some((id, _)) => {
                    active.id = ActiveValue::Unchanged(id);
                }
                None => {
                    let uuid = SplitUuid::from(conversation.id);

                    active.uuid0 = ActiveValue::Set(uuid.0);
                    active.uuid1 = ActiveValue::Set(uuid.1);
                    active.uuid2 = ActiveValue::Set(uuid.2);
                    active.uuid3 = ActiveValue::Set(uuid.3);
                }
            }

            active.save(self).await.unwrap();

            conversation
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <Conversation as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<(i32, Conversation)>> {
        async move {
            let uuid_filter = SplitUuid::from(id).to_filter::<conversation::Column>();

            conversation::Entity::find()
                .filter(uuid_filter.0)
                .filter(uuid_filter.1)
                .filter(uuid_filter.2)
                .filter(uuid_filter.3)
                .one(self)
                .await
                .unwrap()
                .map(|model| {
                    let id = model.id;
                    (id, model.into())
                })
        }
        .boxed_local()
    }
}
