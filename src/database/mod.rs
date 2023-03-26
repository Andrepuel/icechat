pub mod error;
pub mod sqlite_sync;
pub mod sync;

use self::{error::DatabaseResult, sync::PatchSync};
use crate::channel::{Ed25519Cert, Ed25519Seed};
use entity::{
    crdt::{
        sequence::{CrdtWritableSequence, CrdtWritableSequenceTransaction},
        writable::{CrdtWritable, CrdtWritableTransaction},
        Author, CrdtAddOnly, CrdtInstance, CrdtTransaction,
    },
    entity::{channel, contact, conversation, initial_sync, local, member, message},
    patch::{self, Patch},
    uuid::{SplitUuid, UuidValue},
};
use futures_util::future::LocalBoxFuture;
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, ModelTrait, Order, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    SqlxSqliteConnector, TransactionTrait, TryIntoModel,
};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

pub struct Database {
    connection: DatabaseConnection,
    seed: Ed25519Seed,
    public: Ed25519Cert,
    user: i32,
}
impl Database {
    pub async fn connect(path: &str) -> DatabaseResult<Self> {
        let connection = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .max_lifetime(None)
            .idle_timeout(None)
            .connect_with(format!("sqlite://{path}?mode=rwc").parse::<SqliteConnectOptions>()?)
            .await?;

        let connection = SqlxSqliteConnector::from_sqlx_sqlite_pool(connection);
        migration::Migrator::up(&connection, None).await?;
        Self::first_time(&connection).await?;

        let (seed, user) = Self::fetch_user(&connection).await?;
        let public = seed.public_key();

        Ok(Database {
            connection,
            seed,
            public,
            user,
        })
    }

    async fn first_time(conn: &DatabaseConnection) -> DatabaseResult<()> {
        let trans = conn.begin().await?;

        let existent = local::Entity::find().count(&trans).await?;
        if existent > 0 {
            return Ok(());
        }

        let pvt_key = Ed25519Seed::generate();
        let pub_key = pvt_key.public_key();

        let (pub_key, _) =
            patch::Contact::get_or_create(patch::Key::new_exact(&pub_key.0), &trans).await;

        local::ActiveModel {
            key: ActiveValue::Set(pub_key.id),
            private: ActiveValue::Set(pvt_key.to_vec()),
        }
        .insert(&trans)
        .await?;

        trans.commit().await?;

        Ok(())
    }

    async fn fetch_user(conn: &DatabaseConnection) -> DatabaseResult<(Ed25519Seed, i32)> {
        let trans = conn.begin().await?;
        let local = local::Entity::find().one(&trans).await?.unwrap();

        Ok((
            Ed25519Seed::new(local.private.try_into().expect("Corrupted database")),
            local.key,
        ))
    }

    pub async fn begin(&self) -> DatabaseResult<DatabaseTransaction> {
        Ok(self.connection.begin().await?)
    }

    pub fn private_key(&self) -> &Ed25519Seed {
        &self.seed
    }

    pub fn cert(&self) -> &Ed25519Cert {
        &self.public
    }

    pub fn patch_key(&self) -> patch::Key {
        patch::Key::new_exact(&self.public.0)
    }

    pub fn author(&self) -> Author {
        self.public.as_author()
    }

    pub async fn get_contact(&self, key: &Ed25519Cert) -> DatabaseResult<Option<Contact>> {
        let Some((key, contact)) = entity::entity::key::Entity::find()
            .find_also_related(contact::Entity)
            .filter(entity::entity::key::Column::Public.eq(key.0.to_vec()))
            .one(&self.connection)
            .await?
            else { return Ok(None); };
        let Some(contact) = contact else { return Ok(None); };

        Ok(Some((key, contact).into()))
    }

    pub async fn save_contact(&self, contact: Contact) -> DatabaseResult<()> {
        let mut trans = self.connection.begin().await?;

        self.set_new_patch(
            &mut trans,
            patch::Contact {
                key: patch::Key::new_exact(&contact.key.0),
                name: contact.name,
                crdt: Default::default(),
            },
        )
        .await?;

        trans.commit().await?;

        Ok(())
    }

