//! The event pump and the desktop notifications.
//!
//! The pump reads `ClientEvents` and sends each event to the UI through the sink. For a
//! `ClientEvent::Notification` it also shows a system notification when no window has
//! the focus. The core already applied the notification level and the mute time, so
//! the pump adds no rule of its own. The UI can show its own in-app alert as well: it
//! sees the same event.

use std::sync::{Arc, Mutex};

use chord_core::actor::{ClientEvent, ClientEvents};
use chord_core::features::notify::Notification;
use chord_core::session::Stream;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::state::{EventSink, lock};

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
    if window_has_focus(app) {
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
    fn a_chat_notification_shows_the_sender() {
        assert_eq!(text(&notification(None)), ("bob".into(), "hello".into()));
    }

    #[test]
    fn a_room_notification_names_the_room() {
        let (title, _) = text(&notification(Some("dev@muc.example.org")));
        assert_eq!(title, "bob in dev");
    }
}
