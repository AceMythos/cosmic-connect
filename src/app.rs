use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ashpd::desktop::file_chooser::SelectedFiles;
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
    ChooseFile(String),
    FileChooserFinished(String, Result<String, String>),
    ReadClipboard(String),
    ClipboardReadFinished(String, Result<String, String>),
}

impl CosmicConnect {
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

    fn popup_parent_id(&self) -> Option<Id> {
        self.core.main_window_id()
    }

    fn sync_drafts(&mut self) {
        self.drafts
            .retain(|device_id, _| self.devices.iter().any(|device| &device.id == device_id));

        for device in &self.devices {
            self.drafts.entry(device.id.clone()).or_default();
        }
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
                    let Some(parent_id) = self.popup_parent_id() else {
                        self.error = Some("Applet window not ready yet. Please try again.".into());
                        return Task::none();
                    };

                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        parent_id,
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
                return Task::perform(async {}, |_| Message::RefreshDevices)
                    .map(cosmic::Action::App);
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
            Message::ChooseFile(device_id) => {
                let device_id_for_task = device_id.clone();
                return Task::perform(
                    async move { pick_file_path().await },
                    move |result| Message::FileChooserFinished(device_id_for_task, result),
                )
                .map(cosmic::Action::App);
            }
            Message::FileChooserFinished(device_id, result) => {
                let draft = self.draft_mut(&device_id);
                match result {
                    Ok(path) => {
                        draft.selected_file = path.clone();
                        draft.status = Some(format!("Selected file: {path}"));
                    }
                    Err(error) => {
                        if error.to_lowercase().contains("cancel") {
                            draft.status = None;
                        } else {
                            draft.status = Some(error);
                        }
                    }
                }
            }
            Message::ReadClipboard(device_id) => {
                let device_id_for_task = device_id.clone();
                return Task::perform(
                    async move { read_clipboard_text().await },
                    move |result| Message::ClipboardReadFinished(device_id_for_task, result),
                )
                .map(cosmic::Action::App);
            }
            Message::ClipboardReadFinished(device_id, result) => {
                let draft = self.draft_mut(&device_id);
                match result {
                    Ok(text) => {
                        draft.clipboard_text = text.clone();
                        draft.status = Some("Clipboard contents loaded".into());
                    }
                    Err(error) => {
                        draft.status = Some(error);
                    }
                }
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

                return Task::perform(async {}, |_| Message::RefreshDevices)
                    .map(cosmic::Action::App);
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("smartphone-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        let status_text = if self.backend.is_some() {
            "Online"
        } else {
            "Searching for KDE Connect…"
        };

        let local_badge = container(
            row![
                icon::from_name("computer-symbolic").size(14),
                column![
                    text::caption("Computer"),
                    text::caption("Local device"),
                ]
                .spacing(2),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([6, 10]);

        let remote_badge = if let Some(first_device) = self.devices.first() {
            container(
                row![
                    icon::from_name(first_device.device_type.icon_name()).size(14),
                    column![
                        text::caption(&first_device.name),
                        text::caption(if first_device.is_reachable {
                            "Connected"
                        } else {
                            "Offline"
                        }),
                    ]
                    .spacing(2),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([6, 10])
        } else {
            container(
                row![
                    icon::from_name("phone-symbolic").size(14),
                    text::caption("No device paired"),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([6, 10])
        };

        let refresh_button = button::custom(text::caption("Refresh"))
            .on_press(Message::RefreshDevices)
            .padding([6, 10]);

        content.push(
            container(
                column![
                    row![local_badge, remote_badge, refresh_button]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    text::caption(status_text),
                ]
                .spacing(8),
            )
            .padding([10, 16])
            .into(),
        );

        content.push(divider::horizontal::default().into());

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
            let paired_devices: Vec<&Device> = self.devices.iter().filter(|device| device.is_paired).collect();
            let available_devices: Vec<&Device> = self.devices.iter().filter(|device| !device.is_paired).collect();

            if !paired_devices.is_empty() {
                content.push(
                    container(text::caption("Paired devices"))
                        .padding([10, 16, 4, 16])
                        .into(),
                );
                for (index, device) in paired_devices.iter().enumerate() {
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

            if !available_devices.is_empty() {
                if !paired_devices.is_empty() {
                    content.push(divider::horizontal::default().into());
                }
                content.push(
                    container(text::caption("Available devices"))
                        .padding([10, 16, 4, 16])
                        .into(),
                );
                for (index, device) in available_devices.iter().enumerate() {
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
        }

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

async fn read_clipboard_text() -> Result<String, String> {
    let output = tokio::process::Command::new("wl-paste")
        .arg("-n")
        .output()
        .await
        .map_err(|e| format!("clipboard unavailable: {e}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

async fn pick_file_path() -> Result<String, String> {
    let response = SelectedFiles::open_file()
        .title("Choose a file to send")
        .accept_label("Send")
        .modal(true)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;

    response
        .uris()
        .first()
        .and_then(|uri| uri.to_file_path().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or_else(|| "No file selected".to_string())
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

    let connection_badge = if device.is_reachable {
        text::caption("● Connected")
    } else if device.is_paired {
        text::caption("● Offline")
    } else {
        text::caption("● Not paired")
    };

    let header = row![
        icon::from_name(icon_name).size(24),
        column![
            text::body(&device.name),
            row![connection_badge, text::caption(device.device_type.label())]
                .spacing(8)
                .align_y(Alignment::Center),
        ]
        .spacing(2),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut rows = vec![header.into()];

    rows.push(
        container(info)
            .padding([2, 0, 0, 0])
            .width(Length::Fill)
            .into(),
    );

    fn action_button<'a>(icon_name: &'a str, label: &'a str, message: Message) -> Element<'a, Message> {
        button::custom(
            row![icon::from_name(icon_name).size(14), text::caption(label)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .on_press(message)
        .padding([6, 10])
        .into()
    }

    let mut quick_actions: Vec<Element<Message>> = Vec::new();

    if device.is_reachable && device.has_plugin("kdeconnect_ping") {
        quick_actions.push(action_button(
            "network-transmit-receive-symbolic",
            "Ping",
            Message::PerformAction(device.id.clone(), ActionType::Ping),
        ));
    }

    if device.is_reachable && device.has_plugin("kdeconnect_findmyphone") {
        quick_actions.push(action_button(
            "bell-symbolic",
            "Ring",
            Message::PerformAction(device.id.clone(), ActionType::Ring),
        ));
    }

    if device.is_reachable && device.has_plugin("kdeconnect_sftp") {
        quick_actions.push(action_button(
            "folder-symbolic",
            "Files",
            Message::PerformAction(device.id.clone(), ActionType::BrowseFiles),
        ));
    }

    match device.pair_state {
        0 => {
            quick_actions.push(action_button(
                "emblem-new-symbolic",
                "Pair",
                Message::PerformAction(device.id.clone(), ActionType::Pair),
            ));
        }
        1 => {
            quick_actions.push(action_button(
                "dialog-cancel-symbolic",
                "Cancel",
                Message::PerformAction(device.id.clone(), ActionType::CancelPairing),
            ));
        }
        2 => {
            quick_actions.push(action_button(
                "dialog-ok-symbolic",
                "Accept",
                Message::PerformAction(device.id.clone(), ActionType::AcceptPairing),
            ));
            quick_actions.push(action_button(
                "dialog-cancel-symbolic",
                "Cancel",
                Message::PerformAction(device.id.clone(), ActionType::CancelPairing),
            ));
        }
        3 => {
            quick_actions.push(action_button(
                "user-trash-symbolic",
                "Unpair",
                Message::PerformAction(device.id.clone(), ActionType::Unpair),
            ));
        }
        _ => {}
    }

    if !quick_actions.is_empty() {
        rows.push(
            container(row::with_children(quick_actions).spacing(8))
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
                    button::custom(text::caption("Use Clipboard"))
                        .on_press(Message::ReadClipboard(device.id.clone()))
                        .padding([2, 8]),
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
                    button::custom(text::caption("Choose"))
                        .on_press(Message::ChooseFile(device.id.clone()))
                        .padding([2, 8]),
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
        let icon_name = if status == "Working..." {
            "process-working-symbolic"
        } else if status.contains("error") || status.contains("failed") {
            "dialog-error-symbolic"
        } else {
            "emblem-default-symbolic"
        };

        let status_row = row![
            icon::from_name(icon_name).size(12),
            text::caption(status),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        rows.push(
            container(status_row)
                .padding([6, 8])
                .width(Length::Fill)
                .into(),
        );
    }

    container(column::with_children(rows).spacing(4))
        .padding([8, 16])
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::CosmicConnect;

    #[test]
    fn popup_parent_id_is_none_before_window_is_ready() {
        let app = CosmicConnect::default();
        assert!(app.popup_parent_id().is_none());
    }
}
