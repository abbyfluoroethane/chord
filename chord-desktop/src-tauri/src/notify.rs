//! The event pump and the desktop notifications.
//!
//! The pump reads `ClientEvents` and sends each event to the UI through the sink. For a
//! `ClientEvent::Notification` it also shows a system notification when no window has
//! the focus. The core already applied the notification level and the mute time, so
//! the pump adds no rule of its own. The UI can show its own in-app alert as well: it
//! sees the same event.
//!
//! Four settings of the user narrow or change the system notice: the desktop switch, "mute
//! DMs", the quiet hours, and the message text. Rust reads them from the `prefs` object of the saved settings: at start from the file,
//! and on each `set_settings` from the new value. The UI sends no separate call. The core
//! has no global default level, so the per-chat levels are the only level rules.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use chord_core::actor::{ClientEvent, ClientEvents};
use chord_core::features::notify::Notification;
use chord_core::session::Stream;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::error::{ChordError, Res};
use crate::state::{EventSink, lock};

/// What the user allows for the system notice. Rust reads it from the saved settings.
pub struct NoticePrefs {
    desktop: AtomicBool,
    mute_dms: AtomicBool,
    /// The notice shows the text of the message. Off: it shows only the sender.
    preview: AtomicBool,
    quiet: AtomicBool,
    /// The quiet window, in minutes after midnight, local time.
    quiet_from: AtomicU32,
    quiet_to: AtomicU32,
}

impl Default for NoticePrefs {
    fn default() -> Self {
        Self {
            desktop: AtomicBool::new(true),
            mute_dms: AtomicBool::new(false),
            preview: AtomicBool::new(true),
            quiet: AtomicBool::new(false),
            quiet_from: AtomicU32::new(22 * 60),
            quiet_to: AtomicU32::new(8 * 60),
        }
    }
}

/// True if `now` is inside the window from `from` to `to`. All three are minutes after
/// midnight. The window can cross midnight. Equal times give an empty window.
fn in_quiet_hours(from: u32, to: u32, now: u32) -> bool {
    match from.cmp(&to) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => now >= from && now < to,
        std::cmp::Ordering::Greater => now >= from || now < to,
    }
}

/// The local time as minutes after midnight.
fn local_minutes() -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    now.hour() * 60 + now.minute()
}

impl NoticePrefs {
    /// Take the switches from the `prefs` object of the settings. A value that is not there
    /// (or has the wrong type) goes back to its default.
    pub fn apply(&self, settings: &Value) {
        let get = |key: &str| settings.get("prefs").and_then(|p| p.get(key));
        let flag = |key: &str, default: bool| get(key).and_then(Value::as_bool).unwrap_or(default);
        let minute = |key: &str, default: u32| {
            get(key)
                .and_then(Value::as_u64)
                .filter(|m| *m < 1440)
                .map_or(default, |m| m as u32)
        };
        self.desktop
            .store(flag("desktopNotifications", true), Ordering::Relaxed);
        self.mute_dms
            .store(flag("muteDms", false), Ordering::Relaxed);
        self.preview
            .store(flag("noticePreview", true), Ordering::Relaxed);
        self.quiet
            .store(flag("quietHours", false), Ordering::Relaxed);
        self.quiet_from
            .store(minute("quietFrom", 22 * 60), Ordering::Relaxed);
        self.quiet_to
            .store(minute("quietTo", 8 * 60), Ordering::Relaxed);
    }

    /// True if the settings allow a system notice for `n` at `now` (minutes after midnight).
    pub(crate) fn allows(&self, n: &Notification, now: u32) -> bool {
        self.desktop.load(Ordering::Relaxed)
            && !(n.room.is_none() && self.mute_dms.load(Ordering::Relaxed))
            && !(self.quiet.load(Ordering::Relaxed)
                && in_quiet_hours(
                    self.quiet_from.load(Ordering::Relaxed),
                    self.quiet_to.load(Ordering::Relaxed),
                    now,
                ))
    }

    /// True if the notice shows the text of the message.
    pub(crate) fn shows_text(&self) -> bool {
        self.preview.load(Ordering::Relaxed)
    }
}

/// The body of a notice that hides the text of the message.
const HIDDEN_BODY: &str = "New message";

/// The title and the body of the system notification. With `preview` off, the body does
/// not carry the text of the message.
pub fn text(n: &Notification, preview: bool) -> (String, String) {
    let title = match &n.room {
        Some(room) => format!(
            "{} in {}",
            n.sender_name,
            room.node().map_or("", |n| n.as_str())
        ),
        None => n.sender_name.clone(),
    };
    let body = if preview {
        action_or_body(&n.sender_name, &n.body_preview)
    } else {
        HIDDEN_BODY.to_owned()
    };
    (title, body)
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
    let prefs = app.state::<NoticePrefs>();
    if !prefs.allows(n, local_minutes()) || window_has_focus(app) {
        return;
    }
    let (title, body) = text(n, prefs.shows_text());
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("cannot show a notification: {e}");
    }
}

