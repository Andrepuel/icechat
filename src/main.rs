use clap::Parser;
use icechat::{
    chat::Chat,
    database::{Contact, MessageStatus},
};
use uuid::Uuid;

#[derive(clap::Parser, Debug)]
struct Args {
    /// Path to the database folder
    path: String,

    /// Synchronization channel
    channel: String,

    #[command(subcommand)]
    command: Subcommand,
}

#[derive(clap::Subcommand, Debug)]
enum Subcommand {
    /// Initializes a new database folder
    Init,
    SetProfile {
        name: String,
    },
    /// Views conversation
    View,
    Wait,
    SendMessage {
        content: Vec<String>,
    },
}

fn main() {
    let args = Args::parse();

    let mut chat = match args.command {
        Subcommand::Init => Chat::init(Uuid::new_v4(), &args.path),
        _ => Chat::load(&args.path),
    };

    let changed = match &args.command {
        Subcommand::Init => false,
        Subcommand::SetProfile { name } => {
            let contact = chat.profile();
            let contact = Contact {
                name: name.to_string(),
                ..contact
            };
            chat.set_profile(contact);
            true
        }
        Subcommand::View => view(&mut chat),
        Subcommand::SendMessage { content } => {
            chat.send_message(content.join(" "));
            true
        }
        Subcommand::Wait => false,
    };

    if changed {
        chat.save();
    }

    println!("Starting sync");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        chat.synchronize_with(&args.channel).await;
        chat.save();
    });
    println!("Done syncronization");

    if let Subcommand::Wait = &args.command {
        if view(&mut chat) {
            chat.save();
        }
    }
}

fn view(chat: &mut Chat) -> bool {
    println!("{:#?}", chat.profile());
    for peer in chat.list_peers() {
        println!("{:#?}", peer);
    }
    let mut delivered = vec![];

    for (index, message) in chat.list_messages().enumerate() {
        if message.status == MessageStatus::Sent && message.from != chat.user() {
            delivered.push(index);
        }

        let from = chat.get_peer(message.from).unwrap_or_default();
        println!("{name}:", name = from.name);
        println!("{content}", content = message.content);
        println!("{status:?}", status = message.status);
        println!()
    }

    if delivered.is_empty() {
        return false;
    }

    for index in delivered {
        chat.set_message_status(index, MessageStatus::Delivered);
    }

    true
}
