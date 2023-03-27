use eframe::egui;
use egui_dock::Tree;
use icechat::{
    channel::Ed25519Cert,
    chat::Chat,
    database::{Contact, Content, Conversation},
    notification::NotificationManager,
    poll_runtime::PollRuntime,
};
use rfd::FileDialog;
use std::{borrow::Cow, cell::RefCell, time::Duration};

fn main() {
    env_logger::init();

    let path = std::env::args().nth(1).unwrap_or_else(|| {
        let path = FileDialog::new()
            .add_filter("Icechat Database", &[".sqlite3"])
            .save_file();

        let Some(path) = path else { std::process::exit(0); };
        path.to_string_lossy().into_owned()
    });

    let chat = Chat::load(path);

    eframe::run_native(
        "Icechat",
        Default::default(),
        Box::new(move |_| Box::new(App::new(chat))),
    )
}

struct App {
    chat: Chat,
    conversations: Tree<RefCell<ConversationTab>>,
    runtime: PollRuntime,
    join: String,
}
impl App {
    pub fn new(chat: Chat) -> App {
        let user = chat.profile();
        let mut conversations = Tree::<RefCell<ConversationTab>>::default();
        for conversation in chat.list_conversation() {
            conversations
                .push_to_first_leaf(RefCell::new(ConversationTab::new(conversation, &user)));
        }

        App {
            chat,
            conversations,
            runtime: Default::default(),
            join: Default::default(),
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        #![allow(clippy::await_holding_refcell_ref)]
        ctx.request_repaint_after(Duration::from_millis(5));

        let runtime = self.chat.runtime().handle().clone();
        let changed = self.runtime.poll(runtime, async {
            self.chat.pre_wait().await;
            let wait = self.chat.wait();
            let Ok(value) = tokio::time::timeout(Duration::from_millis(1), wait).await else {
                return false;
            };

            self.chat.then(value).await;
            true
        });

        if changed == Some(true) {
            for message in self.chat.new_messages() {
                NotificationManager::show(message)
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Conversation").clicked() {
                        let new_conversation = self.chat.create_conversation();

                        self.conversations
                            .push_to_first_leaf(RefCell::new(ConversationTab::new(
                                new_conversation,
                                &self.chat.profile(),
                            )));
                    }

                    if ui.button("Quit").clicked() {
                        frame.close();
                    }
                });
                if ui.button("Join:").clicked() {
                    let join = std::mem::take(&mut self.join);
                    let join = join.split_once(':');
                    let join = join.and_then(|(conversation, peer)| {
                        let conversation = conversation
                            .parse()
                            .map_err(|e| {
                                log::error!("Bad uuid {conversation:?}, {e}");
                                log::debug!("{e:?}");
                                e
                            })
                            .ok()?;

                        let peer = (2..=peer.len())
                            .step_by(2)
                            .map(|idx| {
                                u8::from_str_radix(&peer[(idx - 2)..][..2], 16).unwrap_or_default()
                            })
                            .collect::<Vec<u8>>();
                        let peer = peer
                            .as_slice()
                            .try_into()
                            .map_err(|e| {
                                log::error!("Bad key {peer:?}, {e}");
                                log::debug!("{e:?}");
                                e
                            })
                            .ok()?;
                        let peer = Ed25519Cert(peer);

                        Some((conversation, peer))
                    });

                    if let Some((conversation, peer)) = join {
                        let conversation = self.chat.join_conversation(conversation, peer);
                        self.conversations
                            .push_to_first_leaf(RefCell::new(ConversationTab::new(
                                conversation,
                                &self.chat.profile(),
                            )));
                    }
                }
                ui.text_edit_singleline(&mut self.join);
                let key = self.chat.profile().key.hex();

                if ui.button("📋").clicked() {
                    ui.output().copied_text = key.clone();
                }
                ui.label(format!("Pubkey: {key}"));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let style = egui_dock::Style::from_egui(ui.style().as_ref());
            egui_dock::DockArea::new(&mut self.conversations)
                .style(style)
                .show_inside(ui, &mut TabViewer(&mut self.chat));
        });
    }
}

struct TabViewer<'a>(&'a mut Chat);
impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = RefCell<ConversationTab>;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        tab.borrow_mut().ui(ui, self.0);
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.borrow_mut().title().into()
    }
}

struct ConversationTab {
    conversation: Conversation,
    user: Ed25519Cert,
    new_name: String,
    new_title: String,
    new_channel: String,
    message: String,
    max: usize,
}
impl ConversationTab {
    pub fn new(conversation: Conversation, user: &Contact) -> ConversationTab {
        let new_title = conversation.title.clone().unwrap_or_default();

        ConversationTab {
            conversation,
            user: user.key,
            new_name: user.name.to_string(),
            new_title,
            new_channel: Default::default(),
            message: Default::default(),
            max: 10,
        }
    }

    fn title(&self) -> Cow<str> {
        self.conversation
            .title
            .as_deref()
            .map(Cow::Borrowed)
            .unwrap_or_else(|| {
                let other = self
                    .conversation
                    .members
                    .iter()
                    .filter(|member| member.key != self.user)
                    .fold(None, |list, member| match list {
                        Some(list) => Some(format!("{list}, {member}", member = member.name)),
                        None => Some(member.name.clone()),
                    });

                other
                    .map(Cow::Owned)
                    .unwrap_or_else(|| Cow::Borrowed("<empty>"))
            })
    }

