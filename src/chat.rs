use crate::{
    channel::{Channel, ChannelStateLabel, ChannelValue, Ed25519Cert, Ed25519Seed},
    database::{
        error::DatabaseResult, ChannelData, Contact, LocalDatabase, Message, MessageStatus,
        SharedDatabase, UnimplementedSync,
    },
};
use futures_util::{future::select_all, FutureExt};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub struct Chat {
    settings: LocalDatabase,
    database: SharedDatabase,
    path: PathBuf,
    sync: Vec<ChatChannel>,
}
impl Chat {
    pub fn load<P: AsRef<Path>>(path: P) -> Chat {
        let serialized: ChatSerialized =
            bincode::deserialize(&std::fs::read(&path).unwrap()).unwrap();
        let (settings, database) = serialized.load().unwrap();

        let path = path.as_ref().to_owned();

        let mut r = Chat {
            settings,
            database,
            path,
            sync: Default::default(),
        };

        r.sync_channels();

        r
    }

    fn sync_channels(&mut self) {
        let database = self
            .settings
            .get()
            .unwrap()
            .channels
            .into_iter()
            .map(|channel| (channel.channel.clone(), channel))
            .collect::<HashMap<_, _>>();
        let instance = self
            .sync
            .iter()
            .map(|channel| channel.channel().to_string())
            .collect::<HashSet<_>>();

        self.sync.extend(
            database
                .values()
                .filter(|new_channel| !instance.contains(&new_channel.channel))
                .cloned()
                .map(|channel| {
                    Channel::new(
                        channel.channel,
                        Ed25519Seed::new(channel.private_key.try_into().unwrap()),
                        channel.peer_cert.try_into().unwrap(),
                    )
                }),
        );

        self.sync
            .retain_mut(|channel| database.contains_key(channel.channel()))
    }

    pub fn init<P: AsRef<Path>>(user: Uuid, path: P) {
        let mut settings = LocalDatabase::with_user(user).unwrap();
        let mut database = SharedDatabase::with_user(user).unwrap();
        database
            .add_contact(Contact {
                uuid: user,
                name: user.to_string(),
            })
            .unwrap();

        Self::save_with(&mut settings, &mut database, path.as_ref());
    }

    pub fn save(&mut self) {
        Self::save_with(&mut self.settings, &mut self.database, &self.path)
    }

    fn save_with(settings: &mut LocalDatabase, database: &mut SharedDatabase, path: &Path) {
        let contents = bincode::serialize(&ChatSerialized::new(settings, database)).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    pub fn user(&self) -> Uuid {
        self.settings.user()
    }

    pub fn profile(&self) -> Contact {
        self.get_peer(self.user()).unwrap_or_else(|| Contact {
            uuid: self.user(),
            ..Default::default()
        })
    }

    pub fn set_profile(&mut self, contact: Contact) {
        assert_eq!(contact.uuid, self.user());
        self.database.add_contact(contact).unwrap()
    }

    pub fn get_peer(&self, uuid: Uuid) -> Option<Contact> {
        self.database.get_contact(uuid).unwrap()
    }

    pub fn send_message(&mut self, content: String) -> Message {
        let message = Message {
            from: self.user(),
            content,
            ..Default::default()
        };

        self.database.add_message(message.clone()).unwrap();

        message
    }

    pub fn list_peers(&self) -> impl DoubleEndedIterator<Item = Contact> + '_ {
        self.database
            .list_contact()
            .map(Result::unwrap)
            .filter(|contact| contact.uuid != self.user())
    }

    pub fn list_messages(&self) -> impl DoubleEndedIterator<Item = Message> + '_ {
        self.database.list_messages().map(Result::unwrap)
    }

    pub fn new_messages(&mut self) -> Vec<Message> {
        let user = self.user();

        #[allow(clippy::needless_collect)]
        let messages = self
            .list_messages()
            .enumerate()
            .filter(|(_, message)| message.from != user && message.status == MessageStatus::Sent)
            .collect::<Vec<_>>();

        messages
            .into_iter()
            .map(|(index, mut message)| {
                message.status = MessageStatus::Delivered;
                self.database.set_message(index, &message).unwrap();
                message
            })
            .collect()
    }

    pub fn set_message_status(&mut self, index: usize, status: MessageStatus) {
        let message = self.database.list_messages().nth(index).unwrap().unwrap();

        let message = Message { status, ..message };

        self.database.set_message(index, &message).unwrap();
    }

    pub fn add_channel(&mut self, channel: String, key: Ed25519Seed, peer: Ed25519Cert) {
        let mut data = self.settings.get().unwrap();
        data.channels.push(ChannelData {
            channel,
            private_key: key.to_vec(),
            peer_cert: peer.to_vec(),
        });
        self.settings.set(data).unwrap();
        self.sync_channels();
    }

    pub fn remove_channel(&mut self, channel: &str) {
        let mut data = self.settings.get().unwrap();
        data.channels
            .retain(|existent_channel| existent_channel.channel != channel);
        self.settings.set(data).unwrap();
        self.sync_channels();
    }

    pub fn channels(&self) -> impl Iterator<Item = (&str, ChannelStateLabel)> {
        self.sync
            .iter()
            .map(|channel| (channel.channel(), channel.state()))
    }

    pub async fn wait(&mut self) -> ChatValue {
        if self.sync.is_empty() {
            std::future::pending::<()>().await;
            unreachable!()
        }

        for sync in self.sync.iter_mut() {
            if sync.state() == ChannelStateLabel::Offline {
                sync.connect(self.database.start_sync());
            }
            sync.pre_wait(&mut self.database).await;
        }

        let (value, index, _) =
            select_all(self.sync.iter_mut().map(|sync| sync.wait().boxed_local())).await;

        (value, index)
    }

    pub async fn then(&mut self, (value, index): ChatValue) {
        self.sync[index].then(value).await;
        self.save();
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
type ChatChannel = Channel<UnimplementedSync>;

#[derive(serde::Serialize, serde::Deserialize)]
struct ChatSerialized {
    settings: Vec<u8>,
    database: Vec<u8>,
}
impl ChatSerialized {
    fn new(settings: &mut LocalDatabase, database: &mut SharedDatabase) -> Self {
        let settings = settings.save();
        let database = database.save();

        Self { settings, database }
    }

    fn load(&self) -> DatabaseResult<(LocalDatabase, SharedDatabase)> {
        let settings = LocalDatabase::load(&self.settings)?;
        let database = SharedDatabase::load_with_user(&self.database, settings.user())?;

        Ok((settings, database))
    }
}
