use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use ashpd::desktop::file_chooser::SelectedFiles;
use cosmic::app::Core;
use cosmic::iced::core::Alignment;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Background, Border, Color, Length, Limits, Subscription};
use cosmic::widget::text_input;
use cosmic::widget::{
    button, column, container, divider, icon, progress_bar, row, scrollable, text,
};
use cosmic::widget::container as iced_container;
use cosmic::theme;
use cosmic::{Action, Element, Task};
use futures_util::stream::unfold;
use futures_util::StreamExt;
use zbus::{MatchRule, MessageStream};

use crate::backend::KdeConnectBackend;
use crate::model::{ActionType, ConnectivityInfo, ConversationMessage, Device, DeviceType, Notification, PlayerInfo, ReceivedFile};

const ID: &str = "io.github.acemythos.Connect";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_RECEIVED_HISTORY: usize = 50;

#[derive(Default)]
struct DeviceDraft {
    clipboard_text: String,
    share_text: String,
    share_url: String,
    status: Option<String>,
    conversations: Vec<ConversationMessage>,
    selected_conversation: Option<i64>,
    reply_text: String,
    sms_busy: bool,
    notifications: Vec<Notification>,
    selected_notification: Option<String>,
    notify_reply_text: String,
    player: Option<PlayerInfo>,
    connectivity: Option<ConnectivityInfo>,
    last_action: Option<ActionType>,
}

#[derive(Default)]
pub struct CosmicConnect {
    core: Core,
    popup: Option<Id>,
    devices: Vec<Device>,
    drafts: HashMap<String, DeviceDraft>,
    error: Option<String>,
    backend: Option<Arc<KdeConnectBackend>>,
    is_discovering: bool,
    auto_discovering: bool,
    next_auto_discovery: Option<std::time::Instant>,
    selected_device_id: Option<usize>,
    advanced_device: Option<String>,
    received_files: HashMap<String, Vec<ReceivedFile>>,
    active_notifs: HashMap<String, u32>,
    last_notif_pct: HashMap<String, i32>,
    pending_notif_requests: HashSet<String>,
    deferred_notifs: HashMap<String, (String, String, String)>,
    notified_pair_ids: HashSet<String>,
    pairing_notifs: HashMap<u32, String>,
    last_sync: Option<std::time::Instant>,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    RefreshDevices,
    PopupClosed(Id),
    BackendReady(Arc<KdeConnectBackend>),
    DevicesUpdated(Vec<Device>, Vec<String>),
    BackendError(String),
    PerformAction(String, ActionType),
    ActionFinished(String, Result<String, String>),
    ClipboardTextChanged(String, String),
    ShareTextChanged(String, String),
    ShareUrlChanged(String, String),
    ChooseFile(String),
    FileChooserFinished(String, Result<String, String>),
    ReadClipboard(String),
    ClipboardReadFinished(String, Result<String, String>),
    DiscoverDevices,
    RefreshConversations(String),
    ConversationsLoaded(String, Vec<ConversationMessage>),
    SelectConversation(String, i64),
    ReplyTextChanged(String, String),
    SendReply(String, i64),
    RefreshNotifications(String),
    NotificationsLoaded(String, Vec<Notification>),
    SelectNotification(String, String),
    NotifyReplyChanged(String, String),
    SendNotifyReply(String, String),
    DismissNotification(String, String),
    RefreshPlayer(String),
    PlayerInfoUpdated(String, Option<PlayerInfo>),
    MediaAction(String, String),
    SelectPlayer(String, String),
    SelectDevice(usize),
    ToggleAdvanced(String),
    ConnectivityUpdated(String, Option<ConnectivityInfo>),
    FileReceived(String, String),
    ClipboardReceived(String, String),
    PingReceived(String),
    TransferStarted(String, String, String, u64),
    TransferProgress(String, String, u64, u64, i32),
    TransferFinished(String, String, String),
    TransferFailed(String, String, i32, String),
    NotifCreated(String, u32),
    NotifAction(u32, String),
    ReceivedFilesViewed(String),
    NoOp,
}

fn signal_bars(strength: i32) -> &'static str {
    match strength.clamp(0, 4) {
        0 => "○",
        1 => "●○○○",
        2 => "●●○○",
        3 => "●●●○",
        _ => "●●●●",
    }
}

impl CosmicConnect {
    fn draft_mut(&mut self, device_id: &str) -> &mut DeviceDraft {
        self.drafts.entry(device_id.to_string()).or_default()
    }

    fn popup_parent_id(&self) -> Option<Id> {
        self.core.main_window_id()
    }

    fn panel_preview(&self) -> String {
        if self.backend.is_none() {
            return "Connecting…".into();
        }
        if let Some(device) = self.devices.iter().find(|d| d.is_reachable) {
            let net = self.drafts.get(&device.id).and_then(|d| d.connectivity.as_ref());
            if let Some(bat) = &device.battery {
                let charge_label = if bat.is_charging { format!("{}%+", bat.charge) } else { format!("{}%", bat.charge) };
                if let Some(conn) = net {
                    format!("{} - ({}) {}", device.name, charge_label, signal_bars(conn.signal_strength))
                } else {
                    format!("{} - ({})", device.name, charge_label)
                }
            } else if let Some(conn) = net {
                format!("{} - {} - Connected", device.name, signal_bars(conn.signal_strength))
            } else {
                format!("{} - Connected", device.name)
            }
        } else if let Some(device) = self.devices.iter().find(|d| d.is_paired) {
            format!("{} - Offline", device.name)
        } else if !self.devices.is_empty() {
            format!("{} - Not paired", self.devices[0].name)
        } else {
            "No devices".into()
        }
    }

    fn sync_drafts(&mut self) {
        self.drafts
            .retain(|device_id, _| self.devices.iter().any(|device| &device.id == device_id));

        for device in &self.devices {
            self.drafts.entry(device.id.clone()).or_default();
        }
    }

    fn selected_device(&self) -> Option<&Device> {
        self.selected_device_id.and_then(|i| self.devices.get(i))
    }

