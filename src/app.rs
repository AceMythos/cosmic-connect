use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use ashpd::desktop::file_chooser::SelectedFiles;
use cosmic::anim;
use cosmic::app::Core;
use cosmic::iced::core::Alignment;
use cosmic::iced::platform_specific::shell::commands::popup::destroy_popup;
use cosmic::surface;
use cosmic::iced::time;
use cosmic::iced::window::Id;
use cosmic::iced::gradient;
use cosmic::iced::{Background, Border, Color, Gradient, Length, Limits, Shadow, Subscription, Vector};
use cosmic::widget::text_input;
use cosmic::widget::autosize::autosize;
use cosmic::widget::{
    button, column, container, divider, icon, progress_bar, row, scrollable, text,
};
use cosmic::widget::container as iced_container;
use cosmic::theme;
use cosmic::{Action, Element, Task};

use crate::widgets::{
    card_default,
    frosted_bg, on_overlay,
    RADIUS_CARD, RADIUS_MD,
    CARD_LUM_DARKEN,
    OPACITY_CARD, OPACITY_COATING,
    BORDER_GLASS_ALPHA,
    HOVER_OVERLAY_ALPHA, PRESSED_ALPHA,
    SHADOW_PANEL_ALPHA,
    SIZE_BODY, SIZE_CAPTION,
};
use futures_util::stream::unfold;
use futures_util::StreamExt;
use zbus::{MatchRule, MessageStream};

use crate::backend::KdeConnectBackend;
use crate::model::{ActionType, ConnectivityInfo, Device, DeviceType, Notification, PlayerInfo, ReceivedFile};

const ID: &str = "io.github.acemythos.Connect";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_RECEIVED_HISTORY: usize = 50;
const STATUS_TIMEOUT: Duration = Duration::from_secs(6);
const DISCOVERY_INTERVAL_OPEN: Duration = Duration::from_secs(30);
const DISCOVERY_INTERVAL_CLOSED: Duration = Duration::from_secs(120);

static POPUP_AUTOSIZE_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("cosmic-connect-popup"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareKind {
    Text,
    Url,
}

#[derive(Default)]
struct DeviceDraft {
    share_text: String,
    share_url: String,
    status: Option<String>,
    status_error: bool,
    status_at: Option<Instant>,
    notifications: Vec<Notification>,
    show_all_notifications: bool,
    selected_notification: Option<String>,
    notify_reply_text: String,
    player: Option<PlayerInfo>,
    connectivity: Option<ConnectivityInfo>,
    last_action: Option<ActionType>,
    anim_state: anim::State,
    expanded_share: Option<ShareKind>,
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
    next_auto_discovery: Option<Instant>,
    selected_device_id: Option<String>,
    device_list_open: bool,
    pending_unpair: Option<String>,
    unpair_error: Option<(String, String)>,
    advanced_device: Option<String>,
    received_files: HashMap<String, Vec<ReceivedFile>>,
    active_notifs: HashMap<String, u32>,
    last_notif_pct: HashMap<String, i32>,
    pending_notif_requests: HashSet<String>,
    deferred_notifs: HashMap<String, (String, String, String)>,
    notified_pair_ids: HashSet<String>,
    pairing_notifs: HashMap<u32, String>,
    last_sync: Option<Instant>,
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
    ShareTextChanged(String, String),
    ShareUrlChanged(String, String),
    ChooseFile(String),
    FileChooserFinished(String, Result<String, String>),
    DiscoverDevices,
    RefreshNotifications(String),
    NotificationsLoaded(String, Vec<Notification>),
    SelectNotification(String, String),
    NotifyReplyChanged(String, String),
    SendNotifyReply(String, String),
    DismissNotification(String, String),
    DismissAllNotifications(String),
    RefreshPlayer(String),
    PlayerInfoUpdated(String, Option<PlayerInfo>),
    MediaAction(String, String),
    SelectPlayer(String, String),
    SelectDevice(String),
    RequestUnpair(String),
    CancelUnpair,
    ConfirmUnpair,
    ToggleAdvanced(String),
    ToggleDeviceList,
    ToggleNotifications(String),
    ToggleShare(String, ShareKind),
    SendShareText(String),
    SendShareUrl(String),
    OpenReceivedFile(String, String),
    FileOpenFinished(String, Result<(), String>),
    ConnectivityUpdated(String, Option<ConnectivityInfo>),
    Tick,
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
        0 => "○○○○",
        1 => "●○○○",
        2 => "●●○○",
        3 => "●●●○",
        _ => "●●●●",
    }
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

fn parse_device_signal_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/modules/kdeconnect/devices/")?;
    let (device_id, plugin) = rest.split_once('/')?;
    if device_id.is_empty() || plugin.is_empty() || plugin.contains('/') {
        return None;
    }
    Some((device_id, plugin))
}

impl CosmicConnect {
    fn draft_mut(&mut self, device_id: &str) -> &mut DeviceDraft {
        self.drafts.entry(device_id.to_string()).or_default()
    }

    fn set_status(&mut self, device_id: &str, text: impl Into<String>, is_error: bool) {
        let draft = self.draft_mut(device_id);
        draft.status = Some(text.into());
        draft.status_error = is_error;
        draft.status_at = Some(Instant::now());
    }

    fn mark_files_read(&mut self, device_id: &str) {
        if let Some(files) = self.received_files.get_mut(device_id) {
            for file in files.iter_mut() {
                file.unread = false;
            }
        }
    }

    fn popup_parent_id(&self) -> Option<Id> {
        self.core.main_window_id()
    }

