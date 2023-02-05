use crate::{
    database::{AutomergeDbSync, Contact, LocalDatabase, Message, MessageStatus, SharedDatabase},
    pipe_sync::{PipeSync, PipeSyncValue},
};
use icepipe::crypto_stream::Chacha20Stream;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

pub struct Chat {
    settings: LocalDatabase,
    database: SharedDatabase,
    path: PathBuf,
    sync: PipeSync<AutomergeDbSync, Chacha20Stream>,
}
impl Chat {
    pub fn load<P: AsRef<Path>>(path: P, connection: Chacha20Stream) -> Chat {
        let settings = Self::load_settings(&path);
        let database = Self::load_database(&path, settings.user());
        let sync = database.start_sync();
        let sync = PipeSync::new(sync, connection);
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
        LocalDatabase::load(&std::fs::read(Self::settings_path(path)).unwrap())
    }

    fn database_path<P: AsRef<Path>>(path: P) -> PathBuf {
        path.as_ref().join("database.db")
    }

    fn load_database<P: AsRef<Path>>(path: P, user: Uuid) -> SharedDatabase {
        SharedDatabase::load_with_user(&std::fs::read(Self::database_path(path)).unwrap(), user)
    }

    pub fn init<P: AsRef<Path>>(user: Uuid, path: P) {
        let mut settings = LocalDatabase::with_user(user);
        let mut database = SharedDatabase::with_user(user);
        database.add_contact(Contact {
            uuid: user,
            name: user.to_string(),
        });

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
        self.database.add_contact(contact)
    }

    pub fn get_peer(&self, uuid: Uuid) -> Option<Contact> {
        self.database.get_contact(uuid)
    }

    pub fn send_message(&mut self, content: String) -> Message {
        let message = Message {
            from: self.user(),
            content,
            ..Default::default()
        };

        self.database.add_message(message.clone());

        message
    }

    pub fn list_peers(&self) -> impl DoubleEndedIterator<Item = Contact> + '_ {
        self.database
            .list_contact()
            .filter(|contact| contact.uuid != self.user())
    }

    pub fn list_messages(&self) -> impl DoubleEndedIterator<Item = Message> + '_ {
        self.database.list_messages()
    }

    pub fn set_message_status(&mut self, index: usize, status: MessageStatus) {
        let message = self.database.list_messages().nth(index).unwrap();

        let message = Message { status, ..message };

        self.database.set_message(index, message);
    }

    pub async fn wait(&mut self) -> ChatValue {
        self.sync.pre_wait(&mut self.database);
        self.sync.wait().await
    }

    pub async fn then(&mut self, value: ChatValue) {
        self.sync.then(value).await;
        self.save();
    }

    pub fn connected(&self) -> bool {
        !self.sync.rx_closed()
    }

    pub async fn close(mut self) {
        self.sync.close().await
    }
}

type ChatValue = PipeSyncValue;
