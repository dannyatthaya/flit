use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime, State, Url};

use crate::player::{clean_video_id, PlayerHub, PlayerState};
use crate::updater::{UpdateStatus, Updater};

fn eval_main<R: Runtime>(app: &AppHandle<R>, js: &str) {
    if let Some(win) = app.get_webview_window(crate::tray::MAIN_LABEL) {
        let _ = win.eval(js);
    }
}

#[tauri::command]
pub fn show_main_window<R: Runtime>(app: AppHandle<R>) {
    crate::tray::show_main_window(&app);
}

#[tauri::command]
pub fn hide_tray_popup<R: Runtime>(app: AppHandle<R>) {
    crate::tray::hide_popup(&app);
}

/// The popup receives player state over this channel. Unlike a global event,
/// the remote YouTube Music page cannot write into it.
#[tauri::command]
pub fn player_subscribe(hub: State<PlayerHub>, on_state: Channel<PlayerState>) {
    hub.subscribe(on_state);
}

#[tauri::command]
pub fn player_control<R: Runtime>(app: AppHandle<R>, action: String) {
    let call = match action.as_str() {
        "play_pause" => "playPause()",
        "next" => "next()",
        "previous" => "previous()",
        "like" => "like()",
        "dislike" => "dislike()",
        "shuffle" => "shuffle()",
        "repeat" => "repeat()",
        _ => return,
    };
    eval_main(&app, &format!("window.__flit__&&window.__flit__.{call}"));
}

/// Jump to the queue entry at `index`. `video_id` guards against the queue
/// having changed since the popup rendered it: the page looks the track up by
/// id when the index no longer matches.
#[tauri::command]
pub fn player_queue_jump<R: Runtime>(app: AppHandle<R>, index: u32, video_id: String) {
    let id = clean_video_id(&video_id);
    eval_main(
        &app,
        &format!("window.__flit__&&window.__flit__.queueJump({index},'{id}')"),
    );
}

#[tauri::command]
pub fn player_seek<R: Runtime>(app: AppHandle<R>, position: f64) {
    if position.is_finite() {
        let position = position.max(0.0);
        eval_main(
            &app,
            &format!("window.__flit__&&window.__flit__.seek({position})"),
        );
    }
}

#[tauri::command]
pub fn player_volume<R: Runtime>(app: AppHandle<R>, volume: f64) {
    if volume.is_finite() {
        let clamped = volume.clamp(0.0, 100.0).round();
        eval_main(
            &app,
            &format!("window.__flit__&&window.__flit__.setVolume({clamped})"),
        );
    }
}

#[tauri::command]
pub fn resize_popup<R: Runtime>(app: AppHandle<R>, height: f64) {
    if let Some(win) = app.get_webview_window(crate::tray::POPUP_LABEL) {
        let target = crate::tray::clamp_popup_height(&win, height);
        crate::tray::animate_resize(win, target);
    }
}

#[tauri::command]
pub fn autostart_get<R: Runtime>(app: AppHandle<R>) -> bool {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().unwrap_or(false)
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        false
    }
}

#[tauri::command]
pub fn autostart_set<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<(), String> {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let launcher = app.autolaunch();
        let result = if enabled {
            launcher.enable()
        } else {
            launcher.disable()
        };
        result.map_err(|e| e.to_string())
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
        Err("launch at startup is not supported on this platform".into())
    }
}

#[tauri::command]
pub fn update_status(updater: State<Updater>) -> UpdateStatus {
    updater.status()
}

#[tauri::command]
pub async fn update_check<R: Runtime>(app: AppHandle<R>) -> UpdateStatus {
    crate::updater::check_and_download(&app).await;
    app.state::<Updater>().status()
}

/// Async so the install (which can copy a large bundle or wait on a password
/// prompt on Linux) runs off the main thread and never freezes the UI.
#[tauri::command]
pub async fn update_install<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::updater::install(&app))
        .await
        .map_err(|e| e.to_string())?
}

/// Keep the popup open when it loses focus (the pin button in its header).
#[tauri::command]
pub fn popup_set_pinned(pinned: bool) {
    crate::tray::set_popup_pinned(pinned);
}

