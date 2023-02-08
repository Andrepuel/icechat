use crate::database::{Contact, Message};
use notify_rust::Notification;

pub struct NotificationManager;
impl NotificationManager {
    pub fn show(from: Contact, message: Message) {
        let message = match message.content.len() {
            0..=128 => message.content,
            _ => format!(
                "{}...",
                message.content.chars().take(128).collect::<String>()
            ),
        };

        let r = Notification::new()
            .summary(&format!("Message from {from}", from = from.name))
            .body(&message)
            .icon("icechat")
            .appname("icechat")
            .show();

        if let Err(e) = r {
            log::error!("{e}");
            log::debug!("{e:?}");
        }
    }
}