    fn panel_preview(&self) -> String {
        if self.backend.is_none() {
            return "Connecting…".into();
        }
        if let Some(device) = self.selected_device() {
            let net = self.drafts.get(&device.id).and_then(|d| d.connectivity.as_ref());
            if device.is_reachable {
                if let Some(bat) = &device.battery {
                    let charge_label = format!("{}%", bat.charge);
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
            } else if device.is_paired {
                format!("{} - Offline", device.name)
            } else {
                format!("{} - Not paired", device.name)
            }
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
        self.selected_device_id
            .as_deref()
            .and_then(|id| self.devices.iter().find(|d| d.id == *id))
    }

    fn render_device_selector(&self) -> Element<'_, Message> {
        let remote = self.selected_device();
        let remote_icon = remote.map(|d| d.device_type.icon_name()).unwrap_or("phone-symbolic");
        let remote_name = remote.map(|d| d.name.as_str()).unwrap_or("No device");
        let position = self
            .selected_device_id
            .as_deref()
            .and_then(|id| self.devices.iter().position(|d| d.id == *id))
            .map(|idx| idx + 1)
            .unwrap_or(0);
        let remote_sub = match remote {
            Some(d) if d.is_reachable => {
                if self.devices.len() > 1 {
                    format!("Connected · {position} of {}", self.devices.len())
                } else {
                    "Connected".into()
                }
            }
            Some(d) if d.is_paired => "Offline".into(),
            Some(_) => "Not paired".into(),
            None => "No devices".into(),
        };

        let select_msg = (self.devices.len() > 1).then_some(Message::ToggleDeviceList);

        crate::widgets::device_selector_card(
            "computer-symbolic",
            "Computer",
            "This computer",
            remote_icon,
            remote_name,
            remote_sub,
            true,
            select_msg,
        )
    }

    fn render_device_list(&self) -> Option<Element<'_, Message>> {
        if !self.device_list_open || self.devices.len() < 2 {
            return None;
        }

        let mut rows: Vec<Element<Message>> = vec![
            crate::widgets::section_header("Choose device").into(),
        ];
        for device in &self.devices {
            rows.push(
                crate::widgets::paired_device_row(
                    device.device_type.icon_name(),
                    device.name.as_str(),
                    device.is_reachable,
                    self.selected_device_id.as_deref() == Some(device.id.as_str()),
                    Some(Message::SelectDevice(device.id.clone())),
                    None,
                )
                .into(),
            );
        }

        Some(
            container(column::with_children(rows).spacing(4))
                .padding([4, 16, 8, 16])
                .into(),
        )
    }

    fn render_device_status_card<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        let draft = self.drafts.get(&device.id);
        let battery = device
            .battery
            .as_ref()
            .map(|battery| (battery.charge, battery.is_charging));
        let network = draft
            .and_then(|draft| draft.connectivity.as_ref())
            .map(|network| (network.network_type.as_str(), network.signal_strength));
        let updated = self.last_sync.map(|last| {
            let secs = last.elapsed().as_secs();
            if secs < 3 {
                "Updated just now".to_string()
            } else if secs < 60 {
                format!("Updated {secs}s ago")
            } else {
                format!("Updated {}m ago", secs / 60)
            }
        });