    fn ui(&mut self, ui: &mut egui::Ui, chat: &mut Chat) {
        let runtime = chat.runtime().handle().clone();
        chat.refresh_conversation(&mut self.conversation);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let text_edit = ui.text_edit_multiline(&mut self.message);

                if ui.button("Send").clicked() && !self.message.is_empty() {
                    self.send_message(chat);
                }

                if ui.button("Send file").clicked() {
                    self.send_file(chat);
                }

                if ui
                    .input_mut()
                    .consume_key(egui::Modifiers::default(), egui::Key::Enter)
                {
                    self.send_message(chat);
                    text_edit.request_focus();
                }
            });
            egui::containers::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical(|ui| {
                    let length = runtime
                        .block_on(self.conversation.length(chat.database()))
                        .unwrap();
                    let mut n = 0;
                    for index in (0..length).rev() {
                        let message = runtime
                            .block_on(self.conversation.get_message(chat.database(), index))
                            .unwrap();
                        let Some(message) = message else { break; };
                        n += 1;

                        ui.label(format!(
                            "({state:?}) {name}",
                            state = message.status,
                            name = message.from.name
                        ));
                        ui.horizontal(|ui| match message.content {
                            Content::Text(text) => {
                                if ui.button("⬅").clicked() {
                                    self.message = format!(
                                        "{sender} said:\n{message}\n\n",
                                        sender = message.from.name,
                                        message = text,
                                    );
                                }
                                if ui.button("📋").clicked() {
                                    ui.output().copied_text = text.to_string();
                                }

                                ui.label(text);
                            }
                            Content::Attachment(name, id) => {
                                if ui.button("💾").clicked() {
                                    Self::save_file(chat, &name, id);
                                }

                                ui.label(name);
                            }
                        });
                        ui.separator();
                    }

                    if n == self.max && ui.button("Load more").clicked() {
                        self.max += 10;
                    }
                });
            });
        });

        egui::SidePanel::right("channels")
            .default_width(256.0)
            .show_inside(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.heading("Channels");
                    egui::containers::ScrollArea::horizontal().show(ui, |ui| {
                        let channels = chat
                            .channels()
                            .filter(|(channel, _)| channel.conversation == self.conversation.uuid);
                        let mut remove = None;

                        for (channel, state) in channels {
                            ui.horizontal(|ui| {
                                if ui.button("X").clicked() {
                                    remove = Some(channel.peer_cert);
                                }
                                let fp = channel.peer_cert.hex();
                                ui.label(format!("({state:?}) {fp}"));
                            });
                        }

                        if let Some(remove) = remove {
                            chat.remove_channel(self.conversation.clone(), remove);
                        }

                        ui.horizontal(|ui| {
                            let id = self.conversation.uuid;
                            let key = chat.profile().key.hex();
                            let invite = format!("{id}:{key}");

                            if ui.button("📋").clicked() {
                                ui.output().copied_text = invite.clone();
                            }

                            ui.label(format!("Invite: {invite}"));
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Peer pub key:");
                        ui.text_edit_singleline(&mut self.new_channel);
                        if ui.button("Add").clicked() {
                            let peer = std::mem::take(&mut self.new_channel);
                            let mut peer = (0..peer.len())
                                .step_by(2)
                                .map(|idx| {
                                    u8::from_str_radix(&peer[idx..][..2], 16).unwrap_or_default()
                                })
                                .collect::<Vec<u8>>();
                            peer.resize(32, 0);
                            let peer = Ed25519Cert(peer.try_into().unwrap());

                            chat.add_channel(self.conversation.clone(), peer);
                        }
                    });

                    ui.heading("Members");
                    for member in self.conversation.members.iter() {
                        let name = member.name.as_str();
                        let fp = member.key.hex();
                        ui.label(format!("{name} ({fp})"));
                    }

                    ui.heading("Profile");
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.new_name);
                        if ui.button("Save").clicked() {
                            let mut profile = chat.profile();
                            profile.name = self.new_name.to_string();
                            chat.set_profile(profile);
                        }
                    });

                    ui.heading("Conversation");
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.new_title);
                        if ui.button("Save").clicked() {
                            self.conversation.title = match self.new_title.is_empty() {
                                true => None,
                                false => Some(self.new_title.clone()),
                            };
                            chat.save_conversation(self.conversation.clone());
                        }
                    })
                });
            });
    }

    fn send_message(&mut self, chat: &mut Chat) {
        let content = std::mem::take(&mut self.message);
        chat.send_message(self.conversation.clone(), content);
    }

    fn send_file(&mut self, chat: &mut Chat) {
        let path = FileDialog::new().set_title("Send file").pick_file();

        let Some(path) = path else { return; };

        let name = path
            .file_name()
            .map(|x| x.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unnamed file".to_string());
        let blob = std::fs::read(path).unwrap();

        chat.send_file(self.conversation.clone(), name, blob);
    }

    fn save_file(chat: &Chat, name: &str, id: i32) {
        let blob = chat.fetch_file_payload(id);
        let path = FileDialog::new().set_file_name(name).save_file();

        let Some(blob) = blob else { return; };
        let Some(path) = path else { return; };

        std::fs::write(path, blob).unwrap();
    }
}
