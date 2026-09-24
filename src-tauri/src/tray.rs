use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, Runtime, WebviewWindow,
};

use crate::player::PlayerHub;

pub const MAIN_LABEL: &str = "main";
pub const POPUP_LABEL: &str = "tray-popup";
/// Popup width is fixed; the height is driven by the popup UI (compact vs queue).
pub const POPUP_WIDTH: f64 = 360.0;
/// Must match `COMPACT_H` in `src/routes/+page.svelte`.
pub const POPUP_COMPACT_HEIGHT: f64 = 282.0;
pub const MIN_POPUP_HEIGHT: f64 = 160.0;

const RESIZE_STEPS: u32 = 8;
const RESIZE_MS: u64 = 90;
const EDGE_MARGIN: f64 = 8.0;
/// A tray click this soon after the popup hid itself on blur is the click that
/// caused the blur; it must not immediately re-open the popup.
const BLUR_REOPEN_GUARD: Duration = Duration::from_millis(350);

static RESIZE_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_BLUR_HIDE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open_widget = MenuItem::with_id(app, "open_widget", "Open widget", true, None::<&str>)?;
    let show_main = MenuItem::with_id(app, "show_main", "Show Flit", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_widget, &show_main, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("default window icon is bundled in tauri.conf.json");

    TrayIconBuilder::with_id("flit-tray")
        .icon(icon)
        .tooltip("Flit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            // Linux never delivers tray click events, so this menu entry is the
            // only way to reach the popup there; position it near the cursor.
            "open_widget" => toggle_popup(app, app.cursor_position().ok()),
            "show_main" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                toggle_popup(tray.app_handle(), Some(position));
            }
        })
        .build(app)?;

    Ok(())
}

fn popup<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window(POPUP_LABEL)
}

/// Show the popup near `anchor` (physical screen coordinates), or hide it if
/// it is already visible. Without an anchor it opens in a screen corner.
pub fn toggle_popup<R: Runtime>(app: &AppHandle<R>, anchor: Option<PhysicalPosition<f64>>) {
    let Some(win) = popup(app) else { return };
    if win.is_visible().unwrap_or(false) {
        hide_popup(app);
        return;
    }
    let recently_blurred = LAST_BLUR_HIDE
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < BLUR_REOPEN_GUARD);
    if recently_blurred {
        return;
    }
    position_popup(&win, anchor);
    let _ = win.show();
    let _ = win.set_focus();
    strip_dwm_border(&win);
    app.state::<PlayerHub>().set_popup_visible(true);
}

pub fn hide_popup<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = popup(app) {
        let _ = win.hide();
    }
    app.state::<PlayerHub>().set_popup_visible(false);
}

/// Called when the popup reports it lost focus. Focus can flicker between the
/// window and its webview, so re-check after a short delay before hiding.
pub fn popup_blurred<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(120));
        let Some(win) = popup(&app) else { return };
        if win.is_visible().unwrap_or(false) && !win.is_focused().unwrap_or(true) {
            if let Ok(mut t) = LAST_BLUR_HIDE.lock() {
                *t = Some(Instant::now());
            }
            hide_popup(&app);
        }
    });
}

