use zbus::{Connection, Proxy, Result};

use crate::model::{ActionType, Attachment, BatteryInfo, ConnectivityInfo, ConversationAddress, ConversationMessage, Device, DeviceType, Notification, PlayerInfo};

const KDE_CONNECT_SERVICE: &str = "org.kde.kdeconnect";
const DAEMON_PATH: &str = "/modules/kdeconnect";
const DEVICE_IFACE: &str = "org.kde.kdeconnect.device";

#[derive(Debug)]
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

    pub async fn loaded_plugins(&self, id: &str) -> Result<Vec<String>> {
        let p = Self::device_path(id);
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), DEVICE_IFACE).await?;
        proxy.call("loadedPlugins", &()).await
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

    pub async fn connectivity_info(&self, device_id: &str) -> Option<ConnectivityInfo> {
        let p = Self::plugin_path(device_id, "connectivity_report");
        let iface = "org.kde.kdeconnect.device.connectivity_report";
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), iface).await.ok()?;

        Some(ConnectivityInfo {
            network_type: proxy.get_property("cellularNetworkType").await.ok().unwrap_or_default(),
            signal_strength: proxy.get_property("cellularNetworkStrength").await.ok().unwrap_or(0),
        })
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

    pub async fn pairing_request_ids(&self) -> Vec<String> {
        match Proxy::new(
            &self.conn, KDE_CONNECT_SERVICE, DAEMON_PATH,
            "org.kde.kdeconnect.daemon",
        ).await {
            Ok(p) => p.get_property("pairingRequests").await.unwrap_or_default(),
            Err(_) => vec![],
        }
    }

    pub async fn force_discovery(&self) {
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), DAEMON_PATH,
            Some("org.kde.kdeconnect.daemon"), "forceOnNetworkChange", &(),
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
        let loaded_plugins = self.loaded_plugins(id).await.unwrap_or_default();

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

        Device {
            id: id.to_string(),
            name,
            device_type,
            is_reachable,
            is_paired,
            pair_state,
            battery,
            supported_plugins,
            loaded_plugins,
        }
    }

    pub async fn perform_action(&self, device_id: &str, action: &ActionType) -> Result<String> {
        match action {
            ActionType::Ring => {
                self.ring_device(device_id).await;
                Ok("Ring request sent".into())
            }
            ActionType::Ping => {
                self.send_ping(device_id).await;
                Ok("Ping sent".into())
            }
            ActionType::SendClipboard => {
                self.send_clipboard(device_id).await;
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
                self.request_pairing(device_id).await;
                Ok("Pairing requested".into())
            }
            ActionType::AcceptPairing => {
                self.accept_pairing(device_id).await;
                Ok("Pairing accepted".into())
            }
            ActionType::CancelPairing => {
                self.cancel_pairing(device_id).await;
                Ok("Pairing canceled".into())
            }
            ActionType::Unpair => {
                self.unpair(device_id).await;
                Ok("Device unpaired".into())
            }
            ActionType::ReplyToConversation(thread_id, ref text) => {
                self.reply_to_conversation(device_id, *thread_id, text).await?;
                Ok("Reply sent".into())
            }
            ActionType::SendSms(ref addresses, ref text) => {
                self.send_sms(device_id, addresses, text).await?;
                Ok("SMS sent".into())
            }
            ActionType::DismissNotification(ref internal_id) => {
                self.dismiss_notification(device_id, internal_id).await;
                Ok("Notification dismissed".into())
            }
            ActionType::ReplyToNotification(ref internal_id, ref text) => {
                self.reply_to_notification(device_id, internal_id, text).await?;
                Ok("Reply sent to notification".into())
            }
            ActionType::MediaAction(ref action) => {
                self.media_action(device_id, action).await;
                Ok(format!("Media action '{action}' sent"))
            }
            ActionType::SelectPlayer(ref player) => {
                self.select_player(device_id, player).await?;
                Ok(format!("Player '{player}' selected"))
            }
        }
    }

    pub async fn request_all_conversations(&self, device_id: &str) {
        let p = Self::plugin_path(device_id, "sms");
        match self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.sms"), "requestAllConversations", &(),
        ).await {
            Ok(_) => log::info!("requestAllConversations succeeded for {device_id}"),
            Err(e) => log::warn!("requestAllConversations failed for {device_id}: {e}"),
        }
    }

    pub async fn active_conversations(&self, device_id: &str) -> Vec<ConversationMessage> {
        let p = Self::device_path(device_id);
        let result = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.conversations"), "activeConversations", &(),
        ).await;
        match result {
            Ok(reply) => {
                let body = reply.body();
                let raw: Vec<zvariant::Value> = body.deserialize().unwrap_or_default();
                log::info!("activeConversations raw count: {}", raw.len());
                raw.into_iter().filter_map(|v| {
                    let json = serde_json::to_value(&v).ok()?;
                    let arr = json.get("value")?.as_array()?.clone();
                    serde_json::from_value(serde_json::Value::Array(arr)).ok()
                }).collect()
            }
            Err(e) => {
                log::warn!("activeConversations failed for {device_id}: {e}");
                vec![]
            },
        }
    }

    pub async fn reply_to_conversation(&self, device_id: &str, thread_id: i64, message: &str) -> Result<()> {
        let p = Self::device_path(device_id);
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.conversations"), "replyToConversation",
            &(thread_id, message, Vec::<zvariant::Value>::new()),
        ).await?;
        Ok(())
    }

    pub async fn send_sms(&self, device_id: &str, addresses: &[String], message: &str) -> Result<()> {
        let p = Self::plugin_path(device_id, "sms");
        let addrs: Vec<ConversationAddress> = addresses.iter().map(|a| ConversationAddress { address: a.clone() }).collect();
        let attachments: Vec<Attachment> = vec![];
        self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.sms"), "sendSms",
            &(addrs, message, attachments, -1i64),
        ).await?;
        Ok(())
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
                notifs.push(n);
            }
        }
        notifs
    }

    async fn fetch_notification(&self, device_id: &str, notif_id: &str) -> Option<Notification> {
        let np = Self::notification_path(device_id, notif_id);
        let iface = "org.kde.kdeconnect.device.notifications.notification";
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, np.as_str(), iface).await.ok()?;

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

    pub async fn dismiss_notification(&self, device_id: &str, internal_id: &str) {
        let notif_id = self.notif_id_for_internal_id(device_id, internal_id).await;
        if let Some(nid) = notif_id {
            let np = Self::notification_path(device_id, &nid);
            let _ = self.conn.call_method(
                Some(KDE_CONNECT_SERVICE), np.as_str(),
                Some("org.kde.kdeconnect.device.notifications.notification"), "dismiss", &(),
            ).await;
        }
    }

    pub async fn reply_to_notification(&self, device_id: &str, internal_id: &str, text: &str) -> Result<()> {
        let notif_id = self.notif_id_for_internal_id(device_id, internal_id).await;
        if let Some(nid) = notif_id {
            let np = Self::notification_path(device_id, &nid);
            self.conn.call_method(
                Some(KDE_CONNECT_SERVICE), np.as_str(),
                Some("org.kde.kdeconnect.device.notifications.notification"), "sendReply", &(text,),
            ).await?;
        }
        Ok(())
    }

    async fn notif_id_for_internal_id(&self, device_id: &str, internal_id: &str) -> Option<String> {
        let notifs = self.fetch_notifications(device_id).await;
        notifs.into_iter().find(|n| n.internal_id == internal_id).map(|n| n.id)
    }

    pub async fn player_info(&self, device_id: &str) -> Option<PlayerInfo> {
        let p = Self::plugin_path(device_id, "mprisremote");
        let iface = "org.kde.kdeconnect.device.mprisremote";
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), iface).await.ok()?;

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

    pub async fn media_action(&self, device_id: &str, action: &str) {
        let p = Self::plugin_path(device_id, "mprisremote");
        let _ = self.conn.call_method(
            Some(KDE_CONNECT_SERVICE), p.as_str(),
            Some("org.kde.kdeconnect.device.mprisremote"), "sendAction", &(action,),
        ).await;
    }

    pub async fn select_player(&self, device_id: &str, player: &str) -> Result<()> {
        let p = Self::plugin_path(device_id, "mprisremote");
        let proxy = Proxy::new(&self.conn, KDE_CONNECT_SERVICE, p.as_str(), "org.kde.kdeconnect.device.mprisremote").await?;
        proxy.set_property("player", player).await?;
        Ok(())
    }
}