    pub async fn get_conversation(&self, id: Uuid) -> DatabaseResult<Option<Conversation>> {
        let trans = self.connection.begin().await?;

        Self::trans_get_conversation(&trans, id).await
    }

    async fn trans_get_conversation(
        trans: &DatabaseTransaction,
        id: Uuid,
    ) -> DatabaseResult<Option<Conversation>> {
        let id: SplitUuid = id.into();
        let id_filter = id.to_filter::<conversation::Column>();

        let conversation = conversation::Entity::find()
            .filter(id_filter.0)
            .filter(id_filter.1)
            .filter(id_filter.2)
            .filter(id_filter.3)
            .one(trans)
            .await?;

        let Some(conversation) = conversation else { return Ok(None); };

        Ok(Some(Conversation::with_members(trans, conversation).await?))
    }

    pub async fn save_conversation(&self, conversation: Conversation) -> DatabaseResult<()> {
        let mut trans = self.connection.begin().await?;

        self.set_new_patch(
            &mut trans,
            patch::Conversation {
                id: conversation.uuid,
                title: conversation.title,
                crdt: Default::default(),
            },
        )
        .await?;

        trans.commit().await?;
        Ok(())
    }

    pub async fn list_conversation(&self) -> DatabaseResult<Vec<Conversation>> {
        let trans = self.connection.begin().await?;

        let mut r = Vec::new();

        for conversation in conversation::Entity::find().all(&trans).await? {
            r.push(Conversation::with_members(&trans, conversation).await?)
        }

        Ok(r)
    }

    pub async fn create_conversation(&self, title: Option<String>) -> DatabaseResult<Conversation> {
        let mut trans = self.connection.begin().await?;
        let id = Uuid::new_v4();

        let conversation = patch::Conversation {
            id,
            title,
            crdt: Default::default(),
        };

        trans.set(self.author(), conversation).await;
        let conversation = Self::trans_get_conversation(&trans, id).await?.unwrap();

        member::ActiveModel {
            contact: ActiveValue::Set(self.user),
            conversation: ActiveValue::Set(conversation.id),
            crdt_author: ActiveValue::Set(self.author().0),
        }
        .insert(&trans)
        .await?;

        let conversation = Self::trans_get_conversation(&trans, id).await?.unwrap();
        trans.commit().await?;

        Ok(conversation)
    }

    pub async fn join_conversation(&self, uuid: Uuid) -> DatabaseResult<Conversation> {
        let trans = self.connection.begin().await?;

        let existent = Self::trans_get_conversation(&trans, uuid).await?;
        if let Some(existent) = existent {
            return Ok(existent);
        }

        let uuid = SplitUuid::from(uuid);
        let conversation = conversation::ActiveModel {
            id: ActiveValue::NotSet,
            uuid0: ActiveValue::Set(uuid.0),
            uuid1: ActiveValue::Set(uuid.1),
            uuid2: ActiveValue::Set(uuid.2),
            uuid3: ActiveValue::Set(uuid.3),
            title: ActiveValue::Set(Default::default()),
            crdt_generation: ActiveValue::Set(Default::default()),
            crdt_author: ActiveValue::Set(Default::default()),
        }
        .save(&trans)
        .await?;
        let conversation = conversation.try_into_model().unwrap();

        let r = Conversation::with_members(&trans, conversation).await?;

        trans.commit().await?;
        Ok(r)
    }

    pub async fn new_messages(&self) -> DatabaseResult<Vec<Message>> {
        let trans = self.connection.begin().await?;

        let models = message::Entity::find()
            .filter(message::Column::From.ne(self.user))
            .filter(message::Column::Status.eq(0))
            .find_also_related(conversation::Entity)
            .all(&trans)
            .await?;

        let mut r = Vec::new();
        for model in models {
            let (message, conversation) = model;
            let conversation = conversation.unwrap();

            r.push(Message::from_model(&trans, message, conversation.get_uuid().into()).await?);
        }

        Ok(r)
    }

    pub async fn send_message(
        &self,
        conversation: Conversation,
        text: String,
    ) -> DatabaseResult<()> {
        let mut trans = self.connection.begin().await?;
        let id = Uuid::new_v4();

        self.push_new_patch(
            &mut trans,
            patch::NewMessage {
                id,
                from: self.patch_key(),
                conversation: conversation.uuid,
                text,
                crdt: Default::default(),
            },
        )
        .await?;

        trans.commit().await?;
        Ok(())
    }

