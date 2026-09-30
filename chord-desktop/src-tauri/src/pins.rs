//! The pinned messages. The core keeps them in a private PEP node of the account (XEP-0223),
//! so every device of the user sees the same pins.

use chord_core::features::pins::Pin;
use tauri::State;

use crate::error::Res;
use crate::state::AppState;

/// Pin a message. `item_id` is the id of the message in the timeline.
#[tauri::command]
pub async fn pin_message(state: State<'_, AppState>, item_id: String) -> Res<()> {
    Ok(state.handle()?.pin_message(item_id).await?)
}

/// Remove a pin. `chat` and `key` are the fields of the pin.
#[tauri::command]
pub async fn unpin_message(state: State<'_, AppState>, chat: String, key: String) -> Res<()> {
    Ok(state.handle()?.unpin_message(chat, key).await?)
}

/// The pins of one chat (a bare JID), newest first. From the local copy.
#[tauri::command]
pub async fn pins(state: State<'_, AppState>, chat: Option<String>) -> Res<Vec<Pin>> {
    Ok(state.handle()?.pins(chat).await?)
}

/// Ask the server for the pins again, and replace the local copy.
#[tauri::command]
pub async fn refresh_pins(state: State<'_, AppState>) -> Res<()> {
    Ok(state.handle()?.refresh_pins().await?)
}
