use super::{CrdtAddOnly, CrdtValue, CrdtValueTransaction};
use crate::{
    entity::member,
    patch::{Contact, Conversation, Key, Member},
};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
};
use uuid::Uuid;

impl CrdtValue for Member {
    type Id = (Key, Uuid);
    type Crdt = CrdtAddOnly;

    fn id(&self) -> Self::Id {
        (self.key.clone(), self.conversation)
    }

    fn crdt(&self) -> Self::Crdt {
        CrdtAddOnly
    }

    fn set_crdt(&mut self, _crdt: Self::Crdt) {}
}

impl CrdtValueTransaction<Member> for DatabaseTransaction {
    type RowId = (i32, i32);

    fn save(
        &mut self,
        value: Member,
        existent: Option<(Self::RowId, Member)>,
    ) -> LocalBoxFuture<'_, Member> {
        async move {
            if existent.is_none() {
                let (_, contact) = Contact::get_or_create(value.key.clone(), self).await;
                let conversation = Conversation::get_or_create(value.conversation, self).await;

                member::ActiveModel {
                    contact: ActiveValue::Set(contact.key),
                    conversation: ActiveValue::Set(conversation.id),
                }
                .insert(self)
                .await
                .unwrap();
            }

            value
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <Member as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<(Self::RowId, Member)>> {
        async move {
            let (key, contact) = Contact::get_or_create(id.0, self).await;
            let conversation = Conversation::get_or_create(id.1, self).await;

            member::Entity::find()
                .filter(member::Column::Contact.eq(contact.key))
                .filter(member::Column::Conversation.eq(conversation.id))
                .one(self)
                .await
                .unwrap()
                .map(move |model| {
                    let id = (model.contact, model.conversation);
                    (id, (key, conversation).into())
                })
        }
        .boxed_local()
    }
}
