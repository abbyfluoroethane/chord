//! How the app behaves around its window: start at login, the close button, the tray icon,
//! and the window size and position.
//!
//! The switches live in the `prefs` object of the settings file (see `settings.rs`). Rust
//! reads them at start, and again at each save, so a change takes effect at once. The
//! login entry is not a pref. The OS keeps it, and the page asks the OS.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, Window, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

use crate::error::{ChordError, Res};

/// The argument that the login entry passes. A start with it can open hidden.
pub const AUTOSTART_ARG: &str = "--autostart";

/// The label of the window in `tauri.conf.json`.
const MAIN: &str = "main";

/// The file for the window geometry, in the app config directory.
const WINDOW_FILE: &str = "window.json";

/// The event that the tray status items send to the page. The payload is `chat`, `away`
/// or `dnd`.
pub const TRAY_STATUS_EVENT: &str = "tray-status";

/// The switches of this page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Switches {
    /// The close button hides the window and Chord keeps running.
    pub close_to_background: bool,
    /// Show the tray icon (the menu bar icon on macOS).
    pub tray: bool,
    /// Restore the size and position of the window at start.
    pub remember_window: bool,
    /// A start from the login entry hides the window.
    pub start_minimised: bool,
}

impl Switches {
    /// Read the switches from the whole settings object. A missing switch is off.
    pub fn from_settings(settings: &Value) -> Self {
        let flag = |key: &str| {
            settings
                .get("prefs")
                .and_then(|p| p.get(key))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        Self {
            close_to_background: flag("closeToBackground"),
            tray: flag("trayIcon"),
            remember_window: flag("rememberWindow"),
            start_minimised: flag("startMinimised"),
        }
    }

    /// Does the app show the tray icon? On Windows and Linux a window that hides needs a
    /// tray icon to come back, so the icon shows then even when its own switch is off.
    pub fn wants_tray(&self) -> bool {
        self.tray || (self.close_to_background && !cfg!(target_os = "macos"))
    }
}

/// The state that Rust keeps for this page.
#[derive(Default)]
pub struct Behaviour {
    switches: Mutex<Switches>,
    tray: Mutex<Option<TrayIcon>>,
    unread: Mutex<u32>,
    /// The last size and position of the window while it was not maximised.
    window: Mutex<Option<WindowState>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// --- the window geometry -------------------------------------------------------------

/// Where the window was. All values are physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// A screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The smallest size that a saved window can have. The window has its own minimum too.
const MIN_SIZE: u32 = 400;

/// Is the title bar of the saved window on one of the screens? A window on a screen that
/// the user unplugged would be out of reach.
pub fn is_reachable(state: &WindowState, screens: &[Screen]) -> bool {
    let px = i64::from(state.x) + i64::from(state.width / 2);
    let py = i64::from(state.y) + 20;
    screens.iter().any(|s| {
        px >= i64::from(s.x)
            && px < i64::from(s.x) + i64::from(s.width)
            && py >= i64::from(s.y)
            && py < i64::from(s.y) + i64::from(s.height)
    })
}

/// The state to restore, or `None` when the saved size is not usable.
pub fn usable(state: WindowState) -> Option<WindowState> {
    (state.width >= MIN_SIZE && state.height >= MIN_SIZE).then_some(state)
}

fn read_window_state(dir: &Path) -> Option<WindowState> {
    let bytes = std::fs::read(dir.join(WINDOW_FILE)).ok()?;
    usable(serde_json::from_slice(&bytes).ok()?)
}

fn write_window_state(dir: &Path, state: &WindowState) {
    let Ok(bytes) = serde_json::to_vec(state) else {
        return;
    };
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join(format!("{WINDOW_FILE}.tmp"));
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(WINDOW_FILE));
    }
}

fn config_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok()
}

/// Remember the geometry of the window in memory. A maximised window keeps the last normal
/// size and only sets the flag. A minimised or full screen window changes nothing.
fn capture(app: &AppHandle, window: &Window) {
    if window.is_minimized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let behaviour = app.state::<Behaviour>();
    let mut slot = lock(&behaviour.window);
    let old = *slot;
    *slot = Some(if maximized {
        WindowState {
            maximized: true,
            ..old.unwrap_or(WindowState {
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
                maximized: true,
            })
        }
    } else {
        WindowState {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized: false,
        }
    });
}

