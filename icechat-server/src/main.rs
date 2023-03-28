use clap::Parser;
use futures_util::{
    future::{pending, select_all},
    FutureExt,
};
use icechat::{
    channel::{BadEd25519CertStr, ChannelStateLabel, ChannelValue, Ed25519Cert},
    database::{
        error::{DatabaseError, DatabaseResult},
        Conversation, Database, Message,
    },
    SqliteChannel,
};
use std::{collections::HashSet, time::Duration};
use tokio::task::LocalSet;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    env_logger::init();

    let path = std::env::args()
        .nth(1)
        .expect("Must provide path to database");
    let control_user = std::env::args().nth(2);
    let local_set = LocalSet::new();

    local_set.spawn_local(async move {
        loop {
            let r = tokio::task::spawn_local(main2(path.clone(), control_user.clone())).await;
            if let Err(e) = r {
                log::error!("{e}");
                log::debug!("{e:?}");

                tokio::time::sleep(Duration::from_secs(5)).await;

                continue;
            }

            break;
        }
    });
    local_set.await;
}

async fn main2(path: String, control_user: Option<String>) {
    println!("main2");
    let mut server = Server::new(&path).await.unwrap();
    if let Some(control) = control_user {
        server.add_control(control.parse().unwrap()).await.unwrap();
    }
    println!(
        "Control invite: {}:{}",
        server.control.uuid,
        server.database.cert().hex()
    );
    log::info!("teste");

    loop {
        server.sync_channels().await.unwrap();
        for channel in server.channels.iter() {
            println!(
                "{state:?} {key}",
                state = channel.state(),
                key = channel.channel().peer_cert.hex()
            );
        }

        for message in server.control_messages().await.unwrap() {
            let text = message.text();
            let response = server.handle_message(text).await;

            match response {
                Ok(response) => {
                    server.send_control_message(response).await.unwrap();
                }
                Err(e) => {
                    server
                        .send_control_message(format!("{e}\nOn message {text}\n{e:?}"))
                        .await
                        .unwrap();
                }
            }
            server.set_message_handled(&message).await.unwrap();
        }

        let value = server.wait().await.unwrap();
        server.then(value).await.unwrap();
    }
}

struct Server {
    database: Database,
    channels: Vec<SqliteChannel>,
    control: Conversation,
}
impl Server {
    async fn new(path: &str) -> DatabaseResult<Server> {
        let database = Database::connect(path).await?;
        let control = database
            .join_conversation(Self::control_id(database.cert()))
            .await?;

        Ok(Server {
            database,
            channels: Default::default(),
            control,
        })
    }

    fn control_id(cert: &Ed25519Cert) -> Uuid {
        let mut r = [0; 16];
        r.iter_mut().enumerate().for_each(|(i, r)| {
            *r = cert.0[i] ^ cert.0[i * 2];
        });
        Uuid::from_bytes(r)
    }

    async fn sync_channels(&mut self) -> DatabaseResult<()> {
        #![allow(clippy::mutable_key_type)]
        let mut channels = HashSet::new();
        for conversation in self.database.list_conversation().await? {
            for data in self.database.list_channels(&conversation).await? {
                channels.insert(data);
            }
        }

        for i in (0..self.channels.len()).rev() {
            if channels.take(self.channels[i].channel()).is_none() {
                log::info!("Removing {:?}", self.channels[i].channel());
                self.channels.remove(i);
            }
        }

        for channel in channels {
            log::info!("Adding {channel:?}");
            let sync = SqliteChannel::new(channel, self.database.private_key().clone());
            self.channels.push(sync);
        }

        Ok(())
    }

    async fn add_control(&mut self, peer: Ed25519Cert) -> DatabaseResult<()> {
        self.database
            .create_channel(self.control.clone(), peer)
            .await?;

        Ok(())
    }

    async fn control_messages(&mut self) -> DatabaseResult<Vec<Message>> {
        self.database.new_messages(Some(&self.control)).await
    }

    async fn handle_message(&mut self, message: &str) -> CommandResult<String> {
        let mut args = message.trim().split(' ').collect::<Vec<_>>();
        args.insert(0, "");
        let args = CommandArgs::try_parse_from(args)?;

        args.subcommand.run(&mut self.database).await
    }

