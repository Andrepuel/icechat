use super::{CrdtValue, CrdtValueTransaction, CrdtWritable};
use crate::{
    entity::contact,
    patch::{Contact, Key},
};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait};

impl CrdtValue for Contact {
    type Id = Key;
    type Crdt = CrdtWritable;

    fn id(&self) -> Self::Id {
        self.key.clone()
    }

    fn crdt(&self) -> Self::Crdt {
        self.crdt
    }

    fn set_crdt(&mut self, crdt: Self::Crdt) {
        self.crdt = crdt;
    }
}

impl CrdtValueTransaction<Contact> for DatabaseTransaction {
    type RowId = i32;

    fn save(
        &mut self,
        contact: Contact,
        existent: Option<(i32, Contact)>,
    ) -> LocalBoxFuture<'_, Contact> {
        async move {
            let key = match existent {
                Some((id, _)) => id,
                None => contact.key.get_or_create(self).await.id,
            };

            let active = contact::ActiveModel {
                key: ActiveValue::Set(key),
                name: ActiveValue::Set(contact.name.clone()),
                crdt_generation: ActiveValue::Set(contact.crdt.generation),
                crdt_author: ActiveValue::Set(contact.crdt.author.0),
            };

            match existent {
                Some(_) => active.update(self).await.unwrap(),
                None => active.insert(self).await.unwrap(),
            };

            contact
        }
        .boxed_local()
    }

    fn existent(&mut self, key: Key) -> LocalBoxFuture<'_, Option<(i32, Contact)>> {
        async move {
            let key = key.get_or_create(self).await;

            contact::Entity::find_by_id(key.id)
                .one(self)
                .await
                .unwrap()
                .map(move |model| {
                    let id = model.key;
                    (id, (key, model).into())
                })
        }
        .boxed_local()
    }
}
