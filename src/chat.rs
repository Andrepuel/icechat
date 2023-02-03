use crate::{
    database::{Contact, LocalDatabase, Message, MessageStatus, SharedDatabase},
    pipe_sync::PipeSync,
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct Chat {
    settings: LocalDatabase,
    database: SharedDatabase,
    path: PathBuf,
}
impl Chat {
    pub fn load<P: AsRef<Path>>(path: P) -> Chat {
        let settings = Self::load_settings(&path);
        let database = Self::load_database(&path, settings.user());
        let path = path.as_ref().to_owned();

        Chat {
            settings,
            database,
            path,
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

    pub fn init<P: AsRef<Path>>(user: Uuid, path: P) -> Chat {
        let settings = LocalDatabase::with_user(user);
        let mut database = SharedDatabase::with_user(user);
        database.add_contact(Contact {
            uuid: user,
            name: user.to_string(),
        });

        let mut r = Chat {
            settings,
            database,
            path: path.as_ref().into(),
        };
        r.save();
        r
    }

    pub fn save(&mut self) {
        std::fs::create_dir_all(&self.path).unwrap();
        std::fs::write(Self::settings_path(&self.path), self.settings.save()).unwrap();
        std::fs::write(Self::database_path(&self.path), self.database.save()).unwrap();
    }

    pub fn reload(&mut self) {
        *self = Self::load(&self.path)
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

    pub async fn synchronize_with(&mut self, channel: &str) {
        let sync = self.database.start_sync();
        let pipe = icepipe::connect(channel, Default::default(), Default::default())
            .await
            .unwrap();
        let pipe_sync = PipeSync::new(sync, pipe);
        pipe_sync.run().await;
    }
}
