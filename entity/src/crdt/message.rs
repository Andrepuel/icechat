use super::{Author, CrdtValue, CrdtValueTransaction};
use crate::{
    entity::message,
    uuid::{SplitUuid, UuidValue},
};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{
    ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, IntoActiveModel, QueryFilter,
};
use uuid::Uuid;

impl CrdtValue for message::ActiveModel {
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

impl CrdtValueTransaction<message::ActiveModel> for DatabaseTransaction {
    fn save(&mut self, value: message::ActiveModel) -> LocalBoxFuture<'_, message::ActiveModel> {
        async move {
            let existent: Option<message::ActiveModel> = self.existent(value.id()).await;

            let mut active = value.into_active_model();

            if let Some(existent) = existent {
                active.id = existent.id;
            };

            active.save(self).await.unwrap()
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <message::ActiveModel as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<message::ActiveModel>> {
        async move {
            let uuid_filter = SplitUuid::from(id).to_filter::<message::Column>();

            message::Entity::find()
                .filter(uuid_filter.0)
                .filter(uuid_filter.1)
                .filter(uuid_filter.2)
                .filter(uuid_filter.3)
                .one(self)
                .await
                .unwrap()
                .map(IntoActiveModel::into_active_model)
        }
        .boxed_local()
    }
}
