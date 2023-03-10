use super::{Author, CrdtValue, CrdtValueTransaction};
use crate::{
    entity::{conversation, key, message},
    patch::{Contact, Conversation, Key, MessageStatus, NewMessage},
    uuid::SplitUuid,
};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction, EntityTrait, QueryFilter};
use uuid::Uuid;

impl CrdtValue for NewMessage {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn generation(&self) -> i32 {
        self.crdt.generation
    }

    fn set_generation(&mut self, gen: i32) {
        self.crdt.generation = gen;
    }

    fn author(&self) -> Author {
        self.crdt.author
    }

    fn set_author(&mut self, author: Author) {
        self.crdt.author = author;
    }
}

impl CrdtValueTransaction<NewMessage> for DatabaseTransaction {
    type RowId = i32;

    fn save(
        &mut self,
        message: NewMessage,
        existent: Option<(i32, NewMessage)>,
    ) -> LocalBoxFuture<'_, NewMessage> {
        async move {
            let (from, _) = Contact::get_or_create(message.from.clone(), self).await;
            let conversation = Conversation::get_or_create(message.conversation, self).await;

            let mut active = message::ActiveModel {
                id: ActiveValue::NotSet,
                uuid0: ActiveValue::NotSet,
                uuid1: ActiveValue::NotSet,
                uuid2: ActiveValue::NotSet,
                uuid3: ActiveValue::NotSet,
                status: ActiveValue::NotSet,
                from: ActiveValue::Set(from.id),
                conversation: ActiveValue::Set(conversation.id),
                text: ActiveValue::Set(message.text.clone()),
                crdt_generation: ActiveValue::Set(message.crdt.generation),
                crdt_author: ActiveValue::Set(message.crdt.author.0),
                crdt_sequence: ActiveValue::Set(message.sequence.0),
            };

            match existent {
                Some((id, _)) => {
                    active.id = ActiveValue::Unchanged(id);
                }
                None => {
                    let uuid = SplitUuid::from(message.id);
                    active.uuid0 = ActiveValue::Set(uuid.0);
                    active.uuid1 = ActiveValue::Set(uuid.1);
                    active.uuid2 = ActiveValue::Set(uuid.2);
                    active.uuid3 = ActiveValue::Set(uuid.3);
                    active.status = ActiveValue::Set(0);
                }
            }

            active.save(self).await.unwrap();

            message
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <NewMessage as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<(i32, NewMessage)>> {
        async move {
            let uuid_filter = SplitUuid::from(id).to_filter::<message::Column>();

            let message = message::Entity::find()
                .filter(uuid_filter.0)
                .filter(uuid_filter.1)
                .filter(uuid_filter.2)
                .filter(uuid_filter.3)
                .one(self)
                .await
                .unwrap()?;

            let conversation = conversation::Entity::find_by_id(message.conversation)
                .one(self)
                .await
                .unwrap()
                .unwrap();

            let from = key::Entity::find_by_id(message.from)
                .one(self)
                .await
                .unwrap()
                .unwrap();

            let id = message.id;

            Some((id, NewMessage::from((message, from, conversation))))
        }
        .boxed_local()
    }
}

impl CrdtValue for MessageStatus {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn generation(&self) -> i32 {
        self.crdt.generation
    }

    fn set_generation(&mut self, gen: i32) {
        self.crdt.generation = gen;
    }

    fn author(&self) -> Author {
        self.crdt.author
    }

    fn set_author(&mut self, author: Author) {
        self.crdt.author = author;
    }
}

impl CrdtValueTransaction<MessageStatus> for DatabaseTransaction {
    type RowId = i32;

    fn save(
        &mut self,
        status: MessageStatus,
        existent: Option<(Self::RowId, MessageStatus)>,
    ) -> LocalBoxFuture<'_, MessageStatus> {
        async move {
            let active = match existent {
                Some((id, _)) => message::ActiveModel {
                    id: ActiveValue::Unchanged(id),
                    status: ActiveValue::Set(status.status),
                    crdt_generation: ActiveValue::Set(status.crdt.generation),
                    crdt_author: ActiveValue::Set(status.crdt.author.0),
                    ..Default::default()
                },
                None => {
                    let (from, _) = Contact::get_or_create(Key::default(), self).await;
                    let conversation = Conversation::get_or_create(Default::default(), self).await;
                    let uuid = SplitUuid::from(status.id);

                    message::ActiveModel {
                        id: ActiveValue::NotSet,
                        uuid0: ActiveValue::Set(uuid.0),
                        uuid1: ActiveValue::Set(uuid.1),
                        uuid2: ActiveValue::Set(uuid.2),
                        uuid3: ActiveValue::Set(uuid.3),
                        status: ActiveValue::Set(status.status),
                        from: ActiveValue::Set(from.id),
                        conversation: ActiveValue::Set(conversation.id),
                        text: ActiveValue::Set(Default::default()),
                        crdt_generation: ActiveValue::Set(0),
                        crdt_author: ActiveValue::Set(0),
                        crdt_sequence: ActiveValue::Set(0),
                    }
                }
            };

            active.save(self).await.unwrap();

            status
        }
        .boxed_local()
    }

    fn existent(
        &mut self,
        id: <MessageStatus as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<(Self::RowId, MessageStatus)>> {
        async move {
            let uuid_filter = SplitUuid::from(id).to_filter::<message::Column>();

            let message = message::Entity::find()
                .filter(uuid_filter.0)
                .filter(uuid_filter.1)
                .filter(uuid_filter.2)
                .filter(uuid_filter.3)
                .one(self)
                .await
                .unwrap()?;

            let id = message.id;

            Some((id, message.into()))
        }
        .boxed_local()
    }
}
