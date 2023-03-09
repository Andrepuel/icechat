use super::{Author, CrdtValue, CrdtValueTransaction};
use crate::entity::contact;
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{
    ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, IntoActiveModel,
    PaginatorTrait,
};

impl CrdtValue for contact::ActiveModel {
    type Id = i32;

    fn id(&self) -> Self::Id {
        self.key.clone().unwrap()
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
        self.crdt_author = ActiveValue::Set(author.0)
    }
}

impl CrdtValueTransaction<contact::ActiveModel> for DatabaseTransaction {
    fn save(&mut self, value: contact::ActiveModel) -> LocalBoxFuture<'_, contact::ActiveModel> {
        async move {
            let exists = contact::Entity::find_by_id(value.id())
                .count(self)
                .await
                .unwrap();
            let exists = exists > 0;
            let active = value.into_active_model();

            let value = if exists {
                active.update(self).await.unwrap()
            } else {
                active.insert(self).await.unwrap()
            };

            value.into_active_model()
        }
        .boxed_local()
    }

    fn existent(&mut self, key: i32) -> LocalBoxFuture<'_, Option<contact::ActiveModel>> {
        async move {
            contact::Entity::find_by_id(key)
                .one(self)
                .await
                .unwrap()
                .map(IntoActiveModel::into_active_model)
        }
        .boxed_local()
    }
}
