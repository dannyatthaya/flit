//! Player state relay: the remote YouTube Music page reports its state through
//! the `flit-state` event; this module validates it and forwards the cleaned
//! copy to the trusted tray popup over an IPC channel.
//!
//! The page is untrusted (any script on music.youtube.com can emit events), so
//! every field is sanitized here and the popup never listens to page-emitted
//! events directly — only a channel it subscribed to via an app command, which
//! the remote origin cannot call.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::Url;

const MAX_TEXT: usize = 300;
const MAX_QUEUE: usize = 200;
const MAX_ART_DATA: usize = 512 * 1024;
const ART_DATA_PREFIX: &str = "data:image/jpeg;base64,";

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct RawQueueItem {
    index: Option<u32>,
    title: String,
    artist: String,
    thumb: String,
    video_id: String,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct RawState {
    title: String,
    artist: String,
    album: String,
    artwork_url: String,
    /// Only sent when the art for `art_key` first becomes available.
    artwork_data: Option<String>,
    art_key: String,
    color: String,
    duration_sec: Option<f64>,
    position_sec: Option<f64>,
    playing: bool,
    video_id: String,
    shuffle: bool,
    repeat: String,
    like_status: String,
    volume: Option<f64>,
    muted: bool,
    /// `None` means "unchanged since the last report".
    queue: Option<Vec<RawQueueItem>>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    index: u32,
    title: String,
    artist: String,
    thumb: String,
    video_id: String,
}

/// A message to the popup. The heavy fields (`artworkData`, `queue`) are only
/// present when they changed since the popup's previous message; the popup
/// keeps its last copy otherwise.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    title: String,
    artist: String,
    album: String,
    artwork_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    artwork_data: Option<String>,
    color: String,
    duration_sec: f64,
    position_sec: f64,
    playing: bool,
    video_id: String,
    shuffle: bool,
    repeat: &'static str,
    like_status: &'static str,
    volume: Option<f64>,
    muted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue: Option<Vec<QueueItem>>,
}

/// Latest state from the page. `state` holds the light fields; the art and
/// queue live beside it with version counters so each message can tell
/// whether the popup already has them.
#[derive(Default)]
struct Cache {
    state: PlayerState,
    art_key: String,
    art_data: String,
    art_version: u64,
    queue: Vec<QueueItem>,
    queue_version: u64,
}

/// Versions of the art and queue the popup last received (`None` = never).
#[derive(Default)]
struct Sent {
    art_version: Option<u64>,
    queue_version: Option<u64>,
}

#[derive(Default)]
pub struct PlayerHub {
    cache: Mutex<Cache>,
    channel: Mutex<Option<Channel<PlayerState>>>,
    sent: Mutex<Sent>,
    popup_visible: AtomicBool,
}

impl PlayerHub {
    /// Merge a raw `flit-state` payload into the cache and forward it to the
    /// popup when it is visible. Malformed payloads are dropped.
    pub fn ingest(&self, payload: &str) {
        let Ok(raw) = serde_json::from_str::<RawState>(payload) else {
            return;
        };
        {
            let Ok(mut cache) = self.cache.lock() else {
                return;
            };
            let art_key = clean_text(&raw.art_key, 512);
            if art_key != cache.art_key {
                cache.art_key = art_key;
                if !cache.art_data.is_empty() {
                    cache.art_data.clear();
                    cache.art_version += 1;
                }
            }
            if let Some(data) = raw.artwork_data.as_deref().and_then(clean_art_data) {
                if data != cache.art_data {
                    cache.art_data = data;
                    cache.art_version += 1;
                }
            }
            if let Some(items) = raw.queue {
                cache.queue = clean_queue(items);
                cache.queue_version += 1;
            }
            cache.state = PlayerState {
                title: clean_text(&raw.title, MAX_TEXT),
                artist: clean_text(&raw.artist, MAX_TEXT),
                album: clean_text(&raw.album, MAX_TEXT),
                artwork_url: clean_image_url(&raw.artwork_url),
                artwork_data: None,
                color: clean_color(&raw.color),
                duration_sec: clean_seconds(raw.duration_sec),
                position_sec: clean_seconds(raw.position_sec),
                playing: raw.playing,
                video_id: clean_video_id(&raw.video_id),
                shuffle: raw.shuffle,
                repeat: match raw.repeat.as_str() {
                    "all" => "all",
                    "one" => "one",
                    _ => "none",
                },
                like_status: match raw.like_status.as_str() {
                    "like" => "like",
                    "dislike" => "dislike",
                    _ => "none",
                },
                volume: raw
                    .volume
                    .filter(|v| v.is_finite())
                    .map(|v| v.clamp(0.0, 100.0).round()),
                muted: raw.muted,
                queue: None,
            };
        }
        if self.popup_visible.load(Ordering::Relaxed) {
            self.send(false);
        }
    }

