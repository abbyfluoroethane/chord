//! A guard on the main window: it never leaves the app. A link in a message opens in the
//! system browser through the opener plugin. If a bug in the page ever navigates the window
//! itself (for example a `download` link that the webview follows), the page that loads
//! would look like Chord and could ask for the password. So every navigation to a
//! remote address fails here (BRIDGESECURITY-06).

use tauri::Runtime;
use tauri::plugin::{Builder, TauriPlugin};
use url::Url;

/// True if the window may go to `url`: the app itself, its own schemes, and `about:` and
/// `blob:` pages. A `localhost` address is the dev server, so only a debug build allows it.
pub fn allowed(url: &Url) -> bool {
    match url.scheme() {
        "tauri" | "about" | "blob" | "data" | "asset" | "ipc" | "chord-avatar" | "chord-emoji" => {
            true
        }
        // Windows and Android serve the app as http(s)://tauri.localhost.
        "http" | "https" => {
            let host = url.host_str();
            host == Some("tauri.localhost")
                || (cfg!(debug_assertions) && matches!(host, Some("localhost" | "127.0.0.1")))
        }
        _ => false,
    }
}

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("chord-navigation")
        .on_navigation(|_webview, url| {
            let ok = allowed(url);
            if !ok {
                log::warn!(
                    "blocked a navigation of the window to {}",
                    url.host_str().unwrap_or("?")
                );
            }
            ok
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn the_app_and_its_schemes_pass() {
        for u in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
            "https://tauri.localhost/x",
            "about:blank",
            "blob:http://tauri.localhost/1",
            "chord-avatar://localhost/a",
        ] {
            assert!(allowed(&url(u)), "{u}");
        }
    }

    #[test]
    fn a_remote_page_fails() {
        for u in [
            "https://evil.example/login",
            "http://evil.example/",
            "https://tauri.localhost.evil.example/",
            "https://localhost.evil.example/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://x.example/",
        ] {
            assert!(!allowed(&url(u)), "{u}");
        }
    }
}
