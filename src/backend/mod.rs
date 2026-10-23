use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use zbus::proxy::CacheProperties;
use zbus::{Connection, Proxy, Result};

use crate::model::{ActionType, BatteryInfo, ConnectivityInfo, Device, DeviceType, Notification, PlayerInfo};

const KDE_CONNECT_SERVICE: &str = "org.kde.kdeconnect";
const DAEMON_PATH: &str = "/modules/kdeconnect";
const DEVICE_IFACE: &str = "org.kde.kdeconnect.device";
const METHOD_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_REBUILD_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub struct KdeConnectBackend {
    conn: Connection,
    proxies: Mutex<HashMap<String, Proxy<'static>>>,
    proxy_cache_started: Mutex<Instant>,
    notification_ids: Mutex<HashMap<String, String>>,
}

impl KdeConnectBackend {
    pub async fn new() -> Result<Self> {
        let conn = zbus::connection::Builder::session()?
            .method_timeout(METHOD_TIMEOUT)
            .build()
            .await?;
        Ok(Self {
            conn,
            proxies: Mutex::new(HashMap::new()),
            proxy_cache_started: Mutex::new(Instant::now()),
            notification_ids: Mutex::new(HashMap::new()),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Reuse one properties-cached proxy per D-Bus object. The first property read
    /// performs a single `GetAll`; later reads are cache hits kept fresh by
    /// `PropertiesChanged`.
    async fn proxy(&self, path: String, interface: &'static str) -> Result<Proxy<'static>> {
        let key = format!("{path}\u{0}{interface}");

        if let Some(proxy) = self
            .proxies
            .lock()
            .expect("proxy cache poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(proxy);
        }

        let proxy = Proxy::new_owned(self.conn.clone(), KDE_CONNECT_SERVICE, path, interface).await?;
        self.proxies
            .lock()
            .expect("proxy cache poisoned")
            .insert(key, proxy.clone());
        Ok(proxy)
    }

    fn clear_proxy_cache(&self) {
        self.proxies.lock().expect("proxy cache poisoned").clear();
        self.notification_ids
            .lock()
            .expect("notification cache poisoned")
            .clear();
        *self
            .proxy_cache_started
            .lock()
            .expect("proxy cache clock poisoned") = Instant::now();
    }

    fn maybe_rebuild_proxy_cache(&self) {
        let stale = self
            .proxy_cache_started
            .lock()
            .expect("proxy cache clock poisoned")
            .elapsed()
            >= CACHE_REBUILD_INTERVAL;
        if stale {
            self.clear_proxy_cache();
        }
    }

    async fn device_proxy(&self, id: &str) -> Result<Proxy<'static>> {
        self.proxy(Self::device_path(id), DEVICE_IFACE).await
    }

    async fn plugin_proxy(
        &self,
        id: &str,
        plugin: &str,
        interface: &'static str,
    ) -> Result<Proxy<'static>> {
        self.proxy(Self::plugin_path(id, plugin), interface).await
    }

    async fn notification_proxy(
        &self,
        device_id: &str,
        notif_id: &str,
    ) -> Result<Proxy<'static>> {
        self.proxy(
            Self::notification_path(device_id, notif_id),
            "org.kde.kdeconnect.device.notifications.notification",
        )
        .await
    }

    pub async fn device_ids(&self) -> Result<Vec<String>> {
        let result = self
            .conn
            .call_method(
                Some(KDE_CONNECT_SERVICE),
                DAEMON_PATH,
                Some("org.kde.kdeconnect.daemon"),
                "devices",
                &(false, false),
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
        self.device_proxy(id).await?.get_property("name").await
    }

    pub async fn device_type(&self, id: &str) -> Result<String> {
        self.device_proxy(id).await?.get_property("type").await
    }

    pub async fn is_reachable(&self, id: &str) -> Result<bool> {
        self.device_proxy(id).await?.get_property("isReachable").await
    }

    pub async fn is_paired(&self, id: &str) -> Result<bool> {
        self.device_proxy(id).await?.get_property("isPaired").await
    }

    pub async fn pair_state(&self, id: &str) -> Result<i32> {
        self.device_proxy(id).await?.get_property("pairState").await
    }

    pub async fn supported_plugins(&self, id: &str) -> Result<Vec<String>> {
        self.device_proxy(id).await?.get_property("supportedPlugins").await
    }

    pub async fn loaded_plugins(&self, id: &str) -> Result<Vec<String>> {
        self.device_proxy(id).await?.call("loadedPlugins", &()).await
    }

    pub async fn battery_charge(&self, id: &str) -> Option<i32> {
        self.plugin_proxy(id, "battery", "org.kde.kdeconnect.device.battery")
            .await
            .ok()?
            .get_property("charge")
            .await
            .ok()
    }

    pub async fn battery_charging(&self, id: &str) -> Option<bool> {
        self.plugin_proxy(id, "battery", "org.kde.kdeconnect.device.battery")
            .await
            .ok()?
            .get_property("isCharging")
            .await
            .ok()
    }

    pub async fn connectivity_info(&self, device_id: &str) -> Option<ConnectivityInfo> {
        let iface = "org.kde.kdeconnect.device.connectivity_report";
        let proxy = self
            .plugin_proxy(device_id, "connectivity_report", iface)
            .await
            .ok()?;

        Some(ConnectivityInfo {
            network_type: proxy.get_property("cellularNetworkType").await.ok().unwrap_or_default(),
            signal_strength: proxy.get_property("cellularNetworkStrength").await.ok().unwrap_or(0),
        })
    }

    pub async fn ring_device(&self, id: &str) -> Result<()> {
        let p = Self::plugin_path(id, "findmyphone");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.findmyphone"), "ring", &(),
        ).await.map(|_| ())
    }

    pub async fn send_ping(&self, id: &str) -> Result<()> {
        let p = Self::plugin_path(id, "ping");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.ping"), "sendPing", &(),
        ).await.map(|_| ())
    }