#[cfg(windows)]
fn strip_dwm_border<R: Runtime>(win: &WebviewWindow<R>) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;
    if let Ok(hwnd) = win.hwnd() {
        let h = hwnd.0 as _;
        let corner: u32 = DWMWCP_ROUND;
        let color: u32 = DWMWA_COLOR_NONE;
        // SAFETY: `hwnd` is a valid handle owned by the popup window; each value
        // is a u32 whose size matches its attribute.
        unsafe {
            DwmSetWindowAttribute(
                h,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const u32 as *const core::ffi::c_void,
                core::mem::size_of::<u32>() as u32,
            );
            DwmSetWindowAttribute(
                h,
                DWMWA_BORDER_COLOR,
                &color as *const u32 as *const core::ffi::c_void,
                core::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(not(windows))]
fn strip_dwm_border<R: Runtime>(_win: &WebviewWindow<R>) {}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window(MAIN_LABEL) {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Work area (screen minus taskbar/menu bar) of a monitor, in physical pixels:
/// (left, top, right, bottom).
fn work_area(m: &Monitor) -> (f64, f64, f64, f64) {
    let wa = m.work_area();
    let (l, t) = (wa.position.x as f64, wa.position.y as f64);
    (l, t, l + wa.size.width as f64, t + wa.size.height as f64)
}

/// Popup outer size in physical pixels as it would be on `m`.
fn popup_size_on<R: Runtime>(win: &WebviewWindow<R>, m: &Monitor) -> Option<(f64, f64)> {
    let size = win.outer_size().ok()?;
    let ratio = m.scale_factor() / win.scale_factor().unwrap_or(1.0);
    Some((size.width as f64 * ratio, size.height as f64 * ratio))
}

fn clamp_axis(v: f64, min: f64, max: f64, extent: f64) -> f64 {
    let hi = (max - extent - EDGE_MARGIN).max(min + EDGE_MARGIN);
    v.clamp(min + EDGE_MARGIN, hi)
}

fn position_popup<R: Runtime>(win: &WebviewWindow<R>, anchor: Option<PhysicalPosition<f64>>) {
    let monitor = anchor
        .and_then(|p| win.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let Some((pw, ph)) = popup_size_on(win, &m) else {
        return;
    };
    let (left, top, right, bottom) = work_area(&m);

    let (x, y) = match anchor {
        Some(p) => {
            let x = p.x - pw + 20.0;
            // Tray at the bottom (Windows) → open above the click; tray at the
            // top (macOS menu bar, most Linux panels) → open below it.
            let mid = (m.position().y as f64) + (m.size().height as f64) / 2.0;
            let y = if p.y >= mid {
                p.y - ph - 12.0
            } else {
                p.y + 12.0
            };
            (x, y)
        }
        None => (right - pw, bottom - ph),
    };

    let x = clamp_axis(x, left, right, pw);
    let y = clamp_axis(y, top, bottom, ph);
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

pub fn clamp_popup_height<R: Runtime>(win: &WebviewWindow<R>, height: f64) -> f64 {
    let max = win
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| m.work_area().size.height as f64 / m.scale_factor() - 2.0 * EDGE_MARGIN)
        .unwrap_or(900.0);
    let height = if height.is_finite() {
        height
    } else {
        POPUP_COMPACT_HEIGHT
    };
    height.min(max).max(MIN_POPUP_HEIGHT)
}

/// Animate the popup to `target_h` (logical px). The edge nearest the tray
/// stays put: a popup in the lower half of the screen grows upward so it never
/// runs under the taskbar. A newer call cancels any animation in flight.
pub fn animate_resize<R: Runtime>(win: WebviewWindow<R>, target_h: f64) {
    let generation = RESIZE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let scale = win.scale_factor().unwrap_or(1.0);
    let (Ok(inner), Ok(outer), Ok(pos)) =
        (win.inner_size(), win.outer_size(), win.outer_position())
    else {
        return;
    };
    let start_h = inner.height as f64 / scale;
    // Window chrome (zero for this borderless popup, but don't assume it).
    let chrome_h = outer.height as f64 - inner.height as f64;
    let monitor = win.current_monitor().ok().flatten();
    let area = monitor.as_ref().map(work_area);
    let (top_y, bottom_y) = (pos.y as f64, pos.y as f64 + outer.height as f64);
    let grow_up = area.is_some_and(|(_, t, _, b)| (top_y + bottom_y) / 2.0 > (t + b) / 2.0);

    let apply = move |h: f64| {
        let outer_h = h * scale + chrome_h;
        let mut y = if grow_up { bottom_y - outer_h } else { top_y };
        if let Some((_, t, _, b)) = area {
            y = clamp_axis(y, t, b, outer_h);
        }
        let _ = win.set_position(PhysicalPosition::new(pos.x as f64, y));
        let _ = win.set_size(LogicalSize::new(POPUP_WIDTH, h));
    };

    std::thread::spawn(move || {
        for i in 1..=RESIZE_STEPS {
            if RESIZE_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            let t = i as f64 / RESIZE_STEPS as f64;
            let eased = 1.0 - (1.0 - t).powi(3);
            apply(start_h + (target_h - start_h) * eased);
            std::thread::sleep(Duration::from_millis(RESIZE_MS / RESIZE_STEPS as u64));
        }
    });
}
