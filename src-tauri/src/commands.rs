use crate::settings::SettingsState;
use tauri::{AppHandle, Manager, Runtime, State};

fn eval_main<R: Runtime>(app: &AppHandle<R>, js: &str) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.eval(js);
    }
}

#[tauri::command]
pub fn show_main_window<R: Runtime>(app: AppHandle<R>) {
    crate::tray::show_main_window(&app);
}

#[tauri::command]
pub fn hide_tray_popup<R: Runtime>(app: AppHandle<R>) {
    if let Some(win) = app.get_webview_window("tray-popup") {
        let _ = win.hide();
    }
}

#[tauri::command]
pub fn player_control<R: Runtime>(app: AppHandle<R>, action: String) {
    let call = match action.as_str() {
        "play_pause" => "playPause()".to_string(),
        "next" => "next()".to_string(),
        "previous" => "previous()".to_string(),
        "like" => "like()".to_string(),
        "dislike" => "dislike()".to_string(),
        "shuffle" => "shuffle()".to_string(),
        "repeat" => "repeat()".to_string(),
        other => match other.strip_prefix("queue_jump_").and_then(|n| n.parse::<u32>().ok()) {
            Some(index) => format!("queueJump({index})"),
            None => return,
        },
    };
    eval_main(&app, &format!("window.__flit__&&window.__flit__.{call}"));
}

#[tauri::command]
pub fn player_seek<R: Runtime>(app: AppHandle<R>, position: f64) {
    if position.is_finite() {
        eval_main(&app, &format!("window.__flit__&&window.__flit__.seek({position})"));
    }
}

#[tauri::command]
pub fn player_volume<R: Runtime>(app: AppHandle<R>, volume: f64) {
    if volume.is_finite() {
        let clamped = volume.clamp(0.0, 100.0);
        eval_main(&app, &format!("window.__flit__&&window.__flit__.setVolume({clamped})"));
    }
}

#[tauri::command]
pub fn set_popup_size<R: Runtime>(
    app: AppHandle<R>,
    width: f64,
    height: f64,
    state: State<SettingsState>,
) {
    if let Some(win) = app.get_webview_window("tray-popup") {
        let _ = win.set_size(tauri::LogicalSize::new(width, height));
    }
    if let Ok(mut s) = state.0.lock() {
        s.popup_width = width.round().max(0.0) as u32;
        s.popup_height = height.round().max(0.0) as u32;
        crate::settings::save(&app, &s);
    }
}

#[tauri::command]
pub fn resize_popup<R: Runtime>(app: AppHandle<R>, height: f64, state: State<SettingsState>) {
    if let Some(win) = app.get_webview_window("tray-popup") {
        let target = crate::tray::clamp_popup_height(&win, height);
        crate::tray::animate_resize(win, target);
        if let Ok(mut s) = state.0.lock() {
            s.popup_height = target.round().max(0.0) as u32;
            crate::settings::save(&app, &s);
        }
    }
}

#[tauri::command]
pub fn autostart_get<R: Runtime>(app: AppHandle<R>) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn autostart_set<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
    state: State<SettingsState>,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| e.to_string())?;
    if let Ok(mut s) = state.0.lock() {
        s.autostart_configured = true;
        crate::settings::save(&app, &s);
    }
    Ok(())
}

#[tauri::command]
pub fn navigate_ytm<R: Runtime>(app: AppHandle<R>, url: String) -> Result<(), String> {
    let parsed = tauri::Url::parse(&url).map_err(|_| "invalid URL".to_string())?;
    if parsed.host_str() != Some("music.youtube.com") {
        return Err("only music.youtube.com is allowed".into());
    }
    if let Some(win) = app.get_webview_window("main") {
        win.navigate(parsed).map_err(|e| e.to_string())?;
        crate::tray::show_main_window(&app);
    }
    Ok(())
}
