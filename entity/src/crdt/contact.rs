use super::{Author, CrdtValue, CrdtValueTransaction};
use crate::entity::contact;
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{
    ActiveModelTrait, DatabaseTransaction, EntityTrait, IntoActiveModel, PaginatorTrait,
};

impl CrdtValue for contact::Model {
    type Id = i32;

    fn id(&self) -> Self::Id {
        self.key
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
        self.crdt_author = author.0
    }
}

impl CrdtValueTransaction<contact::Model> for DatabaseTransaction {
    fn save(&mut self, value: contact::Model) -> LocalBoxFuture<'_, contact::Model> {
        async move {
            let exists = contact::Entity::find_by_id(value.key)
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

            value
        }
        .boxed_local()
    }

    fn existent(&mut self, key: i32) -> LocalBoxFuture<'_, Option<contact::Model>> {
        async move { contact::Entity::find_by_id(key).one(self).await.unwrap() }.boxed_local()
    }
}
