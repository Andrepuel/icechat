use super::{Author, CrdtValue, CrdtValueTransaction};
use crate::entity::contact;
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, IntoActiveModel};

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
    type RowId = i32;

    fn save(
        &mut self,
        active: contact::ActiveModel,
        existent: Option<(i32, contact::ActiveModel)>,
    ) -> LocalBoxFuture<'_, contact::ActiveModel> {
        async move {
            let value = match existent {
                Some(_) => active.update(self).await.unwrap(),
                None => active.insert(self).await.unwrap(),
            };

            value.into_active_model()
        }
        .boxed_local()
    }

    fn existent(&mut self, key: i32) -> LocalBoxFuture<'_, Option<(i32, contact::ActiveModel)>> {
        async move {
            contact::Entity::find_by_id(key)
                .one(self)
                .await
                .unwrap()
                .map(|model| {
                    let id = model.key;
                    (id, model.into_active_model())
                })
        }
        .boxed_local()
    }
}