    pub async fn set_message_status(
        &self,
        message: &Message,
        status: MessageStatus,
    ) -> DatabaseResult<()> {
        let mut trans = self.connection.begin().await?;

        self.set_new_patch(
            &mut trans,
            patch::MessageStatus {
                id: message.uuid,
                conversation: message.conversation,
                status: status.into(),
                crdt: Default::default(),
            },
        )
        .await?;

        trans.commit().await?;
        Ok(())
    }

    pub async fn list_channels(
        &self,
        conversation: &Conversation,
    ) -> DatabaseResult<Vec<ChannelData>> {
        let trans = self.connection.begin().await?;

        let conversation = conversation::Entity::find_by_id(conversation.id)
            .one(&trans)
            .await?;
        let Some(conversation) = conversation else { return Ok(Default::default()); };
        let uuid = conversation.get_uuid().into();

        let mut r = Vec::new();

        for models in channel::Entity::find()
            .filter(channel::Column::Conversation.eq(conversation.id))
            .find_also_related(entity::entity::key::Entity)
            .all(&trans)
            .await?
        {
            let (channel, Some(peer)) = models else { panic!() };
            let peer = Ed25519Cert(
                peer.public
                    .as_slice()
                    .try_into()
                    .expect("Corrupted database"),
            );

            r.push(ChannelData::new(channel.id, uuid, peer, &self.seed));
        }

        Ok(r)
    }

    pub async fn create_channel(
        &self,
        conversation: Conversation,
        peer: Ed25519Cert,
    ) -> DatabaseResult<()> {
        let mut trans = self.connection.begin().await?;

        let peer_key = patch::Key::new_exact(&peer.0);
        let (peer, ..) = patch::Contact::get_or_create(peer_key.clone(), &trans).await;

        let existent_count = channel::Entity::find()
            .filter(channel::Column::Conversation.eq(conversation.id))
            .filter(channel::Column::Peer.eq(peer.id))
            .count(&trans)
            .await?;

        if existent_count > 0 {
            return Ok(());
        }

        Self::merge_new_patch(
            &mut trans,
            patch::Member {
                key: peer_key,
                conversation: conversation.uuid,
                crdt: CrdtAddOnly(self.author()),
            },
        )
        .await?;

        let new_channel = channel::ActiveModel {
            id: ActiveValue::NotSet,
            conversation: ActiveValue::Set(conversation.id),
            peer: ActiveValue::Set(peer.id),
            sync_index: ActiveValue::Set(Self::current_sync_index(&trans).await?),
        }
        .save(&trans)
        .await?;

        Self::initial_sync(&mut trans, new_channel.id.unwrap(), conversation).await?;

        trans.commit().await?;

        Ok(())
    }

    pub async fn remove_channel(
        &self,
        conversation: Conversation,
        peer: Ed25519Cert,
    ) -> DatabaseResult<()> {
        let trans = self.connection.begin().await?;

        let peer_key = patch::Key::new_exact(&peer.0);
        let (peer, ..) = patch::Contact::get_or_create(peer_key.clone(), &trans).await;

        let existent = channel::Entity::find()
            .filter(channel::Column::Conversation.eq(conversation.id))
            .filter(channel::Column::Peer.eq(peer.id))
            .one(&trans)
            .await?;

        let Some(existent) = existent else { return Ok(()); };
        existent.delete(&trans).await?;

        trans.commit().await?;
        Ok(())
    }

    pub fn start_sync(&self, channel: ChannelData) -> PatchSync<DatabaseTransaction> {
        PatchSync::new(
            channel.id,
            channel.peer_cert.as_author(),
            channel.conversation,
        )
    }

