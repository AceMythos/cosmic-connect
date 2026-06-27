use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cosmic::app::Core;
use cosmic::iced::core::Alignment;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Limits, Subscription};
use cosmic::widget::text_input;
use cosmic::widget::{
    button, column, container, divider, icon, progress_bar, row, scrollable, text,
};
use cosmic::{Action, Element, Task};
use futures_util::stream::unfold;

use crate::backend::KdeConnectBackend;
use crate::model::{ActionType, Device};

const ID: &str = "io.github.acemythos.Connect";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
struct DeviceDraft {
    clipboard_text: String,
    share_text: String,
    share_url: String,
    selected_file: String,
    status: Option<String>,
}

#[derive(Default)]
pub struct CosmicConnect {
    core: Core,
    popup: Option<Id>,
    devices: Vec<Device>,
    drafts: HashMap<String, DeviceDraft>,
    error: Option<String>,
    backend: Option<Arc<KdeConnectBackend>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    RefreshDevices,
    PopupClosed(Id),
    BackendReady(Arc<KdeConnectBackend>),
    DevicesUpdated(Vec<Device>),
    BackendError(String),
    PerformAction(String, ActionType),
    ActionFinished(String, Result<String, String>),
    ClipboardTextChanged(String, String),
    ShareTextChanged(String, String),
    ShareUrlChanged(String, String),
    FilePathChanged(String, String),
}

impl CosmicConnect {
    fn connected_count(&self) -> usize {
        self.devices.iter().filter(|d| d.is_reachable).count()
    }

    fn pair_state_label(pair_state: i32, is_paired: bool) -> &'static str {
        match pair_state {
            1 => "Pairing requested",
            2 => "Waiting for confirmation",
            3 => "Paired",
            _ if is_paired => "Paired",
            _ => "Not paired",
        }
    }

    fn draft_mut(&mut self, device_id: &str) -> &mut DeviceDraft {
        self.drafts.entry(device_id.to_string()).or_default()
    }

    fn sync_drafts(&mut self) {
        self.drafts
            .retain(|device_id, _| self.devices.iter().any(|device| &device.id == device_id));

        for device in &self.devices {
            self.drafts.entry(device.id.clone()).or_default();
        }
    }

    fn header_view(&self) -> Element<'_, Message> {
        let connected = self.connected_count();

        let icon_name = if connected > 0 {
            "phone-connected-symbolic"
        } else if self.devices.is_empty() {
            "phone-symbolic"
        } else {
            "phone-offline-symbolic"
        };

        let content: Element<'_, Message> = if connected > 0 {
            row![
                icon::from_name(icon_name).size(14),
                text::body(format!("{connected}")),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            icon::from_name(icon_name).size(18).into()
        };

        button::custom(content)
            .on_press_down(Message::TogglePopup)
            .padding([4, 8])
            .into()
    }
}

