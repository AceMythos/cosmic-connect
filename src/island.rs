//! Activity feed shared between the panel applet (engine) and the island
//! helper (renderer). The applet owns every decision; the helper is a dumb
//! renderer that receives owned snapshots over stdin.
//!
//! Protocol: one JSON object per line.
//!   {"v":1,"activity":{...}}   show/update an activity
//!   {"v":1,"activity":null}    hide the island

use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const FEED_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    Transfer,
    Media,
    Pairing,
    Notification,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FeedActionKind {
    Media(String),
    AcceptPairing,
    CancelPairing,
    DismissNotification(String),
    NotificationAction { internal_id: String, action: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeedAction {
    pub label: String,
    #[serde(default)]
    pub icon: String,
    pub action: FeedActionKind,
    /// Render as a labelled text button instead of an icon button.
    #[serde(default)]
    pub as_text: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeedActivity {
    /// Identity of the activity. A new key resets the island's expansion state.
    pub key: String,
    pub kind: FeedKind,
    pub device_id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub art: Option<String>,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub accent: [f32; 3],
    #[serde(default)]
    pub progress: Option<f32>,
    #[serde(default)]
    pub progress_text: Option<String>,
    /// Playback position in milliseconds, when known.
    #[serde(default)]
    pub position_ms: Option<i64>,
    /// Track length in milliseconds, when known.
    #[serde(default)]
    pub length_ms: Option<i64>,
    /// Whether the player accepts seek requests.
    #[serde(default)]
    pub can_seek: bool,
    /// Human-readable device name for the expanded action row.
    #[serde(default)]
    pub device_name: String,
    /// Short trailing label for the compact pill (e.g. "42%").
    #[serde(default)]
    pub compact: Option<String>,
    #[serde(default)]
    pub primary: Option<FeedAction>,
    #[serde(default)]
    pub actions: Vec<FeedAction>,
    /// Activity wants to start in the expanded presentation (e.g. pairing).
    #[serde(default)]
    pub expand: bool,
    /// Wall-clock deadline after which the activity is no longer valid.
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct FeedEnvelope {
    v: u32,
    activity: Option<FeedActivity>,
}

struct FeedState {
    seq: u64,
    has_message: bool,
    activity: Option<FeedActivity>,
}

static FEED: OnceLock<Mutex<FeedState>> = OnceLock::new();

fn state() -> &'static Mutex<FeedState> {
    FEED.get_or_init(|| {
        Mutex::new(FeedState {
            seq: 0,
            has_message: false,
            activity: None,
        })
    })
}

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn encode(activity: Option<&FeedActivity>) -> String {
    serde_json::to_string(&FeedEnvelope {
        v: FEED_VERSION,
        activity: activity.cloned(),
    })
    .unwrap_or_else(|_| "{\"v\":1,\"activity\":null}".to_string())
}

fn publish(activity: Option<FeedActivity>) -> u64 {
    let mut state = state().lock().expect("island feed poisoned");
    state.seq = state.seq.wrapping_add(1);
    state.has_message = true;
    state.activity = activity;
    state.seq
}

pub fn store_line(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return true;
    }
    match serde_json::from_str::<FeedEnvelope>(line) {
        Ok(envelope) if envelope.v == FEED_VERSION => {
            publish(envelope.activity);
            true
        }
        Ok(envelope) => {
            log::warn!("Island feed: unsupported version {}", envelope.v);
            false
        }
        Err(error) => {
            log::warn!("Island feed: bad line: {error}");
            false
        }
    }
}

/// Latest message, but only if it is newer than `seen`.
pub fn latest_if_newer(seen: u64) -> Option<(u64, Option<FeedActivity>)> {
    let state = state().lock().expect("island feed poisoned");
    if !state.has_message || state.seq <= seen {
        return None;
    }
    Some((state.seq, state.activity.clone()))
}

/// Reads the applet feed from stdin. EOF means the parent applet is gone.
pub fn watch_stdin() {
    std::thread::spawn(|| {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    store_line(&line);
                }
                Err(_) => std::process::exit(0),
            }
        }
        std::process::exit(0);
    });
}
