use crate::database::Message;
use notify_rust::Notification;

pub struct NotificationManager;
impl NotificationManager {
    pub fn show(message: Message) {
        let text = message.text();
        let text = match text.len() {
            0..=128 => text.to_string(),
            _ => format!("{}...", text.chars().take(128).collect::<String>()),
        };

        let r = Notification::new()
            .summary(&format!("Message from {from}", from = message.from.name))
            .body(&text)
            .icon("icechat")
            .appname("icechat")
            .show();

        if let Err(e) = r {
            log::error!("{e}");
            log::debug!("{e:?}");
        }
    }
}