/// Write the remembered geometry to the file, when the switch is on.
fn flush_window(app: &AppHandle) {
    let behaviour = app.state::<Behaviour>();
    if !lock(&behaviour.switches).remember_window {
        return;
    }
    let state = *lock(&behaviour.window);
    if let (Some(state), Some(dir)) = (state, config_dir(app)) {
        write_window_state(&dir, &state);
    }
}

fn restore_window(app: &AppHandle, window: &tauri::WebviewWindow) {
    let Some(state) = config_dir(app).and_then(|d| read_window_state(&d)) else {
        return;
    };
    let _ = window.set_size(PhysicalSize::new(state.width, state.height));
    let screens: Vec<Screen> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Screen {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
        })
        .collect();
    if is_reachable(&state, &screens) {
        let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
    }
    if state.maximized {
        let _ = window.maximize();
    }
    *lock(&app.state::<Behaviour>().window) = Some(state);
}

// --- the tray icon -------------------------------------------------------------------

/// Draw a red dot in the top right corner of RGBA pixels. The dot has a dark ring so it
/// shows on a light and on a dark menu bar.
pub fn with_dot(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let radius = f64::from(width.min(height)) * 0.27;
    let cx = f64::from(width) - radius - 0.5;
    let cy = radius + 0.5;
    for y in 0..height {
        for x in 0..width {
            let d = ((f64::from(x) + 0.5 - cx).powi(2) + (f64::from(y) + 0.5 - cy).powi(2)).sqrt();
            let color = if d <= radius * 0.72 {
                [0xED, 0x42, 0x45, 0xFF]
            } else if d <= radius {
                [0x1E, 0x1F, 0x22, 0xFF]
            } else {
                continue;
            };
            let at = ((y * width + x) * 4) as usize;
            if let Some(px) = out.get_mut(at..at + 4) {
                px.copy_from_slice(&color);
            }
        }
    }
    out
}

fn tooltip(unread: u32) -> String {
    if unread == 0 {
        "Chord".to_string()
    } else {
        format!("Chord, {unread} unread")
    }
}

/// The icon for the tray: the app icon, with a dot when there are unread messages.
fn tray_image(app: &AppHandle, unread: bool) -> Option<Image<'static>> {
    let base = app.default_window_icon()?;
    let (w, h) = (base.width(), base.height());
    let rgba = if unread {
        with_dot(base.rgba(), w, h)
    } else {
        base.rgba().to_vec()
    };
    Some(Image::new_owned(rgba, w, h))
}

/// Show and focus the window.
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &AppHandle, unread: u32) -> tauri::Result<TrayIcon> {
    let open = MenuItem::with_id(app, "tray-open", "Open Chord", true, None::<&str>)?;
    let online = MenuItem::with_id(app, "tray-online", "Online", true, None::<&str>)?;
    let away = MenuItem::with_id(app, "tray-away", "Away", true, None::<&str>)?;
    let dnd = MenuItem::with_id(app, "tray-dnd", "Do not disturb", true, None::<&str>)?;
    let status = Submenu::with_items(app, "Status", true, &[&online, &away, &dnd])?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit Chord", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &status, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip(tooltip(unread))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-open" => show_main(app),
            "tray-quit" => app.exit(0),
            "tray-online" => {
                let _ = app.emit(TRAY_STATUS_EVENT, "chat");
            }
            "tray-away" => {
                let _ = app.emit(TRAY_STATUS_EVENT, "away");
            }
            "tray-dnd" => {
                let _ = app.emit(TRAY_STATUS_EVENT, "dnd");
            }
            _ => {}
        });
    // On macOS a click opens the menu. Elsewhere a left click opens the window, and the
    // menu stays on the right click.
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
        builder = builder
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main(tray.app_handle());
                }
            });
    }
    if let Some(image) = tray_image(app, unread > 0) {
        builder = builder.icon(image);
    }
    builder.build(app)
}

