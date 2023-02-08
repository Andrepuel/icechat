use eframe::egui;
use egui_dock::Tree;
use futures_util::{future::select_all, FutureExt};
use icechat::{
    chat::{Chat, ChatValue},
    database::Contact,
    notification::NotificationManager,
    poll_runtime::PollRuntime,
};
use rfd::FileDialog;
use std::{cell::RefCell, path::Path, time::Duration};
use uuid::Uuid;

fn main() {
    env_logger::init();

    eframe::run_native(
        "Icechat",
        Default::default(),
        Box::new(|_| Box::<App>::default()),
    )
}

#[derive(Default)]
struct App {
    conversations: Tree<RefCell<ConversationTab>>,
    runtime: PollRuntime,
}
impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        #![allow(clippy::await_holding_refcell_ref)]
        ctx.request_repaint_after(Duration::from_millis(5));

        self.runtime.poll(async {
            let mut tabs = self
                .conversations
                .tabs()
                .map(|tab| tab.borrow_mut())
                .collect::<Vec<_>>();

            if tabs.is_empty() {
                return;
            }

            let wait = select_all(tabs.iter_mut().map(|tab| tab.wait().boxed_local()));
            let Ok((value, index, _)) = tokio::time::timeout(Duration::from_millis(1), wait).await else {
                return;
            };

            tabs[index].then(value).await;
        });

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        let path = FileDialog::new()
                            .add_filter("Icechat File", &["icechat"])
                            .save_file();

                        if let Some(path) = path {
                            Chat::init(Uuid::new_v4(), &path);

                            self.conversations
                                .push_to_first_leaf(RefCell::new(ConversationTab::load(path)));
                        }
                    }

                    if ui.button("Open").clicked() {
                        let path = FileDialog::new()
                            .add_filter("Icechat File", &["icechat"])
                            .pick_file();

                        if let Some(path) = path {
                            self.conversations
                                .push_to_first_leaf(RefCell::new(ConversationTab::load(path)));
                        }
                    }

                    if ui.button("Quit").clicked() {
                        frame.close();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let style = egui_dock::Style::from_egui(ui.style().as_ref());
            egui_dock::DockArea::new(&mut self.conversations)
                .style(style)
                .show_inside(ui, &mut TabViewer {});
        });
    }
}

struct TabViewer {}
impl egui_dock::TabViewer for TabViewer {
    type Tab = RefCell<ConversationTab>;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        tab.borrow_mut().ui(ui);
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.borrow_mut().title().into()
    }
}

struct ConversationTab {
    title: String,
    chat: Chat,
    name: String,
    new_channel: String,
    message: String,
}
impl ConversationTab {
    fn load<P: AsRef<Path>>(path: P) -> ConversationTab {
        let path = path.as_ref();
        let title = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let chat = Chat::load(path);
        let name = chat.profile().name;

        ConversationTab {
            title,
            chat,
            name,
            new_channel: Default::default(),
            message: Default::default(),
        }
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.message);
                if ui.button("Send").clicked() && !self.message.is_empty() {
                    let content = std::mem::take(&mut self.message);
                    self.chat.send_message(content);
                    self.chat.save();
                }
            });
            egui::containers::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical(|ui| {
                    for message in self.chat.list_messages().rev() {
                        let from = self.chat.get_peer(message.from).unwrap_or_default();
                        ui.label(format!(
                            "({state:?}) {name}",
                            state = message.status,
                            name = from.name
                        ));
                        ui.label(message.content);
                        ui.separator();
                    }
                });
            });
        });

        egui::SidePanel::right("channels").show_inside(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.heading("Channels");
                let mut remove = None;
                for (channel, state) in self.chat.channels() {
                    ui.horizontal(|ui| {
                        if ui.button("X").clicked() {
                            remove = Some(channel.to_string());
                        }
                        ui.label(format!("{channel}: {state:?}"));
                    });
                }

                if let Some(remove) = remove {
                    self.chat.remove_channel(&remove);
                    self.chat.save();
                }

                ui.horizontal(|ui| {
                    ui.label("New channel:");
                    ui.text_edit_singleline(&mut self.new_channel);
                    if ui.button("Add").clicked() {
                        self.chat.add_channel(std::mem::take(&mut self.new_channel));
                        self.chat.save();
                    }
                });
                ui.heading("Profile");
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.name);
                    if ui.button("Save").clicked() {
                        self.chat.set_profile(Contact {
                            name: self.name.to_string(),
                            ..self.chat.profile()
                        });
                        self.chat.save();
                    }
                });
            });
        });
    }

    async fn wait(&mut self) -> ChatValue {
        self.chat.wait().await
    }

    async fn then(&mut self, value: ChatValue) {
        self.chat.then(value).await;

        self.poll();
    }

    fn poll(&mut self) {
        for message in self.chat.new_messages() {
            let from = self.chat.get_peer(message.from).unwrap_or_default();
            NotificationManager::show(from, message)
        }
    }
}
