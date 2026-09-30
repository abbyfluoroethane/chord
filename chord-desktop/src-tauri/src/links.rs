//! The xmpp: link handler. The deep-link plugin gets the link and sends an event to the UI.
//!
//! - macOS: the OS opens the running app. The plugin reads the scheme from `tauri.conf.json`
//!   and writes it into the Info.plist of the app bundle. `tauri dev` has no bundle, so
//!   macOS does not know the scheme there.
//! - Windows and Linux: the OS starts a new copy of the app with the link as an argument.
//!   The single-instance plugin passes the arguments to the first copy. The `deep-link`
//!   feature of that plugin turns them into a link event, so this file only shows the window.
//!   A development build registers the scheme at run time, because it has no installer.
//!
//! The UI decides what a link does. It always asks the user first.

use tauri::{AppHandle, Manager};

/// Show and focus the main window.
fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// A second copy of the app started. The first copy gets its arguments.
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
pub fn second_instance(app: &AppHandle, _args: Vec<String>, _cwd: String) {
    focus(app);
}

/// Register the scheme at run time where the OS has no installer that does it.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        if let Err(e) = app.deep_link().register_all() {
            log::warn!("cannot register the xmpp: scheme: {e}");
        }
    }
    #[cfg(not(any(target_os = "linux", all(debug_assertions, windows))))]
    let _ = app;
    Ok(())
}
