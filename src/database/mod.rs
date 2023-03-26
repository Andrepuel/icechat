pub mod error;
pub mod sqlite_sync;
pub mod sync;

use self::error::DatabaseResult;
use futures_util::future::LocalBoxFuture;
use ring::signature::{Ed25519KeyPair, KeyPair};
use uuid::Uuid;

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
