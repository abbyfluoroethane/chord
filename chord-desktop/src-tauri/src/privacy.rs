//! The local caches that the Privacy page can show and clear: the link preview answers
//! (in memory) and the emoji that Chord drew to PNG (files in the emoji folder).

use std::path::Path;

use serde::Serialize;
use tauri::AppHandle;

use crate::error::{ChordError, Res};

/// What the caches hold now.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    /// The link preview answers that Chord keeps for an hour.
    pub preview_entries: usize,
    /// The drawn emoji files, and their size in bytes.
    pub emoji_files: usize,
    pub emoji_bytes: u64,
}

/// Count the files in the drawn folders of all packs, and add their sizes.
fn drawn_totals(emoji_dir: &Path) -> (usize, u64) {
    let mut files = 0;
    let mut bytes = 0;
    let Ok(packs) = std::fs::read_dir(emoji_dir) else {
        return (0, 0);
    };
    for pack in packs.flatten() {
        let Ok(entries) = std::fs::read_dir(pack.path().join(crate::emoji::DRAWN_DIR)) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Some(meta) = entry.metadata().ok().filter(|m| m.is_file()) {
                files += 1;
                bytes += meta.len();
            }
        }
    }
    (files, bytes)
}

/// Remove the drawn folder of each pack. The next picker draws what it needs again.
fn clear_drawn(emoji_dir: &Path) -> std::io::Result<()> {
    let Ok(packs) = std::fs::read_dir(emoji_dir) else {
        return Ok(());
    };
    for pack in packs.flatten() {
        match std::fs::remove_dir_all(pack.path().join(crate::emoji::DRAWN_DIR)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// The sizes of the caches that the user can clear.
#[tauri::command]
pub fn privacy_cache_info(app: AppHandle) -> Res<CacheInfo> {
    let (emoji_files, emoji_bytes) = drawn_totals(&crate::emoji::packs_dir(&app)?);
    Ok(CacheInfo {
        preview_entries: crate::link_preview::cache_len(),
        emoji_files,
        emoji_bytes,
    })
}

/// Clear one cache: `previews` or `emoji`.
#[tauri::command]
pub fn clear_privacy_cache(app: AppHandle, kind: String) -> Res<()> {
    match kind.as_str() {
        "previews" => crate::link_preview::clear_cache(),
        "emoji" => clear_drawn(&crate::emoji::packs_dir(&app)?)
            .map_err(|e| ChordError::io("clear the drawn emoji", e))?,
        other => return Err(ChordError::invalid(format!("no cache {other:?}"))),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawn_emoji_are_counted_and_cleared() {
        let dir = std::env::temp_dir().join(format!("chord-privacy-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(drawn_totals(&dir), (0, 0));
        let drawn = dir.join("twemoji").join(crate::emoji::DRAWN_DIR);
        std::fs::create_dir_all(&drawn).unwrap();
        std::fs::write(drawn.join("1f600-64.png"), [0u8; 10]).unwrap();
        std::fs::write(drawn.join("1f601-64.png"), [0u8; 5]).unwrap();
        // The SVG files of the pack stay.
        std::fs::write(dir.join("twemoji").join("1f600.svg"), "x").unwrap();
        assert_eq!(drawn_totals(&dir), (2, 15));
        clear_drawn(&dir).unwrap();
        assert_eq!(drawn_totals(&dir), (0, 0));
        assert!(dir.join("twemoji").join("1f600.svg").exists());
        clear_drawn(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