    async fn initial_sync(
        trans: &mut DatabaseTransaction,
        channel_id: i32,
        conversation: Conversation,
    ) -> DatabaseResult<()> {
        Self::save_initial_patch(
            trans,
            channel_id,
            patch::Conversation {
                id: conversation.uuid,
                crdt: conversation.crdt,
                title: conversation.title,
            },
        )
        .await?;

        let contacts = contact::Entity::find()
            .find_also_related(entity::entity::key::Entity)
            .all(trans)
            .await?;
        for model in contacts {
            let (contact, key) = model;
            Self::save_initial_patch(
                trans,
                channel_id,
                patch::Contact::from((key.unwrap(), contact)),
            )
            .await?;
        }

        let members = member::Entity::find()
            .filter(member::Column::Conversation.eq(conversation.id))
            .find_also_related(entity::entity::key::Entity)
            .all(trans)
            .await?;
        for model in members {
            let (member, key) = model;
            Self::save_initial_patch(
                trans,
                channel_id,
                patch::Member::from((key.unwrap(), member, conversation.uuid)),
            )
            .await?;
        }

        let messages = message::Entity::find()
            .filter(message::Column::Conversation.eq(conversation.id))
            .find_also_related(entity::entity::key::Entity)
            .all(trans)
            .await?;
        for model in messages {
            let (message, key) = model;
            Self::save_initial_patch(
                trans,
                channel_id,
                patch::NewMessage::from((message.clone(), key.unwrap(), conversation.uuid)),
            )
            .await?;
            Self::save_initial_patch(
                trans,
                channel_id,
                patch::MessageStatus::from((message, conversation.uuid)),
            )
            .await?;
        }

        Ok(())
    }

    async fn set_new_patch<P: CrdtInstance<Crdt = CrdtWritable> + Into<Patch> + 'static>(
        &self,
        trans: &mut DatabaseTransaction,
        patch: P,
    ) -> DatabaseResult<()>
    where
        DatabaseTransaction: CrdtWritableTransaction<P>,
    {
        let patch = trans.set(self.author(), patch).await;

        Self::save_patch_for_sync(trans, patch).await?;

        Ok(())
    }

    async fn push_new_patch<P: CrdtInstance<Crdt = CrdtWritableSequence> + Into<Patch> + 'static>(
        &self,
        trans: &mut DatabaseTransaction,
        patch: P,
    ) -> DatabaseResult<()>
    where
        DatabaseTransaction: CrdtWritableSequenceTransaction<P>,
    {
        let patch = trans.push(self.author(), patch).await;

        Self::save_patch_for_sync(trans, patch).await?;

        Ok(())
    }

    async fn merge_new_patch<P: CrdtInstance + Into<Patch> + 'static>(
        trans: &mut DatabaseTransaction,
        patch: P,
    ) -> DatabaseResult<bool>
    where
        DatabaseTransaction: CrdtTransaction<P>,
    {
        let patch = trans.merge(patch).await;
        let Some(patch) = patch else { return Ok(false); };

        Self::save_patch_for_sync(trans, patch).await?;

        Ok(true)
    }

    async fn save_patch_for_sync<P: Into<Patch>>(
        trans: &DatabaseTransaction,
        patch: P,
    ) -> DatabaseResult<()> {
        let patch: Patch = patch.into();

        entity::entity::sync::ActiveModel {
            id: ActiveValue::NotSet,
            payload: ActiveValue::Set(bincode::serialize(&patch).unwrap()),
        }
        .save(trans)
        .await?;

        Ok(())
    }

    async fn save_initial_patch<P: Into<Patch>>(
        trans: &DatabaseTransaction,
        channel_id: i32,
        patch: P,
    ) -> DatabaseResult<()> {
        let patch: Patch = patch.into();

        initial_sync::ActiveModel {
            id: ActiveValue::NotSet,
            channel: ActiveValue::Set(channel_id),
            payload: ActiveValue::Set(bincode::serialize(&patch).unwrap()),
        }
        .save(trans)
        .await?;

        Ok(())
    }

    async fn current_sync_index(trans: &DatabaseTransaction) -> DatabaseResult<i32> {
        Ok(entity::entity::sync::Entity::find()
            .order_by(entity::entity::sync::Column::Id, Order::Desc)
            .one(trans)
            .await?
            .map(|sync| sync.id)
            .unwrap_or(0))
    }
}

pub struct SharedDatabase {}
impl SharedDatabase {
    pub fn with_user(user: Uuid) -> DatabaseResult<Self> {
        let _ = user;
        unimplemented!()
    }