    async fn send_control_message(&mut self, text: String) -> DatabaseResult<()> {
        log::info!("Response {text:?}");
        self.database.send_message(self.control.clone(), text).await
    }

    async fn set_message_handled(&mut self, message: &Message) -> DatabaseResult<()> {
        self.database
            .set_message_status(message, icechat::database::MessageStatus::Delivered)
            .await
    }

    async fn wait(&mut self) -> DatabaseResult<(ChannelValue, usize)> {
        if self.channels.is_empty() {
            log::error!(
                "List of channels is empty. Server will not do anything. Add a control user"
            );
            return pending().await;
        }

        let mut trans = self.database.begin().await?;

        for channel in self.channels.iter_mut() {
            if channel.state() == ChannelStateLabel::Offline {
                channel.connect(self.database.start_sync(channel.channel().clone()));
            }
            channel.pre_wait(&mut trans).await;
        }

        let waits = self
            .channels
            .iter_mut()
            .map(|channel| channel.wait().boxed_local())
            .collect::<Vec<_>>();

        let (value, index, _) = select_all(waits).await;

        trans.commit().await?;
        Ok((value, index))
    }

    async fn then(&mut self, (value, index): (ChannelValue, usize)) -> DatabaseResult<()> {
        self.channels[index].then(value).await;

        Ok(())
    }
}

#[derive(clap::Parser, Debug)]
pub struct CommandArgs {
    #[command(subcommand)]
    subcommand: Command,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    Echo { echos: Vec<String> },
    SetName { name: String },
    Cert,
    Join { invite: String },
    CreateConversation { title: Option<String> },
    List,
    AddMember { conversation: String, cert: String },
}
impl Command {
    async fn run(self, database: &mut Database) -> CommandResult<String> {
        let _ = database;

        match self {
            Command::Echo { echos } => Ok(format!("{echos:?}")),
            Command::SetName { name } => {
                let mut user = database.get_contact(database.cert()).await?.unwrap();
                user.name = name.clone();
                database.save_contact(user).await?;

                Ok(format!("Name changed to {name:?}"))
            }
            Command::Cert => Ok(database.cert().hex()),
            Command::Join { invite } => {
                let (conversation, cert) = invite.split_once(':').ok_or(CommandError::BadInvite)?;
                let conversation = conversation.parse()?;
                let cert = cert.parse()?;

                let conversation = database.join_conversation(conversation).await?;
                database.create_channel(conversation.clone(), cert).await?;

                Ok(format!("Joined conversation {}", conversation.uuid))
            }
            Command::CreateConversation { title } => {
                let conversation = database.create_conversation(title).await?;

                Ok(format!("{}:{}", conversation.uuid, database.cert().hex()))
            }
            Command::List => {
                let mut r = vec![];
                for conversation in database.list_conversation().await? {
                    r.push(format!(
                        "Conversation {:?} {}:{}",
                        conversation.title,
                        conversation.uuid,
                        database.cert().hex()
                    ));
                    for member in conversation.members.iter() {
                        r.push(format!("  Member {} ({})", member.name, member.key.hex()));
                    }
                    for channel in database.list_channels(&conversation).await? {
                        r.push(format!("  Channel {}", channel.peer_cert.hex()));
                    }
                }

                Ok(r.join("\n"))
            }
            Command::AddMember { conversation, cert } => {
                let conversation = conversation.parse()?;
                let cert = cert.parse()?;

                let conversation = database
                    .get_conversation(conversation)
                    .await?
                    .ok_or(CommandError::InexistentConversation(conversation))?;

                database.create_channel(conversation.clone(), cert).await?;

                Ok(format!(
                    "{cert} added to {conversation}",
                    cert = cert.hex(),
                    conversation = conversation.uuid
                ))
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CommandError {
    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
    #[error(transparent)]
    Clap(#[from] clap::Error),
    #[error("Unknown command {0:?}")]
    UnknownCommand(String),
    #[error("Provided empty command")]
    EmptyCommand,
    #[error("Bad invite to join conversation, missing : separator")]
    BadInvite,
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    #[error(transparent)]
    Ed25519Cert(#[from] BadEd25519CertStr),
    #[error("Conversation {0} does not exist")]
    InexistentConversation(Uuid),
}
pub type CommandResult<T> = Result<T, CommandError>;
