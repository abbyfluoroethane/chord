//! The unread count in the window title and on the dock or taskbar icon.

use tauri::WebviewWindow;

use crate::error::Res;

/// The title of the window for a count: `Chord`, or `(3) Chord`.
fn title_for(count: u32) -> String {
    if count == 0 {
        "Chord".to_string()
    } else {
        format!("({count}) Chord")
    }
}

/// Show the total of unread messages. The UI sends it each time the total changes.
/// The dock badge (macOS) and the taskbar badge (Linux) show the number. Windows has no
/// count badge in Tauri, so there only the title changes.
#[tauri::command]
pub fn set_unread_count(window: WebviewWindow, count: u32) -> Res<()> {
    let _ = window.set_title(&title_for(count));
    // A badge that fails is not worth an error in the UI.
    let badge = (count > 0).then_some(i64::from(count));
    let _ = window.set_badge_count(badge);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::title_for;

    #[test]
    fn title_shows_the_count_only_when_there_is_one() {
        assert_eq!(title_for(0), "Chord");
        assert_eq!(title_for(12), "(12) Chord");
    }
}
