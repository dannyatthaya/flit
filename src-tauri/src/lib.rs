mod commands;
mod inject;
mod player;
mod tray;
mod updater;

use tauri::{Listener, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const YTM_URL: &str = "https://music.youtube.com";

/// Passed by the launch-at-startup entry so login starts Flit in the tray.
const MINIMIZED_ARG: &str = "--minimized";

/// Chrome major version advertised on macOS/Linux, where the webview is WebKit
/// and has no Chromium version of its own. Bump it now and then so YouTube
/// Music doesn't treat the app as an outdated browser.
const FALLBACK_CHROME_MAJOR: u32 = 140;

fn chrome_major() -> u32 {
    // On Windows the webview *is* Chromium (WebView2, kept up to date by the
    // OS), so advertise its real version instead of a hard-coded one.
    #[cfg(windows)]
    if let Some(major) = wry::webview_version()
        .ok()
        .and_then(|v| v.split('.').next().and_then(|m| m.parse::<u32>().ok()))
    {
        return major;
    }
    FALLBACK_CHROME_MAJOR
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
            .build()?;

            {
                let w = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }

            let popup = WebviewWindowBuilder::new(
                handle,
                tray::POPUP_LABEL,
                WebviewUrl::App("index.html".into()),
            )
            .title("Flit")
            .inner_size(tray::POPUP_WIDTH, tray::POPUP_COMPACT_HEIGHT)
            .min_inner_size(tray::POPUP_WIDTH, tray::MIN_POPUP_HEIGHT)
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .build()?;

            {
                let h = handle.clone();
                popup.on_window_event(move |event| match event {
                    // Alt+F4 etc. would destroy the popup for good; hide it instead.
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        tray::hide_popup(&h);
                    }
                    WindowEvent::Focused(false) => tray::popup_blurred(&h),
                    _ => {}
                });
            }

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
                let h = handle.clone();
                handle.listen("flit-toggle-popup", move |_event| {
                    tray::toggle_popup(&h, None);
                });
            }

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
