use crate::{
    channel::{Channel, ChannelStateLabel, ChannelValue},
    database::{AutomergeDbSync, Contact, LocalDatabase, Message, MessageStatus, SharedDatabase},
};
use futures_util::{future::select_all, FutureExt};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct Chat {
    settings: LocalDatabase,
    database: SharedDatabase,
    path: PathBuf,
    sync: Vec<ChatChannel>,
}
impl Chat {
    pub fn load<P: AsRef<Path>, I: IntoIterator<Item = String>>(path: P, connections: I) -> Chat {
        let settings = Self::load_settings(&path);
        let database = Self::load_database(&path, settings.user());
        let sync = connections.into_iter().map(Channel::new).collect();
        let path = path.as_ref().to_owned();

        Chat {
            settings,
            database,
            path,
            sync,
        }
    }

    fn settings_path<P: AsRef<Path>>(path: P) -> PathBuf {
        path.as_ref().join("settings.db")
    }

    fn load_settings<P: AsRef<Path>>(path: P) -> LocalDatabase {
        LocalDatabase::load(&std::fs::read(Self::settings_path(path)).unwrap()).unwrap()
    }

    fn database_path<P: AsRef<Path>>(path: P) -> PathBuf {
        path.as_ref().join("database.db")
    }

    fn load_database<P: AsRef<Path>>(path: P, user: Uuid) -> SharedDatabase {
        SharedDatabase::load_with_user(&std::fs::read(Self::database_path(path)).unwrap(), user)
            .unwrap()
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
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(Self::settings_path(path), settings.save()).unwrap();
        std::fs::write(Self::database_path(path), database.save()).unwrap();
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

    pub async fn wait(&mut self) -> ChatValue {
        let pre_state = self
            .sync
            .iter_mut()
            .map(|sync| {
                let pre_state = sync.state();
                if sync.state() == ChannelStateLabel::Offline {
                    sync.connect(self.database.start_sync());
                }
                sync.pre_wait(&mut self.database);
                pre_state
            })
            .collect::<Vec<_>>();

        let (value, index, _) =
            select_all(self.sync.iter_mut().map(|sync| sync.wait().boxed_local())).await;

        let pre_state = pre_state[index];

        (value, pre_state, index)
    }

    pub async fn then(
        &mut self,
        (value, old_state, index): ChatValue,
    ) -> Option<(ChannelStateLabel, usize)> {
        self.sync[index].then(value).await;
        let new_state = self.sync[index].state();
        self.save();

        if new_state != old_state {
            Some((new_state, index))
        } else {
            None
        }
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

pub type ChatValue = (ChannelValue, ChannelStateLabel, usize);
type ChatChannel = Channel<AutomergeDbSync>;