    pub async fn send_clipboard(&self, id: &str) -> Result<()> {
        let p = Self::plugin_path(id, "clipboard");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.clipboard"), "sendClipboard", &(),
        ).await.map(|_| ())
    }

    pub async fn send_clipboard_text(&self, id: &str, content: &str) -> Result<()> {
        let p = Self::plugin_path(id, "clipboard");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.clipboard"), "sendClipboard", &(content,),
        ).await?;
        Ok(())
    }

    pub async fn share_url(&self, id: &str, url: &str) -> Result<()> {
        let p = Self::plugin_path(id, "share");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.share"), "shareUrl", &(url,),
        ).await?;
        Ok(())
    }

    pub async fn share_text(&self, id: &str, text: &str) -> Result<()> {
        let p = Self::plugin_path(id, "share");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.share"), "shareText", &(text,),
        ).await?;
        Ok(())
    }

    pub async fn share_file(&self, id: &str, file: &str) -> Result<()> {
        let p = Self::plugin_path(id, "share");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.share"), "openFile", &(file,),
        ).await?;
        Ok(())
    }

    pub async fn browse_files(&self, id: &str) -> Result<String> {
        let p = Self::plugin_path(id, "sftp");

        let result = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.sftp"), "startBrowsing", &(),
        ).await?;

        let started: bool = result.body().deserialize()?;

        if started {
            Ok("Opened device files".into())
        } else {
            let error: String = self.conn.call_method(
                Some(KDE_CONNECT_SERVICE), p.as_str(),
                Some("org.kde.kdeconnect.device.sftp"), "getMountError", &(),
            ).await?.body().deserialize()?;
            Err(zbus::Error::Failure(error))
        }
    }

    pub async fn request_pairing(&self, id: &str) -> Result<()> {
        let p = Self::device_path(id);
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "requestPairing", &(),
        ).await.map(|_| ())
    }

    pub async fn accept_pairing(&self, id: &str) -> Result<()> {
        let p = Self::device_path(id);
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "acceptPairing", &(),
        ).await.map(|_| ())
    }

    pub async fn cancel_pairing(&self, id: &str) -> Result<()> {
        let p = Self::device_path(id);
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "cancelPairing", &(),
        ).await.map(|_| ())
    }

    pub async fn unpair(&self, id: &str) -> Result<()> {
        let p = Self::device_path(id);
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some(DEVICE_IFACE), "unpair", &(),
        ).await.map(|_| ())
    }

    async fn daemon_proxy(&self) -> Result<Proxy<'static>> {
        let key = format!("{DAEMON_PATH}\u{0}org.kde.kdeconnect.daemon\u{0}uncached");
        if let Some(proxy) = self
            .proxies
            .lock()
            .expect("proxy cache poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(proxy);
        }

        // Pairing requests are read every poll. Keep the proxy (and its name-owner
        // watch) alive, but do not property-cache this value.
        let proxy: Proxy<'static> = zbus::proxy::Builder::new(&self.conn)
            .destination(KDE_CONNECT_SERVICE)?
            .path(DAEMON_PATH)?
            .interface("org.kde.kdeconnect.daemon")?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        self.proxies
            .lock()
            .expect("proxy cache poisoned")
            .insert(key, proxy.clone());
        Ok(proxy)
    }

    pub async fn pairing_request_ids(&self) -> Vec<String> {
        match self.daemon_proxy().await {
            Ok(p) => p.get_property("pairingRequests").await.unwrap_or_default(),
            Err(_) => vec![],
        }
    }

    pub async fn force_discovery(&self) -> Result<()> {
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), DAEMON_PATH,
            Some("org.kde.kdeconnect.daemon"), "forceOnNetworkChange", &(),
        ).await.map(|_| ())
    }

    pub async fn devices(&self) -> Result<Vec<Device>> {
        self.maybe_rebuild_proxy_cache();

        let ids = match self.device_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                log::warn!("Failed to get device IDs: {e}");
                self.clear_proxy_cache();
                return Err(e);
            }
        };

        let mut devices = Vec::with_capacity(ids.len());
        for id in ids {
            match self.fetch_device(&id).await {
                Some(device) if device.should_display() => devices.push(device),
                Some(device) => {
                    log::debug!(
                        "Ignoring stale KDE Connect device '{}' ({})",
                        device.name,
                        device.id
                    );
                }
                None => {
                    log::warn!("Skipping device '{id}' after a D-Bus read failure");
                    self.forget_device_proxies(&id);
                }
            }
        }
        Ok(devices)
    }

    fn forget_device_proxies(&self, id: &str) {
        let prefix = Self::device_path(id);
        self.proxies
            .lock()
            .expect("proxy cache poisoned")
            .retain(|key, _| !key.starts_with(&prefix));
        self.notification_ids
            .lock()
            .expect("notification cache poisoned")
            .retain(|key, _| !key.starts_with(&format!("{id}\u{0}")));
    }

    async fn fetch_device(&self, id: &str) -> Option<Device> {
        let proxy = self.device_proxy(id).await.ok()?;

        let name: String = proxy.get_property("name").await.ok()?;
        let device_type_raw: String = proxy.get_property("type").await.unwrap_or_default();
        let device_type = DeviceType::from_str(&device_type_raw);
        let is_reachable = proxy.get_property("isReachable").await.unwrap_or(false);
        let is_paired = proxy.get_property("isPaired").await.unwrap_or(false);
        let pair_state = proxy.get_property("pairState").await.unwrap_or(0);
        let supported_plugins = proxy.get_property("supportedPlugins").await.unwrap_or_default();
        let loaded_plugins: Vec<String> = proxy.call("loadedPlugins", &()).await.unwrap_or_default();

        let battery = if is_reachable && loaded_plugins.iter().any(|p| p == "kdeconnect_battery") {
            let charge = self.battery_charge(id).await;
            let is_charging = self.battery_charging(id).await;
            charge.map(|c| BatteryInfo {
                charge: c,
                is_charging: is_charging.unwrap_or(false),
            })
        } else {
            None
        };

        Some(Device {
            id: id.to_string(),
            name,
            device_type,
            is_reachable,
            is_paired,
            pair_state,
            battery,
            supported_plugins,
            loaded_plugins,
        })
    }

    pub async fn perform_action(&self, device_id: &str, action: &ActionType) -> Result<String> {
        match action {
            ActionType::Ring => {
                self.ring_device(device_id).await?;
                Ok("Ring request sent".into())
            }
            ActionType::Ping => {
                self.send_ping(device_id).await?;
                Ok("Ping sent".into())
            }
            ActionType::SendClipboard => {
                self.send_clipboard(device_id).await?;
                Ok("Clipboard sent".into())
            }
            ActionType::SendClipboardText(content) => {
                self.send_clipboard_text(device_id, content).await?;
                Ok("Clipboard text sent".into())
            }
            ActionType::ShareText(text) => {
                self.share_text(device_id, text).await?;
                Ok("Text shared".into())
            }
            ActionType::ShareUrl(url) => {
                self.share_url(device_id, url).await?;
                Ok("URL shared".into())
            }
            ActionType::SendFile(file) => {
                self.share_file(device_id, file).await?;
                Ok("File shared".into())
            }
            ActionType::BrowseFiles => {
                self.browse_files(device_id).await
            }
            ActionType::Pair => {
                self.request_pairing(device_id).await?;
                Ok("Pairing requested".into())
            }
            ActionType::AcceptPairing => {
                self.accept_pairing(device_id).await?;
                Ok("Pairing accepted".into())
            }
            ActionType::CancelPairing => {
                self.cancel_pairing(device_id).await?;
                Ok("Pairing canceled".into())
            }
            ActionType::Unpair => {
                self.unpair(device_id).await?;
                Ok("Device unpaired".into())
            }
            ActionType::DismissNotification(ref internal_id) => {
                self.dismiss_notification(device_id, internal_id).await?;
                Ok("Notification dismissed".into())
            }
            ActionType::ReplyToNotification(ref internal_id, ref text) => {
                self.reply_to_notification(device_id, internal_id, text).await?;
                Ok("Reply sent to notification".into())
            }
            ActionType::MediaAction(ref action) => {
                self.media_action(device_id, action).await?;
                Ok(format!("Media action '{action}' sent"))
            }
            ActionType::SelectPlayer(ref player) => {
                self.select_player(device_id, player).await?;
                Ok(format!("Player '{player}' selected"))
            }
        }
    }

    fn notification_path(device_id: &str, notif_id: &str) -> String {
        format!("/modules/kdeconnect/devices/{device_id}/notifications/{notif_id}")
    }

    pub async fn fetch_notifications(&self, device_id: &str) -> Vec<Notification> {
        let p = Self::plugin_path(device_id, "notifications");
        let ids: Vec<String> = match self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.notifications"), "activeNotifications", &(),
        ).await {
            Ok(reply) => reply.body().deserialize().unwrap_or_default(),
            Err(e) => {
                log::warn!("activeNotifications failed: {e}");
                return vec![];
            }
        };

        let mut notifs = Vec::with_capacity(ids.len());
        for nid in &ids {
            if let Some(n) = self.fetch_notification(device_id, nid).await {
                self.notification_ids
                    .lock()
                    .expect("notification cache poisoned")
                    .insert(
                        format!("{device_id}\u{0}{}", n.internal_id),
                        n.id.clone(),
                    );
                notifs.push(n);
            }
        }
        notifs
    }

    async fn fetch_notification(&self, device_id: &str, notif_id: &str) -> Option<Notification> {
        let proxy = self.notification_proxy(device_id, notif_id).await.ok()?;

        Some(Notification {
            id: notif_id.to_string(),
            internal_id: proxy.get_property("internalId").await.ok().unwrap_or_default(),
            app_name: proxy.get_property("appName").await.ok().unwrap_or_default(),
            title: proxy.get_property("title").await.ok().unwrap_or_default(),
            text: proxy.get_property("text").await.ok().unwrap_or_default(),
            ticker: proxy.get_property("ticker").await.ok().unwrap_or_default(),
            dismissable: proxy.get_property("dismissable").await.ok().unwrap_or_default(),
            reply_id: proxy.get_property("replyId").await.ok().unwrap_or_default(),
        })
    }

    pub async fn dismiss_notification(&self, device_id: &str, internal_id: &str) -> Result<()> {
        let notif_id = self.notif_id_for_internal_id(device_id, internal_id).await;
        if let Some(nid) = notif_id {
            let np = Self::notification_path(device_id, &nid);
            self.conn.call_method(
                Some(KDE_CONNECT_SERVICE), np.as_str(),
                Some("org.kde.kdeconnect.device.notifications.notification"), "dismiss", &(),
            ).await.map(|_| ())
        } else {
            Ok(())
        }
    }

    pub async fn reply_to_notification(&self, device_id: &str, internal_id: &str, text: &str) -> Result<()> {
        let notif_id = self.notif_id_for_internal_id(device_id, internal_id).await;
        if let Some(nid) = notif_id {
            let np = Self::notification_path(device_id, &nid);
            self.conn.call_method(
                Some(KDE_CONNECT_SERVICE), np.as_str(),
                Some("org.kde.kdeconnect.device.notifications.notification"), "sendReply", &(text,),
            ).await.map(|_| ())
        } else {
            Ok(())
        }
    }

    async fn notif_id_for_internal_id(&self, device_id: &str, internal_id: &str) -> Option<String> {
        let key = format!("{device_id}\u{0}{internal_id}");
        if let Some(id) = self
            .notification_ids
            .lock()
            .expect("notification cache poisoned")
            .get(&key)
            .cloned()
        {
            return Some(id);
        }

        self.fetch_notifications(device_id).await;
        self.notification_ids
            .lock()
            .expect("notification cache poisoned")
            .get(&key)
            .cloned()
    }

    pub async fn player_info(&self, device_id: &str) -> Option<PlayerInfo> {
        let iface = "org.kde.kdeconnect.device.mprisremote";
        let proxy = self
            .plugin_proxy(device_id, "mprisremote", iface)
            .await
            .ok()?;

        Some(PlayerInfo {
            player: proxy.get_property("player").await.ok().unwrap_or_default(),
            title: proxy.get_property("title").await.ok().unwrap_or_default(),
            artist: proxy.get_property("artist").await.ok().unwrap_or_default(),
            album: proxy.get_property("album").await.ok().unwrap_or_default(),
            is_playing: proxy.get_property("isPlaying").await.ok().unwrap_or_default(),
            can_seek: proxy.get_property("canSeek").await.ok().unwrap_or_default(),
            length: proxy.get_property("length").await.ok().unwrap_or_default(),
            position: proxy.get_property("position").await.ok().unwrap_or_default(),
            volume: proxy.get_property("volume").await.ok().unwrap_or_default(),
            player_list: proxy.get_property("playerList").await.ok().unwrap_or_default(),
        })
    }

    pub async fn media_action(&self, device_id: &str, action: &str) -> Result<()> {
        let p = Self::plugin_path(device_id, "mprisremote");
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.mprisremote"), "sendAction", &(action,),
        ).await.map(|_| ())
    }

    pub async fn select_player(&self, device_id: &str, player: &str) -> Result<()> {
        let p = Self::plugin_path(device_id, "mprisremote");
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), "org.kde.kdeconnect.device.mprisremote").await?;
        proxy.set_property("player", player).await?;
        Ok(())
    }

    pub async fn notify(&self, title: &str, body: &str, replaces_id: u32, actions: &[&str]) -> Result<u32> {
        let proxy = Proxy::new(
            &self.conn,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        ).await?;

        let actions_vec: Vec<String> = actions.iter().map(|s| s.to_string()).collect();

        let notif_id: u32 = proxy.call("Notify", &(
            "COSMIC Connect",
            replaces_id,
            "io.github.acemythos.Connect-symbolic",
            title,
            body,
            actions_vec,
            HashMap::<String, zvariant::Value>::new(),
            -1i32,
        )).await?;

        Ok(notif_id)
    }
}