/// Make the tray match the switches and the unread count. Call it on the main thread.
fn sync_tray(app: &AppHandle) {
    let behaviour = app.state::<Behaviour>();
    let want = lock(&behaviour.switches).wants_tray();
    let unread = *lock(&behaviour.unread);
    let mut slot = lock(&behaviour.tray);
    match (want, slot.is_some()) {
        (true, false) => match build_tray(app, unread) {
            Ok(tray) => *slot = Some(tray),
            Err(e) => log::warn!("cannot build the tray icon: {e}"),
        },
        (false, true) => {
            *slot = None;
            let _ = app.remove_tray_by_id("main");
        }
        (true, true) => {
            if let Some(tray) = slot.as_ref() {
                let _ = tray.set_tooltip(Some(tooltip(unread)));
                if let Some(image) = tray_image(app, unread > 0) {
                    let _ = tray.set_icon(Some(image));
                }
            }
        }
        (false, false) => {}
    }
}

fn sync_on_main(app: &AppHandle) {
    let handle = app.clone();
    if app.run_on_main_thread(move || sync_tray(&handle)).is_err() {
        log::warn!("cannot reach the main thread for the tray icon");
    }
}

// --- the hooks -----------------------------------------------------------------------

/// A save of the settings: read the switches and apply them.
pub fn apply(app: &AppHandle, settings: &Value) {
    let new = Switches::from_settings(settings);
    let behaviour = app.state::<Behaviour>();
    let old = std::mem::replace(&mut *lock(&behaviour.switches), new);
    if old.wants_tray() != new.wants_tray() {
        sync_on_main(app);
    }
    if new.remember_window
        && !old.remember_window
        && let Some(window) = app.get_webview_window(MAIN)
    {
        capture(app, &window.as_ref().window());
    }
}

/// At start: read the switches, restore the window, and open it. The window starts hidden
/// (`visible: false` in `tauri.conf.json`), so it never jumps from the default place.
pub fn setup(app: &AppHandle) {
    let settings = config_dir(app)
        .map(|d| crate::settings::read_from(&d))
        .unwrap_or(Value::Null);
    let switches = Switches::from_settings(&settings);
    *lock(&app.state::<Behaviour>().switches) = switches;
    if switches.wants_tray() {
        sync_tray(app);
    }
    let Some(window) = app.get_webview_window(MAIN) else {
        return;
    };
    if switches.remember_window {
        restore_window(app, &window);
    }
    let from_login = std::env::args().any(|a| a == AUTOSTART_ARG);
    if switches.start_minimised && from_login {
        // The tray icon or the Dock icon brings the window back. With neither, the window
        // goes to the taskbar.
        if cfg!(target_os = "macos") || switches.wants_tray() {
            return;
        }
        let _ = window.show();
        let _ = window.minimize();
    } else {
        let _ = window.show();
    }
}

/// The close button, and the moves and resizes that the window state needs.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    let app = window.app_handle();
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => capture(app, window),
        WindowEvent::CloseRequested { api, .. } => {
            capture(app, window);
            flush_window(app);
            let behaviour = app.state::<Behaviour>();
            let hides = lock(&behaviour.switches).close_to_background
                && (cfg!(target_os = "macos") || lock(&behaviour.tray).is_some());
            if hides {
                api.prevent_close();
                let _ = window.hide();
            }
        }
        _ => {}
    }
}

/// The run loop events: a click on the Dock icon, and the exit.
pub fn on_run_event(app: &AppHandle, event: &RunEvent) {
    match event {
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } => show_main(app),
        RunEvent::ExitRequested { .. } => flush_window(app),
        _ => {}
    }
}

// --- the commands --------------------------------------------------------------------

/// Does the OS start Chord at login?
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Res<bool> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| ChordError::new("autostart", e.to_string()))
}

/// Turn the login entry on or off.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Res<()> {
    let manager = app.autolaunch();
    let done = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    done.map_err(|e| ChordError::new("autostart", e.to_string()))
}

/// The unread total, for the dot and the tooltip of the tray icon.
#[tauri::command]
pub fn set_tray_unread(app: AppHandle, count: u32) {
    let behaviour = app.state::<Behaviour>();
    let changed = std::mem::replace(&mut *lock(&behaviour.unread), count) != count;
    if changed && lock(&behaviour.tray).is_some() {
        sync_on_main(&app);
    }
}

