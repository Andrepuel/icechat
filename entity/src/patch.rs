use crate::{
    crdt::{Author, CrdtValue},
    entity::{contact, conversation, key, member, message},
    uuid::SplitUuid,
};
use futures::{future::LocalBoxFuture, FutureExt};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, PaginatorTrait,
    QueryFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub enum Patch {
    Contact(Contact),
    Conversation(Conversation),
    Member(Member),
    Message(Message),
}
impl Patch {
    pub async fn to_crdt(&self, trans: &DatabaseTransaction) -> Result<PatchCrdt, PatchError> {
        Ok(match self {
            Patch::Contact(value) => PatchCrdt::Contact(value.to_crdt(trans).await?),
            Patch::Conversation(value) => PatchCrdt::Conversation(value.to_crdt(trans).await?),
            Patch::Member(value) => PatchCrdt::Member(value.to_crdt(trans).await?),
            Patch::Message(value) => PatchCrdt::Message(value.to_crdt(trans).await?),
        })
    }
}

#[derive(Debug)]
pub enum PatchCrdt {
    Contact(<Contact as PatchTrait>::Crdt),
    Conversation(<Conversation as PatchTrait>::Crdt),
    Member(<Member as PatchTrait>::Crdt),
    Message(<Message as PatchTrait>::Crdt),
}

#[derive(thiserror::Error, Debug)]
pub enum PatchError {
    #[error(transparent)]
    BadKey(#[from] BadKeyError),
    #[error("Deserialization error")]
    Deserialization,
}

pub trait PatchTrait {
    type Crdt: CrdtValue;

    fn to_crdt<'a>(
        &'a self,
        trans: &'a DatabaseTransaction,
    ) -> LocalBoxFuture<'a, Result<Self::Crdt, PatchError>>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrdtWritable {
    author: Author,
    generation: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrdtSequence(i32);

#[derive(Debug, Serialize, Deserialize)]
pub struct CrdtAddOnly;

pub const KEY_LENGTH: usize = 32;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Key(pub Vec<u8>);
impl Key {
    pub async fn get_foreign_key(&self, trans: &DatabaseTransaction) -> Result<i32, BadKeyError> {
        let existent = key::Entity::find()
            .filter(key::Column::Public.eq(self.0.clone()))
            .one(trans)
            .await
            .unwrap();

        let model = match existent {
            Some(existent) => existent,
            None => {
                if self.0.len() != KEY_LENGTH {
                    return Err(BadKeyError);
                }

                key::ActiveModel {
                    id: ActiveValue::NotSet,
                    public: ActiveValue::Set(self.0.clone()),
                }
                .insert(trans)
                .await
                .unwrap()
            }
        };

        Ok(model.id)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Key should have 32 bytes")]
pub struct BadKeyError;

#[derive(Debug, Serialize, Deserialize)]
pub struct Contact {
    key: Key,
    name: String,
    crdt: CrdtWritable,
}
impl PatchTrait for Contact {
    type Crdt = contact::ActiveModel;

    fn to_crdt<'a>(
        &'a self,
        trans: &'a DatabaseTransaction,
    ) -> LocalBoxFuture<'a, Result<Self::Crdt, PatchError>> {
        async move {
            let key = self.key.get_foreign_key(trans).await?;

            Ok(contact::ActiveModel {
                key: ActiveValue::Unchanged(key),
                name: ActiveValue::Set(self.name.clone()),
                crdt_generation: ActiveValue::Set(self.crdt.generation),
                crdt_author: ActiveValue::Set(self.crdt.author.0),
            })
        }
        .boxed_local()
    }
}
impl Contact {
    pub async fn get_foreign_key(
        key: Key,
        trans: &DatabaseTransaction,
    ) -> Result<i32, BadKeyError> {
        let key = key.get_foreign_key(trans).await.unwrap();

        let count = contact::Entity::find_by_id(key).count(trans).await.unwrap();

        if count == 0 {
            contact::ActiveModel {
                key: ActiveValue::Set(key),
                name: ActiveValue::Set(Default::default()),
                crdt_generation: ActiveValue::Set(0),
                crdt_author: ActiveValue::Set(0),
            }
            .insert(trans)
            .await
            .unwrap();
        }

        Ok(key)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    id: Uuid,
    title: String,
    crdt: CrdtWritable,
}
impl PatchTrait for Conversation {
    type Crdt = conversation::ActiveModel;