    fn render_device_selector(&self) -> Element<'_, Message> {
        let remote = self.selected_device();
        let remote_icon = remote.map(|d| d.device_type.icon_name()).unwrap_or("phone-symbolic");
        let remote_name = remote.map(|d| d.name.as_str()).unwrap_or("No device");
        let remote_sub = match remote {
            Some(d) if d.is_reachable => "Connected (Active)",
            Some(d) if d.is_paired => "Offline",
            Some(_) => "Not paired",
            None => "No devices",
        };

        let has_next = self.devices.len() > 1;
        let select_msg = has_next.then(|| Message::SelectDevice(
            self.selected_device_id.map(|i| (i + 1) % self.devices.len()).unwrap_or(0)
        ));

        crate::widgets::device_selector_card(
            "computer-symbolic",
            "Computer",
            "Local device",
            remote_icon,
            remote_name,
            remote_sub,
            true,
            select_msg,
        )
    }

    fn render_device_status_card<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        let draft = self.drafts.get(&device.id);
        let net = draft.and_then(|d| d.connectivity.as_ref());
        let battery = device.battery.as_ref().map(|b| (if b.is_charging { "charging" } else { "" }, b.charge));

        crate::widgets::status_card(
            device.name.as_str(),
            device.is_reachable,
            battery,
            net.map(|n| (n.network_type.as_str(), n.signal_strength)),
            if !device.is_reachable { None } else { Some(Message::ToggleAdvanced(device.id.clone())) },
        )
    }

    fn render_quick_action_row(&self, device: &Device) -> Option<Element<'_, Message>> {
        let mut buttons: Vec<Element<Message>> = Vec::new();

        if device.is_reachable && device.has_plugin("kdeconnect_sftp") {
            buttons.push(crate::widgets::quick_action_btn(
                "folder-symbolic",
                "Files",
                Message::PerformAction(device.id.clone(), ActionType::BrowseFiles),
                false,
            ));
        }

        if device.is_reachable && device.has_plugin("kdeconnect_ping") {
            buttons.push(crate::widgets::quick_action_btn(
                "emblem-important-symbolic",
                "Ping",
                Message::PerformAction(device.id.clone(), ActionType::Ping),
                false,
            ));
        }

        if device.is_reachable && device.has_plugin("kdeconnect_findmyphone") {
            buttons.push(crate::widgets::quick_action_btn(
                "bell-symbolic",
                "Ring",
                Message::PerformAction(device.id.clone(), ActionType::Ring),
                false,
            ));
        }

        if device.pair_state == 3 {
            buttons.push(crate::widgets::quick_action_btn(
                "user-trash-symbolic",
                "Unpair",
                Message::PerformAction(device.id.clone(), ActionType::Unpair),
                false,
            ));
        } else if device.pair_state == 2 {
            buttons.push(crate::widgets::quick_action_btn(
                "dialog-ok-symbolic",
                "Accept",
                Message::PerformAction(device.id.clone(), ActionType::AcceptPairing),
                true,
            ));
            buttons.push(crate::widgets::quick_action_btn(
                "dialog-cancel-symbolic",
                "Cancel",
                Message::PerformAction(device.id.clone(), ActionType::CancelPairing),
                false,
            ));
        } else if device.pair_state == 0 {
            buttons.push(crate::widgets::quick_action_btn(
                "emblem-new-symbolic",
                "Pair",
                Message::PerformAction(device.id.clone(), ActionType::Pair),
                false,
            ));
        }

        if buttons.is_empty() {
            None
        } else {
            Some(
                container(
                    column![
                        crate::widgets::section_header("Quick Actions"),
                        row::with_children(buttons).spacing(4),
                    ]
                    .spacing(6),
                )
                .padding([0, 0])
                .into(),
            )
        }
    }

    fn render_info_banner<'a>(&'a self, device: &'a Device, draft: &'a DeviceDraft) -> Option<Element<'a, Message>> {
        if let Some(player) = &draft.player {
            if !player.title.is_empty() {
                return Some(crate::widgets::info_banner(
                    "Now Playing",
                    &player.title,
                ));
            }
        }

        if let Some(rf) = self.received_files.get(&device.id)
            .and_then(|v| v.last())
        {
            return Some(crate::widgets::info_banner(
                "File Received",
                &rf.file_name,
            ));
        }

        if let Some(notif) = draft.notifications.first() {
            return Some(crate::widgets::info_banner(
                &notif.app_name,
                &notif.text,
            ));
        }

        None
    }

    fn render_clipboard_row(&self, device: &Device) -> Element<'_, Message> {
        crate::widgets::list_row(
            "edit-paste-symbolic",
            "Send Clipboard",
            Message::ReadClipboard(device.id.clone()),
        )
    }

    fn render_share_row(&self, device: &Device) -> Element<'_, Message> {
        crate::widgets::list_row(
            "document-send-symbolic",
            "Share",
            Message::ChooseFile(device.id.clone()),
        )
    }

    fn render_advanced_section<'a>(&'a self, device: &'a Device, draft: &'a DeviceDraft) -> Element<'a, Message> {
        if self.advanced_device.as_deref() != Some(&device.id) {
            return crate::widgets::disclosure_row(
                "Advanced",
                false,
                Message::ToggleAdvanced(device.id.clone()),
            );
        }

        let mut children: Vec<Element<Message>> = Vec::new();

        children.push(crate::widgets::disclosure_row(
            "Advanced",
            true,
            Message::ToggleAdvanced(device.id.clone()),
        ));

        if device.is_paired && device.is_reachable && device.has_plugin("kdeconnect_sms") {
            children.push(divider::horizontal::default().into());

            if draft.conversations.is_empty() && !draft.sms_busy {
                children.push(
                    container(
                        button::custom(
                            row![
                                icon::from_name("mail-send-symbolic").size(14),
                                text::caption("Load Conversations"),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::RefreshConversations(device.id.clone()))
                        .padding([8, 12])
                        .width(Length::Fill)
                    )
                    .padding([4, 0])
                    .into(),
                );
            }

            for msg in &draft.conversations {
                let is_selected = draft.selected_conversation == Some(msg.thread_id);
                let preview = if msg.body.len() > 40 {
                    format!("{}…", &msg.body[..40])
                } else {
                    msg.body.clone()
                };
                let sender = if msg.is_incoming() { msg.sender().to_string() } else { "Me".to_string() };

                children.push(
                    button::custom(
                        row![
                            text::caption(format!("{}: {}", sender, preview)).size(12),
                        ]
                    )
                    .on_press(Message::SelectConversation(device.id.clone(), msg.thread_id))
                    .padding([6, 10])
                    .width(Length::Fill)
                    .into(),
                );

                if is_selected {
                    children.push(
                        container(
                            row![
                                text_input::text_input("Reply…", &draft.reply_text)
                                    .on_input({
                                        let did = device.id.clone();
                                        move |v| Message::ReplyTextChanged(did.clone(), v)
                                    })
                                    .width(Length::Fill),
                                button::custom(text::caption("Send"))
                                    .on_press(Message::SendReply(device.id.clone(), msg.thread_id))
                                    .padding([6, 12]),
                            ]
                            .spacing(6),
                        )
                        .padding([4, 0, 8, 0])
                        .into(),
                    );
                }
            }
        }

        if device.is_reachable && device.has_plugin("kdeconnect_notifications") {
            children.push(divider::horizontal::default().into());

            for notif in &draft.notifications {
                let is_selected = draft.selected_notification.as_deref() == Some(&notif.internal_id);
                let label = if notif.title.is_empty() {
                    format!("{}: {}", notif.app_name, notif.text)
                } else {
                    format!("{}: {} - {}", notif.app_name, notif.title, notif.text)
                };

                children.push(
                    button::custom(
                        row![
                            icon::from_name("dialog-information-symbolic").size(12),
                            text::caption(if label.len() > 50 { format!("{}…", &label[..50]) } else { label.clone() }).size(11),
                        ]
                        .spacing(6)
                        .width(Length::Fill),
                    )
                    .on_press(Message::SelectNotification(device.id.clone(), notif.internal_id.clone()))
                    .padding([6, 10])
                    .width(Length::Fill)
                    .into(),
                );

                if notif.dismissable {
                    children.push(
                        container(
                            button::custom(
                                row![
                                    icon::from_name("window-close-symbolic").size(10),
                                    text::caption("Dismiss"),
                                ]
                                .spacing(4)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::DismissNotification(device.id.clone(), notif.internal_id.clone()))
                            .padding([4, 8]),
                        )
                        .padding([0, 0, 0, 12])
                        .into(),
                    );
                }

                if is_selected && !notif.reply_id.is_empty() {
                    children.push(
                        container(
                            row![
                                text_input::text_input("Reply…", &draft.notify_reply_text)
                                    .on_input({
                                        let did = device.id.clone();
                                        move |v| Message::NotifyReplyChanged(did.clone(), v)
                                    })
                                    .width(Length::Fill),
                                button::custom(text::caption("Send"))
                                    .on_press(Message::SendNotifyReply(device.id.clone(), notif.internal_id.clone()))
                                    .padding([6, 12]),
                            ]
                            .spacing(6),
                        )
                        .padding([4, 0, 8, 12])
                        .into(),
                    );
                }
            }
        }

        if let Some(files) = self.received_files.get(&device.id) {
            if !files.is_empty() {
                children.push(divider::horizontal::default().into());

                for rf in files.iter().rev().take(10) {
                    if rf.active {
                        children.push(
                            container(
                                column![
                                    row![
                                        icon::from_name("document-save-symbolic").size(12),
                                        text::caption(&rf.file_name).size(11),
                                        container(row![]).width(Length::Fill),
                                        text::caption(format!("{}%", rf.progress)).size(10),
                                    ].spacing(4).align_y(Alignment::Center),
                                    progress_bar::determinate_linear(
                                        (rf.progress as f32 / 100.0).clamp(0.0, 1.0)
                                    )
                                    .width(Length::Fill)
                                    .girth(4),
                                ].spacing(2)
                            )
                            .padding([4, 12])
                            .width(Length::Fill)
                            .into(),
                        );
                    } else {
                        let short_path = rf.file_path.rsplit('/').next().unwrap_or(&rf.file_path);
                        children.push(
                            container(
                                row![
                                    icon::from_name("document-save-symbolic").size(12),
                                    text::caption(format!("{} → {}", rf.file_name, short_path)).size(11),
                                ].spacing(4)
                            )
                            .padding([4, 12])
                            .width(Length::Fill)
                            .into(),
                        );
                    }
                }
            }
        }

        let mut refresh_discover = row![
            button::custom(
                row![
                    icon::from_name("view-refresh-symbolic").size(12),
                    text::caption("Refresh"),
                ].spacing(4).align_y(Alignment::Center),
            )
            .on_press(Message::RefreshDevices)
            .padding([6, 12]),
        ];

        let discover_label = if self.is_discovering {
            "Discovering…"
        } else {
            "Discover"
        };
        refresh_discover = refresh_discover.push(
            button::custom(
                row![
                    icon::from_name("network-wireless-symbolic").size(12),
                    text::caption(discover_label),
                ].spacing(4).align_y(Alignment::Center),
            )
            .on_press(Message::DiscoverDevices)
            .padding([6, 12]),
        );

        children.push(divider::horizontal::default().into());
        children.push(
            container(refresh_discover.spacing(6))
            .padding([4, 0])
            .into(),
        );

        if let Some(status) = &draft.status {
            children.push(
                container(
                    row![
                        icon::from_name(
                            if status == "Working..." { "process-working-symbolic" }
                            else if status.contains("error") || status.contains("failed") { "dialog-error-symbolic" }
                            else { "emblem-default-symbolic" }
                        ).size(12),
                        text::caption(status).size(11),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding([6, 12])
                .width(Length::Fill)
                .into(),
            );
        }

        container(column::with_children(children).spacing(0))
            .padding([0, 0])
            .width(Length::Fill)
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
                    let Some(parent_id) = self.popup_parent_id() else {
                        self.error = Some("Applet window not ready yet. Please try again.".into());
                        return Task::none();
                    };

                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        parent_id,
                        new_id,
                        Some((340, 500)),
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .min_width(340.0)
                        .max_width(360.0)
                        .min_height(200.0)
                        .max_height(750.0);
                    get_popup(popup_settings)
                };
            }
            Message::RefreshDevices => {
                let Some(backend) = self.backend.clone() else {
                    self.error = Some("KDE Connect backend unavailable".into());
                    return Task::none();
                };
                self.is_discovering = false;

                return Task::perform(
                    async move {
                        let devices = backend.devices().await;
                        let pairing = backend.pairing_request_ids().await;
                        (devices, pairing)
                    },
                    |(devices, pairing)| Message::DevicesUpdated(devices, pairing),
                )
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
            Message::DevicesUpdated(devices, pairing_ids) => {
                log::info!("DevicesUpdated: {} devices, {} pairing requests", devices.len(), pairing_ids.len());
                let mut merged = devices;
                for pid in &pairing_ids {
                    if !merged.iter().any(|d| &d.id == pid) {
                        merged.push(crate::model::Device {
                            id: pid.clone(),
                            name: "Unknown device".into(),
                            device_type: DeviceType::Unknown("".into()),
                            is_reachable: true,
                            is_paired: false,
                            pair_state: 2,
                            battery: None,
                            supported_plugins: vec![],
                            loaded_plugins: vec![],
                        });
                    }
                }
                self.devices = merged;
                self.last_sync = Some(std::time::Instant::now());
                if self.selected_device_id.map(|i| i >= self.devices.len()).unwrap_or(true) && !self.devices.is_empty() {
                    self.selected_device_id = Some(0);
                }
                self.sync_drafts();
                self.error = None;

                let mut tasks = Task::none();

                for pid in &pairing_ids {
                    if self.notified_pair_ids.insert(pid.clone()) {
                        let dev_name = self.devices.iter()
                            .find(|d| d.id == *pid)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| "Unknown device".into());
                        if let Some(backend) = self.backend.clone() {
                            let name = dev_name;
                            let did = pid.clone();
                            tasks = tasks.chain(Task::perform(
                                async move {
                                    backend.notify(
                                        "Pairing request",
                                        &format!("{name} wants to pair"),
                                        0,
                                        &["accept", "Accept", "reject", "Reject"],
                                    ).await.ok()
                                },
                                move |notif_id| {
                                    if let Some(id) = notif_id {
                                        Message::NotifCreated(did, id)
                                    } else {
                                        Message::NoOp
                                    }
                                },
                            ).map(cosmic::Action::App));
                        }
                    }
                }
                for device in &self.devices {
                    if !device.is_reachable { continue; }
                    if device.has_plugin("kdeconnect_notifications") {
                        let Some(backend) = self.backend.clone() else { continue; };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(Task::perform(
                            async move { backend.fetch_notifications(&did).await },
                            move |n| Message::NotificationsLoaded(did2, n),
                        ).map(cosmic::Action::App));
                    }
                    if device.has_plugin("kdeconnect_mprisremote") {
                        let Some(backend) = self.backend.clone() else { continue; };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(Task::perform(
                            async move { backend.player_info(&did).await },
                            move |p| Message::PlayerInfoUpdated(did2, p),
                        ).map(cosmic::Action::App));
                    }
                    if device.has_plugin("kdeconnect_connectivity_report") {
                        let Some(backend) = self.backend.clone() else { continue; };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(Task::perform(
                            async move { backend.connectivity_info(&did).await },
                            move |c| Message::ConnectivityUpdated(did2, c),
                        ).map(cosmic::Action::App));
                    }
                }

                let has_reachable = self.devices.iter().any(|d| d.is_reachable);
                let now = std::time::Instant::now();
                if has_reachable {
                    self.auto_discovering = false;
                    self.next_auto_discovery = None;
                } else if self.next_auto_discovery.map_or(true, |t| now >= t) {
                    self.auto_discovering = true;
                    self.next_auto_discovery = Some(now + Duration::from_secs(30));
                    if let Some(backend) = self.backend.clone() {
                        tasks = tasks.chain(Task::perform(
                            async move {
                                backend.force_discovery().await;
                            },
                            |_| Message::NoOp,
                        ).map(cosmic::Action::App));
                    }
                }

                self.notified_pair_ids.retain(|id| pairing_ids.contains(id));

                return tasks;
            }
            Message::DiscoverDevices => {
                log::info!("DiscoverDevices: starting discovery");
                let Some(backend) = self.backend.clone() else {
                    self.error = Some("KDE Connect backend unavailable".into());
                    return Task::none();
                };
                self.is_discovering = true;
                self.error = None;
                return Task::perform(
                    async move {
                        backend.force_discovery().await;
                        tokio::time::sleep(Duration::from_secs(4)).await;
                    },
                    |_| {
                        Message::RefreshDevices
                    },
                )
                .map(cosmic::Action::App);
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
            Message::ChooseFile(device_id) => {
                let device_id_for_task = device_id.clone();
                return Task::perform(
                    async move { pick_file_path().await },
                    move |result| Message::FileChooserFinished(device_id_for_task, result),
                )
                .map(cosmic::Action::App);
            }
            Message::FileChooserFinished(device_id, result) => {
                match result {
                    Ok(path) => {
                        let did = device_id.clone();
                        let fname = path.rsplit('/').next().unwrap_or(&path).to_string();
                        self.draft_mut(&device_id).status = Some(format!("Sending {fname}…"));
                        return Task::perform(
                            async move {},
                            move |_| Message::PerformAction(did, ActionType::SendFile(path)),
                        ).map(cosmic::Action::App);
                    }
                    Err(error) => {
                        let draft = self.draft_mut(&device_id);
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

                let draft = self.draft_mut(&device_id);
                draft.status = Some("Working...".into());
                draft.last_action = Some(action.clone());

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
                let device_name = self.devices.iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                let backend = self.backend.clone();

                let draft = self.draft_mut(&device_id);
                let action_label = draft.last_action.as_ref().map(|a| a.label()).unwrap_or("Action").to_string();
                let fname = draft.last_action.as_ref()
                    .and_then(|a| if let ActionType::SendFile(p) = a { Some(p.clone()) } else { None })
                    .and_then(|p| p.rsplit('/').next().map(|s| s.to_string()))
                    .unwrap_or_default();
                draft.last_action = None;
                draft.status = Some(match &result {
                    Ok(message) => message.clone(),
                    Err(error) => error.clone(),
                });

                let notif_msg = match &result {
                    Ok(_) => {
                        if fname.is_empty() {
                            format!("{action_label} sent successfully")
                        } else {
                            format!("{fname} sent successfully")
                        }
                    }
                    Err(e) => {
                        if fname.is_empty() {
                            format!("{action_label} failed: {e}")
                        } else {
                            format!("{fname} failed: {e}")
                        }
                    }
                };
                return Task::perform(
                    async move {
                        if let Some(backend) = backend {
                            let _ = backend.notify(
                                &format!("{device_name}"),
                                &notif_msg,
                                0,
                                &[],
                            ).await;
                        }
                    },
                    |_| Message::RefreshDevices,
                ).map(cosmic::Action::App);
            }
            Message::RefreshConversations(device_id) => {
                let Some(backend) = self.backend.clone() else {
                    self.draft_mut(&device_id).sms_busy = false;
                    return Task::none();
                };
                self.draft_mut(&device_id).sms_busy = true;
                let did = device_id.clone();
                return Task::perform(
                    async move {
                        backend.request_all_conversations(&did).await;
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        let convos = backend.active_conversations(&did).await;
                        convos
                    },
                    move |convos| Message::ConversationsLoaded(device_id, convos),
                )
                .map(cosmic::Action::App);
            }
            Message::ConversationsLoaded(device_id, conversations) => {
                let draft = self.draft_mut(&device_id);
                draft.conversations = conversations;
                draft.sms_busy = false;
            }
            Message::SelectConversation(device_id, thread_id) => {
                let draft = self.draft_mut(&device_id);
                draft.selected_conversation = Some(thread_id);
                draft.reply_text.clear();
            }
            Message::ReplyTextChanged(device_id, text) => {
                self.draft_mut(&device_id).reply_text = text;
            }
            Message::SendReply(device_id, thread_id) => {
                let text = self.draft_mut(&device_id).reply_text.clone();
                if text.trim().is_empty() {
                    return Task::none();
                }
                self.draft_mut(&device_id).reply_text.clear();
                let action = ActionType::ReplyToConversation(thread_id, text);
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, action),
                )
                .map(cosmic::Action::App);
            }
            Message::RefreshNotifications(device_id) => {
                let Some(backend) = self.backend.clone() else { return Task::none(); };
                let did = device_id.clone();
                return Task::perform(
                    async move { backend.fetch_notifications(&did).await },
                    move |notifs| Message::NotificationsLoaded(device_id, notifs),
                )
                .map(cosmic::Action::App);
            }
            Message::NotificationsLoaded(device_id, notifications) => {
                self.draft_mut(&device_id).notifications = notifications;
            }
            Message::SelectNotification(device_id, internal_id) => {
                let draft = self.draft_mut(&device_id);
                draft.selected_notification = Some(internal_id);
                draft.notify_reply_text.clear();
            }
            Message::NotifyReplyChanged(device_id, text) => {
                self.draft_mut(&device_id).notify_reply_text = text;
            }
            Message::SendNotifyReply(device_id, internal_id) => {
                let text = self.draft_mut(&device_id).notify_reply_text.clone();
                if text.trim().is_empty() { return Task::none(); }
                self.draft_mut(&device_id).notify_reply_text.clear();
                let action = ActionType::ReplyToNotification(internal_id, text);
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, action),
                )
                .map(cosmic::Action::App);
            }
            Message::DismissNotification(device_id, internal_id) => {
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, ActionType::DismissNotification(internal_id)),
                )
                .map(cosmic::Action::App);
            }
            Message::RefreshPlayer(device_id) => {
                let Some(backend) = self.backend.clone() else { return Task::none(); };
                let did = device_id.clone();
                return Task::perform(
                    async move { backend.player_info(&did).await },
                    move |player| Message::PlayerInfoUpdated(device_id, player),
                )
                .map(cosmic::Action::App);
            }
            Message::PlayerInfoUpdated(device_id, player) => {
                self.draft_mut(&device_id).player = player;
            }
            Message::ConnectivityUpdated(device_id, connectivity) => {
                self.draft_mut(&device_id).connectivity = connectivity;
            }
            Message::MediaAction(device_id, action) => {
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, ActionType::MediaAction(action)),
                )
                .map(cosmic::Action::App);
            }
            Message::SelectPlayer(device_id, player) => {
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, ActionType::SelectPlayer(player)),
                )
                .map(cosmic::Action::App);
            }
            Message::TransferStarted(device_id, transfer_id, file_name, total_bytes) => {
                let rf = ReceivedFile {
                    device_id: device_id.clone(),
                    file_path: String::new(),
                    file_name,
                    received_at: std::time::SystemTime::now(),
                    unread: false,
                    transfer_id: transfer_id.clone(),
                    progress: 0,
                    active: true,
                    total_bytes,
                };
                self.received_files.entry(device_id).or_default().push(rf);
            }
            Message::TransferProgress(device_id, transfer_id, _transferred, _total, percent) => {
                let nkey = format!("transfer:{transfer_id}");
                let files = self.received_files.get_mut(&device_id);
                if let Some(files) = files {
                if let Some(rf) = files.iter_mut().find(|f| f.transfer_id == transfer_id) {
                    rf.progress = percent.clamp(0, 100) as u32;
                    }
                }
                let prev = self.last_notif_pct.get(&nkey).copied().unwrap_or(-5);
                let pct_block = (percent / 5) * 5;
                if pct_block <= prev { return Task::none(); }
                self.last_notif_pct.insert(nkey.clone(), pct_block);
                if let Some(&notif_id) = self.active_notifs.get(&nkey) {
                    let Some(backend) = self.backend.clone() else { return Task::none() };
                    let nk = nkey.clone();
                    return Task::perform(
                        async move {
                            backend.notify("Transfer in progress", &format!("{percent}%"), notif_id, &[]).await.ok()
                        },
                        move |new_id| {
                            if let Some(id) = new_id {
                                Message::NotifCreated(nk, id)
                            } else {
                                Message::NoOp
                            }
                        },
                    ).map(cosmic::Action::App);
                } else if self.pending_notif_requests.contains(&nkey) {
                    return Task::none();
                } else {
                    self.pending_notif_requests.insert(nkey.clone());
                    let Some(backend) = self.backend.clone() else { return Task::none() };
                    let nk = nkey.clone();
                    return Task::perform(
                        async move {
                            backend.notify("Receiving file", &format!("{percent}%"), 0, &[]).await.ok()
                        },
                        move |notif_id| {
                            if let Some(id) = notif_id {
                                Message::NotifCreated(nk, id)
                            } else {
                                Message::NoOp
                            }
                        },
                    ).map(cosmic::Action::App);
                }
            }
            Message::TransferFinished(device_id, transfer_id, file_path) => {
                let nkey = format!("transfer:{transfer_id}");
                let file_name = file_path.rsplit('/').next()
                    .unwrap_or(&file_path)
                    .to_string();
                let notif_name = file_name.clone();

                let device_name = self.devices.iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();

                let files = self.received_files.entry(device_id.clone()).or_default();

                if let Some(rf) = files.iter_mut().find(|f| f.transfer_id == transfer_id) {
                    rf.file_path = file_path.clone();
                    rf.progress = 100;
                    rf.active = false;
                    rf.file_name = file_name;
                    rf.received_at = std::time::SystemTime::now();
                    rf.unread = true;
                } else {
                    files.push(ReceivedFile {
                        device_id: device_id.clone(),
                        file_path: file_path.clone(),
                        file_name,
                        received_at: std::time::SystemTime::now(),
                        unread: true,
                        transfer_id: transfer_id.clone(),
                        progress: 100,
                        active: false,
                        total_bytes: 0,
                    });
                }

                if files.len() > MAX_RECEIVED_HISTORY {
                    files.remove(0);
                }

                let notif_id = self.active_notifs.remove(&nkey).unwrap_or(0);
                self.pending_notif_requests.remove(&nkey);
                self.last_notif_pct.remove(&nkey);
                let dest_path = file_path.trim_start_matches("file://").to_string();
                if notif_id == 0 {
                    self.deferred_notifs.insert(nkey, (device_name, notif_name, dest_path));
                } else {
                    let backend = self.backend.clone();
                    return Task::perform(
                        async move {
                            let Some(backend) = backend else { return };
                            let _ = backend.notify(
                                &format!("File received from {device_name}"),
                                &format!("{notif_name} → {dest_path}"),
                                notif_id,
                                &[],
                            ).await;
                        },
                        |_| Message::NoOp,
                    ).map(cosmic::Action::App);
                }
            }
            Message::TransferFailed(device_id, transfer_id, _error_code, error_string) => {
                let nkey = format!("transfer:{transfer_id}");
                log::warn!("Transfer {transfer_id} failed: {error_string}");
                if let Some(files) = self.received_files.get_mut(&device_id) {
                    if let Some(rf) = files.iter_mut().find(|f| f.transfer_id == transfer_id) {
                        rf.active = false;
                    }
                }
                let notif_id = self.active_notifs.remove(&nkey).unwrap_or(0);
                self.pending_notif_requests.remove(&nkey);
                self.last_notif_pct.remove(&nkey);
                let Some(backend) = self.backend.clone() else { return Task::none() };
                return Task::perform(
                    async move {
                        let _ = backend.notify("Transfer failed", &error_string, notif_id, &[]).await;
                    },
                    |_| Message::NoOp,
                ).map(cosmic::Action::App);
            }
            Message::FileReceived(device_id, file_path) => {
                let file_name = file_path.rsplit('/').next()
                    .unwrap_or(&file_path)
                    .to_string();

                let rf = ReceivedFile {
                    device_id: device_id.clone(),
                    file_path: file_path.clone(),
                    file_name,
                    received_at: std::time::SystemTime::now(),
                    unread: true,
                    transfer_id: String::new(),
                    progress: 100,
                    active: false,
                    total_bytes: 0,
                };

                let files = self.received_files.entry(device_id).or_default();

                if files.iter().any(|f| f.file_path == file_path && !f.file_path.is_empty()) {
                    return Task::none();
                }

                files.push(rf);
                if files.len() > MAX_RECEIVED_HISTORY {
                    files.remove(0);
                }
            }
            Message::PingReceived(device_id) => {
                let device_name = self.devices.iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| device_id.clone());
                if let Some(backend) = self.backend.clone() {
                    return Task::perform(
                        async move {
                            let _ = backend.notify(
                                &format!("Ping from {device_name}"),
                                "Your device sent a ping!",
                                0,
                                &[],
                            ).await;
                        },
                        |_| Message::NoOp,
                    ).map(cosmic::Action::App);
                }
            }
            Message::ClipboardReceived(device_id, content) => {
                let device_name = self.devices.iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| device_id.clone());
                let preview = if content.len() > 80 {
                    format!("{}...", &content[..80])
                } else {
                    content.clone()
                };
                if let Some(backend) = self.backend.clone() {
                    return Task::perform(
                        async move {
                            let _ = backend.notify(
                                &format!("Clipboard from {device_name}"),
                                &preview,
                                0,
                                &[],
                            ).await;
                        },
                        |_| Message::NoOp,
                    ).map(cosmic::Action::App);
                }
            }
            Message::ReceivedFilesViewed(device_id) => {
                if let Some(files) = self.received_files.get_mut(&device_id) {
                    for file in files.iter_mut() {
                        file.unread = false;
                    }
                }
            }
            Message::NotifCreated(key, notif_id) => {
                self.pending_notif_requests.remove(&key);
                if key.contains(':') {
                    if let Some((device_name, notif_name, dest_path)) = self.deferred_notifs.remove(&key) {
                        if let Some(backend) = self.backend.clone() {
                            return Task::perform(
                                async move {
                                    let _ = backend.notify(
                                        &format!("File received from {device_name}"),
                                        &format!("{notif_name} → {dest_path}"),
                                        notif_id,
                                        &[],
                                    ).await;
                                },
                                |_| Message::NoOp,
                            ).map(cosmic::Action::App);
                        }
                    } else {
                        self.active_notifs.insert(key, notif_id);
                    }
                } else {
                    self.pairing_notifs.insert(notif_id, key);
                }
            }
            Message::NotifAction(notif_id, action_key) => {
                if let Some(device_id) = self.pairing_notifs.remove(&notif_id) {
                    self.notified_pair_ids.remove(&device_id);
                    if let Some(backend) = self.backend.clone() {
                        return Task::perform(
                            async move {
                                match action_key.as_str() {
                                    "accept" => backend.accept_pairing(&device_id).await,
                                    "reject" => backend.cancel_pairing(&device_id).await,
                                    _ => {}
                                }
                            },
                            |_| Message::RefreshDevices,
                        ).map(cosmic::Action::App);
                    }
                }
            }
            Message::NoOp => {}
            Message::SelectDevice(index) => {
                if index < self.devices.len() {
                    self.selected_device_id = Some(index);
                    self.advanced_device = None;
                }
            }
            Message::ToggleAdvanced(id) => {
                if self.advanced_device.as_deref() == Some(&id) {
                    self.advanced_device = None;
                } else {
                    self.advanced_device = Some(id);
                }
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let suggested = self.core.applet.suggested_size(false);
        let icon_name = if self.devices.iter().any(|d| d.is_reachable) {
            "io.github.acemythos.Connect-symbolic"
        } else {
            "io.github.acemythos.Connect-off-symbolic"
        };
        let icon = icon::from_name(icon_name)
            .size(suggested.1.saturating_sub(4));

        let unread: usize = self.received_files.values()
            .flat_map(|v| v.iter())
            .filter(|f| f.unread)
            .count();

        let preview = text::caption(self.panel_preview()).size(12);
        let mut content = row![icon, preview]
            .spacing(6)
            .align_y(Alignment::Center);

        if unread > 0 {
            content = content.push(
                text::caption(format!("({unread})")).size(11)
            );
        }

        let btn = self.core
            .applet
            .button_from_element(content, true)
            .width(Length::Shrink)
            .on_press_down(Message::TogglePopup);

        self.core.applet.autosize_window(btn).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        content.push(self.render_device_selector().into());

        let is_discovering = self.is_discovering || self.auto_discovering;
        content.push(
            container(
                row![
                    crate::widgets::pill_button("view-refresh-symbolic", "Refresh", Message::RefreshDevices, false),
                    crate::widgets::pill_button(
                        "network-wireless-symbolic",
                        if is_discovering { "Searching…" } else { "Discover" },
                        Message::DiscoverDevices,
                        is_discovering,
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([0, 14, 12, 14])
            .into(),
        );

        if let Some(last) = self.last_sync {
            let secs = last.elapsed().as_secs();
            let label: String = if secs < 3 {
                "Updated just now".into()
            } else if secs < 60 {
                format!("Updated {}s ago", secs)
            } else {
                format!("Updated {}m ago", secs / 60)
            };
            content.push(
                container(text::caption(label).size(10))
                    .padding([0, 14, 8, 14])
                    .into(),
            );
        }

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
                        text::body("No devices found"),
                        text::caption(
                            "Make sure KDE Connect is installed\nand a device is paired."
                        ),
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(32)
                .width(Length::Fill)
                .into(),
            );
        } else if let Some(device) = self.selected_device() {
            let device_id = &device.id;
            if let Some(draft) = self.drafts.get(device_id) {
                content.push(self.render_device_status_card(device));

                if device.is_reachable {
                    if let Some(qa) = self.render_quick_action_row(device) {
                        content.push(qa);
                    }

                    if let Some(banner) = self.render_info_banner(device, draft) {
                        content.push(container(banner).padding([4, 0, 12, 0]).into());
                    }

                    content.push(divider::horizontal::default().into());

                    if device.has_plugin("kdeconnect_clipboard") {
                        content.push(self.render_clipboard_row(device));
                    }
                    if device.has_plugin("kdeconnect_share") {
                        content.push(self.render_share_row(device));
                    }

                    content.push(divider::horizontal::default().into());
                }

                content.push(self.render_advanced_section(device, draft));
            }
        }

        let body = column::with_children(content).spacing(0);

        let panel = scrollable(
            container(body)
                .class(theme::Container::custom(|theme| {
                    let cosmic = theme.cosmic();
                    iced_container::Style {
                        background: Some(Background::Color(Color::from_rgb8(0x18, 0x18, 0x18))),
                        border: Border {
                            radius: cosmic.radius_m().into(),
                            width: 1.0,
                            color: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08),
                        },
                        ..Default::default()
                    }
                }))
                .width(Length::Fill),
        )
        .height(Length::Shrink)
        .width(Length::Fill);

        self.core
            .applet
            .popup_container(panel)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(std::any::TypeId::of::<()>(), |_state| {
            let poll = unfold(PollState::new(), |mut state| async {
                tokio::time::sleep(POLL_INTERVAL).await;
                let msg = match state.poll().await {
                    Ok((devices, pairing)) => Message::DevicesUpdated(devices, pairing),
                    Err(e) => Message::BackendError(e),
                };
                Some((msg, state))
            });

            let signals = unfold(ShareSignalState::new(), |mut state| async {
                let msg = state.next().await;
                Some((msg, state))
            });

            let notif_actions = unfold(NotificationActionState::new(), |mut state| async {
                let msg = state.next().await;
                Some((msg, state))
            });

            futures_util::stream::select(
                futures_util::stream::select(poll, signals),
                notif_actions,
            )
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

    async fn poll(&mut self) -> Result<(Vec<Device>, Vec<String>), String> {
        if self.backend.is_none() {
            match KdeConnectBackend::new().await {
                Ok(backend) => self.backend = Some(backend),
                Err(error) => {
                    log::info!("Poll: D-Bus connection failed: {error}");
                    return Err(format!("D-Bus: {error}"));
                }
            }
        }

        let backend = self.backend.as_ref().unwrap();
        let devices = backend.devices().await;
        let pairing = backend.pairing_request_ids().await;
        log::info!("Poll: {} devices, {} pairing requests", devices.len(), pairing.len());
        for d in &devices {
            log::debug!("Poll: device '{}' (reachable={}, paired={}, state={})", d.name, d.is_reachable, d.is_paired, d.pair_state);
        }
        Ok((devices, pairing))
    }
}

struct ShareSignalState {
    stream: Option<MessageStream>,
}

impl ShareSignalState {
    fn new() -> Self {
        Self { stream: None }
    }

    async fn next(&mut self) -> Message {
        loop {
            if self.stream.is_none() {
                match zbus::Connection::session().await {
                    Ok(conn) => {
                        let rule = match MatchRule::builder()
                            .msg_type(zbus::message::Type::Signal)
                            .path_namespace("/modules/kdeconnect/devices")
                        {
                            Ok(r) => r.build(),
                            Err(e) => {
                                log::warn!("MatchRule build failed: {e}, retrying in 5s");
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                continue;
                            }
                        };
                        match MessageStream::for_match_rule(rule, &conn, None).await {
                            Ok(stream) => {
                                self.stream = Some(stream);
                            }
                            Err(e) => {
                                log::warn!("Signal stream setup failed: {e}, retrying in 5s");
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("D-Bus connection failed: {e}, retrying in 5s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }

            match self.stream.as_mut().unwrap().next().await {
                Some(Ok(msg)) => {
                    let header = msg.header();
                    let path = match header.path() {
                        Some(p) => p,
                        None => continue,
                    };
                    let path_str = path.as_str();
                    let device_id = match path_str
                        .strip_prefix("/modules/kdeconnect/devices/")
                        .and_then(|s| {
                            if s.ends_with("/share") || s.ends_with("/ping") {
                                s.strip_suffix("/share").or_else(|| s.strip_suffix("/ping"))
                            } else {
                                None
                            }
                        })
                    {
                        Some(id) => id.to_string(),
                        None => continue,
                    };
                    let member = match header.member() {
                        Some(m) => m.to_string(),
                        None => continue,
                    };

                    match member.as_str() {
                        "shareReceived" => {
                            if let Ok(file_path) = msg.body().deserialize::<String>() {
                                return Message::FileReceived(device_id, file_path);
                            }
                        }
                        "transferStarted" => {
                            if let Ok((transfer_id, file_name, total_bytes)) =
                                msg.body().deserialize::<(String, String, u64)>()
                            {
                                return Message::TransferStarted(
                                    device_id, transfer_id, file_name, total_bytes,
                                );
                            }
                        }
                        "transferProgress" => {
                            if let Ok((transfer_id, transferred, total, percent)) =
                                msg.body().deserialize::<(String, u64, u64, i32)>()
                            {
                                return Message::TransferProgress(
                                    device_id, transfer_id, transferred, total, percent,
                                );
                            }
                        }
                        "transferFinished" => {
                            if let Ok((transfer_id, url)) =
                                msg.body().deserialize::<(String, String)>()
                            {
                                return Message::TransferFinished(
                                    device_id, transfer_id, url,
                                );
                            }
                        }
                        "transferFailed" => {
                            if let Ok((transfer_id, code, error_str)) =
                                msg.body().deserialize::<(String, i32, String)>()
                            {
                                return Message::TransferFailed(
                                    device_id, transfer_id, code, error_str,
                                );
                            }
                        }
                        "pingReceived" => {
                            return Message::PingReceived(device_id);
                        }
                        "clipboardReceived" => {
                            if let Ok(content) = msg.body().deserialize::<String>() {
                                return Message::ClipboardReceived(device_id, content);
                            }
                        }
                        _ => {}
                    }
                }
                Some(Err(e)) => {
                    log::warn!("Signal stream error: {e}");
                    continue;
                }
                None => {
                    log::info!("Signal stream ended, reconnecting");
                    self.stream = None;
                }
            }
        }
    }
}

struct NotificationActionState {
    stream: Option<MessageStream>,
}

impl NotificationActionState {
    fn new() -> Self {
        Self { stream: None }
    }

    async fn next(&mut self) -> Message {
        loop {
            if self.stream.is_none() {
                match zbus::Connection::session().await {
                    Ok(conn) => {
                        let rule = MatchRule::builder()
                            .msg_type(zbus::message::Type::Signal)
                            .interface("org.freedesktop.Notifications")
                            .unwrap()
                            .member("ActionInvoked")
                            .unwrap()
                            .build();
                        match MessageStream::for_match_rule(rule, &conn, None).await {
                            Ok(stream) => {
                                self.stream = Some(stream);
                            }
                            Err(e) => {
                                log::warn!("Notification action stream setup failed: {e}, retrying in 5s");
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("D-Bus connection failed: {e}, retrying in 5s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }

            match self.stream.as_mut().unwrap().next().await {
                Some(Ok(msg)) => {
                    let body: Result<(u32, String), _> = msg.body().deserialize();
                    if let Ok((notif_id, action_key)) = body {
                        return Message::NotifAction(notif_id, action_key);
                    }
                }
                Some(Err(e)) => {
                    log::warn!("Notification action stream error: {e}");
                    continue;
                }
                None => {
                    log::info!("Notification action stream ended, reconnecting");
                    self.stream = None;
                }
            }
        }
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



#[cfg(test)]
mod tests {
    use super::CosmicConnect;

    #[test]
    fn popup_parent_id_is_none_before_window_is_ready() {
        let app = CosmicConnect::default();
        assert!(app.popup_parent_id().is_none());
    }
}