    /// Forget everything about the page (it navigated away from YouTube
    /// Music, so nothing is playing and the old track is gone).
    pub fn reset(&self) {
        {
            let Ok(mut cache) = self.cache.lock() else {
                return;
            };
            let (art_version, queue_version) = (cache.art_version, cache.queue_version);
            *cache = Cache {
                art_version: art_version + 1,
                queue_version: queue_version + 1,
                ..Cache::default()
            };
        }
        if self.popup_visible.load(Ordering::Relaxed) {
            self.send(false);
        }
    }

    /// A (re)loaded popup subscribes; it has nothing yet, so send everything.
    pub fn subscribe(&self, channel: Channel<PlayerState>) {
        if let Ok(mut slot) = self.channel.lock() {
            *slot = Some(channel);
        }
        self.send(true);
    }

    /// The popup webview was closed; stop sending to its channel.
    pub fn unsubscribe(&self) {
        if let Ok(mut slot) = self.channel.lock() {
            *slot = None;
        }
    }

    pub fn popup_visible(&self) -> bool {
        self.popup_visible.load(Ordering::Relaxed)
    }

    pub fn set_popup_visible(&self, visible: bool) {
        let was = self.popup_visible.swap(visible, Ordering::Relaxed);
        if visible && !was {
            // The popup was not receiving updates while hidden; catch it up.
            self.send(false);
        }
    }

    /// Build the next message: light fields always, art and queue only when
    /// `full` or when they changed since the last message. Returns `None` if
    /// the cache is unavailable.
    fn next_message(&self, full: bool) -> Option<PlayerState> {
        let cache = self.cache.lock().ok()?;
        let mut sent = self.sent.lock().ok()?;
        let mut msg = cache.state.clone();
        if full || sent.art_version != Some(cache.art_version) {
            msg.artwork_data = Some(cache.art_data.clone());
            sent.art_version = Some(cache.art_version);
        }
        if full || sent.queue_version != Some(cache.queue_version) {
            msg.queue = Some(cache.queue.clone());
            sent.queue_version = Some(cache.queue_version);
        }
        Some(msg)
    }

    fn send(&self, full: bool) {
        let Ok(slot) = self.channel.lock() else {
            return;
        };
        let Some(ch) = slot.as_ref() else { return };
        if let Some(msg) = self.next_message(full) {
            let _ = ch.send(msg);
        }
    }
}

fn clean_text(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

fn clean_seconds(v: Option<f64>) -> f64 {
    match v {
        Some(v) if v.is_finite() && v > 0.0 => v,
        _ => 0.0,
    }
}

fn clean_color(s: &str) -> String {
    let ok = s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit());
    if ok {
        s.to_ascii_lowercase()
    } else {
        String::new()
    }
}

pub fn clean_video_id(s: &str) -> String {
    let ok = !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        s.to_string()
    } else {
        String::new()
    }
}

/// Only HTTPS images from YouTube/Google image hosts may reach the popup
/// (mirrors the popup's CSP `img-src`).
fn clean_image_url(s: &str) -> String {
    let Ok(url) = Url::parse(s) else {
        return String::new();
    };
    let host = url.host_str().unwrap_or("");
    let allowed = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && (host.ends_with(".ytimg.com")
            || host.ends_with(".googleusercontent.com")
            || host.ends_with(".ggpht.com"));
    if allowed && s.len() <= 2048 {
        url.to_string()
    } else {
        String::new()
    }
}

fn clean_art_data(s: &str) -> Option<String> {
    let body = s.strip_prefix(ART_DATA_PREFIX)?;
    let ok = s.len() <= MAX_ART_DATA
        && !body.is_empty()
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=');
    ok.then(|| s.to_string())
}

