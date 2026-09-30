//! The event pump and the desktop notifications.
//!
//! The pump reads `ClientEvents` and sends each event to the UI through the sink. For a
//! `ClientEvent::Notification` it also shows a system notification when no window has
//! the focus. The core already applied the notification level and the mute time, so
//! the pump adds no rule of its own. The UI can show its own in-app alert as well: it
//! sees the same event.
//!
//! Two settings of the user narrow the system notice: the desktop switch, and "mute DMs".
//! Rust reads them from the `prefs` object of the saved settings: at start from the file,
//! and on each `set_settings` from the new value. The UI sends no separate call. The core
//! has no global default level, so the per-chat levels are the only level rules.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chord_core::actor::{ClientEvent, ClientEvents};
use chord_core::features::notify::Notification;
use chord_core::session::Stream;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::state::{EventSink, lock};

/// What the user allows for the system notice. Rust reads it from the saved settings.
pub struct NoticePrefs {
    desktop: AtomicBool,
    mute_dms: AtomicBool,
}

impl Default for NoticePrefs {
    fn default() -> Self {
        Self {
            desktop: AtomicBool::new(true),
            mute_dms: AtomicBool::new(false),
        }
    }
}

impl NoticePrefs {
    /// Take the two switches from the `prefs` object of the settings. A switch that is not
    /// there (or not a boolean) goes back to its default.
    pub fn apply(&self, settings: &Value) {
        let flag = |key: &str, default: bool| {
            settings
                .get("prefs")
                .and_then(|p| p.get(key))
                .and_then(Value::as_bool)
                .unwrap_or(default)
        };
        self.desktop
            .store(flag("desktopNotifications", true), Ordering::Relaxed);
        self.mute_dms
            .store(flag("muteDms", false), Ordering::Relaxed);
    }

    /// True if the settings allow a system notice for `n`.
    pub(crate) fn allows(&self, n: &Notification) -> bool {
        self.desktop.load(Ordering::Relaxed)
            && !(n.room.is_none() && self.mute_dms.load(Ordering::Relaxed))
    }
}

/// The title and the body of the system notification.
pub fn text(n: &Notification) -> (String, String) {
    let title = match &n.room {
        Some(room) => format!(
            "{} in {}",
            n.sender_name,
            room.node().map_or("", |n| n.as_str())
        ),
        None => n.sender_name.clone(),
    };
    (title, action_or_body(&n.sender_name, &n.body_preview))
}

/// XEP-0245: a body that starts with "/me " reads as an action: "* Alice waves".
fn action_or_body(name: &str, body: &str) -> String {
    match body.strip_prefix("/me ").map(str::trim) {
        Some(rest) if !rest.is_empty() => format!("* {name} {rest}"),
        _ => body.to_owned(),
    }
}

fn window_has_focus(app: &AppHandle) -> bool {
    app.webview_windows()
        .values()
        .any(|w| w.is_focused().unwrap_or(false))
}

fn show(app: &AppHandle, n: &Notification) {
    if !app.state::<NoticePrefs>().allows(n) || window_has_focus(app) {
        return;
    }
    let (title, body) = text(n);
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("cannot show a notification: {e}");
    }
}

/// Run until the actor stops. `open` spawns it.
pub async fn pump(app: AppHandle, mut events: ClientEvents, sink: Arc<Mutex<EventSink>>) {
    while let Some(event) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut events).poll_next(cx)).await
    {
        if let ClientEvent::Notification(n) = &event {
            show(&app, n);
        }
        lock(&sink).push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(room: Option<&str>) -> Notification {
        Notification {
            peer: room.unwrap_or("bob@example.org").into(),
            room: room.map(|r| r.parse().unwrap()),
            sender: "bob".into(),
            sender_name: "bob".into(),
            body_preview: "hello".into(),
            mention: false,
            item_id: "m:1".into(),
        }
    }

    #[test]
    fn the_settings_narrow_the_system_notice() {
        let prefs = NoticePrefs::default();
        let chat = notification(None);
        let room = notification(Some("dev@muc.example.org"));
        assert!(prefs.allows(&chat) && prefs.allows(&room));
        prefs.mute_dms.store(true, Ordering::Relaxed);
        assert!(!prefs.allows(&chat));
        assert!(prefs.allows(&room));
        prefs.desktop.store(false, Ordering::Relaxed);
        assert!(!prefs.allows(&room));
    }

    #[test]
    fn the_saved_settings_set_the_switches() {
        use serde_json::json;
        let prefs = NoticePrefs::default();
        prefs.apply(&json!({"prefs": {"desktopNotifications": false, "muteDms": true}}));
        assert!(!prefs.allows(&notification(Some("dev@muc.example.org"))));
        prefs.apply(&json!({"prefs": {"muteDms": true}}));
        assert!(prefs.allows(&notification(Some("dev@muc.example.org"))));
        assert!(!prefs.allows(&notification(None)));
        // No prefs, a wrong type: the defaults.
        prefs.apply(&json!({"prefs": {"desktopNotifications": "no", "muteDms": 1}}));
        assert!(prefs.allows(&notification(None)));
        prefs.apply(&json!({}));
        assert!(prefs.allows(&notification(None)));
    }

    #[test]
    fn a_chat_notification_shows_the_sender() {
        assert_eq!(text(&notification(None)), ("bob".into(), "hello".into()));
    }

    #[test]
    fn a_me_body_reads_as_an_action() {
        let mut n = notification(None);
        n.body_preview = "/me waves".into();
        assert_eq!(text(&n).1, "* bob waves");
        n.body_preview = "/me".into();
        assert_eq!(text(&n).1, "/me");
        n.body_preview = "say /me waves".into();
        assert_eq!(text(&n).1, "say /me waves");
    }

    #[test]
    fn a_room_notification_names_the_room() {
        let (title, _) = text(&notification(Some("dev@muc.example.org")));
        assert_eq!(title, "bob in dev");
    }
}
