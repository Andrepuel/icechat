use super::{Author, CrdtValue, CrdtValueTransaction};
use crate::{
    entity::conversation,
    uuid::{SplitUuid, UuidValue},
};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{
    ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, IntoActiveModel, QueryFilter,
};
use uuid::Uuid;

impl CrdtValue for conversation::ActiveModel {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.get_uuid().into()
    }

    fn generation(&self) -> i32 {
        self.crdt_generation.clone().unwrap()
    }

    fn set_generation(&mut self, gen: i32) {
        self.crdt_generation = ActiveValue::Set(gen);
    }

    fn author(&self) -> Author {
        Author(self.crdt_author.clone().unwrap())
    }

    fn set_author(&mut self, author: Author) {
        self.crdt_author = ActiveValue::Set(author.0);
    }
}

impl CrdtValueTransaction<conversation::ActiveModel> for DatabaseTransaction {
    type RowId = i32;

    fn save(
        &mut self,
        mut active: conversation::ActiveModel,
        existent: Option<(i32, conversation::ActiveModel)>,
    ) -> LocalBoxFuture<'_, conversation::ActiveModel> {
        async move {
            if let Some((id, _)) = existent {
                active.id = ActiveValue::Unchanged(id);
            };

            active.save(self).await.unwrap()
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <conversation::ActiveModel as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<(i32, conversation::ActiveModel)>> {
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
                    (id, model.into_active_model())
                })
        }
        .boxed_local()
    }
}