/// Seconds since the last key or mouse input in the whole session. `None` where Chord has
/// no way to ask. Then the page watches its own window.
#[tauri::command]
pub fn system_idle_seconds() -> Option<u64> {
    system_idle()
}

#[cfg(target_os = "macos")]
fn system_idle() -> Option<u64> {
    // CoreGraphics is part of every macOS and Tauri links it already.
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }
    // SAFETY: the function takes two numbers and returns a number. State 0 is the combined
    // session state. Event type `u32::MAX` means any input event.
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds as u64)
}

#[cfg(not(target_os = "macos"))]
fn system_idle() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_switch_is_off_without_a_file() {
        assert_eq!(Switches::from_settings(&json!({})), Switches::default());
        assert_eq!(Switches::from_settings(&Value::Null), Switches::default());
    }

    #[test]
    fn switches_come_from_the_prefs_object() {
        let s = Switches::from_settings(&json!({"prefs": {
            "closeToBackground": true, "trayIcon": true, "rememberWindow": true,
            "startMinimised": true
        }}));
        assert!(s.close_to_background && s.tray && s.remember_window && s.start_minimised);
        let s = Switches::from_settings(&json!({"prefs": {"trayIcon": "yes"}}));
        assert!(!s.tray, "a value that is not a boolean counts as off");
    }

    #[test]
    fn a_hidden_window_off_macos_forces_the_tray() {
        let s = Switches {
            close_to_background: true,
            ..Switches::default()
        };
        assert_eq!(s.wants_tray(), !cfg!(target_os = "macos"));
        assert!(
            Switches {
                tray: true,
                ..Switches::default()
            }
            .wants_tray()
        );
        assert!(!Switches::default().wants_tray());
    }

    fn state(x: i32, y: i32) -> WindowState {
        WindowState {
            x,
            y,
            width: 1280,
            height: 800,
            maximized: false,
        }
    }

    fn screen(x: i32, y: i32) -> Screen {
        Screen {
            x,
            y,
            width: 1920,
            height: 1080,
        }
    }

    #[test]
    fn a_window_on_a_screen_is_reachable() {
        assert!(is_reachable(&state(100, 100), &[screen(0, 0)]));
        assert!(is_reachable(
            &state(-1900, 50),
            &[screen(-1920, 0), screen(0, 0)]
        ));
    }

    #[test]
    fn a_window_on_a_missing_screen_is_not_reachable() {
        assert!(!is_reachable(&state(-1900, 50), &[screen(0, 0)]));
        assert!(!is_reachable(&state(100, 5000), &[screen(0, 0)]));
        assert!(!is_reachable(&state(100, 100), &[]));
    }

    #[test]
    fn a_tiny_saved_size_is_not_used() {
        assert!(usable(state(0, 0)).is_some());
        let tiny = WindowState {
            width: 10,
            ..state(0, 0)
        };
        assert!(usable(tiny).is_none());
    }

    #[test]
    fn the_window_state_survives_the_file() {
        let dir = std::env::temp_dir().join(format!("chord-window-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read_window_state(&dir), None);
        let saved = WindowState {
            maximized: true,
            ..state(-20, 30)
        };
        write_window_state(&dir, &saved);
        assert_eq!(read_window_state(&dir), Some(saved));
        std::fs::write(dir.join(WINDOW_FILE), b"{not json").unwrap();
        assert_eq!(read_window_state(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dot_sits_in_the_top_right_corner_only() {
        let base = vec![0u8; 32 * 32 * 4];
        let out = with_dot(&base, 32, 32);
        assert_eq!(out.len(), base.len());
        let at = |x: usize, y: usize| &out[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(at(26, 6), &[0xED, 0x42, 0x45, 0xFF], "the centre is red");
        assert_eq!(
            at(2, 29),
            &[0, 0, 0, 0],
            "the opposite corner stays as it was"
        );
    }

    #[test]
    fn the_tooltip_shows_the_count() {
        assert_eq!(tooltip(0), "Chord");
        assert_eq!(tooltip(4), "Chord, 4 unread");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reports_the_idle_time() {
        assert!(system_idle().is_some());
    }
}