fn clean_queue(items: Vec<RawQueueItem>) -> Vec<QueueItem> {
    items
        .into_iter()
        .enumerate()
        .take(MAX_QUEUE)
        .filter_map(|(pos, it)| {
            let title = clean_text(&it.title, MAX_TEXT);
            if title.is_empty() {
                return None;
            }
            Some(QueueItem {
                index: it.index.unwrap_or(pos as u32),
                title,
                artist: clean_text(&it.artist, MAX_TEXT),
                thumb: clean_image_url(&it.thumb),
                video_id: clean_video_id(&it.video_id),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_css_injection_in_color() {
        assert_eq!(clean_color("#A1b2C3"), "#a1b2c3");
        assert_eq!(clean_color("red; background:url(x)"), "");
        assert_eq!(clean_color("#12345"), "");
    }

    #[test]
    fn image_urls_are_restricted() {
        assert!(!clean_image_url("https://i.ytimg.com/vi/abc/hq.jpg").is_empty());
        assert!(!clean_image_url("https://lh3.googleusercontent.com/x=w544").is_empty());
        assert_eq!(clean_image_url("http://i.ytimg.com/vi/abc/hq.jpg"), "");
        assert_eq!(clean_image_url("https://evil.com/i.ytimg.com.jpg"), "");
        assert_eq!(clean_image_url("https://ytimg.com.evil.com/x"), "");
        assert_eq!(clean_image_url("javascript:alert(1)"), "");
    }

    #[test]
    fn art_data_must_be_base64_jpeg() {
        assert!(clean_art_data("data:image/jpeg;base64,AAAA").is_some());
        assert!(clean_art_data("data:image/svg+xml;base64,AAAA").is_none());
        assert!(clean_art_data("data:image/jpeg;base64,AA\"><x").is_none());
        assert!(clean_art_data("data:image/jpeg;base64,").is_none());
    }

    #[test]
    fn video_ids_are_restricted() {
        assert_eq!(clean_video_id("dQw4w9WgXcQ"), "dQw4w9WgXcQ");
        assert_eq!(clean_video_id("a'),alert(1)//"), "");
        assert_eq!(clean_video_id(""), "");
    }

    #[test]
    fn ingest_merges_art_and_queue() {
        let hub = PlayerHub::default();
        hub.ingest(r#"{"title":"A","artKey":"k1","artworkData":"data:image/jpeg;base64,QQ==","queue":[{"index":3,"title":"Q","videoId":"v1"},{"index":4,"title":""}],"durationSec":null,"color":"red;x"}"#);
        let s = hub.next_message(true).unwrap();
        assert_eq!(
            s.artwork_data.as_deref(),
            Some("data:image/jpeg;base64,QQ==")
        );
        let queue = s.queue.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].index, 3);
        assert_eq!(s.duration_sec, 0.0);
        assert_eq!(s.color, "");

        // Same art key, no data, no queue: both are kept.
        hub.ingest(r#"{"title":"A","artKey":"k1"}"#);
        let s = hub.next_message(true).unwrap();
        assert_eq!(
            s.artwork_data.as_deref(),
            Some("data:image/jpeg;base64,QQ==")
        );
        assert_eq!(s.queue.unwrap().len(), 1);

        // New art key: stale art is dropped until the new data arrives.
        hub.ingest(r#"{"title":"B","artKey":"k2","queue":[]}"#);
        let s = hub.next_message(true).unwrap();
        assert_eq!(s.artwork_data.as_deref(), Some(""));
        assert!(s.queue.unwrap().is_empty());
    }

    #[test]
    fn heavy_fields_are_only_resent_when_changed() {
        let hub = PlayerHub::default();
        hub.ingest(r#"{"title":"A","artKey":"k1","artworkData":"data:image/jpeg;base64,QQ==","queue":[{"title":"Q"}],"positionSec":1}"#);
        let first = hub.next_message(false).unwrap();
        assert!(first.artwork_data.is_some() && first.queue.is_some());

        // Only the position changed: the message carries neither art nor queue.
        hub.ingest(r#"{"title":"A","artKey":"k1","positionSec":2}"#);
        let json = serde_json::to_string(&hub.next_message(false).unwrap()).unwrap();
        assert!(
            !json.contains("artworkData") && !json.contains("\"queue\""),
            "{json}"
        );
        assert!(json.contains("\"positionSec\":2.0"), "{json}");

        // A new queue is sent once, without the unchanged art.
        hub.ingest(r#"{"title":"A","artKey":"k1","queue":[{"title":"Q"},{"title":"R"}]}"#);
        let s = hub.next_message(false).unwrap();
        assert!(s.artwork_data.is_none());
        assert_eq!(s.queue.unwrap().len(), 2);

        // A full message (new subscriber) always carries both.
        let s = hub.next_message(true).unwrap();
        assert!(s.artwork_data.is_some() && s.queue.is_some());
    }

    #[test]
    fn reset_clears_state_and_resends_empty_heavy_fields() {
        let hub = PlayerHub::default();
        hub.ingest(r#"{"title":"A","playing":true,"artKey":"k1","artworkData":"data:image/jpeg;base64,QQ==","queue":[{"title":"Q"}]}"#);
        let _ = hub.next_message(false);
        hub.reset();
        let s = hub.next_message(false).unwrap();
        assert_eq!(s.title, "");
        assert!(!s.playing);
        assert_eq!(s.artwork_data.as_deref(), Some(""));
        assert!(s.queue.unwrap().is_empty());
    }

    #[test]
    fn malformed_payload_is_ignored() {
        let hub = PlayerHub::default();
        hub.ingest(r#"{"title":"A"}"#);
        hub.ingest("not json");
        hub.ingest(r#"{"title":5}"#);
        assert_eq!(hub.next_message(false).unwrap().title, "A");
    }
}
