use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use ashpd::desktop::file_chooser::SelectedFiles;
use cosmic::app::Core;
use cosmic::cctk::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use cosmic::cosmic_config::{Config as CosmicConfig, ConfigGet, ConfigSet};
use cosmic::iced::animation::{Animation, Easing};
use cosmic::iced::core::text::{Ellipsize, EllipsizeHeightLimit, LineHeight};
use cosmic::iced::core::Alignment;
use cosmic::iced::gradient;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
};
use cosmic::iced::platform_specific::shell::commands::popup::destroy_popup;
use cosmic::iced::runtime::platform_specific::wayland::layer_surface::Action as LayerSurfaceAction;
use cosmic::iced::runtime::platform_specific::wayland::Action as WaylandAction;
use cosmic::iced::runtime::platform_specific::Action as PlatformAction;
use cosmic::iced::time;
use cosmic::iced::widget::Stack;
use cosmic::iced::window::Id;
use cosmic::iced::{
    Background, Border, Color, Gradient, Length, Limits, Rectangle, Shadow, Subscription, Vector,
};
use cosmic::surface;
use cosmic::theme;
use cosmic::widget::autosize::autosize;
use cosmic::widget::container as iced_container;
use cosmic::widget::text_input;
use cosmic::widget::{
    button, column, container, divider, icon, progress_bar, row, scrollable, settings, slider,
    text, toggler, tooltip,
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
use crate::island::{FeedAction, FeedActionKind, FeedActivity, FeedKind};
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
const FLOATING_ISLAND_KEY: &str = "floating_island";
const PANEL_BOLT_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M10 1 2 9.5h4.5L6 15l8-8.5H9.5L10 1Z" fill="white"/></svg>"#;

const ISLAND_CANVAS_WIDTH: u32 = 520;
const ISLAND_CANVAS_HEIGHT: u32 = 208;
const ISLAND_TOP_OFFSET: f32 = 8.0;
const ISLAND_PEEK_WIDTH: f32 = 304.0;
const ISLAND_PEEK_HEIGHT: f32 = 46.0;
const ISLAND_PEEK_RADIUS: f32 = 23.0;
const ISLAND_EXPANDED_WIDTH: f32 = 428.0;
const ISLAND_EXPANDED_HEIGHT: f32 = 158.0;
const ISLAND_EXPANDED_RADIUS: f32 = 30.0;
const ISLAND_MORPH_DURATION: Duration = Duration::from_millis(420);
const ISLAND_COLLAPSE_MORPH_DURATION: Duration = Duration::from_millis(280);
const ISLAND_HOVER_EXPAND_DELAY: Duration = Duration::from_millis(420);
const ISLAND_HOVER_COLLAPSE_DELAY: Duration = Duration::from_millis(450);
const ISLAND_EQ_INTERVAL: Duration = Duration::from_millis(90);
const ISLAND_RECONCILE_INTERVAL: Duration = Duration::from_millis(250);
const HELPER_FEED_INTERVAL: Duration = Duration::from_millis(200);
const PLAYER_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const TRANSFER_DONE_HOLD: Duration = Duration::from_secs(4);
const TRANSFER_FAIL_HOLD: Duration = Duration::from_secs(6);
const MEDIA_PAUSE_HOLD: Duration = Duration::from_secs(3);
const HELPER_IDLE_GRACE: Duration = Duration::from_secs(8);
const HELPER_HIDDEN_EXIT: Duration = Duration::from_secs(5);
const ISLAND_NOTIFICATION_HOLD: Duration = Duration::from_secs(6);
const ISLAND_NOTIFICATION_QUEUE_MAX: usize = 3;
const ISLAND_NOTIFICATION_ACTION_MAX: usize = 3;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum IslandZone {
    #[default]
    Hidden,
    Peek,
    Expanded,
}

struct TransferFailure {
    file_name: String,
    device_id: String,
    error: String,
    at: Instant,
}

struct IslandNotification {
    activity: FeedActivity,
    expires_at: Option<Instant>,
}

struct IslandCard<'a> {
    kind: FeedKind,
    accent: Color,
    badge: String,
    title: String,
    subtitle: String,
    detail: Option<String>,
    art: Option<&'a str>,
    art_icon: &'a str,
    progress: Option<f32>,
    progress_text: Option<String>,
    device_id: String,
    device_name: String,
    position_ms: Option<i64>,
    length_ms: Option<i64>,
    can_seek: bool,
    compact: Option<String>,
    actions: Vec<Element<'a, Action<Message>>>,
    lock_expanded: bool,
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
    player_updated_at: Option<Instant>,
    player_last_active: Option<Instant>,
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
    phone_toasts: HashMap<String, u32>,
    last_sync: Option<Instant>,
    compact_panel: bool,
    panel_bolt_dimmed: bool,
    island_enabled: bool,
    island_notification: Option<IslandNotification>,
    island_notification_queue: VecDeque<FeedActivity>,
    island: Option<Id>,
    island_expanded: bool,
    island_pinned: bool,
    island_hovered: bool,
    island_hover_at: Option<Instant>,
    island_unhover_at: Option<Instant>,
    island_morph: Option<Animation<f32>>,
    island_eq_phase: f32,
    island_seek_preview: Option<i64>,
    island_seek_dragging: bool,
    island_seek_preview_at: Option<Instant>,
    island_hidden_key: Option<String>,
    island_zone: IslandZone,
    island_helper: Option<std::process::Child>,
    feed_activity: Option<FeedActivity>,
    feed_key: Option<String>,
    last_feed_line: Option<String>,
    helper_idle_since: Option<Instant>,
    helper_feed_seq: u64,
    helper_hidden_at: Option<Instant>,
    pairing_requests: HashSet<String>,
    transfer_failures: HashMap<String, TransferFailure>,
    player_refresh_inflight: bool,
    panel_hosted: bool,
    helper_mode: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    RefreshDevices,
    PopupClosed(Id),
    HelperWindowOpened(Id),
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
    PhoneNotificationChanged(String, String),
    PhoneNotificationRemoved(String, String),
    PhoneNotificationsCleared(String),
    PhoneNotificationFetched(String, String, Option<Notification>),
    PhoneToastShown(String, Result<u32, String>),
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
    IslandReconcile,
    IslandToggleExpand,
    IslandCollapse,
    IslandDismiss,
    IslandHoverEnter,
    IslandHoverExit,
    IslandCursorLeft(Id),
    IslandExpandIfHovered,
    IslandCollapseIfIdle,
    IslandSeekPreview(String, i64),
    IslandSeekCommit(String),
    IslandFrame,
    IslandEqTick,
    IslandFeedAction(FeedActionKind),
    HelperFeedPoll,
    HelperExitCheck,
    PlayerTick,
    SetCompactPanel(bool),
    SetIslandEnabled(bool),
    SetPopupView(PopupView),
    SelectShareKind(String, ShareKind),
    SendShareText(String),
    SendShareUrl(String),
    OpenReceivedFile(String, String),
    FileOpenFinished(String, Result<(), String>),
    ConnectivityUpdated(String, Option<ConnectivityInfo>),
    Tick,
    BoltBlink,
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

fn is_android_hotspot_status(notif: &Notification) -> bool {
    notif.ongoing
        && notif.app_name.trim().eq_ignore_ascii_case("Android System")
        && [
            notif.title.as_str(),
            notif.text.as_str(),
            notif.ticker.as_str(),
        ]
        .iter()
        .any(|field| field.to_ascii_lowercase().contains("mobile data hotspot"))
}

/// True when a phone notification is just the now-playing card for the player
/// that the MPRIS feed is already showing. Those must not take the island: the
/// notification is ongoing, so it would never expire and would outrank the real
/// media card forever. The notification still appears in the popup list.
fn is_media_notification(notif: &Notification, player: Option<&PlayerInfo>) -> bool {
    let Some(player) = player else {
        return false;
    };
    if !notif.ongoing || player.title.trim().is_empty() {
        return false;
    }
    let app = notif.app_name.trim();
    let player_name = player.player.trim();
    !app.is_empty() && !player_name.is_empty() && app.eq_ignore_ascii_case(player_name)
}

/// Android media notifications expose the full MediaSession action list with
/// accessibility-length labels. Shorten the ones we know so the 428 px island
/// card stays readable; unknown actions pass through untouched.
fn short_island_action_label(action: &str) -> &str {
    match action {
        "Rewind to previous item" => "Prev",
        "Forward to next item" => "Next",
        "Repeat mode off" | "Repeat mode on" => "Repeat",
        "Shuffle on" | "Shuffle off" => "Shuffle",
        other => other,
    }
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

fn load_island_enabled() -> bool {
    match CosmicConfig::new(ID, CONFIG_VERSION) {
        Ok(config) => match config.get::<bool>(FLOATING_ISLAND_KEY) {
            Ok(enabled) => enabled,
            Err(error) => {
                if error.is_err() {
                    log::warn!("Failed to load floating island setting: {error}");
                }
                true
            }
        },
        Err(error) => {
            if error.is_err() {
                log::warn!("Failed to open applet config: {error}");
            }
            true
        }
    }
}

fn save_island_enabled(enabled: bool) {
    match CosmicConfig::new(ID, CONFIG_VERSION) {
        Ok(config) => match config.set(FLOATING_ISLAND_KEY, enabled) {
            Ok(()) => log::info!(
                "Floating island set to {}",
                if enabled { "on" } else { "off" }
            ),
            Err(error) => log::warn!("Failed to save floating island setting: {error}"),
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

fn phone_toast_key(device_id: &str, public_id: &str) -> String {
    format!("{device_id}\u{0}{public_id}")
}

fn island_notification_key(device_id: &str, public_id: &str) -> String {
    format!("notification:{device_id}:{public_id}")
}

/// Identity used to keep a locally dismissed feed off the island. Media keeps a
/// stable feed key so a track change does not collapse an open card, so its
/// identity also carries the track title: dismissing music hides it until the
/// next track starts.
fn island_hide_key(activity: &FeedActivity) -> String {
    if activity.kind == FeedKind::Media {
        format!("{}|{}", activity.key, activity.title)
    } else {
        activity.key.clone()
    }
}

fn battery_plug_label(plugged: i32) -> &'static str {
    match plugged {
        1 => "AC",
        2 => "USB",
        4 => "Wireless",
        8 => "Dock",
        _ => "External power",
    }
}

fn battery_health_label(health: i32) -> &'static str {
    match health {
        2 => "Good",
        3 => "Overheat",
        4 => "Dead",
        5 => "Over-voltage",
        6 => "Unspecified failure",
        7 => "Cold",
        _ => "Unknown health",
    }
}

fn battery_telemetry(battery: &crate::model::BatteryInfo) -> Option<String> {
    let mut details = Vec::new();
    if let Some(temperature) = battery.temperature {
        details.push(format!("{temperature:.1} °C"));
    }
    if let Some(plugged) = battery.plugged {
        details.push(battery_plug_label(plugged).to_string());
    }
    if let Some(health) = battery.health {
        details.push(battery_health_label(health).to_string());
    }
    if let Some(technology) = battery.technology.as_deref() {
        details.push(technology.to_string());
    }
    if let Some(cycle_count) = battery.cycle_count {
        details.push(format!("{cycle_count} cycles"));
    }
    (!details.is_empty()).then(|| details.join(" · "))
}

fn island_notification_feed(device_id: &str, notif: &Notification) -> FeedActivity {
    let app = notif.app_name.trim();
    let title = notif.title.trim();
    let body = notif.text.trim();
    let ticker = notif.ticker.trim();

    let summary = if app.is_empty() {
        "Phone".to_string()
    } else {
        app.to_string()
    };
    let subtitle = if !title.is_empty() {
        title.to_string()
    } else if !body.is_empty() {
        truncate_text(body, 80)
    } else if !ticker.is_empty() {
        truncate_text(ticker, 80)
    } else {
        "New notification".to_string()
    };
    let detail = if !title.is_empty() && !body.is_empty() {
        Some(truncate_text(body, 160))
    } else if !ticker.is_empty() && ticker != subtitle {
        Some(truncate_text(ticker, 160))
    } else {
        None
    };
    let art = if notif.has_icon && !notif.icon_path.trim().is_empty() {
        Some(notif.icon_path.clone())
    } else {
        None
    };
    let mut actions: Vec<FeedAction> = notif
        .actions
        .iter()
        .map(|action| action.trim())
        .filter(|action| !action.is_empty() && !notif.internal_id.is_empty())
        .take(ISLAND_NOTIFICATION_ACTION_MAX)
        .map(|action| FeedAction {
            label: short_island_action_label(action).to_string(),
            icon: String::new(),
            action: FeedActionKind::NotificationAction {
                internal_id: notif.internal_id.clone(),
                action: action.to_string(),
            },
            as_text: true,
        })
        .collect();
    if notif.dismissable {
        actions.push(FeedAction {
            label: "Dismiss".to_string(),
            icon: "window-close-symbolic".to_string(),
            action: FeedActionKind::DismissNotification(notif.internal_id.clone()),
            as_text: true,
        });
    }

    FeedActivity {
        key: island_notification_key(device_id, &notif.id),
        kind: FeedKind::Notification,
        device_id: device_id.to_string(),
        title: summary,
        subtitle,
        detail,
        art,
        icon: "mail-unread-symbolic".to_string(),
        badge: "PHONE".to_string(),
        accent: [0.72, 0.58, 1.0],
        progress: None,
        progress_text: None,
        position_ms: None,
        length_ms: None,
        can_seek: false,
        device_name: String::new(),
        compact: None,
        primary: None,
        actions,
        expand: false,
        expires_at_ms: (!notif.ongoing)
            .then_some(crate::island::unix_ms() + ISLAND_NOTIFICATION_HOLD.as_millis() as u64),
    }
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

    fn selected_device_charging(&self) -> bool {
        self.selected_device().is_some_and(|device| {
            device.is_reachable
                && device
                    .battery
                    .as_ref()
                    .is_some_and(|battery| battery.is_charging)
        })
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
                settings::item(
                    "Floating island",
                    toggler(self.island_enabled).on_toggle(Message::SetIslandEnabled),
                ),
                text::caption(
                    "Show live activities in a floating capsule. When on, phone notifications, pairing, and transfers skip desktop toasts.",
                )
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
            if let Some(telemetry) = battery_telemetry(battery) {
                metadata = metadata.push(
                    row![
                        icon::from_name("battery-symbolic").size(16),
                        text::caption("Battery detail").size(SIZE_CAPTION),
                        container(row![]).width(Length::Fill),
                        text::body(telemetry).size(SIZE_CAPTION),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
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
        content.push(self.render_sub_header("Panel", None));
        content.push(self.render_panel_settings());
        if let Some(status) = self.render_status_banner(draft) {
            content.push(status);
        }

        column::with_children(content).spacing(10).into()
    }

    fn selected_device_name(&self, device_id: &str) -> String {
        self.devices
            .iter()
            .find(|device| device.id == device_id)
            .map(|device| device.name.clone())
            .unwrap_or_else(|| device_id.to_string())
    }

    fn tick_island_notifications(&mut self) {
        let now = Instant::now();
        if self
            .island_notification
            .as_ref()
            .is_some_and(|notification| {
                notification
                    .expires_at
                    .is_some_and(|expires_at| now >= expires_at)
            })
        {
            self.island_notification = None;
        }
        while self.island_notification.is_none() {
            let Some(activity) = self.island_notification_queue.pop_front() else {
                break;
            };
            let expires_at = activity
                .expires_at_ms
                .map(|_| now + ISLAND_NOTIFICATION_HOLD);
            self.island_notification = Some(IslandNotification {
                activity,
                expires_at,
            });
        }
    }

    fn push_island_notification(&mut self, activity: FeedActivity) {
        let now = Instant::now();
        if let Some(current) = self.island_notification.as_mut() {
            if current.activity.key == activity.key {
                let expires_at = activity
                    .expires_at_ms
                    .map(|_| now + ISLAND_NOTIFICATION_HOLD);
                current.activity = activity;
                current.expires_at = expires_at;
                return;
            }
            if self.island_notification_queue.len() >= ISLAND_NOTIFICATION_QUEUE_MAX {
                self.island_notification_queue.pop_front();
            }
            // An ongoing card never expires on its own, so once something newer
            // is waiting give it a bounded hold instead of starving the queue.
            if current.expires_at.is_none() {
                current.expires_at = Some(now + ISLAND_NOTIFICATION_HOLD);
            }
            self.island_notification_queue.push_back(activity);
            return;
        }
        let expires_at = activity
            .expires_at_ms
            .map(|_| now + ISLAND_NOTIFICATION_HOLD);
        self.island_notification = Some(IslandNotification {
            activity,
            expires_at,
        });
    }

    fn remove_island_notification(&mut self, device_id: &str, public_id: &str) -> bool {
        self.remove_island_notification_key(&island_notification_key(device_id, public_id))
    }

    fn remove_island_notification_key(&mut self, key: &str) -> bool {
        let changed = self
            .island_notification
            .as_ref()
            .is_some_and(|notification| notification.activity.key == key);
        if changed {
            self.island_notification = None;
        }
        let before = self.island_notification_queue.len();
        self.island_notification_queue
            .retain(|notification| notification.key != key);
        changed || self.island_notification_queue.len() != before
    }

    /// Drops island notifications that are only the now-playing card for the
    /// player we now know about. Notifications are fetched before the first
    /// MPRIS poll, so this also cleans up after the startup race: otherwise the
    /// ongoing card would keep the island forever and the media feed never wins.
    fn prune_media_island_notifications(&mut self, device_id: &str) -> bool {
        let keys: Vec<String> = {
            let Some(draft) = self.drafts.get(device_id) else {
                return false;
            };
            let Some(player) = draft.player.as_ref() else {
                return false;
            };
            draft
                .notifications
                .iter()
                .filter(|notification| is_media_notification(notification, Some(player)))
                .map(|notification| island_notification_key(device_id, &notification.id))
                .collect()
        };

        keys.iter().fold(false, |changed, key| {
            self.remove_island_notification_key(key) || changed
        })
    }

    fn clear_transient_island_notifications(&mut self, device_id: &str) -> bool {
        let prefix = format!("notification:{device_id}:");
        let changed = self
            .island_notification
            .as_ref()
            .is_some_and(|notification| {
                notification.activity.key.starts_with(&prefix) && notification.expires_at.is_some()
            });
        if changed {
            self.island_notification = None;
        }
        let before = self.island_notification_queue.len();
        self.island_notification_queue.retain(|notification| {
            !notification.key.starts_with(&prefix) || notification.expires_at_ms.is_none()
        });
        changed || self.island_notification_queue.len() != before
    }

    fn clear_all_island_notifications(&mut self) {
        self.island_notification = None;
        self.island_notification_queue.clear();
    }

    /// Highest-priority activity for the island, minus anything dismissed with
    /// the card's close button. A dismissal lasts until the activity changes.
    fn current_feed(&mut self) -> Option<FeedActivity> {
        let activity = self.select_feed();
        self.feed_visible(activity)
    }

    /// Applies a local dismissal. Both the applet (when it draws the island
    /// itself) and the helper process (which draws it in panel mode) run this,
    /// because a click lands in whichever process owns the surface.
    fn feed_visible(&mut self, activity: Option<FeedActivity>) -> Option<FeedActivity> {
        let Some(activity) = activity else {
            self.island_hidden_key = None;
            return None;
        };
        if self.island_hidden_key.as_deref() == Some(island_hide_key(&activity).as_str()) {
            return None;
        }
        self.island_hidden_key = None;
        Some(activity)
    }

    fn select_feed(&mut self) -> Option<FeedActivity> {
        self.tick_island_notifications();
        let now = Instant::now();
        self.transfer_failures
            .retain(|_, failure| now.duration_since(failure.at) < TRANSFER_FAIL_HOLD);

        let pairing_device = self
            .devices
            .iter()
            .find(|device| !device.is_paired && self.pairing_requests.contains(&device.id))
            .or_else(|| {
                self.devices
                    .iter()
                    .find(|device| !device.is_paired && device.pair_state == 2)
            });
        if let Some(device) = pairing_device {
            let device_id = device.id.clone();
            let device_name = device.name.clone();
            let icon = device.device_type.icon_name().to_string();
            return Some(FeedActivity {
                key: format!("pairing:{device_id}"),
                kind: FeedKind::Pairing,
                device_id,
                title: device_name.clone(),
                subtitle: "wants to pair".to_string(),
                detail: Some("Accept to link this device".to_string()),
                art: None,
                icon,
                badge: "PAIR".to_string(),
                accent: [1.0, 0.72, 0.35],
                progress: None,
                progress_text: None,
                position_ms: None,
                length_ms: None,
                can_seek: false,
                device_name,
                compact: None,
                primary: None,
                actions: vec![
                    FeedAction {
                        label: "Decline".to_string(),
                        icon: "window-close-symbolic".to_string(),
                        action: FeedActionKind::CancelPairing,
                        as_text: true,
                    },
                    FeedAction {
                        label: "Accept".to_string(),
                        icon: "emblem-ok-symbolic".to_string(),
                        action: FeedActionKind::AcceptPairing,
                        as_text: true,
                    },
                ],
                expand: true,
                expires_at_ms: None,
            });
        }

        if let Some(notification) = self.island_notification.as_ref() {
            return Some(notification.activity.clone());
        }

        let active_transfer = self
            .received_files
            .iter()
            .flat_map(|(device_id, files)| files.iter().map(move |file| (device_id.as_str(), file)))
            .filter(|(_, file)| file.active)
            .max_by_key(|(_, file)| file.received_at);
        if let Some((device_id, file)) = active_transfer {
            let device_id = device_id.to_string();
            let device_name = self.selected_device_name(&device_id);
            let transfer_id = if file.transfer_id.is_empty() {
                file.file_name.clone()
            } else {
                file.transfer_id.clone()
            };
            let percent = file.progress.min(100);
            let transferred = file.total_bytes.saturating_mul(percent as u64) / 100;
            let detail = if file.total_bytes > 0 {
                format!(
                    "{percent}% · {} of {}",
                    format_bytes(transferred),
                    format_bytes(file.total_bytes)
                )
            } else {
                format!("{percent}% received")
            };
            return Some(FeedActivity {
                key: format!("transfer:{device_id}:{transfer_id}"),
                kind: FeedKind::Transfer,
                device_id,
                title: file.file_name.clone(),
                subtitle: format!("Incoming from {device_name}"),
                detail: Some(detail),
                art: None,
                icon: "document-save-symbolic".to_string(),
                badge: "RECEIVING".to_string(),
                accent: [0.45, 0.90, 0.65],
                progress: Some(percent as f32 / 100.0),
                progress_text: Some(format!("{percent}%")),
                position_ms: None,
                length_ms: None,
                can_seek: false,
                device_name,
                compact: Some(format!("{percent}%")),
                primary: None,
                actions: Vec::new(),
                expand: false,
                expires_at_ms: None,
            });
        }

        let failure = self
            .transfer_failures
            .iter()
            .max_by_key(|(_, failure)| failure.at)
            .map(|(key, failure)| (key.clone(), failure));
        if let Some((key, failure)) = failure {
            let device_name = self.selected_device_name(&failure.device_id);
            return Some(FeedActivity {
                key: format!("transfer-fail:{key}"),
                kind: FeedKind::Transfer,
                device_id: failure.device_id.clone(),
                title: failure.file_name.clone(),
                subtitle: format!("Transfer failed · {device_name}"),
                detail: Some(failure.error.clone()),
                art: None,
                icon: "dialog-error-symbolic".to_string(),
                badge: "FAILED".to_string(),
                accent: [1.0, 0.45, 0.45],
                progress: None,
                progress_text: None,
                position_ms: None,
                length_ms: None,
                can_seek: false,
                device_name,
                compact: Some("Failed".to_string()),
                primary: None,
                actions: Vec::new(),
                expand: false,
                expires_at_ms: None,
            });
        }

        let done_transfer = self
            .received_files
            .iter()
            .flat_map(|(device_id, files)| files.iter().map(move |file| (device_id.as_str(), file)))
            .filter(|(_, file)| {
                !file.active
                    && file.progress >= 100
                    && std::time::SystemTime::now()
                        .duration_since(file.received_at)
                        .map(|elapsed| elapsed < TRANSFER_DONE_HOLD)
                        .unwrap_or(false)
            })
            .max_by_key(|(_, file)| file.received_at);
        if let Some((device_id, file)) = done_transfer {
            let device_id = device_id.to_string();
            let device_name = self.selected_device_name(&device_id);
            let transfer_id = if file.transfer_id.is_empty() {
                format!("file:{}", file.file_path)
            } else {
                file.transfer_id.clone()
            };
            let detail = if file.total_bytes > 0 {
                format!("{} · saved", format_bytes(file.total_bytes))
            } else {
                "Saved to Downloads".to_string()
            };
            return Some(FeedActivity {
                key: format!("transfer-done:{device_id}:{transfer_id}"),
                kind: FeedKind::Transfer,
                device_id,
                title: file.file_name.clone(),
                subtitle: format!("Received from {device_name}"),
                detail: Some(detail),
                art: None,
                icon: "emblem-ok-symbolic".to_string(),
                badge: "RECEIVED".to_string(),
                accent: [0.45, 0.90, 0.65],
                progress: Some(1.0),
                progress_text: Some("Done".to_string()),
                position_ms: None,
                length_ms: None,
                can_seek: false,
                device_name,
                compact: Some("Done".to_string()),
                primary: None,
                actions: Vec::new(),
                expand: false,
                expires_at_ms: None,
            });
        }

        let device_id = self.selected_device_id.clone()?;
        let device_name = self.selected_device_name(&device_id);
        let draft = self.drafts.get(&device_id)?;
        let player = draft.player.as_ref()?.clone();
        if player.title.trim().is_empty() {
            return None;
        }
        let show_media = player.is_playing
            || draft
                .player_last_active
                .is_some_and(|at| at.elapsed() < MEDIA_PAUSE_HOLD);
        if !show_media {
            return None;
        }
        let player_app = player.player.trim();
        let art = draft
            .notifications
            .iter()
            .rev()
            .find(|notification| {
                !player_app.is_empty()
                    && !notification.icon_path.trim().is_empty()
                    && notification
                        .app_name
                        .trim()
                        .eq_ignore_ascii_case(player_app)
            })
            .map(|notification| notification.icon_path.clone());
        let subtitle = if !player.artist.trim().is_empty() {
            player.artist.clone()
        } else if !player.album.trim().is_empty() {
            player.album.clone()
        } else {
            device_name.clone()
        };
        let detail = if !player.album.trim().is_empty() {
            Some(player.album.clone())
        } else {
            Some(format!("Playing on {device_name}"))
        };
        let length = player.length.max(0);
        let (progress, progress_text) = if length > 0 {
            let position = player.position.clamp(0, length);
            (
                Some(position as f32 / length as f32),
                Some(format!("{} / {}", format_ms(position), format_ms(length))),
            )
        } else {
            (None, None)
        };
        let (play_label, play_icon) = if player.is_playing {
            ("Pause", "media-playback-pause-symbolic")
        } else {
            ("Play", "media-playback-start-symbolic")
        };
        Some(FeedActivity {
            key: format!("media:{device_id}"),
            kind: FeedKind::Media,
            device_id,
            title: player.title.clone(),
            subtitle,
            detail,
            art,
            icon: "audio-x-generic-symbolic".to_string(),
            badge: if player.is_playing {
                "PLAYING".to_string()
            } else {
                "PAUSED".to_string()
            },
            accent: [0.55, 0.72, 1.0],
            progress,
            progress_text,
            position_ms: if length > 0 {
                Some(player.position.clamp(0, length))
            } else {
                None
            },
            length_ms: (length > 0).then_some(length),
            can_seek: player.can_seek && length > 0,
            device_name,
            compact: None,
            primary: Some(FeedAction {
                label: play_label.to_string(),
                icon: play_icon.to_string(),
                action: FeedActionKind::Media("PlayPause".to_string()),
                as_text: false,
            }),
            actions: vec![
                FeedAction {
                    label: "Previous".to_string(),
                    icon: "media-skip-backward-symbolic".to_string(),
                    action: FeedActionKind::Media("Previous".to_string()),
                    as_text: false,
                },
                FeedAction {
                    label: play_label.to_string(),
                    icon: play_icon.to_string(),
                    action: FeedActionKind::Media("PlayPause".to_string()),
                    as_text: false,
                },
                FeedAction {
                    label: "Next".to_string(),
                    icon: "media-skip-forward-symbolic".to_string(),
                    action: FeedActionKind::Media("Next".to_string()),
                    as_text: false,
                },
            ],
            expand: false,
            expires_at_ms: None,
        })
    }

    fn island_target_zone(&self) -> IslandZone {
        if self.feed_activity.is_none() {
            IslandZone::Hidden
        } else if self.island_expanded {
            IslandZone::Expanded
        } else {
            IslandZone::Peek
        }
    }

    fn set_island_expanded(&mut self, expanded: bool, pinned: bool) -> Task<Action<Message>> {
        let pairing = self
            .feed_activity
            .as_ref()
            .is_some_and(|activity| activity.kind == FeedKind::Pairing);
        if !expanded && pairing {
            return Task::none();
        }

        if self.island_expanded == expanded {
            if !expanded {
                self.island_pinned = false;
            } else if pinned {
                self.island_pinned = true;
            }
            return Task::none();
        }

        self.island_expanded = expanded;
        self.island_pinned = pinned;
        self.island_hover_at = None;
        self.island_unhover_at = None;

        if expanded {
            self.extend_island_notification_hold();
        }

        let target = if expanded { 1.0 } else { 0.0 };
        let now = Instant::now();
        let duration = if expanded {
            ISLAND_MORPH_DURATION
        } else {
            ISLAND_COLLAPSE_MORPH_DURATION
        };

        if self.island.is_none() {
            self.island_morph = Some(island_morph_animation(target, duration));
            return Task::none();
        }

        match self.island_morph.as_mut() {
            Some(morph) => {
                let current = morph.interpolate_with(|value| value, now);
                *morph = island_morph_animation(current, duration);
                morph.go_mut(target, now);
            }
            None => self.island_morph = Some(island_morph_animation(target, duration)),
        }

        let zone = if expanded {
            IslandZone::Expanded
        } else {
            IslandZone::Peek
        };
        self.island_zone = zone;
        log::debug!(
            "Island: expanded={} input zone -> {zone:?}",
            self.island_expanded
        );
        match self.island {
            Some(id) => island_set_zone(id, zone),
            None => Task::none(),
        }
    }

    fn extend_island_notification_hold(&mut self) {
        if let Some(notification) = self
            .island_notification
            .as_mut()
            .filter(|notification| notification.activity.kind == FeedKind::Notification)
        {
            if notification.expires_at.is_some() {
                notification.expires_at = Some(Instant::now() + ISLAND_NOTIFICATION_HOLD);
            }
        }
    }

    fn tick_island_hover(&mut self) -> Task<Action<Message>> {
        if self.island.is_none() {
            return Task::none();
        }

        let pairing = self
            .feed_activity
            .as_ref()
            .is_some_and(|activity| activity.kind == FeedKind::Pairing);

        if !self.island_expanded {
            if !self.island_hovered || pairing {
                return Task::none();
            }
            let started = *self.island_hover_at.get_or_insert_with(Instant::now);
            if started.elapsed() >= ISLAND_HOVER_EXPAND_DELAY {
                return self.set_island_expanded(true, false);
            }
            return Task::none();
        }

        if self.island_pinned || self.island_hovered || pairing {
            return Task::none();
        }

        let started = *self.island_unhover_at.get_or_insert_with(Instant::now);
        if started.elapsed() >= ISLAND_HOVER_COLLAPSE_DELAY {
            return self.set_island_expanded(false, false);
        }

        Task::none()
    }

    fn island_hover_enter(&mut self) -> Task<Action<Message>> {
        if self.island.is_none() {
            return Task::none();
        }
        self.island_hovered = true;
        self.island_unhover_at = None;
        if !self.island_expanded {
            self.island_hover_at = Some(Instant::now());
            return Task::perform(
                async { tokio::time::sleep(ISLAND_HOVER_EXPAND_DELAY).await },
                |_| Message::IslandExpandIfHovered,
            )
            .map(cosmic::Action::App);
        }
        Task::none()
    }

    fn island_hover_exit(&mut self) -> Task<Action<Message>> {
        self.island_hovered = false;
        self.island_hover_at = None;
        if self.island_expanded && !self.island_pinned {
            self.island_unhover_at = Some(Instant::now());
            return Task::perform(
                async { tokio::time::sleep(ISLAND_HOVER_COLLAPSE_DELAY).await },
                |_| Message::IslandCollapseIfIdle,
            )
            .map(cosmic::Action::App);
        }
        Task::none()
    }

    fn reconcile_island_seek_preview(&mut self, key_changed: bool) {
        if key_changed {
            self.island_seek_preview = None;
            self.island_seek_dragging = false;
            self.island_seek_preview_at = None;
            return;
        }
        if self.island_seek_dragging {
            return;
        }
        let Some(preview) = self.island_seek_preview else {
            return;
        };
        let position = self
            .feed_activity
            .as_ref()
            .and_then(|activity| activity.position_ms);
        if position.is_some_and(|position| (preview - position).abs() <= 1500) {
            self.island_seek_preview = None;
            self.island_seek_preview_at = None;
        }
    }

    fn tick_island_seek_preview(&mut self) {
        if self.island_seek_dragging {
            return;
        }
        if self
            .island_seek_preview_at
            .is_some_and(|at| at.elapsed() > Duration::from_secs(3))
        {
            self.island_seek_preview = None;
            self.island_seek_preview_at = None;
        }
    }

    fn reset_island_motion(&mut self) {
        self.island_expanded = false;
        self.island_pinned = false;
        self.island_hovered = false;
        self.island_hover_at = None;
        self.island_unhover_at = None;
        self.island_morph = None;
        self.island_eq_phase = 0.0;
        self.island_seek_preview = None;
        self.island_seek_dragging = false;
        self.island_seek_preview_at = None;
    }

    fn refresh_island(&mut self) -> Task<Action<Message>> {
        if self.helper_mode {
            return Task::none();
        }
        if !self.island_enabled {
            self.feed_activity = None;
            self.feed_key = None;
            self.last_feed_line = None;
            self.island_hidden_key = None;
            self.island_notification = None;
            self.island_notification_queue.clear();
            self.stop_island_helper();
            return self.destroy_island();
        }
        let activity = self.current_feed();
        let key_changed = activity.as_ref().map(|a| a.key.as_str()) != self.feed_key.as_deref();
        if key_changed {
            self.feed_key = activity.as_ref().map(|activity| activity.key.clone());
            self.island_hover_at = None;
            self.island_unhover_at = None;
        }
        self.feed_activity = activity;
        self.reconcile_island_seek_preview(key_changed);
        let expand = key_changed
            && self
                .feed_activity
                .as_ref()
                .is_some_and(|activity| activity.expand);
        let motion = if key_changed {
            self.set_island_expanded(expand, false)
        } else {
            Task::none()
        };
        motion.chain(self.sync_island())
    }

    fn apply_feed(&mut self, activity: Option<FeedActivity>) -> Task<Action<Message>> {
        let activity = self.feed_visible(activity);
        let key_changed = activity.as_ref().map(|a| a.key.as_str()) != self.feed_key.as_deref();
        if key_changed {
            self.feed_key = activity.as_ref().map(|activity| activity.key.clone());
            self.island_hover_at = None;
            self.island_unhover_at = None;
        }
        self.feed_activity = activity;
        self.reconcile_island_seek_preview(key_changed);
        let expand = key_changed
            && self
                .feed_activity
                .as_ref()
                .is_some_and(|activity| activity.expand);
        let motion = if key_changed {
            self.set_island_expanded(expand, false)
        } else {
            Task::none()
        };
        motion.chain(self.sync_island())
    }

    fn handle_feed_action(&mut self, action: FeedActionKind) -> Task<Action<Message>> {
        let Some(activity) = self.feed_activity.clone() else {
            return Task::none();
        };
        let device_id = activity.device_id.clone();
        log::info!("Island: feed action {action:?} for {device_id}");
        match action {
            FeedActionKind::Media(action) => Task::perform(async {}, move |_| {
                Message::PerformAction(device_id, ActionType::MediaAction(action))
            })
            .map(cosmic::Action::App),
            FeedActionKind::AcceptPairing => {
                self.pairing_requests.remove(&device_id);
                self.notified_pair_ids.remove(&device_id);
                let island_task = if self.helper_mode {
                    self.apply_feed(None)
                } else {
                    self.refresh_island()
                };
                let Some(backend) = self.backend.clone() else {
                    return island_task;
                };
                log::info!("Island: accepting pairing for {device_id}");
                let task = Task::perform(
                    async move { backend.accept_pairing(&device_id).await },
                    |result| match result {
                        Ok(()) => Message::RefreshDevices,
                        Err(error) => Message::BackendError(format!("Pairing accept: {error}")),
                    },
                )
                .map(cosmic::Action::App);
                island_task.chain(task)
            }
            FeedActionKind::CancelPairing => {
                self.pairing_requests.remove(&device_id);
                self.notified_pair_ids.remove(&device_id);
                let island_task = if self.helper_mode {
                    self.apply_feed(None)
                } else {
                    self.refresh_island()
                };
                let Some(backend) = self.backend.clone() else {
                    return island_task;
                };
                log::info!("Island: declining pairing for {device_id}");
                let task = Task::perform(
                    async move { backend.cancel_pairing(&device_id).await },
                    |result| match result {
                        Ok(()) => Message::RefreshDevices,
                        Err(error) => Message::BackendError(format!("Pairing cancel: {error}")),
                    },
                )
                .map(cosmic::Action::App);
                island_task.chain(task)
            }
            FeedActionKind::DismissNotification(internal_id) => {
                self.island_notification = None;
                let island_task = if self.helper_mode {
                    self.apply_feed(None)
                } else {
                    self.refresh_island()
                };
                let Some(backend) = self.backend.clone() else {
                    return island_task;
                };
                log::info!("Island: dismissing notification {internal_id} for {device_id}");
                let dismiss_device = device_id.clone();
                let task = Task::perform(
                    async move {
                        backend
                            .dismiss_notification(&dismiss_device, &internal_id)
                            .await
                    },
                    |result| match result {
                        Ok(()) => Message::RefreshNotifications(device_id),
                        Err(error) => {
                            Message::BackendError(format!("Dismiss notification: {error}"))
                        }
                    },
                )
                .map(cosmic::Action::App);
                island_task.chain(task)
            }
            FeedActionKind::NotificationAction {
                internal_id,
                action,
            } => {
                let Some(backend) = self.backend.clone() else {
                    return Task::none();
                };
                log::info!("Island: sending notification action {action:?} for {internal_id}");
                let action_device = device_id.clone();
                let refresh_device = device_id.clone();
                Task::perform(
                    async move {
                        backend
                            .send_notification_action(&action_device, &internal_id, &action)
                            .await
                    },
                    move |result| match result {
                        Ok(()) => Message::RefreshNotifications(refresh_device),
                        Err(error) => {
                            Message::BackendError(format!("Notification action: {error}"))
                        }
                    },
                )
                .map(cosmic::Action::App)
            }
        }
    }

    fn sync_island(&mut self) -> Task<Action<Message>> {
        if self.helper_mode {
            return self.sync_helper_surface();
        }
        if self.panel_hosted {
            return self.sync_panel_helper();
        }
        self.sync_standalone_surface()
    }

    fn sync_panel_helper(&mut self) -> Task<Action<Message>> {
        self.reap_island_helper();
        if self.feed_activity.is_none() {
            if self.island_helper.is_some() {
                self.push_feed_to_helper(None);
                let idle_since = *self.helper_idle_since.get_or_insert_with(Instant::now);
                if idle_since.elapsed() >= HELPER_IDLE_GRACE {
                    log::info!("Island helper: idle grace elapsed, stopping");
                    self.stop_island_helper();
                    self.last_feed_line = None;
                }
            }
            return Task::none();
        }

        self.helper_idle_since = None;
        if self.island_helper.is_none() {
            self.spawn_island_helper();
        }
        let activity = self.feed_activity.clone();
        self.push_feed_to_helper(activity.as_ref());
        Task::none()
    }

    fn sync_helper_surface(&mut self) -> Task<Action<Message>> {
        if self.feed_activity.is_none() {
            if self.island.is_some() {
                self.helper_hidden_at.get_or_insert_with(Instant::now);
            }
            return self.destroy_island();
        }
        self.helper_hidden_at = None;
        if self.island.is_none() {
            return self.spawn_island();
        }
        let target = self.island_target_zone();
        if target != self.island_zone {
            let Some(id) = self.island else {
                return Task::none();
            };
            self.island_zone = target;
            log::debug!("Island: input zone -> {target:?}");
            return island_set_zone(id, target);
        }
        Task::none()
    }

    fn sync_standalone_surface(&mut self) -> Task<Action<Message>> {
        if self.feed_activity.is_none() {
            self.island_expanded = false;
            return self.destroy_island();
        }
        if self.island.is_none() {
            return self.spawn_island();
        }
        let target = self.island_target_zone();
        if target != self.island_zone {
            let Some(id) = self.island else {
                return Task::none();
            };
            self.island_zone = target;
            log::debug!("Island: input zone -> {target:?}");
            return island_set_zone(id, target);
        }
        Task::none()
    }

    fn push_feed_to_helper(&mut self, activity: Option<&FeedActivity>) {
        let line = crate::island::encode(activity);
        if self.last_feed_line.as_deref() == Some(line.as_str()) {
            return;
        }
        let result = match self
            .island_helper
            .as_mut()
            .and_then(|child| child.stdin.as_mut())
        {
            Some(stdin) => {
                use std::io::Write;
                stdin
                    .write_all(format!("{line}\n").as_bytes())
                    .and_then(|_| stdin.flush())
            }
            None => return,
        };
        match result {
            Ok(()) => {
                log::debug!("Island feed: {line}");
                self.last_feed_line = Some(line);
            }
            Err(error) => {
                log::warn!("Island feed write failed: {error}");
                self.island_helper = None;
                self.last_feed_line = None;
            }
        }
    }

    fn destroy_island(&mut self) -> Task<Action<Message>> {
        let Some(id) = self.island.take() else {
            self.island_zone = IslandZone::Hidden;
            self.reset_island_motion();
            return Task::none();
        };
        self.reset_island_motion();
        self.island_zone = IslandZone::Hidden;
        log::info!("Island: destroying layer surface {id:?}");
        surface::surface_task(surface::action::destroy_layer_shell(id))
    }

    fn island_open(&self) -> bool {
        self.island.is_some() || self.island_helper.is_some()
    }

    fn reap_island_helper(&mut self) {
        let Some(child) = self.island_helper.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                log::info!("Island helper: exited ({status})");
                self.island_helper = None;
                self.last_feed_line = None;
                self.helper_idle_since = None;
            }
            Ok(None) => {}
            Err(error) => {
                log::warn!("Island helper: wait failed: {error}");
                self.island_helper = None;
                self.last_feed_line = None;
                self.helper_idle_since = None;
            }
        }
    }

    fn stop_island_helper(&mut self) {
        let Some(mut child) = self.island_helper.take() else {
            return;
        };
        log::info!("Island helper: stopping PID {}", child.id());
        let _ = child.kill();
        let _ = child.wait();
        self.last_feed_line = None;
        self.helper_idle_since = None;
    }

    fn spawn_island_helper(&mut self) {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(error) => {
                log::warn!("Island helper: cannot resolve executable: {error}");
                self.error = Some(format!("Floating island: {error}"));
                return;
            }
        };

        let mut command = std::process::Command::new(exe);
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .env_remove("WAYLAND_SOCKET")
            .env("COSMIC_CONNECT_HELPER", "1")
            .env("COSMIC_CONNECT_LOG_SUFFIX", "helper");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("COSMIC_PANEL_") {
                command.env_remove(key);
            }
        }
        if let Some(device_id) = self.selected_device_id.as_deref() {
            command.env("COSMIC_CONNECT_DEVICE_ID", device_id);
        }

        match command.spawn() {
            Ok(child) => {
                log::info!("Island helper: spawned PID {}", child.id());
                self.island_helper = Some(child);
                self.last_feed_line = None;
                self.helper_idle_since = None;
            }
            Err(error) => {
                log::warn!("Island helper: spawn failed: {error}");
                self.error = Some(format!("Floating island: {error}"));
            }
        }
    }

    fn spawn_island(&mut self) -> Task<Action<Message>> {
        let id = Id::unique();
        let initial_zone = if self.island_expanded {
            IslandZone::Expanded
        } else {
            IslandZone::Peek
        };
        self.island = Some(id);
        self.island_zone = initial_zone;
        self.island_morph = Some(island_morph_animation(
            if self.island_expanded { 1.0 } else { 0.0 },
            ISLAND_MORPH_DURATION,
        ));
        log::info!("Island: spawning layer surface {id:?} zone={initial_zone:?}");

        surface::surface_task(surface::action::app_layer_shell(
            |_app: &CosmicConnect| surface::action::LiveSettings {
                padding: Some(IcedMargin::default()),
                blur: Some(false),
                ..Default::default()
            },
            move |_app: &mut CosmicConnect| island_settings(id, initial_zone),
            Some(Box::new(|app: &CosmicConnect| app.render_island())),
        ))
    }

    fn toggle_island_expand(&mut self) -> Task<Action<Message>> {
        if self.feed_activity.is_none() {
            return Task::none();
        }
        if self.island_expanded {
            self.island_pinned = true;
            self.island_unhover_at = None;
            return Task::none();
        }
        self.set_island_expanded(true, true)
    }

    fn collapse_island(&mut self) -> Task<Action<Message>> {
        self.island_hovered = false;
        self.island_hover_at = None;
        self.island_unhover_at = None;
        self.set_island_expanded(false, false)
    }

    /// Hides the current card without touching the phone. The activity stays
    /// visible in the popup (Alerts, Files, or Home) and returns to the island
    /// once its identity changes.
    fn dismiss_island_feed(&mut self) -> Task<Action<Message>> {
        let Some(activity) = self.feed_activity.clone() else {
            return Task::none();
        };
        if activity.kind == FeedKind::Pairing {
            return Task::none();
        }
        log::info!("Island: dismissed {} locally", activity.key);
        let hidden = island_hide_key(&activity);

        if self.helper_mode {
            // The helper owns the surface here, so it clears its own card and
            // keeps filtering the same feed until the parent sends a new one.
            let task = self.apply_feed(None);
            self.island_hidden_key = Some(hidden);
            return task;
        }

        if activity.kind == FeedKind::Notification {
            // Otherwise an ongoing notification would stay current forever and
            // never let the queued ones through.
            self.remove_island_notification_key(&activity.key);
        }
        self.island_hidden_key = Some(hidden);
        self.reset_island_motion();
        self.refresh_island()
    }

    fn island_morph_at(&self, now: Instant) -> f32 {
        self.island_morph
            .as_ref()
            .map(|morph| morph.interpolate_with(|value| value, now))
            .unwrap_or(if self.island_expanded { 1.0 } else { 0.0 })
    }

    fn island_morph_animating(&self, now: Instant) -> bool {
        self.island.is_some()
            && self
                .island_morph
                .as_ref()
                .is_some_and(|morph| morph.is_animating(now))
    }

    fn island_media_playing(&self) -> bool {
        self.feed_activity.as_ref().is_some_and(|activity| {
            activity.kind == FeedKind::Media && activity.badge.eq_ignore_ascii_case("PLAYING")
        })
    }

    fn island_eq_active(&self) -> bool {
        self.island.is_some() && !self.island_expanded && self.island_media_playing()
    }

    fn render_island(&self) -> Element<'_, Action<Message>> {
        if self.island_zone == IslandZone::Hidden {
            return container(row![])
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        let Some(activity) = self.feed_activity.as_ref() else {
            return container(row![])
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        };

        let accent = Color::from_rgb(activity.accent[0], activity.accent[1], activity.accent[2]);
        let actions = activity
            .actions
            .iter()
            .map(|action| island_feed_action(action, accent))
            .collect::<Vec<_>>();

        let card = IslandCard {
            kind: activity.kind,
            accent,
            badge: activity.badge.clone(),
            title: activity.title.clone(),
            subtitle: activity.subtitle.clone(),
            detail: activity.detail.clone(),
            art: activity.art.as_deref(),
            art_icon: if activity.icon.trim().is_empty() {
                "phone-symbolic"
            } else {
                activity.icon.as_str()
            },
            progress: activity.progress,
            progress_text: activity.progress_text.clone(),
            device_id: activity.device_id.clone(),
            device_name: activity.device_name.clone(),
            position_ms: activity.position_ms,
            length_ms: activity.length_ms,
            can_seek: activity.can_seek,
            compact: activity.compact.clone(),
            actions,
            lock_expanded: activity.kind == FeedKind::Pairing,
        };

        let now = Instant::now();
        let morph = self.island_morph_at(now);
        let show_expanded = morph >= 0.5;
        let width = lerp(ISLAND_PEEK_WIDTH, ISLAND_EXPANDED_WIDTH, morph);
        let height = lerp(ISLAND_PEEK_HEIGHT, ISLAND_EXPANDED_HEIGHT, morph);
        let radius = lerp(ISLAND_PEEK_RADIUS, ISLAND_EXPANDED_RADIUS, morph);

        let content: Element<'_, Action<Message>> = if show_expanded {
            self.render_island_expanded(card)
        } else {
            self.render_island_peek(card)
        };

        let capsule = container(content)
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .padding(if show_expanded {
                [13.0, 16.0]
            } else {
                [6.0, 11.0]
            })
            .clip(true)
            .style(island_capsule_style(accent, radius));

        let capsule = cosmic::widget::mouse_area(capsule)
            .on_enter(Action::App(Message::IslandHoverEnter))
            .on_exit(Action::App(Message::IslandHoverExit));

        let capsule = if show_expanded {
            capsule
        } else {
            capsule.on_press(Action::App(Message::IslandToggleExpand))
        };

        container(capsule)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([ISLAND_TOP_OFFSET, 0.0])
            .align_x(Alignment::Center)
            .align_y(Alignment::Start)
            .into()
    }
    fn render_island_peek<'a>(&self, card: IslandCard<'a>) -> Element<'a, Action<Message>> {
        let IslandCard {
            kind,
            accent,
            title,
            subtitle,
            art,
            art_icon,
            progress,
            compact,
            ..
        } = card;

        let art = island_art_or_ring(kind, art, art_icon, progress, accent, 32.0, 9.0);

        let labels = column![
            text::text(truncate_text(title.trim(), 26))
                .size(13.0)
                .line_height(LineHeight::Absolute(15.0.into()))
                .class(theme::Text::Color(Color::from_rgb(0.96, 0.97, 0.99))),
            text::text(truncate_text(subtitle.trim(), 30))
                .size(11.0)
                .line_height(LineHeight::Absolute(13.0.into()))
                .class(theme::Text::Color(Color::from_rgb(0.70, 0.74, 0.82))),
        ]
        .spacing(0);

        let body = container(row![art, labels].spacing(9).align_y(Alignment::Center))
            .width(Length::Fill)
            .align_x(Alignment::Start)
            .align_y(Alignment::Center);

        let trailing: Option<Element<'_, Action<Message>>> = match kind {
            FeedKind::Media => Some(island_eq(self.island_eq_phase, accent)),
            _ => compact.map(|label| {
                container(
                    text::text(label)
                        .size(10.0)
                        .line_height(LineHeight::Absolute(12.0.into()))
                        .class(theme::Text::Color(accent)),
                )
                .padding([3, 7])
                .style(move |_theme: &cosmic::Theme| iced_container::Style {
                    background: Some(Background::Color(Color::from_rgba(
                        accent.r, accent.g, accent.b, 0.14,
                    ))),
                    border: Border {
                        radius: 9.0.into(),
                        width: 1.0,
                        color: Color::from_rgba(accent.r, accent.g, accent.b, 0.30),
                    },
                    ..Default::default()
                })
                .into()
            }),
        };

        let content: Element<'_, Action<Message>> = match trailing {
            Some(action) => row![body, action]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            None => row![body].into(),
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Start)
            .align_y(Alignment::Center)
            .into()
    }

    fn render_island_expanded<'a>(&self, card: IslandCard<'a>) -> Element<'a, Action<Message>> {
        let IslandCard {
            kind,
            accent,
            badge,
            title,
            subtitle,
            detail,
            art,
            art_icon,
            progress,
            progress_text,
            device_id,
            device_name,
            position_ms,
            length_ms,
            can_seek,
            actions,
            lock_expanded,
            ..
        } = card;

        let art = island_art_or_ring(kind, art, art_icon, progress, accent, 38.0, 11.0);

        let heading = column![
            text::text(truncate_text(title.trim(), 40))
                .size(14.5)
                .line_height(LineHeight::Absolute(17.0.into()))
                .wrapping(Wrapping::None)
                .ellipsize(Ellipsize::End)
                .class(theme::Text::Color(Color::from_rgb(0.97, 0.98, 1.0))),
            text::text(truncate_text(subtitle.trim(), 52))
                .size(11.5)
                .line_height(LineHeight::Absolute(14.0.into()))
                .wrapping(Wrapping::None)
                .ellipsize(Ellipsize::End)
                .class(theme::Text::Color(Color::from_rgb(0.70, 0.74, 0.82))),
        ]
        .spacing(1);

        let heading = cosmic::widget::mouse_area(
            container(heading)
                .width(Length::Fill)
                .align_x(Alignment::Start),
        )
        .on_press(Action::App(Message::IslandToggleExpand));

        let badge = container(
            text::text(badge)
                .size(9.0)
                .line_height(LineHeight::Absolute(11.0.into()))
                .class(theme::Text::Color(accent)),
        )
        .padding([3, 7])
        .style(move |_theme: &cosmic::Theme| iced_container::Style {
            background: Some(Background::Color(Color::from_rgba(
                accent.r, accent.g, accent.b, 0.14,
            ))),
            border: Border {
                radius: 9.0.into(),
                width: 1.0,
                color: Color::from_rgba(accent.r, accent.g, accent.b, 0.30),
            },
            ..Default::default()
        });

        let collapse: Element<'_, Action<Message>> = if lock_expanded {
            container(row![]).width(Length::Fixed(0.0)).into()
        } else {
            island_icon_button("go-up-symbolic", Message::IslandCollapse, accent, false)
        };

        let header = row![art, heading, badge, collapse,]
            .spacing(9)
            .align_y(Alignment::Center);

        let mut body = column![header].spacing(8);

        if let Some(detail) = detail {
            body = body.push(
                text::text(truncate_text(detail.trim(), 64))
                    .size(11.0)
                    .line_height(LineHeight::Absolute(14.0.into()))
                    .wrapping(Wrapping::None)
                    .ellipsize(Ellipsize::End)
                    .class(theme::Text::Color(Color::from_rgb(0.78, 0.81, 0.88))),
            );
        }

        if let Some(progress) = progress {
            if kind == FeedKind::Media && can_seek && length_ms.is_some_and(|length| length > 0) {
                let length = length_ms.unwrap_or_default().clamp(1, i32::MAX as i64);
                let current = self
                    .island_seek_preview
                    .or(position_ms)
                    .unwrap_or_default()
                    .clamp(0, length);
                let seek_device = device_id.clone();
                let commit_device = device_id.clone();
                let seek: Element<'_, Message> =
                    slider(0..=length as i32, current as i32, move |value| {
                        Message::IslandSeekPreview(seek_device.clone(), i64::from(value))
                    })
                    .on_release(Message::IslandSeekCommit(commit_device))
                    .step(1000_i32)
                    .height(20)
                    .handle_height(16)
                    .width(Length::Fill)
                    .into();
                let seek: Element<'_, Action<Message>> = seek.map(Action::App);
                body = body.push(
                    row![
                        text::text(format_ms(current))
                            .size(9.5)
                            .line_height(LineHeight::Absolute(12.0.into()))
                            .class(theme::Text::Color(Color::from_rgb(0.66, 0.70, 0.78))),
                        seek,
                        text::text(format_ms(length))
                            .size(9.5)
                            .line_height(LineHeight::Absolute(12.0.into()))
                            .class(theme::Text::Color(Color::from_rgb(0.66, 0.70, 0.78))),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            } else {
                let bar = progress_bar::determinate_linear(progress)
                    .width(Length::Fill)
                    .girth(4);

                let mut progress_row = row![bar].spacing(8).align_y(Alignment::Center);
                if let Some(progress_text) = progress_text {
                    progress_row = progress_row.push(
                        text::text(progress_text)
                            .size(9.5)
                            .line_height(LineHeight::Absolute(12.0.into()))
                            .class(theme::Text::Color(Color::from_rgb(0.66, 0.70, 0.78))),
                    );
                }
                body = body.push(progress_row);
            }
        }

        // Pairing is the only card with no close button: Decline and Accept are
        // its exits, and lock_expanded keeps it open until one is pressed.
        if kind != FeedKind::Pairing {
            let mut action_row = row![].spacing(6).align_y(Alignment::Center);
            for action in actions {
                action_row = action_row.push(action);
            }
            action_row = action_row.push(container(row![]).width(Length::Fill));
            if kind == FeedKind::Media && !device_name.is_empty() {
                action_row = action_row.push(island_device_chip(device_name, accent));
            }
            action_row = action_row.push(island_icon_button(
                "window-close-symbolic",
                Message::IslandDismiss,
                accent,
                false,
            ));
            body = body.push(container(row![]).height(Length::Fill));
            body = body.push(action_row);
        }

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Start)
            .align_y(Alignment::Start)
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
        let helper_mode = std::env::var_os("COSMIC_CONNECT_HELPER").is_some();
        let panel_hosted = !helper_mode && std::env::var_os("COSMIC_PANEL_NAME").is_some();
        let initial_device = std::env::var("COSMIC_CONNECT_DEVICE_ID").ok();

        let app = Self {
            core,
            compact_panel: load_compact_panel(),
            island_enabled: load_island_enabled(),
            helper_mode,
            panel_hosted,
            selected_device_id: initial_device,
            ..Default::default()
        };

        let backend_task = Task::perform(
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
        .map(cosmic::Action::App);

        (app, backend_task)
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
            Message::HelperWindowOpened(window_id) => {
                if self.core.main_window_id() != Some(window_id) {
                    return Task::none();
                }
                log::info!("Island helper: closing auxiliary window {window_id:?}");
                let _ = self.core.set_main_window_id(None);
                return cosmic::iced::window::close(window_id).map(cosmic::Action::App);
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
                self.pairing_requests = pairing_ids.iter().cloned().collect();
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
                        if !self.island_enabled {
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
                }
                let selected_id = self.selected_device_id.clone();
                let popup_open = self.popup.is_some();
                for device in &self.devices {
                    if !device.is_reachable {
                        continue;
                    }
                    let selected = selected_id.as_deref() == Some(device.id.as_str());

                    if selected && device.has_plugin("kdeconnect_notifications") {
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

                let island_task = self.refresh_island();
                return tasks.chain(island_task);
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
            Message::PhoneNotificationChanged(device_id, public_id) => {
                let Some(backend) = self.backend.clone() else {
                    return Task::none();
                };
                let did = device_id.clone();
                let pid = public_id.clone();
                return Task::perform(
                    async move { backend.fetch_notification(&did, &pid).await },
                    move |notif| Message::PhoneNotificationFetched(device_id, public_id, notif),
                )
                .map(cosmic::Action::App);
            }
            Message::PhoneNotificationFetched(device_id, public_id, notif) => {
                let key = phone_toast_key(&device_id, &public_id);
                let Some(notif) = notif else {
                    log::debug!(
                        "Phone notification {public_id} vanished before it could be fetched"
                    );
                    return Task::none();
                };

                let media_notification = self
                    .drafts
                    .get(&device_id)
                    .and_then(|draft| draft.player.as_ref())
                    .is_some_and(|player| is_media_notification(&notif, Some(player)));

                if notif.silent || is_android_hotspot_status(&notif) || media_notification {
                    let island_changed = self.remove_island_notification(&device_id, &public_id);
                    let mut tasks = if island_changed {
                        self.refresh_island()
                    } else {
                        Task::none()
                    };
                    if let Some(id) = self.phone_toasts.remove(&key) {
                        if let Some(backend) = self.backend.clone() {
                            tasks = tasks.chain(
                                Task::perform(
                                    async move {
                                        let _ = backend.close_notification(id).await;
                                    },
                                    |_| Message::NoOp,
                                )
                                .map(cosmic::Action::App),
                            );
                        }
                    }
                    return tasks;
                }

                if self.island_enabled {
                    let stale = self.phone_toasts.remove(&key);
                    self.push_island_notification(island_notification_feed(&device_id, &notif));
                    let mut tasks = self.refresh_island();
                    if let (Some(id), Some(backend)) = (stale, self.backend.clone()) {
                        tasks = tasks.chain(
                            Task::perform(
                                async move {
                                    let _ = backend.close_notification(id).await;
                                },
                                |_| Message::NoOp,
                            )
                            .map(cosmic::Action::App),
                        );
                    }
                    return tasks;
                }

                let Some(backend) = self.backend.clone() else {
                    return Task::none();
                };
                let replaces = self.phone_toasts.get(&key).copied().unwrap_or(0);
                let summary = if notif.app_name.trim().is_empty() {
                    "Phone".to_string()
                } else {
                    truncate_text(notif.app_name.trim(), 64)
                };
                let body = match (notif.title.trim(), notif.text.trim()) {
                    ("", "") => truncate_text(notif.ticker.trim(), 220),
                    ("", text) => truncate_text(text, 220),
                    (title, "") => truncate_text(title, 220),
                    (title, text) => truncate_text(&format!("{title}: {text}"), 220),
                };
                let body = if body.is_empty() {
                    "New notification".to_string()
                } else {
                    body
                };
                let icon = if notif.has_icon && !notif.icon_path.is_empty() {
                    notif.icon_path.clone()
                } else {
                    "io.github.acemythos.Connect-symbolic".to_string()
                };

                log::info!(
                    "Phone notification from '{summary}' on {device_id} (replaces {replaces})"
                );
                let toast_key = key.clone();
                return Task::perform(
                    async move {
                        backend
                            .notify_with_icon(&summary, &body, replaces, &[], &icon)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |result| Message::PhoneToastShown(toast_key, result),
                )
                .map(cosmic::Action::App);
            }
            Message::PhoneToastShown(key, result) => match result {
                Ok(id) => {
                    self.phone_toasts.insert(key, id);
                }
                Err(error) => log::warn!("Failed to show phone notification toast: {error}"),
            },
            Message::PhoneNotificationRemoved(device_id, public_id) => {
                let key = phone_toast_key(&device_id, &public_id);
                let island_changed = self.remove_island_notification(&device_id, &public_id);
                let mut tasks = if island_changed {
                    self.refresh_island()
                } else {
                    Task::none()
                };
                if let Some(id) = self.phone_toasts.remove(&key) {
                    if let Some(backend) = self.backend.clone() {
                        tasks = tasks.chain(
                            Task::perform(
                                async move {
                                    let _ = backend.close_notification(id).await;
                                },
                                |_| Message::NoOp,
                            )
                            .map(cosmic::Action::App),
                        );
                    }
                }
                return tasks;
            }
            Message::PhoneNotificationsCleared(device_id) => {
                let prefix = format!("{device_id}\u{0}");
                let island_changed = self.clear_transient_island_notifications(&device_id);
                let mut tasks = if island_changed {
                    self.refresh_island()
                } else {
                    Task::none()
                };
                let mut ids = Vec::new();
                self.phone_toasts.retain(|key, id| {
                    if key.starts_with(&prefix) {
                        ids.push(*id);
                        false
                    } else {
                        true
                    }
                });
                if !ids.is_empty() {
                    if let Some(backend) = self.backend.clone() {
                        tasks = tasks.chain(
                            Task::perform(
                                async move {
                                    for id in ids {
                                        let _ = backend.close_notification(id).await;
                                    }
                                },
                                |_| Message::NoOp,
                            )
                            .map(cosmic::Action::App),
                        );
                    }
                }
                return tasks;
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
                return self.refresh_island();
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
                self.player_refresh_inflight = false;
                let now = Instant::now();
                let is_playing = player.as_ref().is_some_and(|player| player.is_playing);
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
                draft.player_updated_at = Some(now);
                if is_playing {
                    draft.player_last_active = Some(now);
                }
                self.prune_media_island_notifications(&device_id);
                return self.refresh_island();
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
                self.transfer_failures.remove(&transfer_id);
                return self.refresh_island();
            }
            Message::TransferProgress(device_id, transfer_id, _transferred, _total, percent) => {
                let nkey = format!("transfer:{transfer_id}");
                if let Some(files) = self.received_files.get_mut(&device_id) {
                    if let Some(rf) = files.iter_mut().find(|f| f.transfer_id == transfer_id) {
                        rf.progress = percent.clamp(0, 100) as u32;
                    }
                }
                let island_task = self.refresh_island();
                if self.island_enabled {
                    return island_task;
                }
                let prev = self.last_notif_pct.get(&nkey).copied().unwrap_or(-5);
                let pct_block = (percent / 5) * 5;
                if pct_block <= prev {
                    return island_task;
                }
                self.last_notif_pct.insert(nkey.clone(), pct_block);
                log::debug!("TransferProgress: {transfer_id} at {percent}%");
                if let Some(&notif_id) = self.active_notifs.get(&nkey) {
                    let Some(backend) = self.backend.clone() else {
                        return island_task;
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
                    .map(cosmic::Action::App)
                    .chain(island_task);
                } else if self.pending_notif_requests.contains(&nkey) {
                    return island_task;
                } else {
                    self.pending_notif_requests.insert(nkey.clone());
                    let Some(backend) = self.backend.clone() else {
                        return island_task;
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
                    .map(cosmic::Action::App)
                    .chain(island_task);
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

                self.transfer_failures.remove(&transfer_id);
                let island_task = self.refresh_island();
                if self.island_enabled {
                    self.active_notifs.remove(&nkey);
                    self.pending_notif_requests.remove(&nkey);
                    self.last_notif_pct.remove(&nkey);
                    self.deferred_notifs.remove(&nkey);
                    return island_task;
                }
                let notif_id = self.active_notifs.remove(&nkey).unwrap_or(0);
                self.pending_notif_requests.remove(&nkey);
                self.last_notif_pct.remove(&nkey);
                let dest_path = file_path.trim_start_matches("file://").to_string();
                if notif_id == 0 {
                    self.deferred_notifs
                        .insert(nkey, (device_name, notif_name, dest_path));
                    return island_task;
                }
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
                .map(cosmic::Action::App)
                .chain(island_task);
            }
            Message::TransferFailed(device_id, transfer_id, _error_code, error_string) => {
                let nkey = format!("transfer:{transfer_id}");
                log::warn!("Transfer {transfer_id} failed: {error_string}");
                let file_name = self
                    .received_files
                    .get(&device_id)
                    .and_then(|files| {
                        files
                            .iter()
                            .find(|file| file.transfer_id == transfer_id)
                            .map(|file| file.file_name.clone())
                    })
                    .unwrap_or_else(|| "File".to_string());
                if let Some(files) = self.received_files.get_mut(&device_id) {
                    if let Some(rf) = files.iter_mut().find(|f| f.transfer_id == transfer_id) {
                        rf.active = false;
                    }
                }
                self.transfer_failures.insert(
                    transfer_id.clone(),
                    TransferFailure {
                        file_name,
                        device_id: device_id.clone(),
                        error: error_string.clone(),
                        at: Instant::now(),
                    },
                );
                let island_task = self.refresh_island();
                if self.island_enabled {
                    self.active_notifs.remove(&nkey);
                    self.pending_notif_requests.remove(&nkey);
                    self.last_notif_pct.remove(&nkey);
                    return island_task;
                }
                let notif_id = self.active_notifs.remove(&nkey).unwrap_or(0);
                self.pending_notif_requests.remove(&nkey);
                self.last_notif_pct.remove(&nkey);
                let Some(backend) = self.backend.clone() else {
                    return island_task;
                };
                return Task::perform(
                    async move {
                        let _ = backend
                            .notify("Transfer failed", &error_string, notif_id, &[])
                            .await;
                    },
                    |_| Message::NoOp,
                )
                .map(cosmic::Action::App)
                .chain(island_task);
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
                return self.refresh_island();
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
                    self.pairing_requests.remove(&device_id);
                    let island_task = self.refresh_island();
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
                        .map(cosmic::Action::App)
                        .chain(island_task);
                    }
                    return island_task;
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
            Message::IslandReconcile => {
                self.tick_island_seek_preview();
                return self.refresh_island().chain(self.tick_island_hover());
            }
            Message::IslandToggleExpand => {
                return self.toggle_island_expand();
            }
            Message::IslandCollapse => {
                return self.collapse_island();
            }
            Message::IslandDismiss => {
                return self.dismiss_island_feed();
            }
            Message::IslandHoverEnter => {
                return self.island_hover_enter();
            }
            Message::IslandHoverExit => {
                return self.island_hover_exit();
            }
            Message::IslandCursorLeft(id) => {
                if Some(id) == self.island {
                    return self.island_hover_exit();
                }
            }
            Message::IslandExpandIfHovered => {
                if self.island.is_none() || self.island_expanded || !self.island_hovered {
                    return Task::none();
                }
                if self
                    .island_hover_at
                    .is_some_and(|at| at.elapsed() >= ISLAND_HOVER_EXPAND_DELAY)
                {
                    return self.set_island_expanded(true, false);
                }
            }
            Message::IslandCollapseIfIdle => {
                if self.island.is_none()
                    || !self.island_expanded
                    || self.island_hovered
                    || self.island_pinned
                {
                    return Task::none();
                }
                let pairing = self
                    .feed_activity
                    .as_ref()
                    .is_some_and(|activity| activity.kind == FeedKind::Pairing);
                if pairing {
                    return Task::none();
                }
                if self
                    .island_unhover_at
                    .is_some_and(|at| at.elapsed() >= ISLAND_HOVER_COLLAPSE_DELAY)
                {
                    return self.set_island_expanded(false, false);
                }
            }
            Message::IslandSeekPreview(device_id, position) => {
                let length = self
                    .feed_activity
                    .as_ref()
                    .filter(|activity| activity.device_id == device_id && activity.can_seek)
                    .and_then(|activity| activity.length_ms)
                    .filter(|length| *length > 0);
                let Some(length) = length else {
                    return Task::none();
                };
                self.island_seek_preview = Some(position.clamp(0, length));
                self.island_seek_dragging = true;
                self.island_seek_preview_at = Some(Instant::now());
            }
            Message::IslandSeekCommit(device_id) => {
                self.island_seek_dragging = false;
                let (matches, can_seek, current) = self
                    .feed_activity
                    .as_ref()
                    .map(|activity| {
                        (
                            activity.device_id == device_id,
                            activity.can_seek,
                            activity.position_ms,
                        )
                    })
                    .unwrap_or((false, false, None));
                if !matches || !can_seek {
                    self.island_seek_preview = None;
                    return Task::none();
                }
                let Some(position) = self.island_seek_preview else {
                    return Task::none();
                };
                if current.is_some_and(|current| (position - current).abs() < 500) {
                    self.island_seek_preview = None;
                    self.island_seek_preview_at = None;
                    return Task::none();
                }
                self.island_seek_preview_at = Some(Instant::now());
                return Task::perform(async {}, move |_| {
                    Message::PerformAction(device_id, ActionType::Seek(position))
                })
                .map(cosmic::Action::App);
            }
            Message::IslandFrame => {}
            Message::IslandEqTick => {
                self.island_eq_phase = (self.island_eq_phase + 0.16) % 1.0;
            }
            Message::IslandFeedAction(action) => {
                return self.handle_feed_action(action);
            }
            Message::HelperFeedPoll => {
                if !self.helper_mode {
                    return Task::none();
                }
                let Some((seq, activity)) = crate::island::latest_if_newer(self.helper_feed_seq)
                else {
                    return Task::none();
                };
                self.helper_feed_seq = seq;
                return self.apply_feed(activity);
            }
            Message::HelperExitCheck => {
                if !self.helper_mode {
                    return Task::none();
                }
                if self.feed_activity.is_some() || self.island_hidden_key.is_some() {
                    self.helper_hidden_at = None;
                    return Task::none();
                }
                let hidden_at = *self.helper_hidden_at.get_or_insert_with(Instant::now);
                if hidden_at.elapsed() >= HELPER_HIDDEN_EXIT {
                    log::info!("Island helper: idle for too long, exiting");
                    std::process::exit(0);
                }
                return Task::none();
            }
            Message::PlayerTick => {
                if self.helper_mode {
                    return Task::none();
                }
                if self.player_refresh_inflight {
                    return self.refresh_island();
                }
                let Some(device_id) = self.selected_device_id.clone() else {
                    return self.refresh_island();
                };
                let Some(backend) = self.backend.clone() else {
                    return self.refresh_island();
                };
                self.player_refresh_inflight = true;
                let did = device_id.clone();
                return Task::perform(
                    async move {
                        tokio::time::timeout(Duration::from_millis(1500), backend.player_info(&did))
                            .await
                            .ok()
                            .flatten()
                    },
                    move |player| Message::PlayerInfoUpdated(device_id, player),
                )
                .map(cosmic::Action::App);
            }
            Message::SetCompactPanel(enabled) => {
                self.compact_panel = enabled;
                save_compact_panel(enabled);
            }
            Message::SetIslandEnabled(enabled) => {
                if self.island_enabled == enabled {
                    return Task::none();
                }
                self.island_enabled = enabled;
                save_island_enabled(enabled);
                self.clear_all_island_notifications();
                let mut tasks = self.refresh_island();
                if enabled {
                    if let Some(backend) = self.backend.clone() {
                        let mut toast_ids: Vec<u32> = self.phone_toasts.values().copied().collect();
                        toast_ids.extend(self.active_notifs.values().copied());
                        toast_ids.extend(self.pairing_notifs.keys().copied());
                        toast_ids.sort_unstable();
                        toast_ids.dedup();
                        self.phone_toasts.clear();
                        self.active_notifs.clear();
                        self.pairing_notifs.clear();
                        for id in toast_ids {
                            let backend = backend.clone();
                            tasks = tasks.chain(
                                Task::perform(
                                    async move {
                                        let _ = backend.close_notification(id).await;
                                    },
                                    |_| Message::NoOp,
                                )
                                .map(cosmic::Action::App),
                            );
                        }
                    }
                }
                return tasks;
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
            Message::BoltBlink => {
                self.panel_bolt_dimmed = !self.panel_bolt_dimmed;
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        if self.helper_mode {
            return container(row![])
                .width(Length::Fixed(1.0))
                .height(Length::Fixed(1.0))
                .into();
        }

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
            if self.selected_device_charging() {
                tooltip_label.push_str(" - charging");
            }
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

        if self.selected_device_charging() {
            let slot: Element<'_, Message> = if self.panel_bolt_dimmed {
                row![].into()
            } else {
                icon::from_svg_bytes(PANEL_BOLT_SVG)
                    .symbolic(true)
                    .icon()
                    .size(SIZE_CAPTION)
                    .into()
            };
            content = content.push(
                container(slot)
                    .width(Length::Fixed(14.0))
                    .align_x(Alignment::Center),
            );
        }

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
        let mut subscriptions = Vec::new();

        if !self.helper_mode {
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

            subscriptions.push(inner);
            subscriptions.push(time::every(PLAYER_REFRESH_INTERVAL).map(|_| Message::PlayerTick));
        }

        if self.popup.is_some() {
            subscriptions.push(time::every(Duration::from_millis(500)).map(|_| Message::Tick));
        }

        if self.selected_device_charging() {
            subscriptions.push(time::every(Duration::from_millis(600)).map(|_| Message::BoltBlink));
        }

        if self.helper_mode {
            subscriptions.push(time::every(HELPER_FEED_INTERVAL).map(|_| Message::HelperFeedPoll));
            subscriptions
                .push(time::every(Duration::from_millis(500)).map(|_| Message::HelperExitCheck));
            subscriptions
                .push(cosmic::iced::window::open_events().map(Message::HelperWindowOpened));
        }

        if self.island_open() {
            subscriptions
                .push(time::every(ISLAND_RECONCILE_INTERVAL).map(|_| Message::IslandReconcile));
        }

        if self.island.is_some() {
            subscriptions.push(cosmic::iced::event::listen_with(
                |event, _status, window| {
                    matches!(
                        event,
                        cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::CursorLeft)
                    )
                    .then_some(Message::IslandCursorLeft(window))
                },
            ));
        }

        if self.island_morph_animating(Instant::now()) {
            subscriptions.push(cosmic::iced::window::frames().map(|_| Message::IslandFrame));
        }

        if self.island_eq_active() {
            subscriptions.push(time::every(ISLAND_EQ_INTERVAL).map(|_| Message::IslandEqTick));
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

fn island_input_region(zone: IslandZone) -> Option<Vec<Rectangle>> {
    let region = match zone {
        IslandZone::Hidden => Vec::new(),
        IslandZone::Peek => vec![Rectangle {
            x: (ISLAND_CANVAS_WIDTH as f32 - ISLAND_PEEK_WIDTH) / 2.0,
            y: ISLAND_TOP_OFFSET,
            width: ISLAND_PEEK_WIDTH,
            height: ISLAND_PEEK_HEIGHT,
        }],
        IslandZone::Expanded => vec![Rectangle {
            x: (ISLAND_CANVAS_WIDTH as f32 - ISLAND_EXPANDED_WIDTH) / 2.0,
            y: ISLAND_TOP_OFFSET,
            width: ISLAND_EXPANDED_WIDTH,
            height: ISLAND_EXPANDED_HEIGHT,
        }],
    };

    Some(region)
}

fn island_settings(id: Id, zone: IslandZone) -> SctkLayerSurfaceSettings {
    SctkLayerSurfaceSettings {
        id,
        layer: Layer::Top,
        keyboard_interactivity: KeyboardInteractivity::None,
        input_zone: island_input_region(zone),
        anchor: Anchor::TOP,
        output: IcedOutput::Active,
        namespace: "cosmic-connect-island".to_string(),
        margin: IcedMargin {
            top: 0,
            right: 0,
            bottom: 0,
            left: 0,
        },
        size: Some((Some(ISLAND_CANVAS_WIDTH), Some(ISLAND_CANVAS_HEIGHT))),
        exclusive_zone: 0,
        size_limits: Limits::NONE
            .min_width(ISLAND_CANVAS_WIDTH as f32)
            .max_width(ISLAND_CANVAS_WIDTH as f32)
            .min_height(ISLAND_CANVAS_HEIGHT as f32)
            .max_height(ISLAND_CANVAS_HEIGHT as f32),
    }
}

fn island_set_zone(id: Id, zone: IslandZone) -> Task<Action<Message>> {
    cosmic::iced::runtime::task::effect(cosmic::iced::runtime::Action::PlatformSpecific(
        PlatformAction::Wayland(WaylandAction::LayerSurface(LayerSurfaceAction::InputZone {
            id,
            zone: island_input_region(zone),
        })),
    ))
}

fn island_feed_action<'a>(action: &'a FeedAction, accent: Color) -> Element<'a, Action<Message>> {
    let message = Message::IslandFeedAction(action.action.clone());
    let primary = matches!(
        &action.action,
        FeedActionKind::Media(kind) if kind == "PlayPause"
    );
    if action.as_text {
        island_text_button(&action.label, message, accent)
    } else {
        let icon_name = if action.icon.trim().is_empty() {
            "media-playback-start-symbolic"
        } else {
            action.icon.as_str()
        };
        island_icon_button(icon_name, message, accent, primary)
    }
}

fn island_art<'a>(
    path: Option<&str>,
    size: f32,
    radius: f32,
) -> Option<Element<'a, Action<Message>>> {
    let path = path?;
    let path = std::path::Path::new(path);
    if path.as_os_str().is_empty() || !path.is_file() {
        return None;
    }

    Some(
        cosmic::widget::image(cosmic::widget::image::Handle::from_path(path.to_path_buf()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .border_radius(radius)
            .into(),
    )
}

fn island_art_or_ring<'a>(
    kind: FeedKind,
    art: Option<&'a str>,
    art_icon: &'a str,
    progress: Option<f32>,
    accent: Color,
    size: f32,
    radius: f32,
) -> Element<'a, Action<Message>> {
    let base =
        island_art(art, size, radius).unwrap_or_else(|| island_icon_chip(art_icon, accent, size));

    if kind != FeedKind::Transfer {
        return base;
    }
    let Some(progress) = progress else {
        return base;
    };

    let ring = container(
        progress_bar::determinate_circular(progress.clamp(0.0, 1.0))
            .size(size)
            .bar_height(3.0),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center);

    Stack::with_children([base, ring.into()]).into()
}

fn island_eq<'a>(phase: f32, accent: Color) -> Element<'a, Action<Message>> {
    let mut bars = row![].spacing(2).align_y(Alignment::Center);

    for (index, scale) in [0.52_f32, 1.0, 0.68, 0.38].into_iter().enumerate() {
        let wave = ((phase + index as f32 * 0.18) * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        let height = (5.0 + wave * 10.0) * scale + 2.0;
        bars = bars.push(
            container(row![])
                .width(Length::Fixed(3.0))
                .height(Length::Fixed(height))
                .style(move |_theme: &cosmic::Theme| iced_container::Style {
                    background: Some(Background::Color(accent)),
                    border: Border {
                        radius: 2.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        );
    }

    container(bars)
        .height(Length::Fixed(16.0))
        .align_y(Alignment::Center)
        .into()
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

fn island_morph_animation(target: f32, duration: Duration) -> Animation<f32> {
    Animation::new(target)
        .duration(duration)
        .easing(Easing::EaseOutBack)
}

fn mix_color(from: Color, to: Color, t: f32) -> Color {
    Color::from_rgb(
        from.r + (to.r - from.r) * t,
        from.g + (to.g - from.g) * t,
        from.b + (to.b - from.b) * t,
    )
}

fn island_icon_chip<'a>(
    icon_name: &'a str,
    accent: Color,
    size: f32,
) -> Element<'a, Action<Message>> {
    container(icon::from_name(icon_name).size((size * 0.5) as u16))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &cosmic::Theme| iced_container::Style {
            background: Some(Background::Color(Color::from_rgba(
                accent.r, accent.g, accent.b, 0.16,
            ))),
            border: Border {
                radius: (size * 0.28).into(),
                width: 1.0,
                color: Color::from_rgba(accent.r, accent.g, accent.b, 0.28),
            },
            icon_color: Some(accent),
            ..Default::default()
        })
        .into()
}

fn island_capsule_style(
    accent: Color,
    radius: f32,
) -> impl Fn(&cosmic::Theme) -> iced_container::Style {
    move |_theme: &cosmic::Theme| {
        let top = mix_color(Color::from_rgb(0.105, 0.115, 0.145), accent, 0.10);
        let bottom = mix_color(Color::from_rgb(0.035, 0.040, 0.055), accent, 0.05);
        iced_container::Style {
            background: Some(Background::Gradient(Gradient::Linear(
                gradient::Linear::new(std::f32::consts::PI)
                    .add_stop(0.0, Color::from_rgba(top.r, top.g, top.b, 0.97))
                    .add_stop(1.0, Color::from_rgba(bottom.r, bottom.g, bottom.b, 0.985)),
            ))),
            border: Border {
                radius: radius.into(),
                width: 1.0,
                color: Color::from_rgba(accent.r, accent.g, accent.b, 0.30),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.55),
                offset: Vector::new(0.0, 12.0),
                blur_radius: 32.0,
            },
            text_color: Some(Color::from_rgb(0.96, 0.97, 0.99)),
            icon_color: Some(Color::from_rgb(0.96, 0.97, 0.99)),
            ..Default::default()
        }
    }
}

fn island_device_chip<'a>(name: String, accent: Color) -> Element<'a, Action<Message>> {
    let dot = container(row![])
        .width(Length::Fixed(5.0))
        .height(Length::Fixed(5.0))
        .style(move |_theme: &cosmic::Theme| iced_container::Style {
            background: Some(Background::Color(accent)),
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            shadow: Shadow {
                color: Color::from_rgba(accent.r, accent.g, accent.b, 0.55),
                offset: Vector::new(0.0, 0.0),
                blur_radius: 8.0,
            },
            ..Default::default()
        });

    row![
        dot,
        text::text(name)
            .size(10.5)
            .line_height(LineHeight::Absolute(12.0.into()))
            .class(theme::Text::Color(Color::from_rgb(0.553, 0.596, 0.667))),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn island_icon_button<'a>(
    icon_name: &'a str,
    message: Message,
    accent: Color,
    primary: bool,
) -> Element<'a, Action<Message>> {
    let primary_icon = Color::from_rgb(0.06, 0.07, 0.10);
    button::custom(icon::from_name(icon_name).size(15))
        .on_press(Action::App(message))
        .padding(6)
        .class(theme::Button::Custom {
            active: Box::new(move |focused, _theme| button::Style {
                background: Some(Background::Color(if primary {
                    Color::from_rgba(
                        accent.r,
                        accent.g,
                        accent.b,
                        if focused { 0.95 } else { 0.85 },
                    )
                } else {
                    Color::from_rgba(1.0, 1.0, 1.0, if focused { 0.16 } else { 0.08 })
                })),
                border_radius: 14.0.into(),
                border_width: if focused { 1.0 } else { 0.0 },
                border_color: if focused { accent } else { Color::TRANSPARENT },
                icon_color: Some(if primary {
                    primary_icon
                } else if focused {
                    accent
                } else {
                    Color::from_rgb(0.92, 0.93, 0.96)
                }),
                ..button::Style::new()
            }),
            hovered: Box::new(move |_focused, _theme| button::Style {
                background: Some(Background::Color(if primary {
                    Color::from_rgba(accent.r, accent.g, accent.b, 0.95)
                } else {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.16)
                })),
                border_radius: 14.0.into(),
                border_width: 1.0,
                border_color: if primary {
                    accent
                } else {
                    Color::from_rgba(accent.r, accent.g, accent.b, 0.35)
                },
                icon_color: Some(if primary { primary_icon } else { accent }),
                ..button::Style::new()
            }),
            pressed: Box::new(move |_focused, _theme| button::Style {
                background: Some(Background::Color(if primary {
                    Color::from_rgba(accent.r, accent.g, accent.b, 1.0)
                } else {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.20)
                })),
                border_radius: 14.0.into(),
                icon_color: Some(if primary { primary_icon } else { accent }),
                ..button::Style::new()
            }),
            disabled: Box::new(|_theme| button::Style {
                background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.04))),
                border_radius: 14.0.into(),
                icon_color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.30)),
                ..button::Style::new()
            }),
        })
        .into()
}

