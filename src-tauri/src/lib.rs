mod commands;
#[cfg(desktop)]
mod external;
mod inject;
mod player;
mod tray;
mod updater;

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{Listener, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const YTM_URL: &str = "https://music.youtube.com";
const YTM_HOST: &str = "music.youtube.com";

/// Passed by the launch-at-startup entry so login starts Flit in the tray.
const MINIMIZED_ARG: &str = "--minimized";

/// Chrome 140 reached stable on 2025-09-02 (UTC); Chrome ships a new major
/// version every four weeks.
const CHROME_ANCHOR_MAJOR: u32 = 140;
const CHROME_ANCHOR_UNIX: u64 = 1_756_771_200;
const CHROME_RELEASE_SECS: u64 = 28 * 24 * 60 * 60;

/// The Chrome major version that is current around `unix_secs`, estimated from
/// Chrome's release cadence. Capped so a wildly wrong clock can't produce an
/// absurd version.
fn estimated_chrome_major(unix_secs: u64) -> u32 {
    let releases = unix_secs.saturating_sub(CHROME_ANCHOR_UNIX) / CHROME_RELEASE_SECS;
    CHROME_ANCHOR_MAJOR + releases.min(130) as u32
}

fn chrome_major() -> u32 {
    // On Windows the webview *is* Chromium (WebView2, kept up to date by the
    // OS), so advertise its real version.
    #[cfg(windows)]
    if let Some(major) = wry::webview_version()
        .ok()
        .and_then(|v| v.split('.').next().and_then(|m| m.parse::<u32>().ok()))
    {
        return major;
    }
    // macOS/Linux use WebKit, which has no Chromium version; estimate the
    // current one so YouTube Music never sees an outdated browser.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(CHROME_ANCHOR_UNIX);
    estimated_chrome_major(now)
}

fn user_agent() -> String {
    let platform = if cfg!(target_os = "macos") {
        "Macintosh; Intel Mac OS X 10_15_7"
    } else if cfg!(target_os = "linux") {
        "X11; Linux x86_64"
    } else {
        "Windows NT 10.0; Win64; x64"
    };
    format!(
        "Mozilla/5.0 ({platform}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36",
        chrome_major()
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ));

    let app = builder
        .manage(player::PlayerHub::default())
        .manage(updater::Updater::default())
        .invoke_handler(tauri::generate_handler![
            commands::show_main_window,
            commands::hide_tray_popup,
            commands::player_subscribe,
            commands::player_control,
            commands::player_queue_jump,
            commands::player_seek,
            commands::player_volume,
            commands::resize_popup,
            commands::autostart_get,
            commands::autostart_set,
            commands::update_status,
            commands::update_check,
            commands::update_install,
            commands::navigate_ytm,
            commands::popup_set_pinned,
        ])
        .setup(|app| {
            let handle = app.handle();
            let start_minimized = std::env::args().any(|a| a == MINIMIZED_ARG);

            let main_window = WebviewWindowBuilder::new(
                handle,
                tray::MAIN_LABEL,
                WebviewUrl::External(YTM_URL.parse().expect("valid YTM URL")),
            )
            .title("Flit")
            .inner_size(1200.0, 800.0)
            .min_inner_size(940.0, 560.0)
            .visible(!start_minimized)
            .user_agent(&user_agent())
            .initialization_script(inject::INJECT_JS)
            // Links that ask for a new window (target="_blank", window.open)
            // open in the system browser; the app never spawns extra windows.
            .on_new_window(|url, _features| {
                #[cfg(desktop)]
                external::open_in_browser(&url);
                let _ = url;
                NewWindowResponse::Deny
            })
            .on_page_load(|window, payload| match payload.event() {
                // Leaving YouTube Music (sign-in, a link that navigated away)
                // stops playback; don't keep showing the old track as playing.
                PageLoadEvent::Started if payload.url().host_str() != Some(YTM_HOST) => {
                    window.state::<player::PlayerHub>().reset();
                }
                // The page can't reliably tell when Tauri hides its window or
                // opens the popup, so tell the new bridge after every load.
                PageLoadEvent::Finished => tray::sync_bridge(&window),
                _ => {}
            })
            .build()?;

            {
                let h = handle.clone();
                let w = main_window.clone();
                main_window.on_window_event(move |event| match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        tray::hide_main_window(&h);
                    }
                    // Minimizing/restoring arrives as a resize.
                    WindowEvent::Resized(_) => tray::sync_main_visibility(&w, false),
                    _ => {}
                });
            }

            // The popup webview is created on first open (see tray.rs).

            tray::build_tray(handle)?;

            // Events below come from the injected bridge in the *remote* page.
            // Treat them as untrusted: the payload is validated in `player`,
            // and the only other action is toggling the popup.
            {
                let h = handle.clone();
                handle.listen("flit-state", move |event| {
                    h.state::<player::PlayerHub>().ingest(event.payload());
                });
            }
            {
                // Rate-limited so a script on the page can't make the popup
                // flash open and shut.
                let h = handle.clone();
                let last = Mutex::new(None::<Instant>);
                handle.listen("flit-toggle-popup", move |_event| {
                    let Ok(mut last) = last.lock() else { return };
                    if last.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
                        return;
                    }
                    *last = Some(Instant::now());
                    drop(last);
                    tray::toggle_popup(&h, None);
                });
            }

            refresh_autostart_entry(handle);
            updater::spawn_background_checks(handle);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, _event| {
        // macOS: clicking the Dock icon should bring back the hidden window.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event {
            tray::show_main_window(_app);
        }
    });
}

/// Launch-at-startup entries written by older versions have no
/// `--minimized` argument (and 0.1.0 enabled the entry without asking), so
/// rewrite an existing entry with the current arguments. Never creates one.
fn refresh_autostart_entry<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    #[cfg(desktop)]
    if !cfg!(debug_assertions) {
        use tauri_plugin_autostart::ManagerExt;
        let launcher = app.autolaunch();
        if launcher.is_enabled().unwrap_or(false) {
            let _ = launcher.enable();
        }
    }
    #[cfg(not(desktop))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_version_estimate_follows_release_cadence() {
        assert_eq!(estimated_chrome_major(0), CHROME_ANCHOR_MAJOR);
        assert_eq!(estimated_chrome_major(CHROME_ANCHOR_UNIX), 140);
        // 2026-09-24 is 387 days after the anchor: 13 four-week releases.
        assert_eq!(
            estimated_chrome_major(CHROME_ANCHOR_UNIX + 387 * 86_400),
            153
        );
        assert_eq!(estimated_chrome_major(u64::MAX), 270);
    }
}
