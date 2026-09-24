//! Auto-updater: checks GitHub Releases in the background and, when the user
//! clicks "Update" in the tray popup, downloads the newer signed build (the
//! plugin verifies the minisign signature against the `pubkey` in
//! tauri.conf.json), installs it and restarts.
//!
//! The download only starts on that click: installers are 5–100 MB (a Linux
//! AppImage is the large end), and holding one in memory for hours until the
//! user gets round to it would dwarf the app's own footprint.
//!
//! All updater calls happen in Rust, so no webview holds updater permissions.
//! The popup only reads the status and asks to install the verified update.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(15);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Emitted to the popup (no payload) whenever the status changes; the popup
/// then reads the status through the `update_status` command.
pub const STATUS_EVENT: &str = "flit-update-status";

#[derive(Serialize, Clone, Default)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
    },
    Downloading {
        version: String,
        percent: Option<u8>,
    },
    Installing {
        version: String,
    },
    Error {
        message: String,
    },
}

#[derive(Default)]
pub struct Updater {
    status: Mutex<UpdateStatus>,
    /// The newest update found; downloaded only when the user installs it.
    #[cfg(desktop)]
    pending: Mutex<Option<tauri_plugin_updater::Update>>,
    busy: AtomicBool,
}

impl Updater {
    pub fn status(&self) -> UpdateStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Version of the update waiting to be installed, if any.
    fn pending_version(&self) -> Option<String> {
        #[cfg(desktop)]
        {
            self.pending
                .lock()
                .ok()
                .and_then(|p| p.as_ref().map(|u| u.version.clone()))
        }
        #[cfg(not(desktop))]
        None
    }

    fn set_status<R: Runtime>(&self, app: &AppHandle<R>, status: UpdateStatus) {
        let available_version = match &status {
            UpdateStatus::Available { version } => Some(version.clone()),
            _ => None,
        };
        if let Ok(mut s) = self.status.lock() {
            *s = status;
        }
        let _ = app.emit_to(crate::tray::POPUP_LABEL, STATUS_EVENT, ());
        if let Some(version) = available_version {
            notify_main_window(app, &version);
        }
    }
}

/// Show a small "update available" notice inside the YouTube Music window.
fn notify_main_window<R: Runtime>(app: &AppHandle<R>, version: &str) {
    let Some(win) = app.get_webview_window(crate::tray::MAIN_LABEL) else {
        return;
    };
    // JSON string literals are valid JS string literals, so this cannot break
    // out of the call even if the release manifest carried odd characters.
    let Ok(arg) = serde_json::to_string(version) else {
        return;
    };
    let _ = win.eval(format!(
        "window.__flit__&&window.__flit__.showUpdateNotif&&window.__flit__.showUpdateNotif({arg})"
    ));
}

/// Keep only characters that belong in a version string.
fn clean_version(v: &str) -> String {
    v.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        .take(64)
        .collect()
}

/// Start the periodic background check (release builds only, so development
/// runs never nag or download).
pub fn spawn_background_checks<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            tauri::async_runtime::block_on(check(&app));
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

/// Look for an update. Once one is found, later checks only replace it with a
/// strictly newer version. Does nothing while a check, download or install
/// is running.
pub async fn check<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<Updater>();
    if state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = run_check(app, &state).await;
    // With an update already found, a failed re-check must not hide the
    // "Update" button; keep offering what we have.
    if let Err(message) = result {
        if state.pending_version().is_none() {
            state.set_status(app, UpdateStatus::Error { message });
        }
    }
    state.busy.store(false, Ordering::SeqCst);
}

/// True when `candidate` is a strictly newer semver version than `current`.
fn is_newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(current),
    ) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}

