use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub popup_width: u32,
    pub popup_height: u32,
    pub autostart_configured: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            popup_width: 360,
            popup_height: 282,
            autostart_configured: false,
        }
    }
}

pub struct SettingsState(pub Mutex<Settings>);

fn file_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("settings.json"))
}

pub fn load<R: Runtime>(app: &AppHandle<R>) -> Settings {
    if let Some(path) = file_path(app) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<Settings>(&text) {
                return parsed;
            }
        }
    }
    Settings::default()
}

pub fn save<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    let Some(path) = file_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(settings) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                eprintln!("[flit] failed to write settings: {e}");
            }
        }
        Err(e) => eprintln!("[flit] failed to serialize settings: {e}"),
    }
}
