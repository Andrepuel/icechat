use futures_util::{future::select_all, FutureExt};
use icechat::{
    channel::{Channel, ChannelStateLabel, ChannelValue, Ed25519Cert},
    database::{ChannelData, Contact, Conversation, Database, Message, MessageStatus},
    SqliteChannel,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub struct Chat {
    runtime: Runtime,
    database: Database,
    sync: Vec<SqliteChannel>,
}
impl Chat {
    pub fn load<P: AsRef<Path>>(path: P) -> Chat {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let database = runtime
            .block_on(Database::connect(&path.as_ref().to_string_lossy()))
            .unwrap();

        let mut r = Chat {
            runtime,
            database,
            sync: Default::default(),
        };

        let runtime = r.runtime.handle().clone();
        runtime.block_on(r.sync_channels());

        r
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    async fn sync_channels(&mut self) {
        let mut database = Vec::new();

        for conversation in self.database.list_conversation().await.unwrap() {
            for channel in self.database.list_channels(&conversation).await.unwrap() {
                database.push(channel);
            }
        }

        let database = database
            .into_iter()
            .map(|channel| (channel.channel.clone(), channel))
            .collect::<HashMap<_, _>>();

        let instance = self
            .sync
            .iter()
            .map(|channel| channel.channel().channel.to_string())
            .collect::<HashSet<_>>();

        self.sync.extend(
            database
                .values()
                .filter(|new_channel| !instance.contains(&new_channel.channel))
                .cloned()
                .map(|channel| Channel::new(channel, self.database.private_key().clone())),
        );

        self.sync
            .retain_mut(|channel| database.contains_key(&channel.channel().channel))
    }

    pub fn profile(&self) -> Contact {
        self.runtime
            .block_on(self.database.get_contact(self.database.cert()))
            .unwrap()
            .unwrap()
    }

    pub fn set_profile(&mut self, contact: Contact) {
        self.runtime
            .block_on(self.database.save_contact(contact))
            .unwrap()
    }

    pub fn send_message(&mut self, conversation: Conversation, content: String) {
        self.runtime
            .block_on(self.database.send_message(conversation, content))
            .unwrap()
    }

    pub fn send_file(&mut self, conversation: Conversation, filename: String, payload: Vec<u8>) {
        self.runtime
            .block_on(self.database.send_file(conversation, filename, payload))
            .unwrap()
    }

    pub fn fetch_file_payload(&self, id: i32) -> Option<Vec<u8>> {
        self.runtime
            .block_on(self.database.fetch_file_payload(id))
            .unwrap()
    }

    pub fn create_conversation(&self) -> Conversation {
        self.runtime
            .block_on(self.database.create_conversation(None))
            .unwrap()
    }

    pub fn join_conversation(&mut self, conversation: Uuid, peer: Ed25519Cert) -> Conversation {
        let runtime = self.runtime.handle().clone();
        runtime.block_on(async {
            let conversation = self.database.join_conversation(conversation).await.unwrap();
            self.database
                .create_channel(conversation.clone(), peer)
                .await
                .unwrap();
            self.sync_channels().await;
            conversation
        })
    }

    pub fn refresh_conversation(&self, conversation: &mut Conversation) {
        self.runtime.block_on(async {
            *conversation = self
                .database
                .get_conversation(conversation.uuid)
                .await
                .unwrap()
                .unwrap();
        });
    }

    pub fn save_conversation(&self, conversation: Conversation) {
        self.runtime
            .block_on(self.database.save_conversation(conversation))
            .unwrap()
    }

    pub fn list_conversation(&self) -> Vec<Conversation> {
        self.runtime
            .block_on(self.database.list_conversation())
            .unwrap()
    }

    pub fn new_messages(&mut self) -> Vec<Message> {
        self.runtime.block_on(async {
            let messages = self.database.new_messages(None).await.unwrap();

            for message in messages.iter() {
                self.database
                    .set_message_status(message, MessageStatus::Delivered)
                    .await
                    .unwrap();
            }

            messages
        })
    }

    pub fn add_channel(&mut self, conversation: Conversation, peer: Ed25519Cert) {
        let runtime = self.runtime.handle().clone();

        runtime.block_on(async {
            self.database
                .create_channel(conversation, peer)
                .await
                .unwrap();
            self.sync_channels().await;
        });
    }

    pub fn remove_channel(&mut self, conversation: Conversation, peer: Ed25519Cert) {
        let runtime = self.runtime.handle().clone();

        runtime.block_on(async {
            self.database
                .remove_channel(conversation, peer)
                .await
                .unwrap();
            self.sync_channels().await;
        });
    }

    pub fn channels(&self) -> impl Iterator<Item = (&ChannelData, ChannelStateLabel)> {
        self.sync
            .iter()
            .map(|channel| (channel.channel(), channel.state()))
    }

    pub async fn pre_wait(&mut self) {
        let mut trans = self.database.begin().await.unwrap();
        for sync in self.sync.iter_mut() {
            if sync.state() == ChannelStateLabel::Offline {
                sync.connect(self.database.start_sync(sync.channel().clone()));
            }
            sync.pre_wait(&mut trans).await;
        }
        trans.commit().await.unwrap();
    }

    pub async fn wait(&mut self) -> ChatValue {
        if self.sync.is_empty() {
            std::future::pending::<()>().await;
            unreachable!()
        }

        let (value, index, _) =
            select_all(self.sync.iter_mut().map(|sync| sync.wait().boxed_local())).await;

        (value, index)
    }

    pub async fn then(&mut self, (value, index): ChatValue) {
        self.sync[index].then(value).await;
    }

    pub fn connected(&self) -> bool {
        self.sync
            .iter()
            .any(|sync| sync.state() == ChannelStateLabel::Connected)
    }

    pub async fn close(self) {
        for mut sync in self.sync {
            sync.close().await
        }
    }
}

pub type ChatValue = (ChannelValue, usize);