#[cfg(desktop)]
async fn run_check<R: Runtime>(app: &AppHandle<R>, state: &Updater) -> Result<(), String> {
    use tauri_plugin_updater::{Error as UpdaterError, UpdaterExt};

    // Once an update has been found, later checks run quietly: the popup
    // keeps offering it and only switches when a newer one appears.
    let pending = state.pending_version();
    let quiet = pending.is_some();
    if !quiet {
        state.set_status(app, UpdateStatus::Checking);
    }
    let update = match app.updater().map_err(|e| e.to_string())?.check().await {
        Ok(update) => update,
        // The latest release has no build for this platform (e.g. a
        // Windows-only release): nothing to install here yet.
        Err(UpdaterError::TargetNotFound(_) | UpdaterError::TargetsNotFound(_)) => None,
        Err(e) => return Err(e.to_string()),
    };
    let Some(update) = update else {
        if !quiet {
            state.set_status(app, UpdateStatus::UpToDate);
        }
        return Ok(());
    };
    if let Some(pending) = &pending {
        if !is_newer(&update.version, pending) {
            return Ok(());
        }
    }
    let version = clean_version(&update.version);
    if let Ok(mut slot) = state.pending.lock() {
        *slot = Some(update);
    }
    state.set_status(app, UpdateStatus::Available { version });
    Ok(())
}

#[cfg(not(desktop))]
async fn run_check<R: Runtime>(_app: &AppHandle<R>, _state: &Updater) -> Result<(), String> {
    Err("updates are not supported on this platform".into())
}

/// Download the update the user asked for, install it and restart. On
/// Windows the installer exits the app itself. On failure the update stays
/// on offer so the user can retry.
#[cfg(desktop)]
pub async fn download_and_install<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = app.state::<Updater>();
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("An update check is running; try again in a moment.".into());
    }
    let result = run_install(app, &state).await;
    state.busy.store(false, Ordering::SeqCst);
    result
}

#[cfg(desktop)]
async fn run_install<R: Runtime>(app: &AppHandle<R>, state: &Updater) -> Result<(), String> {
    let update = state.pending.lock().ok().and_then(|p| p.clone());
    let Some(update) = update else {
        return Err("No update is available".into());
    };
    let version = clean_version(&update.version);
    let offer_again = |message: String| {
        state.set_status(
            app,
            UpdateStatus::Available {
                version: version.clone(),
            },
        );
        message
    };

    state.set_status(
        app,
        UpdateStatus::Downloading {
            version: version.clone(),
            percent: Some(0),
        },
    );
    let mut received: u64 = 0;
    let mut last_percent: Option<u8> = Some(0);
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let percent = total
                    .filter(|t| *t > 0)
                    .map(|t| ((received * 100) / t).min(100) as u8);
                if percent != last_percent {
                    last_percent = percent;
                    state.set_status(
                        app,
                        UpdateStatus::Downloading {
                            version: version.clone(),
                            percent,
                        },
                    );
                }
            },
            || {},
        )
        .await
        .map_err(|e| offer_again(e.to_string()))?;

    state.set_status(
        app,
        UpdateStatus::Installing {
            version: version.clone(),
        },
    );
    // Installing can copy a large bundle or wait on a password prompt (Linux
    // packages), so keep it off the async workers.
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| offer_again(e.to_string()))?
        .map_err(|e| offer_again(e.to_string()))?;
    app.restart();
}

#[cfg(not(desktop))]
pub async fn download_and_install<R: Runtime>(_app: &AppHandle<R>) -> Result<(), String> {
    Err("updates are not supported on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::clean_version;

    #[test]
    fn newer_versions_are_detected() {
        use super::is_newer;
        assert!(is_newer("0.3.0", "0.2.9"));
        assert!(is_newer("1.0.0", "1.0.0-beta.2"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.0"));
    }

    #[test]
    fn version_is_sanitized() {
        assert_eq!(clean_version("1.2.3-beta.1+x"), "1.2.3-beta.1+x");
        assert_eq!(clean_version("1.0\");alert(1);//"), "1.0alert1");
    }
}
