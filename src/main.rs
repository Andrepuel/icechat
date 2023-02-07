use clap::Parser;
use icechat::{
    chat::Chat,
    database::{Contact, Message, MessageStatus},
};
use tokio::io::AsyncBufReadExt;
use uuid::Uuid;

#[derive(clap::Parser, Debug)]
struct Args {
    /// Path to the database folder
    path: String,

    #[command(subcommand)]
    command: Subcommand,
}

#[derive(clap::Subcommand, Debug)]
enum Subcommand {
    /// Initializes a new database folder
    Init,
    /// Connects to chat and then set profile
    SetProfile { name: String },
    /// Connects to chat
    Chat {
        /// Synchronization channels
        channels: Vec<String>,
    },
}

fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}

async fn async_main() {
    env_logger::init();

    let args = Args::parse();

    let channels = match &args.command {
        Subcommand::Init => {
            Chat::init(Uuid::new_v4(), &args.path);
            return;
        }
        Subcommand::SetProfile { .. } => {
            static EMPTY: Vec<String> = vec![];
            &EMPTY
        }
        Subcommand::Chat { channels } => channels,
    };

    let channels = channels.iter().cloned();

    let mut chat = Chat::load(&args.path, channels);

    match &args.command {
        Subcommand::Init => {
            unreachable!()
        }
        Subcommand::SetProfile { name } => {
            let contact = chat.profile();
            let contact = Contact {
                name: name.to_string(),
                ..contact
            };
            chat.set_profile(contact);
        }
        Subcommand::Chat { .. } => {}
    };

    let mut read_messages = view(&mut chat, 0);
    view_users(&chat);

    let stdin = tokio::io::stdin();
    let mut stdin = tokio::io::BufReader::new(stdin);

    loop {
        let mut buf = Default::default();
        tokio::select! {
            r = stdin.read_line(&mut buf) => {
                r.unwrap();
                if buf.is_empty() {
                    break;
                }
                chat.send_message(buf);
            }
            value = chat.wait() => {
                let changed_channel = chat.then(value).await;

                if let Some((new_state, index)) = changed_channel {
                    println!("Channel {index}: {new_state:?}");
                }
            }
        }
        read_messages = view(&mut chat, read_messages);
    }
    println!("Closing...");
    chat.close().await;
}

fn view(chat: &mut Chat, index: usize) -> usize {
    let mut delivered = vec![];

    let mut last = None;
    for (index, message) in chat.list_messages().enumerate().skip(index) {
        if message.status == MessageStatus::Sent && message.from != chat.user() {
            delivered.push(index);
        }

        print_message(chat, &message);
        last = Some(index);
    }

    for index in delivered {
        chat.set_message_status(index, MessageStatus::Delivered);
    }

    last.map(|last| last + 1).unwrap_or(index)
}

fn view_users(chat: &Chat) {
    for peer in chat.list_peers() {
        println!("{:#?}", peer);
    }
    println!("{:#?}", chat.profile());
}

fn print_message(chat: &Chat, message: &Message) {
    let from = chat.get_peer(message.from).unwrap_or_default();
    println!("{name}:", name = from.name);
    println!("{content}", content = message.content);
    println!("{status:?}", status = message.status);
    println!()
}