impl cosmic::Application for CosmicConnect {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        (
            Self {
                core,
                ..Default::default()
            },
            Task::perform(
                async {
                    KdeConnectBackend::new()
                        .await
                        .map(Arc::new)
                        .map_err(|e| format!("D-Bus: {e}"))
                },
                |result| match result {
                    Ok(backend) => Message::BackendReady(backend),
                    Err(error) => Message::BackendError(error),
                },
            )
            .map(cosmic::Action::App),
        )
    }

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(460.0)
                        .min_width(320.0)
                        .min_height(140.0)
                        .max_height(720.0);
                    get_popup(popup_settings)
                };
            }
            Message::RefreshDevices => {
                let Some(backend) = self.backend.clone() else {
                    self.error = Some("KDE Connect backend unavailable".into());
                    return Task::none();
                };

                return Task::perform(async move { backend.devices().await }, |devices| {
                    Message::DevicesUpdated(devices)
                })
                .map(cosmic::Action::App);
            }
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                }
            }
            Message::BackendReady(backend) => {
                self.backend = Some(backend);
                self.error = None;
            }
            Message::DevicesUpdated(devices) => {
                self.devices = devices;
                self.sync_drafts();
                self.error = None;
            }
            Message::BackendError(error) => {
                self.error = Some(error);
            }
            Message::ClipboardTextChanged(device_id, value) => {
                self.draft_mut(&device_id).clipboard_text = value;
            }
            Message::ShareTextChanged(device_id, value) => {
                self.draft_mut(&device_id).share_text = value;
            }
            Message::ShareUrlChanged(device_id, value) => {
                self.draft_mut(&device_id).share_url = value;
            }
            Message::FilePathChanged(device_id, value) => {
                self.draft_mut(&device_id).selected_file = value;
            }
            Message::PerformAction(device_id, action) => {
                let Some(backend) = self.backend.clone() else {
                    self.draft_mut(&device_id).status = Some("KDE Connect backend unavailable".into());
                    return Task::none();
                };

                self.draft_mut(&device_id).status = Some("Working...".into());

                let device_id_for_task = device_id.clone();
                let action_for_task = action.clone();

                return Task::perform(
                    async move {
                        backend
                            .perform_action(&device_id_for_task, &action_for_task)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| Message::ActionFinished(device_id.clone(), result),
                )
                .map(cosmic::Action::App);
            }
            Message::ActionFinished(device_id, result) => {
                let draft = self.draft_mut(&device_id);
                draft.status = Some(match result {
                    Ok(message) => message,
                    Err(error) => error,
                });
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core.applet.autosize_window(self.header_view()).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        content.push(
            container(
                row![
                    icon::from_name("phone-symbolic").size(20),
                    text::title4("COSMIC Connect"),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([12, 16])
            .into(),
        );

        content.push(divider::horizontal::default().into());

        let status_text = if self.backend.is_some() {
            "Backend connected"
        } else {
            "Connecting to KDE Connect"
        };

        content.push(
            container(
                row![
                    text::caption(status_text),
                    button::custom(text::caption("Refresh"))
                        .on_press(Message::RefreshDevices)
                        .padding([2, 8]),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([8, 16, 4, 16])
            .into(),
        );

        if let Some(err) = &self.error {
            content.push(
                container(
                    column![
                        icon::from_name("dialog-warning-symbolic").size(32),
                        text::caption(err),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(24)
                .width(Length::Fill)
                .into(),
            );
        } else if self.devices.is_empty() {
            content.push(
                container(
                    column![
                        icon::from_name("phone-symbolic").size(48),
                        text::caption("No devices found"),
                        text::caption(
                            "Make sure KDE Connect is installed\nand a device is paired."
                        ),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(24)
                .width(Length::Fill)
                .into(),
            );
        } else {
            for (index, device) in self.devices.iter().enumerate() {
                if index > 0 {
                    content.push(divider::horizontal::default().into());
                }

                let draft = self
                    .drafts
                    .get(&device.id)
                    .expect("draft state should exist for each device");
                content.push(device_row(device, draft).into());
            }
        }

        content.push(divider::horizontal::default().into());
        content.push(
            container(text::caption(format!("{} device(s)", self.devices.len())))
                .padding([6, 16])
                .into(),
        );

        self.core
            .applet
            .popup_container(scrollable(column::with_children(content)).height(Length::Shrink))
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(std::any::TypeId::of::<()>(), |_state| {
            unfold(PollState::new(), |mut state| async {
                tokio::time::sleep(POLL_INTERVAL).await;
                let msg = match state.poll().await {
                    Ok(devices) => Message::DevicesUpdated(devices),
                    Err(e) => Message::BackendError(e),
                };
                Some((msg, state))
            })
        })
    }
}

struct PollState {
    backend: Option<KdeConnectBackend>,
}

impl PollState {
    fn new() -> Self {
        Self { backend: None }
    }

    async fn poll(&mut self) -> Result<Vec<Device>, String> {
        if self.backend.is_none() {
            match KdeConnectBackend::new().await {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => return Err(format!("D-Bus: {error}")),
            }
        }

        let backend = self.backend.as_ref().unwrap();
        Ok(backend.devices().await)
    }
}

fn device_row<'a>(device: &'a Device, draft: &'a DeviceDraft) -> Element<'a, Message> {
    let icon_name = device.device_type.icon_name();
    let status_text = if device.is_reachable {
        "Connected"
    } else if device.is_paired {
        "Offline"
    } else {
        "Not paired"
    };
    let pair_text = CosmicConnect::pair_state_label(device.pair_state, device.is_paired);

    let mut info = row![
        icon::from_name(if device.is_reachable {
            "network-transmit-receive-symbolic"
        } else {
            "network-offline-symbolic"
        })
        .size(10),
        text::caption(status_text),
        text::caption(pair_text),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    if let Some(battery) = &device.battery {
        info = info.push(
            progress_bar::determinate_linear(battery.charge as f32 / 100.0)
                .width(Length::Fixed(60.0))
                .girth(6),
        );
        info = info.push(text::caption(format!(
            "{}%{}",
            battery.charge,
            if battery.is_charging { " charging" } else { "" }
        )));
    }

    let header = row![
        icon::from_name(icon_name).size(24),
        column![text::body(&device.name), info].spacing(2),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut rows = vec![header.into()];

    let mut quick_actions: Vec<Element<Message>> = Vec::new();

    if device.is_reachable && device.has_plugin("kdeconnect_ping") {
        quick_actions.push(
            button::custom(text::caption("Ping"))
                .on_press(Message::PerformAction(device.id.clone(), ActionType::Ping))
                .padding([2, 8])
                .into(),
        );
    }

    if device.is_reachable && device.has_plugin("kdeconnect_findmyphone") {
        quick_actions.push(
            button::custom(text::caption("Ring"))
                .on_press(Message::PerformAction(device.id.clone(), ActionType::Ring))
                .padding([2, 8])
                .into(),
        );
    }

    if device.is_reachable && device.has_plugin("kdeconnect_sftp") {
        quick_actions.push(
            button::custom(text::caption("Browse Files"))
                .on_press(Message::PerformAction(
                    device.id.clone(),
                    ActionType::BrowseFiles,
                ))
                .padding([2, 8])
                .into(),
        );
    }

    match device.pair_state {
        0 => {
            quick_actions.push(
                button::custom(text::caption("Pair"))
                    .on_press(Message::PerformAction(device.id.clone(), ActionType::Pair))
                    .padding([2, 8])
                    .into(),
            );
        }
        1 => {
            quick_actions.push(
                button::custom(text::caption("Cancel Pairing"))
                    .on_press(Message::PerformAction(
                        device.id.clone(),
                        ActionType::CancelPairing,
                    ))
                    .padding([2, 8])
                    .into(),
            );
        }
        2 => {
            quick_actions.push(
                button::custom(text::caption("Accept Pairing"))
                    .on_press(Message::PerformAction(
                        device.id.clone(),
                        ActionType::AcceptPairing,
                    ))
                    .padding([2, 8])
                    .into(),
            );
            quick_actions.push(
                button::custom(text::caption("Cancel Pairing"))
                    .on_press(Message::PerformAction(
                        device.id.clone(),
                        ActionType::CancelPairing,
                    ))
                    .padding([2, 8])
                    .into(),
            );
        }
        3 => {
            quick_actions.push(
                button::custom(text::caption("Unpair"))
                    .on_press(Message::PerformAction(device.id.clone(), ActionType::Unpair))
                    .padding([2, 8])
                    .into(),
            );
        }
        _ => {}
    }

    if !quick_actions.is_empty() {
        rows.push(
            row::with_children(quick_actions)
                .spacing(4)
                .padding([4, 0, 0, 0])
                .into(),
        );
    }

    if device.is_reachable && device.has_plugin("kdeconnect_clipboard") {
        rows.push(
            column![
                text::caption("Clipboard"),
                row![
                    text_input::text_input("Send text to device clipboard", &draft.clipboard_text)
                        .on_input({
                            let device_id = device.id.clone();
                            move |value| Message::ClipboardTextChanged(device_id.clone(), value)
                        })
                        .width(Length::Fill),
                    button::custom(text::caption("Push"))
                        .on_press(Message::PerformAction(
                            device.id.clone(),
                            ActionType::SendClipboardText(draft.clipboard_text.clone()),
                        ))
                        .padding([2, 8]),
                ]
                .spacing(6),
            ]
            .spacing(4)
            .padding([6, 0, 0, 0])
            .into(),
        );
    }

    if device.is_reachable && device.has_plugin("kdeconnect_share") {
        rows.push(
            column![
                text::caption("Share"),
                row![
                    text_input::text_input("Share URL", &draft.share_url)
                        .on_input({
                            let device_id = device.id.clone();
                            move |value| Message::ShareUrlChanged(device_id.clone(), value)
                        })
                        .width(Length::Fill),
                    button::custom(text::caption("Send URL"))
                        .on_press(Message::PerformAction(
                            device.id.clone(),
                            ActionType::ShareUrl(draft.share_url.clone()),
                        ))
                        .padding([2, 8]),
                ]
                .spacing(6),
                row![
                    text_input::text_input("Share text", &draft.share_text)
                        .on_input({
                            let device_id = device.id.clone();
                            move |value| Message::ShareTextChanged(device_id.clone(), value)
                        })
                        .width(Length::Fill),
                    button::custom(text::caption("Send Text"))
                        .on_press(Message::PerformAction(
                            device.id.clone(),
                            ActionType::ShareText(draft.share_text.clone()),
                        ))
                        .padding([2, 8]),
                ]
                .spacing(6),
                row![
                    text_input::text_input("Path to local file", &draft.selected_file)
                        .on_input({
                            let device_id = device.id.clone();
                            move |value| Message::FilePathChanged(device_id.clone(), value)
                        })
                        .width(Length::Fill),
                    button::custom(text::caption("Send File"))
                        .on_press_maybe((!draft.selected_file.trim().is_empty()).then(|| {
                                Message::PerformAction(
                                    device.id.clone(),
                                    ActionType::SendFile(draft.selected_file.clone()),
                                )
                            }))
                        .padding([2, 8]),
                ]
                .spacing(6),
            ]
            .spacing(6)
            .padding([8, 0, 0, 0])
            .into(),
        );
    }

    if let Some(status) = &draft.status {
        rows.push(
            text::caption(status)
                .width(Length::Fill)
                .into(),
        );
    }

    container(column::with_children(rows).spacing(4))
        .padding([8, 16])
        .width(Length::Fill)
        .into()
}
