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
}

impl Device {
    pub fn has_plugin(&self, name: &str) -> bool {
        self.supported_plugins.iter().any(|p| p.as_str() == name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatteryInfo {
    pub charge: i32,
    pub is_charging: bool,
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
    ShareUrl(String),
    SendFile,
    Pair,
    AcceptPairing,
    CancelPairing,
    Unpair,
}

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    Added(String),
    Removed(String),
    VisibilityChanged(String, bool),
}
