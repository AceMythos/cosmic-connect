use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use ashpd::desktop::file_chooser::SelectedFiles;
use cosmic::app::Core;
use cosmic::cosmic_config::{Config as CosmicConfig, ConfigGet, ConfigSet};
use cosmic::iced::core::Alignment;
use cosmic::iced::gradient;
use cosmic::iced::platform_specific::shell::commands::popup::destroy_popup;
use cosmic::iced::time;
use cosmic::iced::window::Id;
use cosmic::iced::{
    Background, Border, Color, Gradient, Length, Limits, Shadow, Subscription, Vector,
};
use cosmic::surface;
use cosmic::theme;
use cosmic::widget::autosize::autosize;
use cosmic::widget::container as iced_container;
use cosmic::widget::text_input;
use cosmic::widget::{
    button, column, container, divider, icon, progress_bar, row, scrollable, settings, slider, text,
    toggler, tooltip,
};
use cosmic::{Action, Element, Task};

use crate::widgets::{
    frosted_bg, on_overlay, BORDER_GLASS_ALPHA, CARD_LUM_DARKEN, OPACITY_CARD, OPACITY_COATING,
    PRESSED_ALPHA, RADIUS_CARD, RADIUS_MD, SHADOW_PANEL_ALPHA, SIZE_BODY, SIZE_CAPTION,
    SIZE_HEADING,
};
use futures_util::stream::unfold;
use futures_util::StreamExt;
use zbus::{MatchRule, MessageStream};

use crate::backend::KdeConnectBackend;
use crate::model::{
    ActionType, ConnectivityInfo, Device, DeviceType, Notification, PlayerInfo, ReceivedFile,
};

const ID: &str = "io.github.acemythos.Connect";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_RECEIVED_HISTORY: usize = 50;
const STATUS_TIMEOUT: Duration = Duration::from_secs(6);
const DISCOVERY_INTERVAL_OPEN: Duration = Duration::from_secs(30);
const DISCOVERY_INTERVAL_CLOSED: Duration = Duration::from_secs(120);
const CONFIG_VERSION: u64 = 1;
const COMPACT_PANEL_KEY: &str = "compact_panel";