        crate::widgets::status_card(
            device.device_type.icon_name(),
            device.name.as_str(),
            device.is_reachable,
            battery,
            network,
            updated,
            Some(Message::ToggleAdvanced(device.id.clone())),
            Some(Message::RefreshDevices),
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
                "notification-alert-symbolic",
                "Ring",
                Message::PerformAction(device.id.clone(), ActionType::Ring),
                false,
            ));
        }

        if device.pair_state == 2 {
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
        } else if device.pair_state == 0 && !device.is_paired {
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

    fn render_other_devices_section(&self) -> Option<Element<'_, Message>> {
        let others: Vec<&Device> = self
            .devices
            .iter()
            .filter(|device| {
                (device.is_paired || device.pair_state == 3)
                    && self.selected_device_id.as_deref() != Some(device.id.as_str())
            })
            .collect();

        if others.is_empty() {
            return None;
        }

        let mut children: Vec<Element<Message>> = Vec::new();
        children.push(divider::horizontal::default().into());
        children.push(crate::widgets::section_header("Other devices").into());

        for device in others {
            children.push(
                crate::widgets::paired_device_row(
                    device.device_type.icon_name(),
                    device.name.as_str(),
                    device.is_reachable,
                    false,
                    Some(Message::SelectDevice(device.id.clone())),
                    None,
                )
                .into(),
            );
        }

        Some(column::with_children(children).spacing(4).into())
    }

    fn render_unpair_confirmation<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        container(
            column![
                text::body(format!("Unpair {}?", device.name)).size(SIZE_BODY),
                text::caption("This device will need to be paired again before it can reconnect.")
                    .size(SIZE_CAPTION),
                row![
                    crate::widgets::pill_button(
                        "dialog-cancel-symbolic",
                        "Cancel",
                        Message::CancelUnpair,
                        false,
                    ),
                    crate::widgets::danger_button(
                        "user-trash-symbolic",
                        "Unpair",
                        Message::ConfirmUnpair,
                    ),
                ]
                .spacing(6),
            ]
            .spacing(8)
            .align_x(Alignment::Center),
        )
        .style(glass_card)
        .padding(12)
        .width(Length::Fill)
        .into()
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

        None
    }

    fn render_notification_section<'a>(&'a self, device: &'a Device, draft: &'a DeviceDraft) -> Option<Element<'a, Message>> {
        if draft.notifications.is_empty() {
            return None;
        }

        let count = draft.notifications.len();
        let mut children: Vec<Element<Message>> = Vec::new();

        let mut header = row![
            text::body(format!("Notifications · {count}")).size(SIZE_BODY),
            container(row![]).width(Length::Fill),
        ]
        .align_y(Alignment::Center);

        if count > 1 {
            header = header.push(crate::widgets::text_button(
                if draft.show_all_notifications { "Show less" } else { "View all" },
                Message::ToggleNotifications(device.id.clone()),
            ));
        }
        children.push(header.into());

        let visible: Vec<&Notification> = if draft.show_all_notifications {
            draft.notifications.iter().collect()
        } else {
            draft.notifications.iter().take(1).collect()
        };

        for notif in visible {
            let is_selected = draft.selected_notification.as_deref() == Some(notif.internal_id.as_str());

            let mut body = column![
                text::caption(&notif.app_name).size(SIZE_CAPTION),
            ]
            .spacing(1);
            if !notif.title.is_empty() {
                body = body.push(text::body(&notif.title).size(SIZE_BODY));
            }
            if !notif.text.is_empty() {
                body = body.push(text::caption(&notif.text).size(SIZE_CAPTION));
            }

            let select = button::custom(
                row![
                    icon::from_name("dialog-information-symbolic").size(18),
                    body.width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SelectNotification(device.id.clone(), notif.internal_id.clone()))
            .class(theme::Button::Custom {
                active: Box::new(|focused, theme| button::Style {
                    background: None,
                    border_radius: RADIUS_CARD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused { crate::widgets::accent_color(theme) } else { Color::TRANSPARENT },
                    ..button::Style::new()
                }),
                hovered: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, HOVER_OVERLAY_ALPHA))),
                    border_radius: RADIUS_CARD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused { crate::widgets::accent_color(theme) } else { Color::TRANSPARENT },
                    ..button::Style::new()
                }),
                pressed: Box::new(|focused, theme| button::Style {
                    background: Some(Background::Color(on_overlay(theme, PRESSED_ALPHA))),
                    border_radius: RADIUS_CARD.into(),
                    border_width: if focused { 1.0 } else { 0.0 },
                    border_color: if focused { crate::widgets::accent_color(theme) } else { Color::TRANSPARENT },
                    ..button::Style::new()
                }),
                disabled: Box::new(|_theme| button::Style::new()),
            })
            .padding([10, 12])
            .width(Length::Fill);

            let mut notification_row = row![select].align_y(Alignment::Center);
            if notif.dismissable {
                notification_row = notification_row.push(crate::widgets::icon_button(
                    "window-close-symbolic",
                    "Dismiss notification",
                    Message::DismissNotification(device.id.clone(), notif.internal_id.clone()),
                ));
            }
            children.push(notification_row.into());

            if is_selected && !notif.reply_id.is_empty() {
                children.push(
                    container(
                        row![
                            text_input::text_input("Reply…", &draft.notify_reply_text)
                                .on_input({
                                    let did = device.id.clone();
                                    move |value| Message::NotifyReplyChanged(did.clone(), value)
                                })
                                .width(Length::Fill),
                            button::custom(text::caption("Send").size(SIZE_CAPTION))
                                .on_press(Message::SendNotifyReply(device.id.clone(), notif.internal_id.clone()))
                                .padding([6, 12]),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([2, 12, 6, 12])
                    .into(),
                );
            }
        }

        if draft.show_all_notifications && count > 1 {
            children.push(
                row![
                    container(row![]).width(Length::Fill),
                    crate::widgets::text_button(
                        "Clear all",
                        Message::DismissAllNotifications(device.id.clone()),
                    ),
                ]
                .align_y(Alignment::Center)
                .into(),
            );
        }

        Some(column::with_children(children).spacing(2).into())
    }


    fn render_share_section<'a>(&'a self, device: &'a Device, draft: &'a DeviceDraft) -> Option<Element<'a, Message>> {
        let has_clipboard = device.has_plugin("kdeconnect_clipboard");
        let has_share = device.has_plugin("kdeconnect_share");
        if !has_clipboard && !has_share {
            return None;
        }

        let mut rows: Vec<Element<Message>> = Vec::new();
        rows.push(crate::widgets::section_header("Send").into());

        let mut list: Vec<Element<Message>> = Vec::new();
        if has_clipboard {
            list.push(crate::widgets::list_row(
                "edit-paste-symbolic",
                "Send clipboard",
                Message::PerformAction(device.id.clone(), ActionType::SendClipboard),
            ));
        }
        if has_share {
            list.push(crate::widgets::list_row(
                "document-send-symbolic",
                "Send file",
                Message::ChooseFile(device.id.clone()),
            ));
            list.push(crate::widgets::list_row(
                "document-edit-symbolic",
                "Send text",
                Message::ToggleShare(device.id.clone(), ShareKind::Text),
            ));
            if draft.expanded_share == Some(ShareKind::Text) {
                list.push(
                    container(
                        row![
                            text_input::text_input("Message to send…", &draft.share_text)
                                .on_input({
                                    let did = device.id.clone();
                                    move |value| Message::ShareTextChanged(did.clone(), value)
                                })
                                .width(Length::Fill),
                            crate::widgets::pill_button(
                                "document-send-symbolic",
                                "Send",
                                Message::SendShareText(device.id.clone()),
                                false,
                            ),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([0, 12, 10, 12])
                    .into(),
                );
            }
            list.push(crate::widgets::list_row(
                "insert-link-symbolic",
                "Send URL",
                Message::ToggleShare(device.id.clone(), ShareKind::Url),
            ));
            if draft.expanded_share == Some(ShareKind::Url) {
                list.push(
                    container(
                        row![
                            text_input::text_input("https://…", &draft.share_url)
                                .on_input({
                                    let did = device.id.clone();
                                    move |value| Message::ShareUrlChanged(did.clone(), value)
                                })
                                .width(Length::Fill),
                            crate::widgets::pill_button(
                                "document-send-symbolic",
                                "Send",
                                Message::SendShareUrl(device.id.clone()),
                                false,
                            ),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([0, 12, 10, 12])
                    .into(),
                );
            }
        }

        rows.push(
            container(column::with_children(list).spacing(0))
                .style(card_default)
                .width(Length::Fill)
                .into(),
        );

        Some(column::with_children(rows).spacing(4).into())
    }

    fn render_advanced_section<'a>(&'a self, device: &'a Device, draft: &'a DeviceDraft) -> Element<'a, Message> {
        if self.advanced_device.as_deref() != Some(&device.id) {
            return crate::widgets::disclosure_row(
                "Connection details",
                false,
                Message::ToggleAdvanced(device.id.clone()),
            );
        }

        let mut children: Vec<Element<Message>> = Vec::new();

        children.push(crate::widgets::disclosure_row(
            "Connection details",
            true,
            Message::ToggleAdvanced(device.id.clone()),
        ));

        let mut metadata = column![
            row![
                icon::from_name(device.device_type.icon_name()).size(16),
                text::caption("Type").size(SIZE_CAPTION),
                container(row![]).width(Length::Fill),
                text::body(device.device_type.label()).size(SIZE_CAPTION),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8);

        if let Some(battery) = &device.battery {
            metadata = metadata.push(
                row![
                    icon::from_name("battery-symbolic").size(16),
                    text::caption("Battery").size(SIZE_CAPTION),
                    container(row![]).width(Length::Fill),
                    text::body(format!(
                        "{}{}",
                        format!("{}%", battery.charge),
                        if battery.is_charging { " · Charging" } else { "" }
                    ))
                    .size(SIZE_CAPTION),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        if let Some(network) = draft.connectivity.as_ref() {
            metadata = metadata.push(
                row![
                    icon::from_name("network-cellular-symbolic").size(16),
                    text::caption("Network").size(SIZE_CAPTION),
                    container(row![]).width(Length::Fill),
                    text::body(format!(
                        "{} · {}/4",
                        network.network_type, network.signal_strength
                    ))
                    .size(SIZE_CAPTION),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        metadata = metadata.push(
            row![
                icon::from_name("application-x-addon-symbolic").size(16),
                text::caption("Plugins").size(SIZE_CAPTION),
                container(row![]).width(Length::Fill),
                text::body(format!(
                    "{} loaded · {} supported",
                    device.loaded_plugins.len(),
                    device.supported_plugins.len()
                ))
                .size(SIZE_CAPTION),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        children.push(
            container(metadata)
                .style(glass_card)
                .padding([10, 12])
                .width(Length::Fill)
                .into(),
        );

        if let Some(files) = self.received_files.get(&device.id) {
            if !files.is_empty() {
                children.push(divider::horizontal::default().into());
                children.push(crate::widgets::section_header("Received files").into());

                for rf in files.iter().rev().take(10) {
                    if rf.active {
                        children.push(
                            container(
                                container(
                                    column![
                                        row![
                                            icon::from_name("document-save-symbolic").size(14),
                                            text::caption(&rf.file_name).size(SIZE_CAPTION),
                                            container(row![]).width(Length::Fill),
                                            text::caption(format!("{}%", rf.progress)).size(SIZE_CAPTION),
                                        ]
                                        .spacing(6)
                                        .align_y(Alignment::Center),
                                        progress_bar::determinate_linear(
                                            (rf.progress as f32 / 100.0).clamp(0.0, 1.0)
                                        )
                                        .width(Length::Fill)
                                        .girth(4),
                                    ]
                                    .spacing(4),
                                )
                                .style(glass_card)
                            )
                            .padding([4, 0])
                            .width(Length::Fill)
                            .into(),
                        );
                    } else {
                        let mut file_row = row![
                            icon::from_name("document-save-symbolic").size(14),
                            column![
                                text::caption(&rf.file_name).size(SIZE_CAPTION),
                                text::caption(truncate_text(&rf.file_path, 42)).size(SIZE_CAPTION),
                            ]
                            .spacing(1)
                            .width(Length::Fill),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center);

                        if !rf.file_path.is_empty() {
                            file_row = file_row.push(crate::widgets::text_button(
                                "Open",
                                Message::OpenReceivedFile(device.id.clone(), rf.file_path.clone()),
                            ));
                        }

                        children.push(
                            container(file_row)
                                .style(glass_card)
                                .padding([8, 10])
                                .width(Length::Fill)
                                .into(),
                        );
                    }
                }
            }
        }

        children.push(divider::horizontal::default().into());

        let discover_label = if self.is_discovering {
            "Discovering…"
        } else {
            "Discover"
        };
        children.push(
            container(
                row![
                    crate::widgets::pill_button("view-refresh-symbolic", "Refresh", Message::RefreshDevices, false),
                    crate::widgets::pill_button("network-wireless-symbolic", discover_label, Message::DiscoverDevices, self.is_discovering),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([4, 0])
            .into(),
        );

        if device.is_paired || device.pair_state == 3 {
            children.push(divider::horizontal::default().into());
            if self.pending_unpair.as_deref() == Some(device.id.as_str()) {
                children.push(self.render_unpair_confirmation(device));
            } else {
                children.push(
                    container(crate::widgets::danger_button(
                        "user-trash-symbolic",
                        "Unpair device",
                        Message::RequestUnpair(device.id.clone()),
                    ))
                    .padding([4, 0])
                    .into(),
                );
            }
        }

        if let Some((id, error)) = &self.unpair_error {
            if id == &device.id {
                children.push(
                    container(
                        row![
                            icon::from_name("dialog-error-symbolic").size(14),
                            text::caption(error).size(SIZE_CAPTION),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .style(glass_card)
                    .padding([8, 10])
                    .width(Length::Fill)
                    .into(),
                );
            }
        }

        let anim_progress = draft.anim_state.t(Duration::from_millis(250), true);
        let eased = anim::smootherstep(anim_progress);

        container(column::with_children(children).spacing(0))
            .style(move |theme: &cosmic::Theme| {
                let max_alpha = 0.80;
                let bg_alpha = max_alpha * eased;
                let border_alpha = BORDER_GLASS_ALPHA * eased;
                let base: Color = theme.cosmic().background(theme.transparent).base.into();
                let on: Color = theme.cosmic().background(theme.transparent).on.into();
                iced_container::Style {
                    background: Some(Background::Color(Color::from_rgba(
                        base.r * CARD_LUM_DARKEN,
                        base.g * CARD_LUM_DARKEN,
                        base.b * CARD_LUM_DARKEN,
                        bg_alpha,
                    ))),
                    border: Border {
                        radius: RADIUS_CARD.into(),
                        width: 1.0,
                        color: Color::from_rgba(on.r, on.g, on.b, border_alpha),
                    },
                    ..Default::default()
                }
            })
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
                    if self.popup_parent_id().is_none() {
                        self.error = Some("Applet window not ready yet. Please try again.".into());
                        return Task::none();
                    }
                    self.device_list_open = false;
                    if let Some(device_id) = self.selected_device_id.clone() {
                        self.mark_files_read(&device_id);
                    }

                    surface::surface_task(surface::action::app_popup(
                        |_| surface::action::LiveSettings::default(),
                        |app: &mut Self| {
                            let parent_id = app.popup_parent_id().expect("checked before creating popup");
                            let new_id = Id::unique();
                            app.popup.replace(new_id);
                            let mut popup_settings = app.core.applet.get_popup_settings(
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
                            popup_settings
                        },
                        None,
                    ))
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
                        let devices = backend
                            .devices()
                            .await
                            .map_err(|error| format!("devices: {error}"))?;
                        let pairing = backend.pairing_request_ids().await;
                        Ok::<_, String>((devices, pairing))
                    },
                    |result| match result {
                        Ok((devices, pairing)) => Message::DevicesUpdated(devices, pairing),
                        Err(error) => Message::BackendError(error),
                    },
                )
                .map(cosmic::Action::App);
            }
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                }
            }
            Message::BackendReady(backend) => {
                log::info!("Backend ready");
                self.backend = Some(backend);
                self.error = None;
                return Task::perform(async {}, |_| Message::RefreshDevices)
                    .map(cosmic::Action::App);
            }
            Message::DevicesUpdated(devices, pairing_ids) => {
                log::debug!("DevicesUpdated: {} devices, {} pairing requests", devices.len(), pairing_ids.len());
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
                for old in &self.devices {
                    if let Some(new) = merged.iter().find(|d| d.id == old.id) {
                        if old.is_reachable != new.is_reachable {
                            log::info!("Device '{}' {} (paired={})", new.name, if new.is_reachable { "became reachable" } else { "became unreachable" }, new.is_paired);
                        }
                    } else {
                        log::debug!("Device '{}' removed", old.name);
                    }
                }
                for new in &merged {
                    if !self.devices.iter().any(|d| d.id == new.id) {
                        log::debug!("Device '{}' added (reachable={})", new.name, new.is_reachable);
                    }
                }
                self.devices = merged;
                self.last_sync = Some(std::time::Instant::now());
                if let Some(pending) = &self.pending_unpair {
                    if !self
                        .devices
                        .iter()
                        .any(|d| &d.id == pending && (d.is_paired || d.pair_state == 3))
                    {
                        self.pending_unpair = None;
                    }
                }
                let selection_ok = self
                    .selected_device_id
                    .as_deref()
                    .map(|id| self.devices.iter().any(|d| d.id == *id))
                    .unwrap_or(false);
                if !selection_ok && !self.devices.is_empty() {
                    self.selected_device_id = self
                        .devices
                        .iter()
                        .find(|d| d.is_reachable)
                        .or_else(|| self.devices.first())
                        .map(|d| d.id.clone());
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
                        log::info!("Pairing request from '{dev_name}' ({pid})");
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
                let selected_id = self.selected_device_id.clone();
                let popup_open = self.popup.is_some();
                for device in &self.devices {
                    if !device.is_reachable { continue; }
                    let selected = selected_id.as_deref() == Some(device.id.as_str());

                    if selected && popup_open && device.has_plugin("kdeconnect_notifications") {
                        let Some(backend) = self.backend.clone() else { continue; };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(Task::perform(
                            async move { backend.fetch_notifications(&did).await },
                            move |n| Message::NotificationsLoaded(did2, n),
                        ).map(cosmic::Action::App));
                    }
                    if selected && popup_open && device.has_plugin("kdeconnect_mprisremote") {
                        let Some(backend) = self.backend.clone() else { continue; };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(Task::perform(
                            async move { backend.player_info(&did).await },
                            move |p| Message::PlayerInfoUpdated(did2, p),
                        ).map(cosmic::Action::App));
                    }
                    if selected && device.has_plugin("kdeconnect_connectivity_report") {
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
                let now = Instant::now();
                if has_reachable {
                    self.auto_discovering = false;
                    self.next_auto_discovery = None;
                } else if self.next_auto_discovery.map_or(true, |t| now >= t) {
                    log::debug!("No reachable devices, triggering auto-discovery");
                    self.auto_discovering = true;
                    self.next_auto_discovery = Some(
                        now + if popup_open { DISCOVERY_INTERVAL_OPEN } else { DISCOVERY_INTERVAL_CLOSED }
                    );
                    if let Some(backend) = self.backend.clone() {
                        tasks = tasks.chain(Task::perform(
                            async move {
                                if let Err(error) = backend.force_discovery().await {
                                    log::warn!("Auto-discovery failed: {error}");
                                }
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
                        if let Err(error) = backend.force_discovery().await {
                            log::warn!("Discovery request failed: {error}");
                        }
                        tokio::time::sleep(Duration::from_secs(4)).await;
                    },
                    |_| {
                        Message::RefreshDevices
                    },
                )
                .map(cosmic::Action::App);
            }
            Message::BackendError(error) => {
                log::error!("Backend error: {error}");
                self.error = Some(error);
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
                        self.set_status(&device_id, format!("Sending {fname}…"), false);
                        return Task::perform(
                            async move {},
                            move |_| Message::PerformAction(did, ActionType::SendFile(path)),
                        ).map(cosmic::Action::App);
                    }
                    Err(error) => {
                        if error.to_lowercase().contains("cancel") {
                            let draft = self.draft_mut(&device_id);
                            draft.status = None;
                            draft.status_error = false;
                            draft.status_at = None;
                        } else {
                            self.set_status(&device_id, error, true);
                        }
                    }
                }
            }
            Message::PerformAction(device_id, action) => {
                let Some(backend) = self.backend.clone() else {
                    self.set_status(&device_id, "KDE Connect backend unavailable", true);
                    return Task::none();
                };

                let draft = self.draft_mut(&device_id);
                draft.status = Some("Working…".into());
                draft.status_error = false;
                draft.status_at = Some(Instant::now());
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
                let last_action = draft.last_action.take();
                let was_unpair = matches!(last_action, Some(ActionType::Unpair));
                let quiet = matches!(
                    last_action,
                    Some(ActionType::MediaAction(_))
                        | Some(ActionType::SelectPlayer(_))
                        | Some(ActionType::DismissNotification(_))
                        | Some(ActionType::ReplyToNotification(_, _))
                        | Some(ActionType::BrowseFiles)
                );
                let action_label = last_action.as_ref().map(|a| a.label()).unwrap_or("Action").to_string();
                let fname = last_action.as_ref()
                    .and_then(|a| if let ActionType::SendFile(p) = a { Some(p.clone()) } else { None })
                    .and_then(|p| p.rsplit('/').next().map(|s| s.to_string()))
                    .unwrap_or_default();
                match &result {
                    Ok(message) => log::info!("Action '{action_label}' on '{device_name}' succeeded: {message}"),
                    Err(error) => log::error!("Action '{action_label}' on '{device_name}' failed: {error}"),
                }
                if result.is_ok() {
                    match last_action {
                        Some(ActionType::ShareText(_)) => self.draft_mut(&device_id).share_text.clear(),
                        Some(ActionType::ShareUrl(_)) => self.draft_mut(&device_id).share_url.clear(),
                        _ => {}
                    }
                }
                let status_text = match &result {
                    Ok(message) => message.clone(),
                    Err(error) => error.clone(),
                };
                self.set_status(&device_id, status_text, result.is_err());

                if was_unpair {
                    if self.pending_unpair.as_deref() == Some(device_id.as_str()) {
                        self.pending_unpair = None;
                    }
                    match &result {
                        Ok(_) => self.unpair_error = None,
                        Err(error) => {
                            self.unpair_error =
                                Some((device_id.clone(), format!("Failed to unpair: {error}")));
                        }
                    }
                }

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
                        if !quiet {
                            if let Some(backend) = backend {
                                let _ = backend.notify(
                                    &format!("{device_name}"),
                                    &notif_msg,
                                    0,
                                    &[],
                                ).await;
                            }
                        }
                    },
                    |_| Message::RefreshDevices,
                ).map(cosmic::Action::App);
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
                log::debug!("SendNotifyReply: notif {internal_id}, {} chars", text.len());
                let action = ActionType::ReplyToNotification(internal_id, text);
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, action),
                )
                .map(cosmic::Action::App);
            }
            Message::DismissNotification(device_id, internal_id) => {
                log::debug!("DismissNotification: notif {internal_id} on {device_id}");
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, ActionType::DismissNotification(internal_id)),
                )
                .map(cosmic::Action::App);
            }
            Message::DismissAllNotifications(device_id) => {
                let backend = self.backend.clone();
                let notifs = self.draft_mut(&device_id).notifications.clone();
                self.draft_mut(&device_id).notifications.clear();
                log::debug!("DismissAllNotifications: {} notifications on {device_id}", notifs.len());
                let Some(backend) = backend else { return Task::none() };
                return Task::perform(
                    async move {
                        for n in notifs {
                            if let Err(error) = backend
                                .dismiss_notification(&device_id, &n.internal_id)
                                .await
                            {
                                log::warn!(
                                    "Failed to dismiss notification '{}' on {device_id}: {error}",
                                    n.internal_id
                                );
                            }
                        }
                    },
                    |_| Message::NoOp,
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
                log::info!("TransferStarted: '{file_name}' ({total_bytes} bytes) from {device_id} (transfer {transfer_id})");
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
                log::debug!("TransferProgress: {transfer_id} at {percent}%");
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
                log::info!("TransferFinished: {file_path} from {device_id} (transfer {transfer_id})");
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
                log::info!("FileReceived: {file_path} from {device_id}");
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
                log::info!("PingReceived from {device_id}");
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
                log::debug!("ClipboardReceived from {device_id} ({} chars)", content.len());
                let device_name = self.devices.iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| device_id.clone());
                let preview = truncate_text(&content, 80);
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
                    log::info!("Pairing request notification created for '{key}' (notif {notif_id})");
                    self.pairing_notifs.insert(notif_id, key);
                }
            }
            Message::NotifAction(notif_id, action_key) => {
                if let Some(device_id) = self.pairing_notifs.remove(&notif_id) {
                    log::info!("Pairing action '{action_key}' for {device_id}");
                    self.notified_pair_ids.remove(&device_id);
                    if let Some(backend) = self.backend.clone() {
                        return Task::perform(
                            async move {
                                let result = match action_key.as_str() {
                                    "accept" => backend.accept_pairing(&device_id).await,
                                    "reject" => backend.cancel_pairing(&device_id).await,
                                    _ => Ok(()),
                                };
                                if let Err(error) = result {
                                    log::warn!("Pairing action '{action_key}' failed: {error}");
                                }
                            },
                            |_| Message::RefreshDevices,
                        ).map(cosmic::Action::App);
                    }
                }
            }
            Message::NoOp => {}
            Message::SelectDevice(id) => {
                if self.devices.iter().any(|d| d.id == id) {
                    self.selected_device_id = Some(id);
                    self.advanced_device = None;
                    self.device_list_open = false;
                }
            }
            Message::ToggleDeviceList => {
                self.device_list_open = !self.device_list_open;
            }
            Message::ToggleNotifications(device_id) => {
                let draft = self.draft_mut(&device_id);
                draft.show_all_notifications = !draft.show_all_notifications;
            }
            Message::ToggleShare(device_id, kind) => {
                let draft = self.draft_mut(&device_id);
                draft.expanded_share = if draft.expanded_share == Some(kind) {
                    None
                } else {
                    Some(kind)
                };
            }
            Message::SendShareText(device_id) => {
                let text = self.draft_mut(&device_id).share_text.trim().to_string();
                if text.is_empty() {
                    return Task::none();
                }
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, ActionType::ShareText(text)),
                )
                .map(cosmic::Action::App);
            }
            Message::SendShareUrl(device_id) => {
                let url = self.draft_mut(&device_id).share_url.trim().to_string();
                if url.is_empty() {
                    return Task::none();
                }
                return Task::perform(
                    async {},
                    move |_| Message::PerformAction(device_id, ActionType::ShareUrl(url)),
                )
                .map(cosmic::Action::App);
            }
            Message::OpenReceivedFile(device_id, path) => {
                let path_for_task = path.clone();
                return Task::perform(
                    async move {
                        tokio::process::Command::new("xdg-open")
                            .arg(path_for_task)
                            .spawn()
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    },
                    move |result| match result {
                        Ok(()) => Message::NoOp,
                        Err(error) => Message::FileOpenFinished(
                            device_id.clone(),
                            Err(format!("Open failed: {error}")),
                        ),
                    },
                )
                .map(cosmic::Action::App);
            }
            Message::FileOpenFinished(device_id, result) => {
                match result {
                    Ok(()) => {}
                    Err(error) => self.set_status(&device_id, error, true),
                }
            }
            Message::RequestUnpair(device_id) => {
                self.unpair_error = None;
                self.pending_unpair = Some(device_id);
            }
            Message::CancelUnpair => {
                self.pending_unpair = None;
            }
            Message::ConfirmUnpair => {
                let Some(device_id) = self.pending_unpair.take() else {
                    return Task::none();
                };
                self.unpair_error = None;
                let still_paired = self
                    .devices
                    .iter()
                    .any(|d| d.id == device_id && (d.is_paired || d.pair_state == 3));
                if !still_paired {
                    return Task::none();
                }
                return self.update(Message::PerformAction(device_id, ActionType::Unpair));
            }
            Message::ToggleAdvanced(id) => {
                let anim_id = id.clone();
                if self.advanced_device.as_deref() == Some(&id) {
                    self.advanced_device = None;
                } else {
                    self.mark_files_read(&id);
                    self.advanced_device = Some(id);
                }
                self.draft_mut(&anim_id).anim_state.changed(Duration::from_millis(250));
            }
            Message::Tick => {}
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let suggested = self.core.applet.suggested_size(false);
        let icon_name = if self.selected_device().map(|d| d.is_reachable).unwrap_or(false) {
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

    let preview = text::caption(self.panel_preview()).size(SIZE_CAPTION);
    let mut content = row![icon, preview]
        .spacing(6)
        .align_y(Alignment::Center);

    if unread > 0 {
        content = content.push(
            text::caption(format!("({unread})")).size(SIZE_CAPTION)
        );
    }

    let coated = container(content)
        .padding([5, 8, 0, 8])
        .style(glass_coating_style);

    let btn = self.core
        .applet
        .button_from_element(coated, true)
        .width(Length::Shrink)
        .on_press_down(Message::TogglePopup);

        self.core.applet.autosize_window(btn).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        content.push(
            container(self.render_device_selector())
                .padding([8, 16, 0, 16])
                .into(),
        );

        if let Some(device_list) = self.render_device_list() {
            content.push(device_list);
        }

        content.push(divider::horizontal::default().into());

        if let Some(err) = &self.error {
            let is_backend_err = err.contains("D-Bus") || err.contains("backend");
            content.push(
                container(
                    column![
                        icon::from_name(if is_backend_err { "computer-symbolic" } else { "dialog-warning-symbolic" }).size(32),
                        text::body(if is_backend_err { "Backend unavailable" } else { "Error" }).size(SIZE_BODY),
                        text::caption(err).size(SIZE_CAPTION),
                        if is_backend_err {
                            Element::from(crate::widgets::pill_button("view-refresh-symbolic", "Retry", Message::RefreshDevices, false))
                        } else {
                            row![].into()
                        },
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .padding(24)
                .width(Length::Fill)
                .into(),
            );
        } else if self.backend.is_none() {
            content.push(
                container(
                    column![
                        icon::from_name("process-working-symbolic").size(40),
                        text::body("Connecting to KDE Connect…").size(SIZE_BODY),
                        text::caption("Waiting for the session bus and daemon.").size(SIZE_CAPTION),
                    ]
                    .spacing(10)
                    .align_x(Alignment::Center),
                )
                .padding(32)
                .width(Length::Fill)
                .into(),
            );
        } else if self.devices.is_empty() {
            let is_discovering = self.is_discovering || self.auto_discovering;
            content.push(
                container(
                    column![
                        icon::from_name("network-wireless-symbolic").size(48),
                        text::body("No devices found"),
                        text::caption(
                            "Make sure KDE Connect is installed\nand a device is on the same network."
                        ).size(SIZE_CAPTION),
                        crate::widgets::pill_button("view-refresh-symbolic", "Find Devices", Message::DiscoverDevices, is_discovering),
                    ]
                    .spacing(12)
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

                if device.is_reachable || device.is_paired {
                    if let Some(qa) = self.render_quick_action_row(device) {
                        content.push(container(qa).padding([8, 0, 0, 0]).into());
                    }
                }

                if device.is_reachable {
                    if let Some(banner) = self.render_info_banner(device, draft) {
                        content.push(container(banner).padding([8, 0]).into());
                    }

                    if let Some(notif_section) = self.render_notification_section(device, draft) {
                        content.push(container(notif_section).padding([8, 0]).into());
                    }

                    if let Some(share_section) = self.render_share_section(device, draft) {
                        content.push(container(share_section).padding([8, 0]).into());
                    }
                }

                content.push(self.render_advanced_section(device, draft));
            }
        }

        if let Some(section) = self.render_other_devices_section() {
            content.push(container(section).padding([8, 0, 0, 0]).into());
        }

        if let Some(device) = self.selected_device() {
            if let Some(draft) = self.drafts.get(&device.id) {
                if let Some(status) = &draft.status {
                    let visible = draft
                        .status_at
                        .map(|at| at.elapsed() < STATUS_TIMEOUT)
                        .unwrap_or(false);
                    if visible {
                        let is_error = draft.status_error;
                        let is_working = status.starts_with("Working");
                        let icon_name = if is_working {
                            "process-working-symbolic"
                        } else if is_error {
                            "dialog-error-symbolic"
                        } else {
                            "emblem-default-symbolic"
                        };
                        content.push(
                            container(
                                row![
                                    icon::from_name(icon_name).size(14),
                                    text::caption(status).size(SIZE_CAPTION),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center),
                            )
                            .style(move |theme: &cosmic::Theme| {
                                let tone = if is_error {
                                    crate::widgets::destructive_color(theme)
                                } else {
                                    crate::widgets::accent_color(theme)
                                };
                                iced_container::Style {
                                    background: Some(Background::Color(frosted_bg(
                                        theme,
                                        CARD_LUM_DARKEN,
                                        OPACITY_CARD,
                                    ))),
                                    border: Border {
                                        radius: RADIUS_MD.into(),
                                        width: 1.0,
                                        color: Color::from_rgba(tone.r, tone.g, tone.b, 0.35),
                                    },
                                    ..Default::default()
                                }
                            })
                            .padding([8, 12])
                            .width(Length::Fill)
                            .into(),
                        );
                    }
                }
            }
        }

        let body = column::with_children(content).spacing(0);

        let edge_highlight = container(
            row![]
        )
        .style(|theme: &cosmic::Theme| {
            let on: Color = theme.cosmic().background(theme.transparent).on.into();
            iced_container::Style {
                background: Some(Background::Gradient(Gradient::Linear(
                    gradient::Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, Color::from_rgba(on.r, on.g, on.b, 0.08))
                        .add_stop(1.0, Color::from_rgba(on.r, on.g, on.b, 0.0)),
                ))),
                ..Default::default()
            }
        })
        .height(2.0)
        .width(Length::Fill);

        let panel = scrollable(
            column![
                edge_highlight,
                container(body)
                    .padding([16, 16])
                    .width(Length::Fill),
            ]
            .spacing(0),
        )
        .height(Length::Shrink)
        .width(Length::Fill);

        autosize(
            container(panel)
                .style(|theme: &cosmic::Theme| {
                    let c = theme.cosmic();
                    let base: Color = c.background(theme.transparent).base.into();
                    let on: Color = c.background(theme.transparent).on.into();
                    let (a0, a1, a2) = if self.core.frosted(c) {
                        (0.35, 0.45, 0.55)
                    } else {
                        (0.72, 0.85, 0.92)
                    };
                    let bg = Background::Gradient(Gradient::Linear(
                        gradient::Linear::new(std::f32::consts::PI)
                            .add_stop(0.0, Color::from_rgba(base.r, base.g, base.b, a0))
                            .add_stop(0.5, Color::from_rgba(base.r * 0.96, base.g * 0.96, base.b * 0.96, a1))
                            .add_stop(1.0, Color::from_rgba(base.r * 0.92, base.g * 0.92, base.b * 0.92, a2)),
                    ));
                    iced_container::Style {
                        background: Some(bg),
                        border: Border {
                            radius: RADIUS_CARD.into(),
                            width: 1.0,
                            color: Color::from_rgba(on.r, on.g, on.b, BORDER_GLASS_ALPHA),
                        },
                        shadow: Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.40),
                            offset: Vector::new(0.0, 16.0),
                            blur_radius: 40.0,
                        },
                        text_color: Some(on),
                        icon_color: Some(on),
                        ..Default::default()
                    }
                })
                .height(Length::Shrink)
                .width(Length::Fill),
            POPUP_AUTOSIZE_ID.clone(),
        )
        .limits(
            Limits::NONE
                .min_height(1.)
                .min_width(360.0)
                .max_width(360.0)
                .max_height(1000.0),
        )
        .into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Message> {
        let inner = Subscription::run_with(std::any::TypeId::of::<()>(), |_state| {
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
        });

        let mut subscriptions = vec![inner];

        if self.advanced_device.is_some() {
            subscriptions.push(
                time::every(Duration::from_millis(50)).map(|_| Message::Tick)
            );
        } else if self.popup.is_some() {
            subscriptions.push(
                time::every(Duration::from_millis(500)).map(|_| Message::Tick)
            );
        }

        Subscription::batch(subscriptions)
    }
}

fn glass_coating_style(theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(frosted_bg(theme, CARD_LUM_DARKEN, OPACITY_COATING))),
        border: Border {
            radius: RADIUS_CARD.into(),
            width: 1.0,
            color: on_overlay(theme, PRESSED_ALPHA),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, SHADOW_PANEL_ALPHA),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

fn glass_card(theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(frosted_bg(theme, CARD_LUM_DARKEN, OPACITY_CARD))),
        border: Border {
            radius: RADIUS_CARD.into(),
            width: 1.0,
            color: on_overlay(theme, BORDER_GLASS_ALPHA),
        },
        ..Default::default()
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
        let devices = backend
            .devices()
            .await
            .map_err(|error| format!("devices: {error}"))?;
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
                    let Some((device_id, plugin)) = parse_device_signal_path(path_str) else {
                        continue;
                    };
                    let device_id = device_id.to_string();
                    let plugin = plugin.to_string();
                    let member = match header.member() {
                        Some(m) => m.to_string(),
                        None => continue,
                    };

                    match (plugin.as_str(), member.as_str()) {
                        ("share", "shareReceived") => {
                            if let Ok(file_path) = msg.body().deserialize::<String>() {
                                return Message::FileReceived(device_id, file_path);
                            }
                        }
                        ("share", "transferStarted") => {
                            if let Ok((transfer_id, file_name, total_bytes)) =
                                msg.body().deserialize::<(String, String, u64)>()
                            {
                                return Message::TransferStarted(
                                    device_id, transfer_id, file_name, total_bytes,
                                );
                            }
                        }
                        ("share", "transferProgress") => {
                            if let Ok((transfer_id, transferred, total, percent)) =
                                msg.body().deserialize::<(String, u64, u64, i32)>()
                            {
                                return Message::TransferProgress(
                                    device_id, transfer_id, transferred, total, percent,
                                );
                            }
                        }
                        ("share", "transferFinished") => {
                            if let Ok((transfer_id, url)) =
                                msg.body().deserialize::<(String, String)>()
                            {
                                return Message::TransferFinished(
                                    device_id, transfer_id, url,
                                );
                            }
                        }
                        ("share", "transferFailed") => {
                            if let Ok((transfer_id, code, error_str)) =
                                msg.body().deserialize::<(String, i32, String)>()
                            {
                                return Message::TransferFailed(
                                    device_id, transfer_id, code, error_str,
                                );
                            }
                        }
                        ("ping", "pingReceived") => {
                            return Message::PingReceived(device_id);
                        }
                        ("clipboard", "clipboardReceived") => {
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
        .map(|uri| uri.to_string())
        .ok_or_else(|| "No file selected".to_string())
}



#[cfg(test)]
mod tests {
    use super::{parse_device_signal_path, truncate_text, CosmicConnect};

    #[test]
    fn popup_parent_id_is_none_before_window_is_ready() {
        let app = CosmicConnect::default();
        assert!(app.popup_parent_id().is_none());
    }

    #[test]
    fn parses_share_ping_and_clipboard_signal_paths() {
        assert_eq!(
            parse_device_signal_path("/modules/kdeconnect/devices/abc/share"),
            Some(("abc", "share"))
        );
        assert_eq!(
            parse_device_signal_path("/modules/kdeconnect/devices/abc/ping"),
            Some(("abc", "ping"))
        );
        assert_eq!(
            parse_device_signal_path("/modules/kdeconnect/devices/abc/clipboard"),
            Some(("abc", "clipboard"))
        );
    }

    #[test]
    fn rejects_nested_notification_signal_paths() {
        assert_eq!(
            parse_device_signal_path("/modules/kdeconnect/devices/abc/notifications/7"),
            None
        );
        assert_eq!(
            parse_device_signal_path("/modules/kdeconnect/notifications/abc/share"),
            None
        );
    }

    #[test]
    fn truncates_on_character_boundaries() {
        assert_eq!(truncate_text("héllo wörld", 5), "héllo…");
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("", 5), "");
    }
}
