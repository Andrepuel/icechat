use crate::{
    channel::{Channel, ChannelStateLabel, ChannelValue},
    database::{
        error::DatabaseResult, AutomergeDbSync, Contact, LocalDatabase, Message, MessageStatus,
        SharedDatabase,
    },
};
use futures_util::{future::select_all, FutureExt};
use std::{
    collections::HashSet,
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
            .collect::<HashSet<_>>();
        let instance = self
            .sync
            .iter()
            .map(|channel| channel.channel().to_string())
            .collect::<HashSet<_>>();

        self.sync.extend(
            database
                .difference(&instance)
                .map(|name| Channel::new(name.clone())),
        );

        self.sync
            .retain_mut(|channel| database.contains(channel.channel()))
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

    pub fn set_message_status(&mut self, index: usize, status: MessageStatus) {
        let message = self.database.list_messages().nth(index).unwrap().unwrap();

        let message = Message { status, ..message };

        self.database.set_message(index, message).unwrap();
    }

    pub fn add_channel(&mut self, channel: String) {
        let mut data = self.settings.get().unwrap();
        data.channels.push(channel);
        self.settings.set(data).unwrap();
        self.sync_channels();
    }

    pub fn remove_channel(&mut self, channel: &str) {
        let mut data = self.settings.get().unwrap();
        data.channels
            .retain(|existent_channel| existent_channel != channel);
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
            sync.pre_wait(&mut self.database);
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
type ChatChannel = Channel<AutomergeDbSync>;

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
