// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Set one variable for the webview, but only if the user did not set it.
#[cfg(target_os = "linux")]
fn default_env(name: &str, value: &str) {
    if std::env::var_os(name).is_none() {
        // SAFETY: This runs at the start of `main`, before any thread starts.
        unsafe { std::env::set_var(name, value) };
    }
}

/// The NVIDIA driver on Wayland can break the WebKitGTK renderer. Newer
/// drivers use explicit sync, and WebKitGTK can send a buffer without an
/// acquire point. The compositor then closes the connection (Error 71).
/// This switch keeps hardware acceleration. The user can still override it.
#[cfg(target_os = "linux")]
fn tune_nvidia_webview() {
    let has_nvidia = std::path::Path::new("/proc/driver/nvidia/version").exists()
        || std::path::Path::new("/sys/module/nvidia").exists();
    if has_nvidia {
        default_env("__NV_DISABLE_EXPLICIT_SYNC", "1");
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    tune_nvidia_webview();
    chord_desktop_lib::run()
}
