use zbus::{Connection, Proxy, Result};

use crate::model::{ActionType, BatteryInfo, Device, DeviceType};

const KDE_CONNECT_SERVICE: &str = "org.kde.kdeconnect";
const DAEMON_PATH: &str = "/modules/kdeconnect";
const DEVICE_IFACE: &str = "org.kde.kdeconnect.device";

pub struct KdeConnectBackend {
    conn: Connection,
}

impl KdeConnectBackend {
    pub async fn new() -> Result<Self> {
        let conn = Connection::session().await?;
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub async fn device_ids(&self) -> Result<Vec<String>> {
        let result = self
            .conn
            .call_method(
                Some(KDE_CONNECT_SERVICE),
                DAEMON_PATH,
                Some("org.kde.kdeconnect.daemon"),
                "devices",
                &(false, true),
            )
            .await?;
        result.body().deserialize()
    }

    fn device_path(id: &str) -> String {
        format!("/modules/kdeconnect/devices/{}", id)
    }

    fn plugin_path(id: &str, plugin: &str) -> String {
        format!("/modules/kdeconnect/devices/{}/{}", id, plugin)
    }

    pub async fn device_name(&self, id: &str) -> Result<String> {
        let p = Self::device_path(id);
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), DEVICE_IFACE).await?;
        proxy.get_property("name").await
    }

    pub async fn device_type(&self, id: &str) -> Result<String> {
        let p = Self::device_path(id);
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), DEVICE_IFACE).await?;
        proxy.get_property("type").await
    }

    pub async fn is_reachable(&self, id: &str) -> Result<bool> {
        let p = Self::device_path(id);
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), DEVICE_IFACE).await?;
        proxy.get_property("isReachable").await
    }

    pub async fn is_paired(&self, id: &str) -> Result<bool> {
        let p = Self::device_path(id);
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), DEVICE_IFACE).await?;
        proxy.get_property("isPaired").await
    }

    pub async fn pair_state(&self, id: &str) -> Result<i32> {
        let p = Self::device_path(id);
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), DEVICE_IFACE).await?;
        proxy.get_property("pairState").await
    }

    pub async fn supported_plugins(&self, id: &str) -> Result<Vec<String>> {
        let p = Self::device_path(id);
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), DEVICE_IFACE).await?;
        proxy.get_property("supportedPlugins").await
    }

    pub async fn battery_charge(&self, id: &str) -> Option<i32> {
        let p = Self::plugin_path(id, "battery");
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), "org.kde.kdeconnect.device.battery").await.ok()?;
        proxy.get_property("charge").await.ok()
    }

    pub async fn battery_charging(&self, id: &str) -> Option<bool> {
        let p = Self::plugin_path(id, "battery");
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), "org.kde.kdeconnect.device.battery").await.ok()?;
        proxy.get_property("isCharging").await.ok()
    }

    pub async fn ring_device(&self, id: &str) {
        let p = Self::plugin_path(id, "findmyphone");
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.findmyphone"), "ring", &(),
        ).await;
    }

    pub async fn send_ping(&self, id: &str) {
        let p = Self::plugin_path(id, "ping");
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.ping"), "sendPing", &(),
        ).await;
    }

    pub async fn send_clipboard(&self, id: &str) {
        let p = Self::plugin_path(id, "clipboard");
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.clipboard"), "sendClipboard", &(),
        ).await;
    }

    pub async fn share_url(&self, id: &str, url: &str) {
        let p = Self::plugin_path(id, "share");
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.share"), "shareUrl", &(url,),
        ).await;
    }

    pub async fn request_pairing(&self, id: &str) {
        let p = Self::device_path(id);
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "requestPairing", &(),
        ).await;
    }

    pub async fn accept_pairing(&self, id: &str) {
        let p = Self::device_path(id);
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "acceptPairing", &(),
        ).await;
    }

    pub async fn cancel_pairing(&self, id: &str) {
        let p = Self::device_path(id);
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "cancelPairing", &(),
        ).await;
    }

    pub async fn unpair(&self, id: &str) {
        let p = Self::device_path(id);
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "unpair", &(),
        ).await;
    }

    pub async fn devices(&self) -> Vec<Device> {
        let ids = match self.device_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                log::warn!("Failed to get device IDs: {e}");
                return vec![];
            }
        };

        let mut devices = Vec::with_capacity(ids.len());
        for id in ids {
            let device = self.fetch_device(&id).await;
            devices.push(device);
        }
        devices
    }

    async fn fetch_device(&self, id: &str) -> Device {
        let name = self.device_name(id).await.unwrap_or_else(|_| "?".into());
        let device_type = DeviceType::from_str(
            &self.device_type(id).await.unwrap_or_default()
        );
        let is_reachable = self.is_reachable(id).await.unwrap_or(false);
        let is_paired = self.is_paired(id).await.unwrap_or(false);
        let pair_state = self.pair_state(id).await.unwrap_or(0);
        let supported_plugins = self.supported_plugins(id).await.unwrap_or_default();

        let battery = if is_reachable && supported_plugins.iter().any(|p| p == "kdeconnect_battery") {
            let charge = self.battery_charge(id).await;
            let is_charging = self.battery_charging(id).await;
            charge.map(|c| BatteryInfo {
                charge: c,
                is_charging: is_charging.unwrap_or(false),
            })
        } else {
            None
        };

        Device {
            id: id.to_string(),
            name,
            device_type,
            is_reachable,
            is_paired,
            pair_state,
            battery,
            supported_plugins,
        }
    }

    pub async fn perform_action(&self, device_id: &str, action: &ActionType) {
        match action {
            ActionType::Ring => self.ring_device(device_id).await,
            ActionType::Ping => self.send_ping(device_id).await,
            ActionType::SendClipboard => self.send_clipboard(device_id).await,
            ActionType::ShareUrl(url) => self.share_url(device_id, url).await,
            ActionType::SendFile => {
                log::info!("SendFile not yet implemented");
            }
            ActionType::Pair => self.request_pairing(device_id).await,
            ActionType::AcceptPairing => self.accept_pairing(device_id).await,
            ActionType::CancelPairing => self.cancel_pairing(device_id).await,
            ActionType::Unpair => self.unpair(device_id).await,
        }
    }
}
