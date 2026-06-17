use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, Runtime, WebviewWindow,
};

const RESIZE_STEPS: u32 = 8;
const RESIZE_MS: u64 = 90;
const EDGE_MARGIN: f64 = 8.0;
const MIN_POPUP_HEIGHT: f64 = 120.0;

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
            "open_widget" => toggle_popup(app),
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
                toggle_popup_at(tray.app_handle(), position.x, position.y);
            }
        })
        .build(app)?;

    Ok(())
}

pub fn toggle_popup_at<R: Runtime>(app: &AppHandle<R>, x: f64, y: f64) {
    if let Some(win) = app.get_webview_window("tray-popup") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            position_near(&win, x, y);
            let _ = win.show();
            let _ = win.set_focus();
            strip_dwm_border(&win);
        }
    }
}

pub fn toggle_popup<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("tray-popup") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
            strip_dwm_border(&win);
        }
    }
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
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn monitor_at_point<R: Runtime>(win: &WebviewWindow<R>, x: f64, y: f64) -> Option<Monitor> {
    win.available_monitors().ok()?.into_iter().find(|m| {
        let p = m.position();
        let s = m.size();
        let (px, py) = (p.x as f64, p.y as f64);
        x >= px && x < px + s.width as f64 && y >= py && y < py + s.height as f64
    })
}

fn position_near<R: Runtime>(win: &WebviewWindow<R>, x: f64, y: f64) {
    let size = match win.outer_size() {
        Ok(s) => s,
        Err(_) => return,
    };
    let (pw, ph) = (size.width as f64, size.height as f64);

    let mut nx = x - pw + 20.0;
    let mut ny = y - ph - 12.0;

    if let Some(m) = monitor_at_point(win, x, y) {
        let mp = m.position();
        let ms = m.size();
        let (min_x, min_y) = (mp.x as f64, mp.y as f64);
        let max_x = min_x + ms.width as f64;
        let max_y = min_y + ms.height as f64;
        nx = nx.clamp(min_x + EDGE_MARGIN, (max_x - pw - EDGE_MARGIN).max(min_x + EDGE_MARGIN));
        ny = ny.clamp(min_y + EDGE_MARGIN, (max_y - ph - EDGE_MARGIN).max(min_y + EDGE_MARGIN));
    }

    let _ = win.set_position(PhysicalPosition::new(nx, ny));
}

pub fn clamp_popup_height<R: Runtime>(win: &WebviewWindow<R>, height: f64) -> f64 {
    let max = win
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| (m.size().height as f64 / m.scale_factor()) - 80.0)
        .unwrap_or(900.0);
    height.min(max).max(MIN_POPUP_HEIGHT)
}

pub fn animate_resize<R: Runtime>(win: WebviewWindow<R>, target_h: f64) {
    let scale = win.scale_factor().unwrap_or(1.0);
    let (width, start_h) = match win.inner_size() {
        Ok(s) => (s.width as f64 / scale, s.height as f64 / scale),
        Err(_) => return,
    };

    std::thread::spawn(move || {
        for i in 1..=RESIZE_STEPS {
            let t = i as f64 / RESIZE_STEPS as f64;
            let eased = 1.0 - (1.0 - t).powi(3);
            let h = start_h + (target_h - start_h) * eased;
            let _ = win.set_size(LogicalSize::new(width, h));
            std::thread::sleep(std::time::Duration::from_millis(RESIZE_MS / RESIZE_STEPS as u64));
        }
        let _ = win.set_size(LogicalSize::new(width, target_h));
    });
}
