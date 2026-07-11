#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub is_reachable: bool,
    pub is_paired: bool,
    pub pair_state: i32,
    pub battery: Option<BatteryInfo>,
    pub supported_plugins: Vec<String>,
    pub loaded_plugins: Vec<String>,
}

impl Device {
    pub fn has_plugin(&self, name: &str) -> bool {
        let plugins = if self.loaded_plugins.is_empty() {
            &self.supported_plugins
        } else {
            &self.loaded_plugins
        };

        plugins.iter().any(|p| p.as_str() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::{Device, DeviceType};

    #[test]
    fn has_plugin_prefers_loaded_plugins_when_available() {
        let device = Device {
            id: "device-1".into(),
            name: "Test Device".into(),
            device_type: DeviceType::Phone,
            is_reachable: true,
            is_paired: true,
            pair_state: 3,
            battery: None,
            supported_plugins: vec!["kdeconnect_ping".into(), "kdeconnect_clipboard".into()],
            loaded_plugins: vec!["kdeconnect_ping".into()],
        };

        assert!(device.has_plugin("kdeconnect_ping"));
        assert!(!device.has_plugin("kdeconnect_clipboard"));
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatteryInfo {
    pub charge: i32,
    pub is_charging: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectivityInfo {
    pub network_type: String,
    pub signal_strength: i32,
}

#[derive(Debug, Clone)]
pub enum DeviceType {
    Phone,
    Tablet,
    Laptop,
    Desktop,
    Unknown(String),
}

impl DeviceType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "phone" => Self::Phone,
            "tablet" => Self::Tablet,
            "laptop" => Self::Laptop,
            "desktop" => Self::Desktop,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn icon_name(&self) -> &str {
        match self {
            Self::Phone => "phone-symbolic",
            Self::Tablet => "tablet-symbolic",
            Self::Laptop => "laptop-symbolic",
            Self::Desktop => "computer-symbolic",
            Self::Unknown(_) => "phone-symbolic",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Phone => "Phone",
            Self::Tablet => "Tablet",
            Self::Laptop => "Laptop",
            Self::Desktop => "Desktop",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ActionType {
    Ring,
    Ping,
    SendClipboard,
    SendClipboardText(String),
    ShareText(String),
    ShareUrl(String),
    SendFile(String),
    BrowseFiles,
    Pair,
    AcceptPairing,
    CancelPairing,
    Unpair,
    ReplyToConversation(i64, String),
    SendSms(Vec<String>, String),
    DismissNotification(String),
    ReplyToNotification(String, String),
    MediaAction(String),
    SelectPlayer(String),
}

#[derive(Debug, Clone, Default)]
pub struct Notification {
    pub id: String,
    pub internal_id: String,
    pub app_name: String,
    pub title: String,
    pub text: String,
    pub ticker: String,
    pub dismissable: bool,
    pub reply_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerInfo {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub can_seek: bool,
    pub length: i64,
    pub position: i64,
    pub volume: i32,
    pub player_list: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    Added(String),
    Removed(String),
    VisibilityChanged(String, bool),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, zvariant::Type)]
pub struct ConversationAddress {
    pub address: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, zvariant::Type)]
pub struct Attachment {
    pub part_id: i64,
    pub mime_type: String,
    pub base64_file: String,
    pub unique_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, zvariant::Type)]
pub struct ConversationMessage {
    pub event_field: i32,
    pub body: String,
    pub addresses: Vec<ConversationAddress>,
    pub date: i64,
    pub message_type: i32,
    pub read: i32,
    pub thread_id: i64,
    pub uid: i32,
    pub sub_id: i64,
    pub attachments: Vec<Attachment>,
}

impl ConversationMessage {
    pub fn is_incoming(&self) -> bool {
        self.message_type == 1
    }

    pub fn is_outgoing(&self) -> bool {
        self.message_type == 2
    }

    pub fn sender(&self) -> &str {
        self.addresses.first().map(|a| a.address.as_str()).unwrap_or("Unknown")
    }
}