    pub fn add_contact(&mut self, contact: Contact) -> DatabaseResult<()> {
        let _ = contact;
        unimplemented!()
    }

    pub fn add_message(&mut self, message: Message) -> DatabaseResult<()> {
        let _ = message;
        unimplemented!()
    }

    pub fn set_message(&mut self, index: usize, message: &Message) -> DatabaseResult<()> {
        let _ = index;
        let _ = message;
        unimplemented!()
    }

    pub fn list_messages(&self) -> std::vec::IntoIter<DatabaseResult<Message>> {
        unimplemented!()
    }

    pub fn list_contact(&self) -> std::vec::IntoIter<DatabaseResult<Contact>> {
        unimplemented!()
    }

    pub fn get_contact(&self, uuid: Uuid) -> DatabaseResult<Option<Contact>> {
        let _ = uuid;
        unimplemented!()
    }

    pub fn save(&mut self) -> Vec<u8> {
        unimplemented!()
    }

    pub fn load_with_user(data: &[u8], user: Uuid) -> DatabaseResult<Self> {
        let _ = data;
        let _ = user;
        unimplemented!()
    }

    pub fn start_sync(&self) -> UnimplementedSync {
        UnimplementedSync
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Conversation {
    id: i32,
    pub uuid: Uuid,
    pub title: Option<String>,
    pub crdt: CrdtWritable,
    pub members: Vec<Contact>,
}
impl Conversation {
    async fn with_members(
        trans: &DatabaseTransaction,
        conversation: conversation::Model,
    ) -> DatabaseResult<Conversation> {
        let mut members = Vec::new();

        for models in member::Entity::find()
            .find_also_related(contact::Entity)
            .filter(member::Column::Conversation.eq(conversation.id))
            .all(trans)
            .await?
        {
            let contact = models.1.unwrap();
            let key = entity::entity::key::Entity::find_by_id(contact.key)
                .one(trans)
                .await?
                .unwrap();

            members.push(Contact {
                id: contact.key,
                key: Ed25519Cert(key.public.try_into().unwrap()),
                name: contact.name,
            })
        }

        Ok(Conversation {
            id: conversation.id,
            uuid: conversation.get_uuid().into(),
            title: conversation.title,
            crdt: CrdtWritable {
                generation: conversation.crdt_generation,
                author: Author(conversation.crdt_author),
            },
            members,
        })
    }

    pub async fn length(&self, database: &Database) -> DatabaseResult<usize> {
        let count = message::Entity::find()
            .filter(message::Column::Conversation.eq(self.id))
            .count(&database.connection)
            .await?;

        Ok(count as usize)
    }

    pub async fn get_message(
        &self,
        database: &Database,
        index: usize,
    ) -> DatabaseResult<Option<Message>> {
        let trans = database.connection.begin().await?;

        let message = message::Entity::find()
            .filter(message::Column::Conversation.eq(self.id))
            .order_by(message::Column::CrdtSequence, Order::Asc)
            .order_by(message::Column::CrdtAuthor, Order::Asc)
            .offset(Some(index as u64))
            .one(&trans)
            .await?;

        let Some(message) = message else { return Ok(None); };
        Ok(Some(Message::from_model(&trans, message, self.uuid).await?))
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Contact {
    id: i32,
    pub key: Ed25519Cert,
    pub name: String,
}
impl From<(entity::entity::key::Model, contact::Model)> for Contact {
    fn from((key, contact): (entity::entity::key::Model, contact::Model)) -> Self {
        let key = Ed25519Cert(key.public.as_slice().try_into().unwrap());

        Contact {
            id: contact.key,
            key,
            name: contact.name,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Message {
    id: i32,
    pub uuid: Uuid,
    pub from: Contact,
    pub conversation: Uuid,
    pub content: String,
    pub status: MessageStatus,
}
impl Message {
    pub async fn from_model(
        trans: &DatabaseTransaction,
        message: message::Model,
        conversation: Uuid,
    ) -> DatabaseResult<Self> {
        let (contact, key) = contact::Entity::find_by_id(message.from)
            .find_also_related(entity::entity::key::Entity)
            .one(trans)
            .await?
            .unwrap();
        let key = key.unwrap();

        Ok(Message {
            id: message.id,
            uuid: message.get_uuid().into(),
            from: (key, contact).into(),
            conversation,
            content: message.text,
            status: message.status.into(),
        })
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    #[default]
    Sent,
    Delivered,
    Read,
}
impl From<i32> for MessageStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => MessageStatus::Sent,
            1 => MessageStatus::Delivered,
            2 => MessageStatus::Read,
            _ => panic!(),
        }
    }
}
impl From<MessageStatus> for i32 {
    fn from(value: MessageStatus) -> Self {
        match value {
            MessageStatus::Sent => 0,
            MessageStatus::Delivered => 1,
            MessageStatus::Read => 2,
        }
    }
}

pub trait DbSync {
    type Database;
    type Message: Serialize + DeserializeOwned + std::fmt::Debug;

    fn tx<'a>(
        &'a mut self,
        database: &'a mut Self::Database,
    ) -> LocalBoxFuture<'a, DatabaseResult<Option<Self::Message>>>;
    fn rx<'a>(
        &'a mut self,
        database: &'a mut Self::Database,
        message: Self::Message,
    ) -> LocalBoxFuture<'a, DatabaseResult<()>>;
}

#[derive(Default)]
pub struct UnimplementedSync;
impl DbSync for UnimplementedSync {
    type Database = SharedDatabase;
    type Message = Vec<u8>;

    fn tx<'a>(
        &'a mut self,
        database: &'a mut Self::Database,
    ) -> LocalBoxFuture<'a, DatabaseResult<Option<Vec<u8>>>> {
        let _ = database;
        unimplemented!()
    }

    fn rx<'a>(
        &'a mut self,
        database: &'a mut Self::Database,
        message: Vec<u8>,
    ) -> LocalBoxFuture<'a, DatabaseResult<()>> {
        let _ = database;
        let _ = message;
        unimplemented!()
    }
}

