use super::{Author, CrdtValue, CrdtValueTransaction};
use crate::entity::member;
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{ActiveModelTrait, DatabaseTransaction, EntityTrait, IntoActiveModel};

impl CrdtValue for member::ActiveModel {
    type Id = (i32, i32);

    fn id(&self) -> Self::Id {
        (
            self.contact.clone().unwrap(),
            self.conversation.clone().unwrap(),
        )
    }

    fn generation(&self) -> i32 {
        0
    }

    fn set_generation(&mut self, _gen: i32) {}

    fn author(&self) -> Author {
        Author(0)
    }

    fn set_author(&mut self, _author: Author) {}
}

impl CrdtValueTransaction<member::ActiveModel> for DatabaseTransaction {
    fn save(&mut self, value: member::ActiveModel) -> LocalBoxFuture<'_, member::ActiveModel> {
        async move {
            let existent: Option<member::ActiveModel> = self.existent(value.id()).await;

            if existent.is_none() {
                value.clone().insert(self).await.unwrap();
            }

            value
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <member::ActiveModel as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<member::ActiveModel>> {
        async move {
            member::Entity::find_by_id(id)
                .one(self)
                .await
                .unwrap()
                .map(IntoActiveModel::into_active_model)
        }
        .boxed_local()
    }
}