static POPUP_AUTOSIZE_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("cosmic-connect-popup"));

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopupView {
    #[default]
    Home,
    Alerts,
    Device,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShareKind {
    #[default]
    Text,
    Url,
    File,
}

#[derive(Default)]
struct DeviceDraft {
    share_text: String,
    share_url: String,
    status: Option<String>,
    status_error: bool,
    status_at: Option<Instant>,
    notifications: Vec<Notification>,
    selected_notification: Option<String>,
    notify_reply_text: String,
    player: Option<PlayerInfo>,
    connectivity: Option<ConnectivityInfo>,
    seek_preview: Option<i64>,
    composer_mode: ShareKind,
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
    popup_view: PopupView,
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
    compact_panel: bool,
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
    ActionFinished(String, ActionType, Result<String, String>),
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
    SeekPreview(String, i64),
    SeekCommit(String),
    SelectPlayer(String, String),
    SelectDevice(String),
    RequestUnpair(String),
    CancelUnpair,
    ConfirmUnpair,
    ToggleAdvanced(String),
    ToggleDeviceList,
    SetCompactPanel(bool),
    SetPopupView(PopupView),
    SelectShareKind(String, ShareKind),
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

fn format_ms(ms: i64) -> String {
    let total_seconds = (ms.max(0) / 1000) as u64;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes >= 60 {
        format!("{}:{:02}:{:02}", minutes / 60, minutes % 60, seconds)
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn is_quiet_action(action: &ActionType) -> bool {
    matches!(
        action,
        ActionType::MediaAction(_)
            | ActionType::Seek(_)
            | ActionType::SelectPlayer(_)
            | ActionType::DismissNotification(_)
            | ActionType::ReplyToNotification(..)
            | ActionType::BrowseFiles
    )
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn is_valid_url(value: &str) -> bool {
    let value = value.trim();
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    !host.is_empty() && !host.contains(' ')
}

fn next_player(player: &PlayerInfo) -> Option<String> {
    if player.player_list.len() < 2 {
        return None;
    }
    let index = player
        .player_list
        .iter()
        .position(|name| name == &player.player)
        .unwrap_or(0);
    player
        .player_list
        .get((index + 1) % player.player_list.len())
        .cloned()
}

fn notification_initial(notif: &Notification) -> String {
    notif
        .app_name
        .chars()
        .next()
        .or_else(|| notif.title.chars().next())
        .unwrap_or('?')
        .to_uppercase()
        .to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HeroTone {
    Accent,
    Warning,
    Success,
}

fn tone_color(theme: &cosmic::Theme, tone: HeroTone) -> Color {
    match tone {
        HeroTone::Accent => crate::widgets::accent_color(theme),
        HeroTone::Warning => theme.cosmic().warning_color().into(),
        HeroTone::Success => theme.cosmic().success_color().into(),
    }
}

fn tone_chip_style(theme: &cosmic::Theme, tone: HeroTone) -> iced_container::Style {
    let color = tone_color(theme, tone);
    iced_container::Style {
        background: Some(Background::Color(Color::from_rgba(
            color.r, color.g, color.b, 0.14,
        ))),
        border: Border {
            radius: RADIUS_MD.into(),
            width: 1.0,
            color: Color::from_rgba(color.r, color.g, color.b, 0.24),
        },
        ..Default::default()
    }
}

fn hero_card_style(theme: &cosmic::Theme, tone: HeroTone) -> iced_container::Style {
    let color = tone_color(theme, tone);
    iced_container::Style {
        background: Some(Background::Color(frosted_bg(
            theme,
            CARD_LUM_DARKEN,
            OPACITY_CARD,
        ))),
        border: Border {
            radius: RADIUS_CARD.into(),
            width: 1.0,
            color: Color::from_rgba(color.r, color.g, color.b, 0.28),
        },
        ..Default::default()
    }
}

fn tone_icon_chip<'a>(icon_name: &'a str, tone: HeroTone) -> Element<'a, Message> {
    container(icon::from_name(icon_name).size(20))
        .style(move |theme| tone_chip_style(theme, tone))
        .padding([6, 6])
        .into()
}

fn tone_badge<'a>(label: &'a str, tone: HeroTone) -> Element<'a, Message> {
    container(text::caption(label).size(10))
        .style(move |theme| {
            let color = tone_color(theme, tone);
            iced_container::Style {
                background: Some(Background::Color(Color::from_rgba(
                    color.r, color.g, color.b, 0.14,
                ))),
                border: Border {
                    radius: 999.0.into(),
                    width: 1.0,
                    color: Color::from_rgba(color.r, color.g, color.b, 0.24),
                },
                text_color: Some(color),
                ..Default::default()
            }
        })
        .padding([2, 6])
        .into()
}

fn metric_chip<'a>(icon_name: &'a str, label: String) -> Element<'a, Message> {
    row![
        icon::from_name(icon_name).size(14),
        text::caption(label).size(SIZE_CAPTION),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn empty_state<'a>(icon_name: &'a str, title: &'a str, caption: &'a str) -> Element<'a, Message> {
    container(
        column![
            container(icon::from_name(icon_name).size(28))
                .style(|theme| tone_chip_style(theme, HeroTone::Accent))
                .padding([10, 10]),
            text::body(title).size(SIZE_BODY),
            text::caption(caption).size(SIZE_CAPTION),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .style(glass_card)
    .padding(24)
    .width(Length::Fill)
    .into()
}

fn load_compact_panel() -> bool {
    match CosmicConfig::new(ID, CONFIG_VERSION) {
        Ok(config) => match config.get::<bool>(COMPACT_PANEL_KEY) {
            Ok(enabled) => enabled,
            Err(error) => {
                if error.is_err() {
                    log::warn!("Failed to load compact panel setting: {error}");
                }
                false
            }
        },
        Err(error) => {
            if error.is_err() {
                log::warn!("Failed to open applet config: {error}");
            }
            false
        }
    }
}

fn save_compact_panel(enabled: bool) {
    match CosmicConfig::new(ID, CONFIG_VERSION) {
        Ok(config) => match config.set(COMPACT_PANEL_KEY, enabled) {
            Ok(()) => log::info!(
                "Panel display set to {}",
                if enabled { "compact" } else { "detailed" }
            ),
            Err(error) => log::warn!("Failed to save compact panel setting: {error}"),
        },
        Err(error) => log::warn!("Failed to open applet config: {error}"),
    }
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
            let net = self
                .drafts
                .get(&device.id)
                .and_then(|d| d.connectivity.as_ref());
            if device.is_reachable {
                if let Some(bat) = &device.battery {
                    let charge_label = format!("{}%", bat.charge);
                    if let Some(conn) = net {
                        format!(
                            "{} - ({}) {}",
                            device.name,
                            charge_label,
                            signal_bars(conn.signal_strength)
                        )
                    } else {
                        format!("{} - ({})", device.name, charge_label)
                    }
                } else if let Some(conn) = net {
                    format!(
                        "{} - {} - Connected",
                        device.name,
                        signal_bars(conn.signal_strength)
                    )
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

    fn render_panel_settings(&self) -> Element<'_, Message> {
        container(
            column![
                settings::item(
                    "Compact panel",
                    toggler(self.compact_panel).on_toggle(Message::SetCompactPanel),
                ),
                text::caption("Show only the device icon in the panel. Details stay in the popup.")
                    .size(SIZE_CAPTION),
            ]
            .spacing(4),
        )
        .padding([12, 0, 4, 0])
        .width(Length::Fill)
        .into()
    }

    fn last_sync_label(&self) -> Option<String> {
        self.last_sync.map(|last| {
            let secs = last.elapsed().as_secs();
            if secs < 3 {
                "Updated just now".to_string()
            } else if secs < 60 {
                format!("Updated {secs}s ago")
            } else {
                format!("Updated {}m ago", secs / 60)
            }
        })
    }

    fn render_empty_header(&self) -> Element<'_, Message> {
        let icon = container(icon::from_name("io.github.acemythos.Connect-symbolic").size(20))
            .style(|theme| tone_chip_style(theme, HeroTone::Accent))
            .padding([6, 6]);
        let identity = column![
            text::body("COSMIC Connect").size(SIZE_BODY),
            text::caption("No device selected").size(SIZE_CAPTION),
        ]
        .spacing(1);
        container(row![icon, identity].spacing(9).align_y(Alignment::Center))
            .style(glass_card)
            .padding([10, 12])
            .width(Length::Fill)
            .into()
    }

    fn render_home_header<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        let draft = self.drafts.get(&device.id);
        let connectivity = draft.and_then(|draft| draft.connectivity.as_ref());
        let dot = if device.is_reachable { "●" } else { "○" };
        let status = if device.is_reachable {
            "Connected"
        } else if device.is_paired {
            "Offline"
        } else {
            "Not paired"
        };
        let multiple = self.devices.len() > 1;

        let mut identity = row![
            container(icon::from_name(device.device_type.icon_name()).size(18))
                .style(|theme| tone_chip_style(theme, HeroTone::Accent))
                .padding([6, 6]),
            column![
                text::body(truncate_text(&device.name, 10)).size(SIZE_BODY),
                row![
                    text::caption(dot).size(SIZE_CAPTION),
                    text::caption(status).size(SIZE_CAPTION),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            ]
            .spacing(1),
        ]
        .spacing(9)
        .align_y(Alignment::Center);

        if multiple {
            identity = identity.push(
                icon::from_name(if self.device_list_open {
                    "pan-down-symbolic"
                } else {
                    "pan-end-symbolic"
                })
                .size(12),
            );
        }

        let identity_elem: Element<Message> = if multiple {
            button::custom(identity)
                .on_press(Message::ToggleDeviceList)
                .padding(0)
                .class(theme::Button::Text)
                .into()
        } else {
            identity.into()
        };

        let mut metrics = row![].spacing(8).align_y(Alignment::Center);
        if let Some(battery) = &device.battery {
            metrics = metrics.push(metric_chip(
                crate::widgets::battery_icon(battery.charge, battery.is_charging),
                format!("{}%", battery.charge),
            ));
        }
        if let Some(conn) = connectivity {
            metrics = metrics.push(metric_chip(
                crate::widgets::network_icon(&conn.network_type),
                format!(
                    "{} {}",
                    conn.network_type,
                    signal_bars(conn.signal_strength)
                ),
            ));
        }

        container(
            row![
                identity_elem,
                container(row![]).width(Length::Fill),
                metrics,
                crate::widgets::icon_button(
                    "view-refresh-symbolic",
                    "Refresh devices",
                    Message::RefreshDevices,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .style(glass_card)
        .padding([10, 12])
        .width(Length::Fill)
        .into()
    }

    fn render_device_card<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        let draft = self.drafts.get(&device.id);
        let connectivity = draft.and_then(|draft| draft.connectivity.as_ref());
        let dot = if device.is_reachable { "●" } else { "○" };
        let status = if device.is_reachable {
            "Connected · reachable"
        } else if device.is_paired {
            "Offline"
        } else {
            "Not paired"
        };

        let mut metrics = row![].spacing(12).align_y(Alignment::Center);
        if let Some(battery) = &device.battery {
            metrics = metrics.push(metric_chip(
                crate::widgets::battery_icon(battery.charge, battery.is_charging),
                format!(
                    "{}%{}",
                    battery.charge,
                    if battery.is_charging {
                        " · charging"
                    } else {
                        ""
                    }
                ),
            ));
        }
        if let Some(conn) = connectivity {
            metrics = metrics.push(metric_chip(
                crate::widgets::network_icon(&conn.network_type),
                format!("{} · {}/4", conn.network_type, conn.signal_strength),
            ));
        }

        let mut body = column![
            row![
                container(icon::from_name(device.device_type.icon_name()).size(22))
                    .style(|theme| tone_chip_style(theme, HeroTone::Accent))
                    .padding([8, 8]),
                column![
                    text::body(truncate_text(&device.name, 20)).size(SIZE_HEADING),
                    row![
                        text::caption(dot).size(SIZE_CAPTION),
                        text::caption(status).size(SIZE_CAPTION),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                ]
                .spacing(1),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            metrics,
        ]
        .spacing(10);
        if let Some(updated) = self.last_sync_label() {
            body = body.push(text::caption(updated).size(SIZE_CAPTION));
        }

        container(body)
            .style(glass_card)
            .padding([12, 14])
            .width(Length::Fill)
            .into()
    }

    fn render_device_rows(&self) -> Element<'_, Message> {
        let mut rows: Vec<Element<Message>> = vec![crate::widgets::section_header("Choose device")];
        for device in &self.devices {
            rows.push(crate::widgets::paired_device_row(
                device.device_type.icon_name(),
                device.name.as_str(),
                device.is_reachable,
                self.selected_device_id.as_deref() == Some(device.id.as_str()),
                Some(Message::SelectDevice(device.id.clone())),
                None,
            ));
        }

        container(column::with_children(rows).spacing(4))
            .padding([0, 0])
            .width(Length::Fill)
            .into()
    }

    fn render_device_list(&self) -> Option<Element<'_, Message>> {
        if !self.device_list_open || self.devices.len() < 2 {
            return None;
        }

        Some(self.render_device_rows())
    }

    fn render_action_grid<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        let mut actions: Vec<(&'static str, &'static str, Message, bool)> = Vec::new();

        if device.is_reachable && device.has_plugin("kdeconnect_sftp") {
            actions.push((
                "folder-symbolic",
                "Files",
                Message::PerformAction(device.id.clone(), ActionType::BrowseFiles),
                false,
            ));
        }
        if device.is_reachable && device.has_plugin("kdeconnect_findmyphone") {
            actions.push((
                "notification-alert-symbolic",
                "Ring",
                Message::PerformAction(device.id.clone(), ActionType::Ring),
                false,
            ));
        }
        if device.is_reachable && device.has_plugin("kdeconnect_ping") {
            actions.push((
                "emblem-important-symbolic",
                "Ping",
                Message::PerformAction(device.id.clone(), ActionType::Ping),
                false,
            ));
        }
        if device.is_reachable && device.has_plugin("kdeconnect_clipboard") {
            actions.push((
                "edit-paste-symbolic",
                "Clipboard",
                Message::PerformAction(device.id.clone(), ActionType::SendClipboard),
                false,
            ));
        }
        if device.pair_state == 0 && !device.is_paired {
            actions.push((
                "insert-link-symbolic",
                "Pair",
                Message::PerformAction(device.id.clone(), ActionType::Pair),
                false,
            ));
        }

        let mut rows: Vec<Element<Message>> = Vec::new();
        for chunk in actions.chunks(2) {
            let mut action_row = row![].spacing(6);
            for (icon_name, label, message, active) in chunk {
                action_row = action_row.push(crate::widgets::quick_action_btn(
                    icon_name,
                    label,
                    message.clone(),
                    *active,
                ));
            }
            if chunk.len() == 1 {
                action_row = action_row.push(container(row![]).width(Length::Fill));
            }
            rows.push(action_row.into());
        }

        let body: Element<Message> = if rows.is_empty() {
            container(
                text::caption("No quick actions available for this device.").size(SIZE_CAPTION),
            )
            .style(glass_card)
            .padding([10, 12])
            .width(Length::Fill)
            .into()
        } else {
            column::with_children(rows).spacing(6).into()
        };

        column![crate::widgets::section_header("Actions"), body]
            .spacing(4)
            .into()
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
        children.push(crate::widgets::section_header("Other devices"));

        for device in others {
            children.push(crate::widgets::paired_device_row(
                device.device_type.icon_name(),
                device.name.as_str(),
                device.is_reachable,
                false,
                Some(Message::SelectDevice(device.id.clone())),
                None,
            ));
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
                        "window-close-symbolic",
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

    fn render_media_hero<'a>(
        &'a self,
        device: &'a Device,
        player: &'a PlayerInfo,
    ) -> Element<'a, Message> {
        let track_length = player.length.max(0);
        let display_position = self
            .drafts
            .get(&device.id)
            .and_then(|draft| draft.seek_preview)
            .unwrap_or(player.position)
            .clamp(0, track_length);
        let progress = if track_length > 0 {
            (display_position as f32 / track_length as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let subtitle = match (player.artist.as_str(), player.album.as_str()) {
            ("", "") => String::new(),
            ("", album) => album.to_string(),
            (artist, "") => artist.to_string(),
            (artist, album) => format!("{artist} · {album}"),
        };

        let mut body = column![
            row![
                tone_icon_chip("audio-x-generic-symbolic", HeroTone::Accent),
                tone_badge(
                    if player.is_playing {
                        "PLAYING"
                    } else {
                        "PAUSED"
                    },
                    HeroTone::Accent,
                ),
                container(row![]).width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text::body(player.title.as_str()).size(SIZE_HEADING),
        ]
        .spacing(6);

        if !subtitle.is_empty() {
            body = body.push(text::caption(subtitle).size(SIZE_CAPTION));
        }

        let progress_control: Element<Message> = if player.can_seek && track_length > 0 {
            let slider_length = track_length.min(i32::MAX as i64) as i32;
            let slider_position = display_position.min(i64::from(slider_length)) as i32;
            slider(0..=slider_length, slider_position, {
                let device_id = device.id.clone();
                move |position| Message::SeekPreview(device_id.clone(), i64::from(position))
            })
            .on_release(Message::SeekCommit(device.id.clone()))
            .step(1000_i32)
            .width(Length::Fill)
            .into()
        } else {
            progress_bar::determinate_linear(progress)
                .width(Length::Fill)
                .girth(4)
                .into()
        };

        body = body.push(
            column![
                progress_control,
                row![
                    text::caption(format!(
                        "{} / {}",
                        format_ms(display_position),
                        format_ms(track_length)
                    ))
                    .size(SIZE_CAPTION),
                    container(row![]).width(Length::Fill),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            ]
            .spacing(4),
        );

        if device.is_reachable {
            let mut controls = row![
                crate::widgets::media_button(
                    "media-skip-backward-symbolic",
                    "Previous",
                    Message::MediaAction(device.id.clone(), "Previous".into()),
                    false,
                ),
                crate::widgets::media_button(
                    if player.is_playing {
                        "media-playback-pause-symbolic"
                    } else {
                        "media-playback-start-symbolic"
                    },
                    if player.is_playing { "Pause" } else { "Play" },
                    Message::MediaAction(device.id.clone(), "PlayPause".into()),
                    true,
                ),
                crate::widgets::media_button(
                    "media-skip-forward-symbolic",
                    "Next",
                    Message::MediaAction(device.id.clone(), "Next".into()),
                    false,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            controls = controls.push(container(row![]).width(Length::Fill));

            if !player.player.is_empty() {
                if let Some(next) = next_player(player) {
                    controls = controls.push(crate::widgets::pill_button(
                        "audio-x-generic-symbolic",
                        player.player.as_str(),
                        Message::SelectPlayer(device.id.clone(), next),
                        false,
                    ));
                } else {
                    controls = controls.push(
                        container(
                            row![
                                icon::from_name("audio-x-generic-symbolic").size(14),
                                text::caption(player.player.as_str()).size(SIZE_CAPTION),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center),
                        )
                        .style(|theme| tone_chip_style(theme, HeroTone::Accent))
                        .padding([4, 8]),
                    );
                }
            }

            body = body.push(controls);
        } else {
            body = body.push(text::caption("Device offline").size(SIZE_CAPTION));
        }

        container(body)
            .style(move |theme| hero_card_style(theme, HeroTone::Accent))
            .padding([12, 12])
            .width(Length::Fill)
            .into()
    }

    fn render_pairing_hero<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        container(
            column![
                row![
                    tone_icon_chip("dialog-warning-symbolic", HeroTone::Warning),
                    tone_badge("PAIRING", HeroTone::Warning),
                    container(row![]).width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text::body("Pairing request").size(SIZE_HEADING),
                text::caption(format!("{} wants to pair", device.name)).size(SIZE_CAPTION),
                row![
                    crate::widgets::pill_button(
                        "emblem-ok-symbolic",
                        "Accept",
                        Message::PerformAction(device.id.clone(), ActionType::AcceptPairing),
                        true,
                    ),
                    crate::widgets::pill_button(
                        "window-close-symbolic",
                        "Reject",
                        Message::PerformAction(device.id.clone(), ActionType::CancelPairing),
                        false,
                    ),
                ]
                .spacing(6),
            ]
            .spacing(8),
        )
        .style(move |theme| hero_card_style(theme, HeroTone::Warning))
        .padding([12, 12])
        .width(Length::Fill)
        .into()
    }

    fn render_transfer_hero<'a>(&'a self, file: &'a ReceivedFile) -> Element<'a, Message> {
        let name = if file.file_name.is_empty() {
            "file"
        } else {
            file.file_name.as_str()
        };
        let transferred = file.total_bytes.saturating_mul(file.progress as u64) / 100;
        let detail = if file.total_bytes > 0 {
            format!(
                "{}% · {} of {}",
                file.progress,
                format_bytes(transferred),
                format_bytes(file.total_bytes)
            )
        } else {
            format!("{}%", file.progress)
        };

        container(
            column![
                row![
                    tone_icon_chip("document-save-symbolic", HeroTone::Success),
                    tone_badge("TRANSFER", HeroTone::Success),
                    container(row![]).width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text::body(format!("Receiving {name}")).size(SIZE_HEADING),
                text::caption(detail).size(SIZE_CAPTION),
                progress_bar::determinate_linear((file.progress as f32 / 100.0).clamp(0.0, 1.0))
                    .width(Length::Fill)
                    .girth(4),
            ]
            .spacing(8),
        )
        .style(move |theme| hero_card_style(theme, HeroTone::Success))
        .padding([12, 12])
        .width(Length::Fill)
        .into()
    }

    fn render_idle_hero<'a>(&'a self, device: &'a Device) -> Element<'a, Message> {
        let draft = self.drafts.get(&device.id);
        let connectivity = draft.and_then(|draft| draft.connectivity.as_ref());
        let (badge, tone) = if device.is_reachable {
            ("READY", HeroTone::Accent)
        } else if device.is_paired {
            ("OFFLINE", HeroTone::Warning)
        } else {
            ("NOT PAIRED", HeroTone::Warning)
        };
        let status = if device.is_reachable {
            "Connected · no active transfer"
        } else if device.is_paired {
            "Offline"
        } else {
            "Not paired"
        };

        let mut metrics = row![].spacing(12).align_y(Alignment::Center);
        if let Some(battery) = &device.battery {
            metrics = metrics.push(metric_chip(
                crate::widgets::battery_icon(battery.charge, battery.is_charging),
                format!("{}%", battery.charge),
            ));
        }
        if let Some(conn) = connectivity {
            metrics = metrics.push(metric_chip(
                crate::widgets::network_icon(&conn.network_type),
                format!("{} · {}/4", conn.network_type, conn.signal_strength),
            ));
        }

        container(
            column![
                row![
                    tone_icon_chip(device.device_type.icon_name(), tone),
                    tone_badge(badge, tone),
                    container(row![]).width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text::body(truncate_text(&device.name, 20)).size(SIZE_HEADING),
                text::caption(status).size(SIZE_CAPTION),
                metrics,
            ]
            .spacing(8),
        )
        .style(move |theme| hero_card_style(theme, tone))
        .padding([12, 12])
        .width(Length::Fill)
        .into()
    }

    fn render_contextual_hero<'a>(
        &'a self,
        device: &'a Device,
        draft: &'a DeviceDraft,
    ) -> Element<'a, Message> {
        if device.pair_state == 2 {
            return self.render_pairing_hero(device);
        }
        if let Some(file) = self
            .received_files
            .get(&device.id)
            .and_then(|files| files.iter().rev().find(|file| file.active))
        {
            return self.render_transfer_hero(file);
        }
        if let Some(player) = draft.player.as_ref() {
            if !player.title.is_empty() {
                return self.render_media_hero(device, player);
            }
        }
        self.render_idle_hero(device)
    }

    fn render_notification_card<'a>(
        &'a self,
        device: &'a Device,
        draft: &'a DeviceDraft,
        notif: &'a Notification,
    ) -> Element<'a, Message> {
        let is_selected =
            draft.selected_notification.as_deref() == Some(notif.internal_id.as_str());
        let app_name = if notif.app_name.is_empty() {
            "Notification"
        } else {
            notif.app_name.as_str()
        };
        let text = if !notif.text.is_empty() {
            notif.text.as_str()
        } else {
            notif.ticker.as_str()
        };

        let mut body = column![text::caption(app_name).size(SIZE_CAPTION)].spacing(2);
        if !notif.title.is_empty() {
            body = body.push(text::body(notif.title.as_str()).size(SIZE_BODY));
        }
        if !text.is_empty() {
            body = body.push(text::caption(text).size(SIZE_CAPTION));
        }

        let avatar = container(text::body(notification_initial(notif)).size(SIZE_BODY))
            .style(|theme| tone_chip_style(theme, HeroTone::Accent))
            .padding([6, 8]);

        let mut main = row![avatar, body.width(Length::Fill)]
            .spacing(9)
            .align_y(Alignment::Center);

        let mut actions = row![].spacing(2).align_y(Alignment::Center);
        if !notif.reply_id.is_empty() {
            actions = actions.push(crate::widgets::icon_button(
                "mail-reply-symbolic",
                "Reply",
                Message::SelectNotification(device.id.clone(), notif.internal_id.clone()),
            ));
        }
        if notif.dismissable {
            actions = actions.push(crate::widgets::icon_button(
                "window-close-symbolic",
                "Dismiss",
                Message::DismissNotification(device.id.clone(), notif.internal_id.clone()),
            ));
        }
        main = main.push(actions);

        let mut card = column![main].spacing(8);
        if is_selected && !notif.reply_id.is_empty() {
            card = card.push(
                row![
                    text_input::text_input("Reply to this notification…", &draft.notify_reply_text)
                        .on_input({
                            let did = device.id.clone();
                            move |value| Message::NotifyReplyChanged(did.clone(), value)
                        })
                        .width(Length::Fill),
                    crate::widgets::pill_button(
                        "document-send-symbolic",
                        "Send",
                        Message::SendNotifyReply(device.id.clone(), notif.internal_id.clone(),),
                        false,
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }

        container(card)
            .style(glass_card)
            .padding([10, 12])
            .width(Length::Fill)
            .into()
    }

    fn render_alerts_list<'a>(
        &'a self,
        device: &'a Device,
        draft: &'a DeviceDraft,
    ) -> Element<'a, Message> {
        if draft.notifications.is_empty() {
            return empty_state(
                "mail-unread-symbolic",
                "No active notifications",
                "Phone notifications show up here.",
            );
        }

        let rows: Vec<Element<Message>> = draft
            .notifications
            .iter()
            .map(|notif| self.render_notification_card(device, draft, notif))
            .collect();

        column::with_children(rows).spacing(8).into()
    }

    fn render_home_notifications_row<'a>(&'a self, draft: &'a DeviceDraft) -> Element<'a, Message> {
        let count = draft.notifications.len();
        let label = if count == 1 {
            "1 notification".to_string()
        } else {
            format!("{count} notifications")
        };
        crate::widgets::nav_row(
            "mail-unread-symbolic",
            label,
            "View all".to_string(),
            Message::SetPopupView(PopupView::Alerts),
        )
    }

    fn render_composer<'a>(
        &'a self,
        device: &'a Device,
        draft: &'a DeviceDraft,
    ) -> Option<Element<'a, Message>> {
        if !device.is_reachable || !device.has_plugin("kdeconnect_share") {
            return None;
        }

        let mode = draft.composer_mode;
        let chips = row![
            crate::widgets::pill_button(
                "edit-symbolic",
                "Text",
                Message::SelectShareKind(device.id.clone(), ShareKind::Text),
                mode == ShareKind::Text,
            ),
            crate::widgets::pill_button(
                "insert-link-symbolic",
                "URL",
                Message::SelectShareKind(device.id.clone(), ShareKind::Url),
                mode == ShareKind::Url,
            ),
            crate::widgets::pill_button(
                "document-send-symbolic",
                "File",
                Message::SelectShareKind(device.id.clone(), ShareKind::File),
                mode == ShareKind::File,
            ),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let mut body = column![
            text::caption(format!("Send to {}", truncate_text(&device.name, 24)))
                .size(SIZE_CAPTION),
            chips,
        ]
        .spacing(8);

        match mode {
            ShareKind::Text => {
                body = body.push(
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
                );
            }
            ShareKind::Url => {
                body = body.push(
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
                );
                if !draft.share_url.trim().is_empty() && !is_valid_url(&draft.share_url) {
                    body = body.push(
                        container(text::caption("Enter a valid http(s) URL").size(SIZE_CAPTION))
                            .style(|theme| iced_container::Style {
                                text_color: Some(crate::widgets::destructive_color(theme)),
                                ..Default::default()
                            }),
                    );
                }
            }
            ShareKind::File => {
                body = body.push(crate::widgets::list_row(
                    "document-send-symbolic",
                    "Choose a file…",
                    Message::ChooseFile(device.id.clone()),
                ));
            }
        }

        Some(
            container(body)
                .style(glass_card)
                .padding([10, 12])
                .width(Length::Fill)
                .into(),
        )
    }

    fn render_status_banner<'a>(&'a self, draft: &'a DeviceDraft) -> Option<Element<'a, Message>> {
        let status = draft.status.as_ref()?;
        let visible = draft
            .status_at
            .map(|at| at.elapsed() < STATUS_TIMEOUT)
            .unwrap_or(false);
        if !visible {
            return None;
        }

        let is_error = draft.status_error;
        let is_working = status.starts_with("Working");
        let icon_name = if is_working {
            "process-working-symbolic"
        } else if is_error {
            "dialog-error-symbolic"
        } else {
            "emblem-default-symbolic"
        };

        Some(
            container(
                row![
                    icon::from_name(icon_name).size(14),
                    text::caption(status.as_str()).size(SIZE_CAPTION),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(move |theme| {
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
        )
    }

    fn render_connection_details<'a>(
        &'a self,
        device: &'a Device,
        draft: &'a DeviceDraft,
    ) -> Element<'a, Message> {
        let mut metadata = column![row![
            icon::from_name(device.device_type.icon_name()).size(16),
            text::caption("Type").size(SIZE_CAPTION),
            container(row![]).width(Length::Fill),
            text::body(device.device_type.label()).size(SIZE_CAPTION),
        ]
        .spacing(8)
        .align_y(Alignment::Center),]
        .spacing(8);

        if let Some(battery) = &device.battery {
            metadata = metadata.push(
                row![
                    icon::from_name("battery-symbolic").size(16),
                    text::caption("Battery").size(SIZE_CAPTION),
                    container(row![]).width(Length::Fill),
                    text::body(format!(
                        "{}%{}",
                        battery.charge,
                        if battery.is_charging {
                            " · Charging"
                        } else {
                            ""
                        }
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

        container(metadata)
            .style(glass_card)
            .padding([10, 12])
            .width(Length::Fill)
            .into()
    }

    fn render_device_details_section<'a>(
        &'a self,
        device: &'a Device,
        draft: &'a DeviceDraft,
    ) -> Element<'a, Message> {
        let mut children: Vec<Element<Message>> = Vec::new();

        children.push(crate::widgets::section_header("Received files"));
        let files = self.received_files.get(&device.id);
        let files = files.map(|files| files.as_slice()).unwrap_or(&[]);
        if files.is_empty() {
            children.push(
                container(text::caption("No files received yet.").size(SIZE_CAPTION))
                    .style(glass_card)
                    .padding([10, 12])
                    .width(Length::Fill)
                    .into(),
            );
        } else {
            for rf in files.iter().rev().take(10) {
                if rf.active {
                    let transferred = rf.total_bytes.saturating_mul(rf.progress as u64) / 100;
                    let detail = if rf.total_bytes > 0 {
                        format!(
                            "{}% · {} of {}",
                            rf.progress,
                            format_bytes(transferred),
                            format_bytes(rf.total_bytes)
                        )
                    } else {
                        format!("{}%", rf.progress)
                    };
                    children.push(
                        container(
                            column![
                                row![
                                    icon::from_name("document-save-symbolic").size(14),
                                    text::caption(truncate_text(&rf.file_name, 22))
                                        .size(SIZE_CAPTION),
                                    container(row![]).width(Length::Fill),
                                    text::caption(detail).size(SIZE_CAPTION),
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
                        .padding([8, 10])
                        .width(Length::Fill)
                        .into(),
                    );
                } else {
                    let mut file_row = row![
                        icon::from_name("document-save-symbolic").size(14),
                        column![
                            text::caption(truncate_text(&rf.file_name, 22)).size(SIZE_CAPTION),
                            text::caption(truncate_text(&rf.file_path, 30)).size(SIZE_CAPTION),
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

        children.push(divider::horizontal::default().into());
        let discover_label = if self.is_discovering {
            "Discovering…"
        } else {
            "Discover"
        };
        children.push(
            row![
                crate::widgets::pill_button(
                    "view-refresh-symbolic",
                    "Refresh",
                    Message::RefreshDevices,
                    false,
                ),
                crate::widgets::pill_button(
                    "network-wireless-symbolic",
                    discover_label,
                    Message::DiscoverDevices,
                    self.is_discovering,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        );

        let details_open = self.advanced_device.as_deref() == Some(device.id.as_str());
        children.push(
            container(crate::widgets::disclosure_row(
                "Connection details",
                details_open,
                Message::ToggleAdvanced(device.id.clone()),
            ))
            .style(glass_card)
            .width(Length::Fill)
            .into(),
        );
        if details_open {
            children.push(self.render_connection_details(device, draft));
        }

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
                    .padding([0, 0])
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
                            text::caption(error.as_str()).size(SIZE_CAPTION),
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

        column::with_children(children).spacing(8).into()
    }

    fn render_sub_header<'a>(
        &'a self,
        title: &'a str,
        action: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let mut header = row![
            crate::widgets::icon_button(
                "go-previous-symbolic",
                "Back to Home",
                Message::SetPopupView(PopupView::Home),
            ),
            text::body(title).size(SIZE_HEADING),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        header = header.push(container(row![]).width(Length::Fill));
        if let Some(action) = action {
            header = header.push(action);
        }

        header.into()
    }

    fn render_home_view(&self) -> Element<'_, Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        if let Some(err) = &self.error {
            content.push(self.render_empty_header());
            let is_backend_err = err.contains("D-Bus") || err.contains("backend");
            let retry: Element<Message> = if is_backend_err {
                crate::widgets::pill_button(
                    "view-refresh-symbolic",
                    "Retry",
                    Message::RefreshDevices,
                    false,
                )
            } else {
                row![].into()
            };
            content.push(
                container(
                    column![
                        icon::from_name(if is_backend_err {
                            "computer-symbolic"
                        } else {
                            "dialog-warning-symbolic"
                        })
                        .size(32),
                        text::body(if is_backend_err {
                            "Backend unavailable"
                        } else {
                            "Error"
                        })
                        .size(SIZE_BODY),
                        text::caption(err.as_str()).size(SIZE_CAPTION),
                        retry,
                    ]
                    .spacing(8)
                    .align_x(Alignment::Center),
                )
                .style(glass_card)
                .padding(24)
                .width(Length::Fill)
                .into(),
            );
            return column::with_children(content).spacing(10).into();
        }

        if self.backend.is_none() {
            content.push(self.render_empty_header());
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
                .style(glass_card)
                .padding(32)
                .width(Length::Fill)
                .into(),
            );
            return column::with_children(content).spacing(10).into();
        }

        if self.devices.is_empty() {
            content.push(self.render_empty_header());
            let is_discovering = self.is_discovering || self.auto_discovering;
            content.push(
                container(
                    column![
                        icon::from_name("network-wireless-symbolic").size(48),
                        text::body("No devices found"),
                        text::caption(
                            "Make sure KDE Connect is installed\nand a device is on the same network."
                        )
                        .size(SIZE_CAPTION),
                        crate::widgets::pill_button(
                            "view-refresh-symbolic",
                            "Find Devices",
                            Message::DiscoverDevices,
                            is_discovering,
                        ),
                    ]
                    .spacing(12)
                    .align_x(Alignment::Center),
                )
                .style(glass_card)
                .padding(32)
                .width(Length::Fill)
                .into(),
            );
            return column::with_children(content).spacing(10).into();
        }

        let Some(device) = self.selected_device() else {
            content.push(self.render_empty_header());
            content.push(empty_state(
                "phone-symbolic",
                "No device selected",
                "Choose a paired device to continue.",
            ));
            content.push(self.render_device_rows());
            return column::with_children(content).spacing(10).into();
        };
        let Some(draft) = self.drafts.get(&device.id) else {
            content.push(self.render_empty_header());
            content.push(empty_state(
                "phone-symbolic",
                "Device loading",
                "Waiting for device details.",
            ));
            return column::with_children(content).spacing(10).into();
        };

        content.push(self.render_home_header(device));
        if let Some(list) = self.render_device_list() {
            content.push(list);
        }
        content.push(self.render_contextual_hero(device, draft));
        content.push(self.render_action_grid(device));
        content.push(self.render_home_notifications_row(draft));
        if let Some(composer) = self.render_composer(device, draft) {
            content.push(composer);
        }
        content.push(crate::widgets::nav_row(
            "preferences-system-symbolic",
            "Device details".to_string(),
            String::new(),
            Message::SetPopupView(PopupView::Device),
        ));
        if let Some(status) = self.render_status_banner(draft) {
            content.push(status);
        }

        column::with_children(content).spacing(10).into()
    }

    fn render_alerts_view(&self) -> Element<'_, Message> {
        let Some(device) = self.selected_device() else {
            return self.render_home_view();
        };
        let Some(draft) = self.drafts.get(&device.id) else {
            return self.render_home_view();
        };

        let clear = (!draft.notifications.is_empty()).then(|| {
            crate::widgets::text_button(
                "Clear",
                Message::DismissAllNotifications(device.id.clone()),
            )
        });

        let mut content: Vec<Element<Message>> = Vec::new();
        content.push(self.render_sub_header("Notifications", clear));
        content.push(self.render_alerts_list(device, draft));
        if let Some(status) = self.render_status_banner(draft) {
            content.push(status);
        }

        column::with_children(content).spacing(10).into()
    }

    fn render_device_view(&self) -> Element<'_, Message> {
        let Some(device) = self.selected_device() else {
            return self.render_home_view();
        };
        let Some(draft) = self.drafts.get(&device.id) else {
            return self.render_home_view();
        };

        let mut content: Vec<Element<Message>> = Vec::new();
        content.push(self.render_sub_header("Device details", None));
        content.push(self.render_device_card(device));
        content.push(self.render_device_details_section(device, draft));
        if let Some(section) = self.render_other_devices_section() {
            content.push(section);
        }
        content.push(divider::horizontal::default().into());
        content.push(self.render_panel_settings());
        if let Some(status) = self.render_status_banner(draft) {
            content.push(status);
        }

        column::with_children(content).spacing(10).into()
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
                compact_panel: load_compact_panel(),
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
                    self.popup_view = PopupView::Home;
                    self.advanced_device = None;
                    self.device_list_open = false;
                    self.pending_unpair = None;
                    self.unpair_error = None;

                    surface::surface_task(surface::action::app_popup(
                        |_| surface::action::LiveSettings::default(),
                        |app: &mut Self| {
                            let parent_id = app
                                .popup_parent_id()
                                .expect("checked before creating popup");
                            let new_id = Id::unique();
                            app.popup.replace(new_id);
                            let mut popup_settings = app.core.applet.get_popup_settings(
                                parent_id,
                                new_id,
                                Some((360, 700)),
                                None,
                                None,
                            );
                            popup_settings.positioner.size_limits = Limits::NONE
                                .min_width(360.0)
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
                log::debug!(
                    "DevicesUpdated: {} devices, {} pairing requests",
                    devices.len(),
                    pairing_ids.len()
                );
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
                            log::info!(
                                "Device '{}' {} (paired={})",
                                new.name,
                                if new.is_reachable {
                                    "became reachable"
                                } else {
                                    "became unreachable"
                                },
                                new.is_paired
                            );
                        }
                    } else {
                        log::debug!("Device '{}' removed", old.name);
                    }
                }
                for new in &merged {
                    if !self.devices.iter().any(|d| d.id == new.id) {
                        log::debug!(
                            "Device '{}' added (reachable={})",
                            new.name,
                            new.is_reachable
                        );
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
                        let dev_name = self
                            .devices
                            .iter()
                            .find(|d| d.id == *pid)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| "Unknown device".into());
                        log::info!("Pairing request from '{dev_name}' ({pid})");
                        if let Some(backend) = self.backend.clone() {
                            let name = dev_name;
                            let did = pid.clone();
                            tasks = tasks.chain(
                                Task::perform(
                                    async move {
                                        backend
                                            .notify(
                                                "Pairing request",
                                                &format!("{name} wants to pair"),
                                                0,
                                                &["accept", "Accept", "reject", "Reject"],
                                            )
                                            .await
                                            .ok()
                                    },
                                    move |notif_id| {
                                        if let Some(id) = notif_id {
                                            Message::NotifCreated(did, id)
                                        } else {
                                            Message::NoOp
                                        }
                                    },
                                )
                                .map(cosmic::Action::App),
                            );
                        }
                    }
                }
                let selected_id = self.selected_device_id.clone();
                let popup_open = self.popup.is_some();
                for device in &self.devices {
                    if !device.is_reachable {
                        continue;
                    }
                    let selected = selected_id.as_deref() == Some(device.id.as_str());

                    if selected && popup_open && device.has_plugin("kdeconnect_notifications") {
                        let Some(backend) = self.backend.clone() else {
                            continue;
                        };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(
                            Task::perform(
                                async move { backend.fetch_notifications(&did).await },
                                move |n| Message::NotificationsLoaded(did2, n),
                            )
                            .map(cosmic::Action::App),
                        );
                    }
                    if selected && popup_open && device.has_plugin("kdeconnect_mprisremote") {
                        let Some(backend) = self.backend.clone() else {
                            continue;
                        };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(
                            Task::perform(
                                async move { backend.player_info(&did).await },
                                move |p| Message::PlayerInfoUpdated(did2, p),
                            )
                            .map(cosmic::Action::App),
                        );
                    }
                    if selected && device.has_plugin("kdeconnect_connectivity_report") {
                        let Some(backend) = self.backend.clone() else {
                            continue;
                        };
                        let did = device.id.clone();
                        let did2 = did.clone();
                        tasks = tasks.chain(
                            Task::perform(
                                async move { backend.connectivity_info(&did).await },
                                move |c| Message::ConnectivityUpdated(did2, c),
                            )
                            .map(cosmic::Action::App),
                        );
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
                        now + if popup_open {
                            DISCOVERY_INTERVAL_OPEN
                        } else {
                            DISCOVERY_INTERVAL_CLOSED
                        },
                    );
                    if let Some(backend) = self.backend.clone() {
                        tasks = tasks.chain(
                            Task::perform(
                                async move {
                                    if let Err(error) = backend.force_discovery().await {
                                        log::warn!("Auto-discovery failed: {error}");
                                    }
                                },
                                |_| Message::NoOp,
                            )
                            .map(cosmic::Action::App),
                        );
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
                    |_| Message::RefreshDevices,
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
                return Task::perform(async move { pick_file_path().await }, move |result| {
                    Message::FileChooserFinished(device_id_for_task, result)
                })
                .map(cosmic::Action::App);
            }
            Message::FileChooserFinished(device_id, result) => match result {
                Ok(path) => {
                    let did = device_id.clone();
                    let fname = path.rsplit('/').next().unwrap_or(&path).to_string();
                    self.set_status(&device_id, format!("Sending {fname}…"), false);
                    return Task::perform(async move {}, move |_| {
                        Message::PerformAction(did, ActionType::SendFile(path))
                    })
                    .map(cosmic::Action::App);
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
            },
            Message::PerformAction(device_id, action) => {
                let Some(backend) = self.backend.clone() else {
                    if matches!(&action, ActionType::Seek(_)) {
                        self.draft_mut(&device_id).seek_preview = None;
                    }
                    self.set_status(&device_id, "KDE Connect backend unavailable", true);
                    return Task::none();
                };

                if !is_quiet_action(&action) {
                    let draft = self.draft_mut(&device_id);
                    draft.status = Some("Working…".into());
                    draft.status_error = false;
                    draft.status_at = Some(Instant::now());
                }

                let device_id_for_task = device_id.clone();
                let action_for_task = action.clone();
                let action_for_callback = action.clone();

                return Task::perform(
                    async move {
                        backend
                            .perform_action(&device_id_for_task, &action_for_task)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| {
                        Message::ActionFinished(
                            device_id.clone(),
                            action_for_callback.clone(),
                            result,
                        )
                    },
                )
                .map(cosmic::Action::App);
            }
            Message::ActionFinished(device_id, action, result) => {
                let device_name = self
                    .devices
                    .iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                let backend = self.backend.clone();

                let was_unpair = matches!(&action, ActionType::Unpair);
                let quiet = is_quiet_action(&action);
                let action_label = action.label().to_string();
                let fname = if let ActionType::SendFile(path) = &action {
                    path.rsplit('/')
                        .next()
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                match &result {
                    Ok(message) => log::info!(
                        "Action '{action_label}' on '{device_name}' succeeded: {message}"
                    ),
                    Err(error) => {
                        log::error!("Action '{action_label}' on '{device_name}' failed: {error}")
                    }
                }
                if result.is_ok() {
                    let draft = self.draft_mut(&device_id);
                    match &action {
                        ActionType::ShareText(_) => draft.share_text.clear(),
                        ActionType::ShareUrl(_) => draft.share_url.clear(),
                        ActionType::ReplyToNotification(_, _) => {
                            draft.notify_reply_text.clear();
                            draft.selected_notification = None;
                        }
                        ActionType::Seek(position) => {
                            if let Some(player) = draft.player.as_mut() {
                                player.position = *position;
                            }
                            draft.seek_preview = None;
                        }
                        _ => {}
                    }
                } else if matches!(&action, ActionType::Seek(_)) {
                    self.draft_mut(&device_id).seek_preview = None;
                }
                let status_text = match &result {
                    Ok(message) => message.clone(),
                    Err(error) => error.clone(),
                };
                if !quiet || result.is_err() {
                    self.set_status(&device_id, status_text, result.is_err());
                }

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
                                let _ = backend
                                    .notify(&format!("{device_name}"), &notif_msg, 0, &[])
                                    .await;
                            }
                        }
                    },
                    |_| Message::RefreshDevices,
                )
                .map(cosmic::Action::App);
            }
            Message::RefreshNotifications(device_id) => {
                let Some(backend) = self.backend.clone() else {
                    return Task::none();
                };
                let did = device_id.clone();
                return Task::perform(
                    async move { backend.fetch_notifications(&did).await },
                    move |notifs| Message::NotificationsLoaded(device_id, notifs),
                )
                .map(cosmic::Action::App);
            }
            Message::NotificationsLoaded(device_id, notifications) => {
                let draft = self.draft_mut(&device_id);
                if let Some(selected) = draft.selected_notification.as_deref() {
                    if !notifications
                        .iter()
                        .any(|notif| notif.internal_id.as_str() == selected)
                    {
                        draft.selected_notification = None;
                        draft.notify_reply_text.clear();
                    }
                }
                draft.notifications = notifications;
            }
            Message::SelectNotification(device_id, internal_id) => {
                let draft = self.draft_mut(&device_id);
                if draft.selected_notification.as_deref() == Some(internal_id.as_str()) {
                    draft.selected_notification = None;
                } else {
                    draft.selected_notification = Some(internal_id);
                    draft.notify_reply_text.clear();
                }
            }
            Message::NotifyReplyChanged(device_id, text) => {
                self.draft_mut(&device_id).notify_reply_text = text;
            }
            Message::SendNotifyReply(device_id, internal_id) => {
                let text = self.draft_mut(&device_id).notify_reply_text.clone();
                if text.trim().is_empty() {
                    return Task::none();
                }
                log::debug!("SendNotifyReply: notif {internal_id}, {} chars", text.len());
                let action = ActionType::ReplyToNotification(internal_id, text);
                return Task::perform(async {}, move |_| Message::PerformAction(device_id, action))
                    .map(cosmic::Action::App);
            }
            Message::DismissNotification(device_id, internal_id) => {
                log::debug!("DismissNotification: notif {internal_id} on {device_id}");
                let draft = self.draft_mut(&device_id);
                if draft.selected_notification.as_deref() == Some(internal_id.as_str()) {
                    draft.selected_notification = None;
                    draft.notify_reply_text.clear();
                }
                return Task::perform(async {}, move |_| {
                    Message::PerformAction(device_id, ActionType::DismissNotification(internal_id))
                })
                .map(cosmic::Action::App);
            }
            Message::DismissAllNotifications(device_id) => {
                let backend = self.backend.clone();
                let draft = self.draft_mut(&device_id);
                let notifs = draft.notifications.clone();
                draft.notifications.clear();
                draft.selected_notification = None;
                draft.notify_reply_text.clear();
                log::debug!(
                    "DismissAllNotifications: {} notifications on {device_id}",
                    notifs.len()
                );
                let Some(backend) = backend else {
                    return Task::none();
                };
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
                let Some(backend) = self.backend.clone() else {
                    return Task::none();
                };
                let did = device_id.clone();
                return Task::perform(
                    async move { backend.player_info(&did).await },
                    move |player| Message::PlayerInfoUpdated(device_id, player),
                )
                .map(cosmic::Action::App);
            }
            Message::PlayerInfoUpdated(device_id, player) => {
                let draft = self.draft_mut(&device_id);
                let reset_preview = match (&draft.player, &player) {
                    (_, None) => true,
                    (Some(old), Some(new)) => {
                        old.player != new.player
                            || old.title != new.title
                            || old.length != new.length
                    }
                    (None, Some(_)) => false,
                };
                if reset_preview {
                    draft.seek_preview = None;
                }
                draft.player = player;
            }
            Message::ConnectivityUpdated(device_id, connectivity) => {
                self.draft_mut(&device_id).connectivity = connectivity;
            }
            Message::MediaAction(device_id, action) => {
                return Task::perform(async {}, move |_| {
                    Message::PerformAction(device_id, ActionType::MediaAction(action))
                })
                .map(cosmic::Action::App);
            }
            Message::SeekPreview(device_id, position) => {
                let draft = self.draft_mut(&device_id);
                let length = draft
                    .player
                    .as_ref()
                    .filter(|player| player.can_seek && player.length > 0)
                    .map(|player| player.length);
                let Some(length) = length else {
                    return Task::none();
                };
                draft.seek_preview = Some(position.clamp(0, length));
            }
            Message::SeekCommit(device_id) => {
                let (position, current, can_seek) = {
                    let draft = self.draft_mut(&device_id);
                    (
                        draft.seek_preview,
                        draft.player.as_ref().map(|player| player.position),
                        draft
                            .player
                            .as_ref()
                            .is_some_and(|player| player.can_seek && player.length > 0),
                    )
                };
                let Some(position) = position else {
                    return Task::none();
                };
                if !can_seek {
                    self.draft_mut(&device_id).seek_preview = None;
                    return Task::none();
                }
                if current.is_some_and(|current| (position - current).abs() < 500) {
                    self.draft_mut(&device_id).seek_preview = None;
                    return Task::none();
                }
                return Task::perform(async {}, move |_| {
                    Message::PerformAction(device_id, ActionType::Seek(position))
                })
                .map(cosmic::Action::App);
            }
            Message::SelectPlayer(device_id, player) => {
                return Task::perform(async {}, move |_| {
                    Message::PerformAction(device_id, ActionType::SelectPlayer(player))
                })
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
                if pct_block <= prev {
                    return Task::none();
                }
                self.last_notif_pct.insert(nkey.clone(), pct_block);
                log::debug!("TransferProgress: {transfer_id} at {percent}%");
                if let Some(&notif_id) = self.active_notifs.get(&nkey) {
                    let Some(backend) = self.backend.clone() else {
                        return Task::none();
                    };
                    let nk = nkey.clone();
                    return Task::perform(
                        async move {
                            backend
                                .notify(
                                    "Transfer in progress",
                                    &format!("{percent}%"),
                                    notif_id,
                                    &[],
                                )
                                .await
                                .ok()
                        },
                        move |new_id| {
                            if let Some(id) = new_id {
                                Message::NotifCreated(nk, id)
                            } else {
                                Message::NoOp
                            }
                        },
                    )
                    .map(cosmic::Action::App);
                } else if self.pending_notif_requests.contains(&nkey) {
                    return Task::none();
                } else {
                    self.pending_notif_requests.insert(nkey.clone());
                    let Some(backend) = self.backend.clone() else {
                        return Task::none();
                    };
                    let nk = nkey.clone();
                    return Task::perform(
                        async move {
                            backend
                                .notify("Receiving file", &format!("{percent}%"), 0, &[])
                                .await
                                .ok()
                        },
                        move |notif_id| {
                            if let Some(id) = notif_id {
                                Message::NotifCreated(nk, id)
                            } else {
                                Message::NoOp
                            }
                        },
                    )
                    .map(cosmic::Action::App);
                }
            }
            Message::TransferFinished(device_id, transfer_id, file_path) => {
                log::info!(
                    "TransferFinished: {file_path} from {device_id} (transfer {transfer_id})"
                );
                let nkey = format!("transfer:{transfer_id}");
                let file_name = file_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&file_path)
                    .to_string();
                let notif_name = file_name.clone();

                let device_name = self
                    .devices
                    .iter()
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
                    self.deferred_notifs
                        .insert(nkey, (device_name, notif_name, dest_path));
                } else {
                    let backend = self.backend.clone();
                    return Task::perform(
                        async move {
                            let Some(backend) = backend else { return };
                            let _ = backend
                                .notify(
                                    &format!("File received from {device_name}"),
                                    &format!("{notif_name} → {dest_path}"),
                                    notif_id,
                                    &[],
                                )
                                .await;
                        },
                        |_| Message::NoOp,
                    )
                    .map(cosmic::Action::App);
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
                let Some(backend) = self.backend.clone() else {
                    return Task::none();
                };
                return Task::perform(
                    async move {
                        let _ = backend
                            .notify("Transfer failed", &error_string, notif_id, &[])
                            .await;
                    },
                    |_| Message::NoOp,
                )
                .map(cosmic::Action::App);
            }
            Message::FileReceived(device_id, file_path) => {
                log::info!("FileReceived: {file_path} from {device_id}");
                let file_name = file_path
                    .rsplit('/')
                    .next()
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

                if files
                    .iter()
                    .any(|f| f.file_path == file_path && !f.file_path.is_empty())
                {
                    return Task::none();
                }

                files.push(rf);
                if files.len() > MAX_RECEIVED_HISTORY {
                    files.remove(0);
                }
            }
            Message::PingReceived(device_id) => {
                log::info!("PingReceived from {device_id}");
                let device_name = self
                    .devices
                    .iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| device_id.clone());
                if let Some(backend) = self.backend.clone() {
                    return Task::perform(
                        async move {
                            let _ = backend
                                .notify(
                                    &format!("Ping from {device_name}"),
                                    "Your device sent a ping!",
                                    0,
                                    &[],
                                )
                                .await;
                        },
                        |_| Message::NoOp,
                    )
                    .map(cosmic::Action::App);
                }
            }
            Message::ClipboardReceived(device_id, content) => {
                log::debug!(
                    "ClipboardReceived from {device_id} ({} chars)",
                    content.len()
                );
                let device_name = self
                    .devices
                    .iter()
                    .find(|d| d.id == device_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| device_id.clone());
                let preview = truncate_text(&content, 80);
                if let Some(backend) = self.backend.clone() {
                    return Task::perform(
                        async move {
                            let _ = backend
                                .notify(&format!("Clipboard from {device_name}"), &preview, 0, &[])
                                .await;
                        },
                        |_| Message::NoOp,
                    )
                    .map(cosmic::Action::App);
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
                    if let Some((device_name, notif_name, dest_path)) =
                        self.deferred_notifs.remove(&key)
                    {
                        if let Some(backend) = self.backend.clone() {
                            return Task::perform(
                                async move {
                                    let _ = backend
                                        .notify(
                                            &format!("File received from {device_name}"),
                                            &format!("{notif_name} → {dest_path}"),
                                            notif_id,
                                            &[],
                                        )
                                        .await;
                                },
                                |_| Message::NoOp,
                            )
                            .map(cosmic::Action::App);
                        }
                    } else {
                        self.active_notifs.insert(key, notif_id);
                    }
                } else {
                    log::info!(
                        "Pairing request notification created for '{key}' (notif {notif_id})"
                    );
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
                        )
                        .map(cosmic::Action::App);
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
            Message::SetCompactPanel(enabled) => {
                self.compact_panel = enabled;
                save_compact_panel(enabled);
            }
            Message::SetPopupView(view) => {
                self.popup_view = view;
                self.device_list_open = false;
                if view == PopupView::Device {
                    if let Some(device_id) = self.selected_device_id.clone() {
                        self.mark_files_read(&device_id);
                    }
                }
            }
            Message::SelectShareKind(device_id, kind) => {
                self.draft_mut(&device_id).composer_mode = kind;
            }
            Message::SendShareText(device_id) => {
                let text = self.draft_mut(&device_id).share_text.trim().to_string();
                if text.is_empty() {
                    return Task::none();
                }
                return Task::perform(async {}, move |_| {
                    Message::PerformAction(device_id, ActionType::ShareText(text))
                })
                .map(cosmic::Action::App);
            }
            Message::SendShareUrl(device_id) => {
                let url = self.draft_mut(&device_id).share_url.trim().to_string();
                if url.is_empty() {
                    return Task::none();
                }
                if !is_valid_url(&url) {
                    self.set_status(&device_id, "Enter a valid http(s) URL", true);
                    return Task::none();
                }
                return Task::perform(async {}, move |_| {
                    Message::PerformAction(device_id, ActionType::ShareUrl(url))
                })
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
            Message::FileOpenFinished(device_id, result) => match result {
                Ok(()) => {}
                Err(error) => self.set_status(&device_id, error, true),
            },
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
                if self.advanced_device.as_deref() == Some(&id) {
                    self.advanced_device = None;
                } else {
                    self.advanced_device = Some(id);
                }
            }
            Message::Tick => {}
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let suggested = self.core.applet.suggested_size(false);
        let reachable = self
            .selected_device()
            .map(|device| device.is_reachable)
            .unwrap_or(false);
        let unread: usize = self
            .received_files
            .values()
            .flat_map(|files| files.iter())
            .filter(|file| file.unread)
            .count();

        if self.compact_panel {
            let icon_name = self
                .selected_device()
                .map(|device| device.device_type.icon_name())
                .unwrap_or("phone-symbolic");
            let icon = icon::from_name(icon_name).size(suggested.1.saturating_sub(4));
            let button = self
                .core
                .applet
                .button_from_element(icon, true)
                .on_press_down(Message::TogglePopup);

            let mut tooltip_label = self.panel_preview();
            if unread > 0 {
                tooltip_label.push_str(&format!(" - {unread} unread"));
            }
            let tooltip_text = text::caption(tooltip_label).size(SIZE_CAPTION);

            return self
                .core
                .applet
                .autosize_window(tooltip(button, tooltip_text, tooltip::Position::Top))
                .into();
        }

        let icon_name = if reachable {
            "io.github.acemythos.Connect-symbolic"
        } else {
            "io.github.acemythos.Connect-off-symbolic"
        };
        let icon = icon::from_name(icon_name).size(suggested.1.saturating_sub(4));

        let preview = text::caption(self.panel_preview()).size(SIZE_CAPTION);
        let mut content = row![icon, preview].spacing(6).align_y(Alignment::Center);

        if unread > 0 {
            content = content.push(text::caption(format!("({unread})")).size(SIZE_CAPTION));
        }

        let coated = container(content)
            .padding([5, 8, 0, 8])
            .style(glass_coating_style);

        let btn = self
            .core
            .applet
            .button_from_element(coated, true)
            .width(Length::Shrink)
            .on_press_down(Message::TogglePopup);

        self.core.applet.autosize_window(btn).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let body = match self.popup_view {
            PopupView::Home => self.render_home_view(),
            PopupView::Alerts => self.render_alerts_view(),
            PopupView::Device => self.render_device_view(),
        };

        let edge_highlight = container(row![])
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

        let scroll_id = match self.popup_view {
            PopupView::Home => "cosmic-connect-scroll-home",
            PopupView::Alerts => "cosmic-connect-scroll-alerts",
            PopupView::Device => "cosmic-connect-scroll-device",
        };
        let panel = scrollable(
            column![
                edge_highlight,
                container(body).padding([16, 16]).width(Length::Fill),
            ]
            .spacing(0),
        )
        .id(scroll_id)
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
                            .add_stop(
                                0.5,
                                Color::from_rgba(base.r * 0.96, base.g * 0.96, base.b * 0.96, a1),
                            )
                            .add_stop(
                                1.0,
                                Color::from_rgba(base.r * 0.92, base.g * 0.92, base.b * 0.92, a2),
                            ),
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
                .max_height(750.0),
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

            futures_util::stream::select(futures_util::stream::select(poll, signals), notif_actions)
        });

        let mut subscriptions = vec![inner];

        if self.popup.is_some() {
            subscriptions.push(time::every(Duration::from_millis(500)).map(|_| Message::Tick));
        }

        Subscription::batch(subscriptions)
    }
}

fn glass_coating_style(theme: &cosmic::Theme) -> iced_container::Style {
    iced_container::Style {
        background: Some(Background::Color(frosted_bg(
            theme,
            CARD_LUM_DARKEN,
            OPACITY_COATING,
        ))),
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
        background: Some(Background::Color(frosted_bg(
            theme,
            CARD_LUM_DARKEN,
            OPACITY_CARD,
        ))),
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
        log::info!(
            "Poll: {} devices, {} pairing requests",
            devices.len(),
            pairing.len()
        );
        for d in &devices {
            log::debug!(
                "Poll: device '{}' (reachable={}, paired={}, state={})",
                d.name,
                d.is_reachable,
                d.is_paired,
                d.pair_state
            );
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
                                    device_id,
                                    transfer_id,
                                    file_name,
                                    total_bytes,
                                );
                            }
                        }
                        ("share", "transferProgress") => {
                            if let Ok((transfer_id, transferred, total, percent)) =
                                msg.body().deserialize::<(String, u64, u64, i32)>()
                            {
                                return Message::TransferProgress(
                                    device_id,
                                    transfer_id,
                                    transferred,
                                    total,
                                    percent,
                                );
                            }
                        }
                        ("share", "transferFinished") => {
                            if let Ok((transfer_id, url)) =
                                msg.body().deserialize::<(String, String)>()
                            {
                                return Message::TransferFinished(device_id, transfer_id, url);
                            }
                        }
                        ("share", "transferFailed") => {
                            if let Ok((transfer_id, code, error_str)) =
                                msg.body().deserialize::<(String, i32, String)>()
                            {
                                return Message::TransferFailed(
                                    device_id,
                                    transfer_id,
                                    code,
                                    error_str,
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
                                log::warn!(
                                    "Notification action stream setup failed: {e}, retrying in 5s"
                                );
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
    use super::{
        format_bytes, format_ms, is_quiet_action, is_valid_url, parse_device_signal_path,
        truncate_text, CosmicConnect, DeviceDraft, Message, PlayerInfo, PopupView, ShareKind,
    };
    use crate::model::ActionType;
    use cosmic::Application;

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

    #[test]
    fn accepts_only_http_urls_with_a_host() {
        assert!(is_valid_url("https://example.com/path"));
        assert!(is_valid_url("http://localhost:8080"));
        assert!(!is_valid_url("example.com"));
        assert!(!is_valid_url("ftp://example.com"));
        assert!(!is_valid_url("https://"));
        assert!(!is_valid_url("https://bad host"));
    }

    #[test]
    fn formats_media_times_and_sizes() {
        assert_eq!(format_ms(21_465), "0:21");
        assert_eq!(format_ms(223_981), "3:43");
        assert_eq!(format_ms(3_661_000), "1:01:01");
        assert_eq!(format_ms(-1), "0:00");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(4_200_000), "4.0 MB");
    }

    #[test]
    fn navigates_views_and_switches_composer_mode() {
        let mut app = CosmicConnect::default();
        app.drafts.insert("dev".to_string(), DeviceDraft::default());

        let _ = app.update(Message::SetPopupView(PopupView::Alerts));
        assert_eq!(app.popup_view, PopupView::Alerts);

        let _ = app.update(Message::SelectShareKind("dev".to_string(), ShareKind::Url));
        assert_eq!(app.drafts["dev"].composer_mode, ShareKind::Url);

        let _ = app.update(Message::SelectNotification(
            "dev".to_string(),
            "n1".to_string(),
        ));
        assert_eq!(
            app.drafts["dev"].selected_notification.as_deref(),
            Some("n1")
        );

        let _ = app.update(Message::SelectNotification(
            "dev".to_string(),
            "n1".to_string(),
        ));
        assert_eq!(app.drafts["dev"].selected_notification, None);
    }

    #[test]
    fn seek_action_is_quiet_and_labelled() {
        let action = ActionType::Seek(12_000);
        assert_eq!(action.label(), "Seek");
        assert!(is_quiet_action(&action));
        assert!(!is_quiet_action(&ActionType::Ring));
    }

    #[test]
    fn seek_preview_clamps_to_track_length() {
        let mut app = CosmicConnect::default();
        app.drafts.insert(
            "dev".to_string(),
            DeviceDraft {
                player: Some(PlayerInfo {
                    can_seek: true,
                    length: 10_000,
                    position: 1_000,
                    ..PlayerInfo::default()
                }),
                ..DeviceDraft::default()
            },
        );

        let _ = app.update(Message::SeekPreview("dev".to_string(), 20_000));
        assert_eq!(app.drafts["dev"].seek_preview, Some(10_000));

        let _ = app.update(Message::SeekPreview("dev".to_string(), -500));
        assert_eq!(app.drafts["dev"].seek_preview, Some(0));
    }

    #[test]
    fn successful_seek_updates_position_without_status_banner() {
        let mut app = CosmicConnect::default();
        app.drafts.insert(
            "dev".to_string(),
            DeviceDraft {
                player: Some(PlayerInfo {
                    can_seek: true,
                    length: 10_000,
                    position: 1_000,
                    ..PlayerInfo::default()
                }),
                seek_preview: Some(5_000),
                ..DeviceDraft::default()
            },
        );

        let _ = app.update(Message::ActionFinished(
            "dev".to_string(),
            ActionType::Seek(5_000),
            Ok("Playback position updated".into()),
        ));

        let draft = &app.drafts["dev"];
        assert_eq!(draft.player.as_ref().unwrap().position, 5_000);
        assert_eq!(draft.seek_preview, None);
        assert!(draft.status.is_none());
    }

    #[test]
    fn failed_seek_clears_preview_and_shows_error() {
        let mut app = CosmicConnect::default();
        app.drafts.insert(
            "dev".to_string(),
            DeviceDraft {
                player: Some(PlayerInfo {
                    can_seek: true,
                    length: 10_000,
                    position: 1_000,
                    ..PlayerInfo::default()
                }),
                seek_preview: Some(5_000),
                ..DeviceDraft::default()
            },
        );

        let _ = app.update(Message::ActionFinished(
            "dev".to_string(),
            ActionType::Seek(5_000),
            Err("seek failed".into()),
        ));

        let draft = &app.drafts["dev"];
        assert_eq!(draft.seek_preview, None);
        assert_eq!(draft.status.as_deref(), Some("seek failed"));
        assert!(draft.status_error);
    }

    #[test]
    fn player_update_keeps_preview_for_same_track_and_clears_on_change() {
        let mut app = CosmicConnect::default();
        app.drafts.insert(
            "dev".to_string(),
            DeviceDraft {
                player: Some(PlayerInfo {
                    player: "Metrolist".into(),
                    title: "Song".into(),
                    length: 10_000,
                    position: 1_000,
                    ..PlayerInfo::default()
                }),
                seek_preview: Some(5_000),
                ..DeviceDraft::default()
            },
        );

        let _ = app.update(Message::PlayerInfoUpdated(
            "dev".to_string(),
            Some(PlayerInfo {
                player: "Metrolist".into(),
                title: "Song".into(),
                length: 10_000,
                position: 2_000,
                ..PlayerInfo::default()
            }),
        ));
        assert_eq!(app.drafts["dev"].seek_preview, Some(5_000));

        let _ = app.update(Message::PlayerInfoUpdated(
            "dev".to_string(),
            Some(PlayerInfo {
                player: "Metrolist".into(),
                title: "Other song".into(),
                length: 10_000,
                position: 0,
                ..PlayerInfo::default()
            }),
        ));
        assert_eq!(app.drafts["dev"].seek_preview, None);
    }
}
