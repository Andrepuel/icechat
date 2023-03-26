pub mod error;
pub mod sqlite_sync;
pub mod sync;

use self::error::DatabaseResult;
use crate::channel::{Ed25519Cert, Ed25519Seed};
use entity::{entity::local, patch};
use futures_util::future::LocalBoxFuture;
use migration::MigratorTrait;
use ring::signature::{Ed25519KeyPair, KeyPair};
use sea_orm::{
    ActiveModelTrait, ActiveValue, DatabaseConnection, EntityTrait, PaginatorTrait,
    SqlxSqliteConnector, TransactionTrait,
};
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
            patch::Contact::get_or_create(patch::Key::new_exact(&pub_key), &trans).await;

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

    pub async fn _wip_allow_unused(&self) {
        let _ = &self.connection;
        let _ = &self.seed;
        let _ = &self.public;
        let _ = &self.user;
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

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub uuid: Uuid,
    pub name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub from: Uuid,
    pub content: String,
    pub status: MessageStatus,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    #[default]
    Sent,
    Delivered,
    Read,
}
impl From<u64> for MessageStatus {
    fn from(value: u64) -> Self {
        match value {
            0 => MessageStatus::Sent,
            1 => MessageStatus::Delivered,
            2 => MessageStatus::Read,
            _ => panic!(),
        }
    }
}
impl From<MessageStatus> for u64 {
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
    type Message: serde::Serialize + serde::de::DeserializeOwned;

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

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ChannelData {
    pub channel: String,
    pub private_key: Vec<u8>,
    pub peer_cert: Vec<u8>,
}
impl std::fmt::Debug for ChannelData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelData")
            .field("channel", &self.channel)
            .field("private_key", &self.private_key.len())
            .field("peer_cert", &self.peer_cert)
            .finish()
    }
}
impl ChannelData {
    pub fn zero_key(channel: String) -> ChannelData {
        let private_key = [0; 32].to_vec();
        let peer_key = Ed25519KeyPair::from_seed_unchecked(&private_key).unwrap();
        let peer_cert = peer_key.public_key().as_ref().to_owned();

        ChannelData {
            channel,
            private_key,
            peer_cert,
        }
    }
}