#[tauri::command]
pub fn navigate_ytm<R: Runtime>(app: AppHandle<R>, url: String) -> Result<(), String> {
    let target = normalize_ytm_url(&url)?;
    if let Some(win) = app.get_webview_window(crate::tray::MAIN_LABEL) {
        win.navigate(target).map_err(|e| e.to_string())?;
        crate::tray::show_main_window(&app);
    }
    Ok(())
}

/// Turn a pasted YouTube / YouTube Music link into a music.youtube.com URL.
/// Accepts links without a scheme, youtube.com / m.youtube.com / youtu.be
/// links, and rejects everything else.
pub fn normalize_ytm_url(input: &str) -> Result<Url, String> {
    let input = input.trim();
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let parsed =
        Url::parse(&with_scheme).map_err(|_| "That doesn't look like a link".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only YouTube Music links can be opened".into());
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();

    let mut out = Url::parse("https://music.youtube.com/").expect("valid base URL");
    match host.as_str() {
        "music.youtube.com" | "youtube.com" | "www.youtube.com" | "m.youtube.com" => {
            // Shorts, embed and live links have no page on YouTube Music;
            // play the video through the watch page instead.
            let video_path = ["/shorts/", "/embed/", "/live/"]
                .iter()
                .find_map(|prefix| parsed.path().strip_prefix(prefix))
                .map(|rest| clean_video_id(rest.split('/').next().unwrap_or("")))
                .filter(|id| !id.is_empty());
            match video_path {
                Some(id) => {
                    out.set_path("/watch");
                    out.query_pairs_mut().append_pair("v", &id);
                }
                None => {
                    out.set_path(parsed.path());
                    out.set_query(parsed.query());
                }
            }
        }
        "youtu.be" => {
            let id = clean_video_id(parsed.path().trim_start_matches('/'));
            if id.is_empty() {
                return Err("That youtu.be link has no video id".into());
            }
            let mut pairs = vec![("v".to_string(), id)];
            pairs.extend(
                parsed
                    .query_pairs()
                    .filter(|(k, _)| k == "list")
                    .map(|(k, v)| (k.into_owned(), v.into_owned())),
            );
            out.set_path("/watch");
            out.query_pairs_mut().extend_pairs(pairs);
        }
        _ => return Err("Only YouTube Music links can be opened".into()),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::normalize_ytm_url;

    fn n(s: &str) -> Result<String, String> {
        normalize_ytm_url(s).map(|u| u.to_string())
    }

    #[test]
    fn normalizes_links() {
        assert_eq!(
            n("music.youtube.com/watch?v=abc").unwrap(),
            "https://music.youtube.com/watch?v=abc"
        );
        assert_eq!(
            n("https://www.youtube.com/watch?v=abc&list=PL1").unwrap(),
            "https://music.youtube.com/watch?v=abc&list=PL1"
        );
        assert_eq!(
            n("https://youtu.be/abc?list=PL1&si=x").unwrap(),
            "https://music.youtube.com/watch?v=abc&list=PL1"
        );
        assert_eq!(
            n("http://user:pw@music.youtube.com:8443/playlist?list=PL1").unwrap(),
            "https://music.youtube.com/playlist?list=PL1"
        );
    }

    #[test]
    fn maps_video_only_paths_to_watch() {
        assert_eq!(
            n("https://www.youtube.com/shorts/abc123?feature=share").unwrap(),
            "https://music.youtube.com/watch?v=abc123"
        );
        assert_eq!(
            n("youtube.com/embed/abc123").unwrap(),
            "https://music.youtube.com/watch?v=abc123"
        );
        assert_eq!(
            n("https://youtube.com/live/abc123/").unwrap(),
            "https://music.youtube.com/watch?v=abc123"
        );
        // An unusable id keeps the original path.
        assert_eq!(
            n("https://youtube.com/shorts/").unwrap(),
            "https://music.youtube.com/shorts/"
        );
    }

    #[test]
    fn rejects_other_hosts() {
        assert!(n("https://evil.com/music.youtube.com").is_err());
        assert!(n("https://music.youtube.com.evil.com/").is_err());
        assert!(n("javascript:alert(1)").is_err());
        assert!(n("file:///etc/passwd").is_err());
        assert!(n("https://youtu.be/").is_err());
    }
}