    fn to_crdt<'a>(
        &'a self,
        _trans: &'a DatabaseTransaction,
    ) -> LocalBoxFuture<'a, Result<Self::Crdt, PatchError>> {
        async move {
            let id = SplitUuid::from(self.id);

            Ok(conversation::ActiveModel {
                id: ActiveValue::NotSet,
                uuid0: ActiveValue::Set(id.0),
                uuid1: ActiveValue::Set(id.1),
                uuid2: ActiveValue::Set(id.2),
                uuid3: ActiveValue::Set(id.3),
                title: ActiveValue::Set(Some(self.title.clone())),
                crdt_generation: ActiveValue::Set(self.crdt.generation),
                crdt_author: ActiveValue::Set(self.crdt.author.0),
            })
        }
        .boxed_local()
    }
}
impl Conversation {
    pub async fn get_foreign_key(uuid: Uuid, trans: &DatabaseTransaction) -> i32 {
        let uuid = SplitUuid::from(uuid);
        let uuid_filter = uuid.to_filter::<conversation::Column>();
        let existent = conversation::Entity::find()
            .filter(uuid_filter.0)
            .filter(uuid_filter.1)
            .filter(uuid_filter.2)
            .filter(uuid_filter.3)
            .one(trans)
            .await
            .unwrap();

        let model = match existent {
            Some(existent) => existent,
            None => conversation::ActiveModel {
                id: ActiveValue::NotSet,
                uuid0: ActiveValue::Set(uuid.0),
                uuid1: ActiveValue::Set(uuid.1),
                uuid2: ActiveValue::Set(uuid.2),
                uuid3: ActiveValue::Set(uuid.3),
                title: ActiveValue::Set(Default::default()),
                crdt_generation: ActiveValue::Set(0),
                crdt_author: ActiveValue::Set(0),
            }
            .insert(trans)
            .await
            .unwrap(),
        };

        model.id
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Member {
    key: Key,
    conversation: Uuid,
    crdt: CrdtAddOnly,
}
impl PatchTrait for Member {
    type Crdt = member::ActiveModel;

    fn to_crdt<'a>(
        &'a self,
        trans: &'a DatabaseTransaction,
    ) -> LocalBoxFuture<'a, Result<Self::Crdt, PatchError>> {
        async move {
            let contact = Contact::get_foreign_key(self.key.clone(), trans).await?;
            let conversation = Conversation::get_foreign_key(self.conversation, trans).await;

            Ok(member::ActiveModel {
                contact: ActiveValue::Set(contact),
                conversation: ActiveValue::Set(conversation),
            })
        }
        .boxed_local()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    id: Uuid,
    status: Option<i32>,
    from: Option<Key>,
    conversation: Option<Uuid>,
    text: Option<String>,
    crdt: CrdtWritable,
    sequence: CrdtSequence,
}
impl PatchTrait for Message {
    type Crdt = message::ActiveModel;

    fn to_crdt<'a>(
        &'a self,
        trans: &'a DatabaseTransaction,
    ) -> LocalBoxFuture<'a, Result<Self::Crdt, PatchError>> {
        async move {
            let id = SplitUuid::from(self.id);

            let from = match self.from.clone() {
                Some(from) => Some(Contact::get_foreign_key(from, trans).await?),
                None => None,
            };

            let conversation = match self.conversation {
                Some(conversation) => {
                    Some(Conversation::get_foreign_key(conversation, trans).await)
                }
                None => None,
            };

            Ok(message::ActiveModel {
                id: ActiveValue::NotSet,
                uuid0: ActiveValue::Set(id.0),
                uuid1: ActiveValue::Set(id.1),
                uuid2: ActiveValue::Set(id.2),
                uuid3: ActiveValue::Set(id.3),
                status: match self.status {
                    Some(status) => ActiveValue::Set(status),
                    None => ActiveValue::NotSet,
                },
                from: match from {
                    Some(from) => ActiveValue::Set(from),
                    None => ActiveValue::NotSet,
                },
                conversation: match conversation {
                    Some(conversation) => ActiveValue::Set(conversation),
                    None => ActiveValue::NotSet,
                },
                text: match self.text.clone() {
                    Some(text) => ActiveValue::Set(text),
                    None => ActiveValue::NotSet,
                },
                crdt_generation: ActiveValue::Set(self.crdt.generation),
                crdt_author: ActiveValue::Set(self.crdt.author.0),
                crdt_sequence: ActiveValue::Set(self.sequence.0),
            })
        }
        .boxed_local()
    }
}
