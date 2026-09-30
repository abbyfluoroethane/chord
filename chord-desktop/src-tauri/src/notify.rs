//! The event pump and the desktop notifications.
//!
//! The pump reads `ClientEvents` and sends each event to the UI through the sink. For a
//! `ClientEvent::Notification` it also shows a system notification when no window has
//! the focus. The core already applied the notification level and the mute time, so
//! the pump adds no rule of its own. The UI can show its own in-app alert as well: it
//! sees the same event.
//!
//! Two settings of the user narrow the system notice: the desktop switch, and "mute DMs".
//! The UI sends them with `set_notice_prefs`. The core has no global default level, so the
//! per-chat levels are the only level rules.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chord_core::actor::{ClientEvent, ClientEvents};
use chord_core::features::notify::Notification;
use chord_core::session::Stream;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_notification::NotificationExt;

use crate::state::{EventSink, lock};

/// What the user allows for the system notice. The UI sets it at start and on each change.
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
    /// True if the settings allow a system notice for `n`.
    fn allows(&self, n: &Notification) -> bool {
        self.desktop.load(Ordering::Relaxed)
            && !(n.room.is_none() && self.mute_dms.load(Ordering::Relaxed))
    }
}

/// Set what the system notice may show: `desktop` is the master switch, `mute_dms` silences
/// the notices of chats. A private message from a room member counts as a chat.
#[tauri::command]
pub fn set_notice_prefs(prefs: State<'_, NoticePrefs>, desktop: bool, mute_dms: bool) {
    prefs.desktop.store(desktop, Ordering::Relaxed);
    prefs.mute_dms.store(mute_dms, Ordering::Relaxed);
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
    (title, n.body_preview.clone())
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
    fn a_chat_notification_shows_the_sender() {
        assert_eq!(text(&notification(None)), ("bob".into(), "hello".into()));
    }

    #[test]
    fn a_room_notification_names_the_room() {
        let (title, _) = text(&notification(Some("dev@muc.example.org")));
        assert_eq!(title, "bob in dev");
    }
}