fn island_text_button<'a>(
    label: &'a str,
    message: Message,
    accent: Color,
) -> Element<'a, Action<Message>> {
    button::custom(
        text::text(label)
            .size(11.0)
            .line_height(LineHeight::Absolute(13.0.into()))
            .class(theme::Text::Color(accent)),
    )
    .on_press(Action::App(message))
    .padding([5, 10])
    .class(theme::Button::Custom {
        active: Box::new(move |focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba(
                accent.r,
                accent.g,
                accent.b,
                if focused { 0.22 } else { 0.14 },
            ))),
            border_radius: 12.0.into(),
            border_width: 1.0,
            border_color: Color::from_rgba(accent.r, accent.g, accent.b, 0.38),
            text_color: Some(accent),
            ..button::Style::new()
        }),
        hovered: Box::new(move |_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba(
                accent.r, accent.g, accent.b, 0.22,
            ))),
            border_radius: 12.0.into(),
            border_width: 1.0,
            border_color: Color::from_rgba(accent.r, accent.g, accent.b, 0.55),
            text_color: Some(accent),
            ..button::Style::new()
        }),
        pressed: Box::new(move |_focused, _theme| button::Style {
            background: Some(Background::Color(Color::from_rgba(
                accent.r, accent.g, accent.b, 0.30,
            ))),
            border_radius: 12.0.into(),
            border_width: 1.0,
            border_color: Color::from_rgba(accent.r, accent.g, accent.b, 0.55),
            text_color: Some(accent),
            ..button::Style::new()
        }),
        disabled: Box::new(|_theme| button::Style::new()),
    })
    .into()
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
                        ("notifications", "notificationPosted" | "notificationUpdated") => {
                            if let Ok(public_id) = msg.body().deserialize::<String>() {
                                return Message::PhoneNotificationChanged(device_id, public_id);
                            }
                        }
                        ("notifications", "notificationRemoved") => {
                            if let Ok(public_id) = msg.body().deserialize::<String>() {
                                return Message::PhoneNotificationRemoved(device_id, public_id);
                            }
                        }
                        ("notifications", "allNotificationsRemoved") => {
                            return Message::PhoneNotificationsCleared(device_id);
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
        format_bytes, format_ms, is_media_notification, is_quiet_action, is_valid_url,
        island_hide_key, island_notification_feed, parse_device_signal_path, truncate_text,
        CosmicConnect, DeviceDraft, Message, PlayerInfo, PopupView, ShareKind,
    };
    use crate::island::{FeedActionKind, FeedActivity, FeedKind};
    use crate::model::{ActionType, Notification};
    use cosmic::Application;
    use std::time::{Duration, Instant};

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

    fn phone_notification(id: &str, internal_id: &str) -> Notification {
        Notification {
            id: id.to_string(),
            internal_id: internal_id.to_string(),
            app_name: "Signal".to_string(),
            title: "Alice".to_string(),
            text: "hey there".to_string(),
            ticker: String::new(),
            icon_path: "/tmp/icon.png".to_string(),
            has_icon: true,
            dismissable: true,
            silent: false,
            reply_id: String::new(),
            actions: Vec::new(),
            ongoing: false,
        }
    }

    fn feed(kind: FeedKind, key: &str, title: &str) -> FeedActivity {
        FeedActivity {
            key: key.to_string(),
            kind,
            device_id: "dev".to_string(),
            title: title.to_string(),
            subtitle: String::new(),
            detail: None,
            art: None,
            icon: String::new(),
            badge: String::new(),
            accent: [0.5, 0.7, 1.0],
            progress: None,
            progress_text: None,
            position_ms: None,
            length_ms: None,
            can_seek: false,
            device_name: String::new(),
            compact: None,
            primary: None,
            actions: Vec::new(),
            expand: false,
            expires_at_ms: None,
        }
    }

    #[test]
    fn ongoing_notification_does_not_starve_the_queue() {
        let mut app = CosmicConnect::default();
        app.island_enabled = true;

        let mut ongoing = phone_notification("2", "22");
        ongoing.ongoing = true;
        let first = island_notification_feed("dev", &ongoing);
        let second = island_notification_feed("dev", &phone_notification("3", "33"));
        app.push_island_notification(first.clone());
        app.push_island_notification(second.clone());

        assert_eq!(
            app.island_notification
                .as_ref()
                .map(|notification| notification.activity.key.clone()),
            Some(first.key.clone())
        );
        assert!(app
            .island_notification
            .as_ref()
            .is_some_and(|notification| notification.expires_at.is_some()));

        app.island_notification.as_mut().unwrap().expires_at =
            Some(Instant::now() - Duration::from_secs(1));
        app.tick_island_notifications();
        assert_eq!(
            app.island_notification
                .as_ref()
                .map(|notification| notification.activity.key.clone()),
            Some(second.key.clone())
        );
    }

    #[test]
    fn dismissing_media_hides_it_until_the_track_changes() {
        let mut app = CosmicConnect::default();
        let track = feed(FeedKind::Media, "media:dev", "Let It Happen");

        app.island_hidden_key = Some(island_hide_key(&track));

        assert!(app.feed_visible(Some(track.clone())).is_none());
        assert!(app
            .feed_visible(Some(feed(FeedKind::Media, "media:dev", "Let It Happen")))
            .is_none());

        let next = app.feed_visible(Some(feed(FeedKind::Media, "media:dev", "Borderline")));
        assert_eq!(next.map(|feed| feed.title), Some("Borderline".to_string()));
        assert!(app.island_hidden_key.is_none());
    }

    #[test]
    fn dismissing_a_notification_lets_the_queue_advance() {
        let mut app = CosmicConnect::default();
        app.island_enabled = true;

        let first = island_notification_feed("dev", &phone_notification("1", "11"));
        let second = island_notification_feed("dev", &phone_notification("2", "22"));
        app.push_island_notification(first.clone());
        app.push_island_notification(second.clone());
        app.feed_activity = app.current_feed();

        let _ = app.dismiss_island_feed();

        assert_eq!(
            app.current_feed().map(|feed| feed.key),
            Some(second.key.clone())
        );
    }

    #[test]
    fn pairing_cannot_be_dismissed_from_the_island() {
        let mut app = CosmicConnect::default();
        app.feed_activity = Some(feed(FeedKind::Pairing, "pairing:dev", "Pixel"));

        let _ = app.dismiss_island_feed();

        assert!(app.island_hidden_key.is_none());
    }

    #[test]
    fn media_notification_is_matched_to_the_active_player() {
        let mut notif = phone_notification("2", "7");
        notif.app_name = "Metrolist".to_string();
        notif.ongoing = true;
        let player = PlayerInfo {
            player: "Metrolist".to_string(),
            title: "Let It Happen".to_string(),
            ..PlayerInfo::default()
        };

        assert!(is_media_notification(&notif, Some(&player)));
        assert!(!is_media_notification(&notif, None));

        notif.ongoing = false;
        assert!(!is_media_notification(&notif, Some(&player)));

        notif.ongoing = true;
        notif.app_name = "Signal".to_string();
        assert!(!is_media_notification(&notif, Some(&player)));

        notif.app_name = "Metrolist".to_string();
        let idle_player = PlayerInfo::default();
        assert!(!is_media_notification(&notif, Some(&idle_player)));
    }

    #[test]
    fn island_notification_feed_caps_and_shortens_actions() {
        let mut notif = phone_notification("2", "7");
        notif.dismissable = false;
        notif.actions = vec![
            "Rewind to previous item".to_string(),
            "Play".to_string(),
            "Forward to next item".to_string(),
            "Like".to_string(),
            "Repeat mode off".to_string(),
        ];

        let feed = island_notification_feed("dev", &notif);
        assert_eq!(feed.actions.len(), 3);
        assert_eq!(feed.actions[0].label, "Prev");
        assert_eq!(feed.actions[1].label, "Play");
        assert_eq!(feed.actions[2].label, "Next");
        assert_eq!(
            feed.actions[0].action,
            FeedActionKind::NotificationAction {
                internal_id: "7".to_string(),
                action: "Rewind to previous item".to_string(),
            }
        );
    }

    #[test]
    fn island_notification_feed_carries_phone_notification() {
        let feed = island_notification_feed("dev", &phone_notification("42", "7"));
        assert_eq!(feed.key, "notification:dev:42");
        assert_eq!(feed.kind, FeedKind::Notification);
        assert_eq!(feed.title, "Signal");
        assert_eq!(feed.subtitle, "Alice");
        assert_eq!(feed.detail.as_deref(), Some("hey there"));
        assert_eq!(feed.art.as_deref(), Some("/tmp/icon.png"));
        assert_eq!(feed.badge, "PHONE");
        assert_eq!(feed.actions.len(), 1);
        assert!(matches!(
            &feed.actions[0].action,
            FeedActionKind::DismissNotification(id) if id == "7"
        ));
    }

    #[test]
    fn hotspot_status_notification_is_suppressed_from_the_island() {
        let mut app = CosmicConnect::default();
        app.island_enabled = true;
        let mut hotspot = phone_notification("42", "7");
        hotspot.app_name = "Android System".to_string();
        hotspot.title = "1 device is connected to your mobile data hotspot.".to_string();
        hotspot.text = "Mobile data used: 119 MB".to_string();
        hotspot.ongoing = true;

        let _ = app.update(Message::PhoneNotificationFetched(
            "dev".to_string(),
            hotspot.id.clone(),
            Some(hotspot),
        ));

        assert!(app.island_notification.is_none());
        assert!(app.island_notification_queue.is_empty());
    }

    #[test]
    fn island_notification_feed_pins_ongoing_actions() {
        let mut notification = phone_notification("42", "7");
        notification.ongoing = true;
        notification.actions = vec!["Pause".into(), "Cancel".into()];
        let feed = island_notification_feed("dev", &notification);

        assert_eq!(feed.expires_at_ms, None);
        assert_eq!(feed.actions.len(), 3);
        assert!(matches!(
            &feed.actions[0].action,
            FeedActionKind::NotificationAction { internal_id, action }
                if internal_id == "7" && action == "Pause"
        ));
        assert!(matches!(
            &feed.actions[2].action,
            FeedActionKind::DismissNotification(id) if id == "7"
        ));
    }

    #[test]
    fn island_notification_queue_expires_and_advances() {
        let mut app = CosmicConnect::default();
        app.island_enabled = true;

        let first = island_notification_feed("dev", &phone_notification("1", "11"));
        let second = island_notification_feed("dev", &phone_notification("2", "22"));
        app.push_island_notification(first.clone());
        app.push_island_notification(second.clone());

        assert_eq!(
            app.island_notification
                .as_ref()
                .map(|notification| notification.activity.key.as_str()),
            Some(first.key.as_str())
        );
        assert_eq!(app.island_notification_queue.len(), 1);

        app.island_notification.as_mut().unwrap().expires_at =
            Some(Instant::now() - Duration::from_secs(1));
        app.tick_island_notifications();
        assert_eq!(
            app.island_notification
                .as_ref()
                .map(|notification| notification.activity.key.as_str()),
            Some(second.key.as_str())
        );

        assert!(app.remove_island_notification("dev", "2"));
        app.tick_island_notifications();
        assert!(app.island_notification.is_none());
    }

    #[test]
    fn bulk_clear_keeps_ongoing_phone_notification_until_removed() {
        let mut app = CosmicConnect::default();
        app.island_enabled = true;

        let transient = island_notification_feed("dev", &phone_notification("1", "11"));
        let mut hotspot = phone_notification("2", "22");
        hotspot.ongoing = true;
        let ongoing = island_notification_feed("dev", &hotspot);

        app.push_island_notification(transient);
        app.push_island_notification(ongoing.clone());

        assert!(app.clear_transient_island_notifications("dev"));
        app.tick_island_notifications();
        assert_eq!(
            app.island_notification
                .as_ref()
                .map(|notification| notification.activity.key.as_str()),
            Some(ongoing.key.as_str())
        );

        assert!(app.remove_island_notification("dev", "2"));
        app.tick_island_notifications();
        assert!(app.island_notification.is_none());
    }
}
