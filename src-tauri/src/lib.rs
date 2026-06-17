#![allow(dead_code)]

mod commands;
mod inject;
mod settings;
mod tray;

use std::sync::Mutex;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const YTM_URL: &str = "https://music.youtube.com";

const UA_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const UA_MACOS: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const UA_LINUX: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

fn user_agent() -> &'static str {
    if cfg!(target_os = "macos") {
        UA_MACOS
    } else if cfg!(target_os = "linux") {
        UA_LINUX
    } else {
        UA_WINDOWS
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));

    builder
        .invoke_handler(tauri::generate_handler![
            commands::show_main_window,
            commands::hide_tray_popup,
            commands::player_control,
            commands::player_seek,
            commands::player_volume,
            commands::set_popup_size,
            commands::resize_popup,
            commands::autostart_get,
            commands::autostart_set,
            commands::navigate_ytm,
        ])
        .setup(|app| {
            let handle = app.handle();

            let mut loaded = settings::load(handle);
            let popup_w = loaded.popup_width.max(280) as f64;
            let popup_h = loaded.popup_height.max(160) as f64;

            #[cfg(desktop)]
            if !loaded.autostart_configured {
                use tauri_plugin_autostart::ManagerExt;
                let _ = handle.autolaunch().enable();
                loaded.autostart_configured = true;
                settings::save(handle, &loaded);
            }

            app.manage(settings::SettingsState(Mutex::new(loaded)));

            let main_window = WebviewWindowBuilder::new(
                handle,
                "main",
                WebviewUrl::External(YTM_URL.parse().expect("valid YTM URL")),
            )
            .title("Flit")
            .inner_size(1200.0, 800.0)
            .min_inner_size(940.0, 560.0)
            .user_agent(user_agent())
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

            let _popup = WebviewWindowBuilder::new(
                handle,
                "tray-popup",
                WebviewUrl::App("index.html".into()),
            )
            .title("Flit")
            .inner_size(popup_w, popup_h)
            .min_inner_size(300.0, 160.0)
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .build()?;

            tray::build_tray(handle)?;

            {
                use tauri::{Emitter, Listener};
                let fwd = handle.clone();
                handle.listen("flit-state", move |event| {
                    if let Ok(value) =
                        serde_json::from_str::<serde_json::Value>(event.payload())
                    {
                        let _ = fwd.emit_to("tray-popup", "flit-player-state", value);
                    }
                    #[cfg(debug_assertions)]
                    {
                        let snippet: String = event.payload().chars().take(120).collect();
                        eprintln!("[flit-state] {snippet}");
                    }
                });
            }

            {
                use tauri::Listener;
                let h = handle.clone();
                handle.listen("flit-toggle-popup", move |_event| {
                    tray::toggle_popup(&h);
                });
            }

            {
                use tauri::Listener;
                use tauri_plugin_opener::OpenerExt;
                let h = handle.clone();
                handle.listen("flit-open-url", move |event| {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                        if let Some(url) = value.get("url").and_then(|u| u.as_str()) {
                            let _ = h.opener().open_url(url, None::<&str>);
                        }
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