pub struct LocalDatabase {}
impl LocalDatabase {
    pub fn with_user(user: Uuid) -> DatabaseResult<LocalDatabase> {
        let _ = user;
        unimplemented!()
    }

    pub fn user(&self) -> Uuid {
        unimplemented!()
    }

    pub fn set(&mut self, data: LocalDatabaseData) -> DatabaseResult<()> {
        let _ = data;
        unimplemented!()
    }

    pub fn get(&self) -> DatabaseResult<LocalDatabaseData> {
        unimplemented!()
    }

    pub fn save(&mut self) -> Vec<u8> {
        unimplemented!()
    }

    pub fn load(data: &[u8]) -> DatabaseResult<Self> {
        let _ = data;
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDatabaseData {
    pub user: Uuid,
    pub channels: Vec<ChannelData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChannelData {
    id: i32,
    pub conversation: Uuid,
    pub peer_cert: Ed25519Cert,
    pub channel: String,
}
impl ChannelData {
    pub fn new(
        id: i32,
        conversation: Uuid,
        peer_cert: Ed25519Cert,
        private: &Ed25519Seed,
    ) -> ChannelData {
        let channel = private.x25519_agree(&format!("channel:{conversation}"), &peer_cert);

        ChannelData {
            id,
            conversation,
            peer_cert,
            channel,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    mod given_an_empty_database {
        use super::*;

        type Given = (Database,);
        async fn given() -> Given {
            let database = Database::connect(":memory:").await.unwrap();

            (database,)
        }

        mod when_a_conversation_is_created {
            use super::*;

            type Given = (Database, Conversation);
            async fn given() -> Given {
                let (database, ..) = super::given().await;
                let conversation = database
                    .create_conversation(Some("My Conversation".to_string()))
                    .await
                    .unwrap();

                (database, conversation)
            }

            #[tokio::test]
            async fn then_the_conversation_is_fetchable_by_id() {
                let (database, conversation, ..) = given().await;

                assert_eq!(
                    database.get_conversation(conversation.uuid).await.unwrap(),
                    Some(conversation)
                );
            }

            #[tokio::test]
            async fn then_the_conversation_appears_on_the_list() {
                let (database, conversation, ..) = given().await;

                assert_eq!(
                    database.list_conversation().await.unwrap(),
                    vec![conversation]
                );
            }
        }
    }
}
