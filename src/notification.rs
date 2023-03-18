use crate::database::{Contact, Message};
use notify_rust::Notification;

pub struct NotificationManager;
impl NotificationManager {
    pub fn show(from: Contact, message: Message) {
        let text = message.text();
        let message = match text.len() {
            0..=128 => text.to_string(),
            _ => format!("{}...", text.chars().take(128).collect::<String>()),
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