/// Show a system notice at once, to let the user check that notices work. It ignores the
/// switches, the quiet hours and the focus, but it follows the message text setting.
#[tauri::command]
pub fn send_test_notice(app: AppHandle) -> Res<()> {
    let body = if app.state::<NoticePrefs>().shows_text() {
        "This is a test notice."
    } else {
        HIDDEN_BODY
    };
    app.notification()
        .builder()
        .title("Chord")
        .body(body)
        .show()
        .map_err(|e| ChordError::new("notice", format!("cannot show a notification: {e}")))
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
        assert!(prefs.allows(&chat, 600) && prefs.allows(&room, 600));
        prefs.mute_dms.store(true, Ordering::Relaxed);
        assert!(!prefs.allows(&chat, 600));
        assert!(prefs.allows(&room, 600));
        prefs.desktop.store(false, Ordering::Relaxed);
        assert!(!prefs.allows(&room, 600));
    }

    #[test]
    fn the_saved_settings_set_the_switches() {
        use serde_json::json;
        let prefs = NoticePrefs::default();
        prefs.apply(&json!({"prefs": {"desktopNotifications": false, "muteDms": true}}));
        assert!(!prefs.allows(&notification(Some("dev@muc.example.org")), 600));
        prefs.apply(&json!({"prefs": {"muteDms": true}}));
        assert!(prefs.allows(&notification(Some("dev@muc.example.org")), 600));
        assert!(!prefs.allows(&notification(None), 600));
        // No prefs, a wrong type: the defaults.
        prefs.apply(&json!({"prefs": {"desktopNotifications": "no", "muteDms": 1}}));
        assert!(prefs.allows(&notification(None), 600));
        prefs.apply(&json!({}));
        assert!(prefs.allows(&notification(None), 600));
    }

    #[test]
    fn a_chat_notification_shows_the_sender() {
        assert_eq!(
            text(&notification(None), true),
            ("bob".into(), "hello".into())
        );
    }

    #[test]
    fn a_me_body_reads_as_an_action() {
        let mut n = notification(None);
        n.body_preview = "/me waves".into();
        assert_eq!(text(&n, true).1, "* bob waves");
        n.body_preview = "/me".into();
        assert_eq!(text(&n, true).1, "/me");
        n.body_preview = "say /me waves".into();
        assert_eq!(text(&n, true).1, "say /me waves");
    }

    #[test]
    fn a_room_notification_names_the_room() {
        let (title, _) = text(&notification(Some("dev@muc.example.org")), true);
        assert_eq!(title, "bob in dev");
    }

    #[test]
    fn a_notice_can_hide_the_text() {
        let room = notification(Some("dev@muc.example.org"));
        assert_eq!(
            text(&room, false),
            ("bob in dev".into(), "New message".into())
        );
        let mut n = notification(None);
        n.body_preview = "/me waves".into();
        assert_eq!(text(&n, false).1, "New message");
    }

    #[test]
    fn the_quiet_window_can_cross_midnight() {
        assert!(in_quiet_hours(60, 120, 60));
        assert!(!in_quiet_hours(60, 120, 120));
        assert!(in_quiet_hours(1320, 480, 1400));
        assert!(in_quiet_hours(1320, 480, 100));
        assert!(!in_quiet_hours(1320, 480, 480));
        assert!(!in_quiet_hours(300, 300, 300));
    }

    #[test]
    fn the_quiet_hours_stop_the_notice() {
        use serde_json::json;
        let prefs = NoticePrefs::default();
        let chat = notification(None);
        prefs.apply(&json!({"prefs": {"quietHours": true, "quietFrom": 1320, "quietTo": 480}}));
        assert!(!prefs.allows(&chat, 1400));
        assert!(prefs.allows(&chat, 600));
        prefs.apply(&json!({"prefs": {"quietHours": false}}));
        assert!(prefs.allows(&chat, 1400));
        // A bad time goes back to the default window, 22:00 to 08:00.
        prefs.apply(&json!({"prefs": {"quietHours": true, "quietFrom": 5000, "quietTo": "x"}}));
        assert!(!prefs.allows(&chat, 1400));
        assert!(prefs.allows(&chat, 600));
    }

    #[test]
    fn the_text_switch_comes_from_the_settings() {
        use serde_json::json;
        let prefs = NoticePrefs::default();
        assert!(prefs.shows_text());
        prefs.apply(&json!({"prefs": {"noticePreview": false}}));
        assert!(!prefs.shows_text());
        prefs.apply(&json!({"prefs": {"noticePreview": "no"}}));
        assert!(prefs.shows_text());
    }
}
