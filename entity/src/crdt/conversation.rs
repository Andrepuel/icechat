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

impl CrdtValue for conversation::Model {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.get_uuid().into()
    }

    fn generation(&self) -> i32 {
        self.crdt_generation
    }

    fn set_generation(&mut self, gen: i32) {
        self.crdt_generation = gen;
    }

    fn author(&self) -> Author {
        Author(self.crdt_author)
    }

    fn set_author(&mut self, author: Author) {
        self.crdt_author = author.0;
    }
}

impl CrdtValueTransaction<conversation::Model> for DatabaseTransaction {
    fn save(&mut self, value: conversation::Model) -> LocalBoxFuture<'_, conversation::Model> {
        async move {
            let exists: Option<conversation::Model> = self.existent(value.id()).await;

            let mut active = value.into_active_model();

            let model = if let Some(exists) = exists {
                active.id = ActiveValue::Unchanged(exists.id);
                active.update(self).await.unwrap()
            } else {
                active.id = ActiveValue::NotSet;
                active.insert(self).await.unwrap()
            };

            model
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <conversation::Model as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<conversation::Model>> {
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
        }
        .boxed_local()
    }
}
