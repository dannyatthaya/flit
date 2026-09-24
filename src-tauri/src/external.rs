//! Opening links from the YouTube Music page in the system browser.
//!
//! The page asks for these through `window.open` / `target="_blank"`, so the
//! request is untrusted: only plain http(s) links are opened, and requests are
//! rate-limited so a script can't flood the user with browser tabs.

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Url;

const MIN_INTERVAL: Duration = Duration::from_millis(1500);
static LAST_OPEN: Mutex<Option<Instant>> = Mutex::new(None);

/// Whether `url` may be handed to the system browser.
pub fn is_openable(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|h| !h.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
}

/// Open `url` in the default browser. Returns whether it was opened.
pub fn open_in_browser(url: &Url) -> bool {
    if !is_openable(url) {
        return false;
    }
    let Ok(mut last) = LAST_OPEN.lock() else {
        return false;
    };
    if last.is_some_and(|t| t.elapsed() < MIN_INTERVAL) {
        return false;
    }
    *last = Some(Instant::now());
    drop(last);
    open::that_detached(url.as_str()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::is_openable;
    use tauri::Url;

    fn ok(s: &str) -> bool {
        is_openable(&Url::parse(s).unwrap())
    }

    #[test]
    fn only_plain_web_links_are_openable() {
        assert!(ok("https://support.google.com/youtubemusic"));
        assert!(ok("http://example.com/a?b=c"));
        assert!(!ok("file:///etc/passwd"));
        assert!(!ok("javascript:alert(1)"));
        assert!(!ok("ms-settings:privacy"));
        assert!(!ok("smb://host/share"));
        assert!(!ok("https://user:pw@example.com/"));
    }
}
