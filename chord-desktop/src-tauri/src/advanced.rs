//! The Advanced settings page: the size of the stored data, the caches, the local history,
//! and the facts for the debug text.
//!
//! The commands here stay apart from `commands.rs`. They touch files and the database, not
//! the XMPP session.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::{ChordError, Res};
use crate::state::{AppState, lock};

/// The folder of drawn emoji inside each emoji pack folder (see `emoji.rs`).
const DRAWN_DIR: &str = ".drawn";

/// How much space the data of the open account takes, in bytes.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    /// The database file with its journal files.
    pub database: u64,
    /// The avatar images inside the database. They count in `database` too.
    pub avatars: u64,
    /// The cached PNG files of drawn emoji.
    pub emoji_cache: u64,
}

/// What the debug text needs from the app.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    /// The short hash of the commit that the app was built from.
    pub commit: &'static str,
    /// The build number (see docs/updates.md).
    pub build: i64,
    /// The channel the build came from: `stable`, `beta`, `nightly` or `dev`.
    pub channel: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
}

/// The size of a file, or zero when it does not exist.
fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

/// The size of the database file and its write-ahead log and shared memory files.
fn database_size(path: &Path) -> u64 {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return 0;
    };
    ["", "-wal", "-shm"]
        .iter()
        .map(|suffix| file_size(&path.with_file_name(format!("{name}{suffix}"))))
        .sum()
}

/// The total size of the files below `dir`. A folder that does not exist has size zero.
fn tree_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(t) if t.is_dir() => tree_size(&entry.path()),
            Ok(t) if t.is_file() => entry.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// The `.drawn` folder of each pack folder in `emoji_dir`.
fn drawn_folders(emoji_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(emoji_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path().join(DRAWN_DIR))
        .filter(|p| p.is_dir())
        .collect()
}

/// Remove the drawn emoji. Returns the freed bytes. The next view of an emoji draws it again.
fn clear_drawn(emoji_dir: &Path) -> u64 {
    drawn_folders(emoji_dir)
        .into_iter()
        .map(|folder| {
            let size = tree_size(&folder);
            if std::fs::remove_dir_all(&folder).is_ok() {
                size
            } else {
                0
            }
        })
        .sum()
}

/// The bytes of the avatar images of an account.
fn avatar_bytes(conn: &Connection, account_id: i64) -> u64 {
    conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(data)), 0) FROM avatars WHERE account_id = ?1",
        params![account_id],
        |row| row.get::<_, i64>(0),
    )
    .map_or(0, |n| u64::try_from(n).unwrap_or(0))
}

/// Delete the messages of an account, and what points into them. The sync cursors go too,
/// so that the next login loads the recent history from the server again. Contacts, rooms,
/// spaces, and settings stay. Returns the number of deleted messages.
fn delete_history(conn: &Connection, account_id: i64) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    // Reactions go with their messages (ON DELETE CASCADE).
    let deleted = tx.execute(
        "DELETE FROM messages WHERE account_id = ?1",
        params![account_id],
    )?;
    for table in ["read_state", "pins", "pending_changes", "sync_cursors"] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE account_id = ?1"),
            params![account_id],
        )?;
    }
    tx.commit()?;
    Ok(deleted)
}

fn data_dir(app: &AppHandle) -> Res<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| ChordError::io("no data dir", e))
}

/// The size of the data of the open account.
#[tauri::command]
pub async fn storage_info(app: AppHandle, state: State<'_, AppState>) -> Res<StorageInfo> {
    let client = state.client()?;
    let dir = data_dir(&app)?;
    let account = client.account.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let avatars = {
            let guard = lock(&client.avatars);
            avatar_bytes(guard.0.conn(), guard.1)
        };
        StorageInfo {
            database: database_size(&dir.join(format!("{account}.sqlite3"))),
            avatars,
            emoji_cache: drawn_folders(&dir.join("emoji"))
                .iter()
                .map(|p| tree_size(p))
                .sum(),
        }
    })
    .await
    .map_err(|e| ChordError::io("the size task failed", e))
}

/// Clear the caches that the app fills again by itself: the link previews in memory and the
/// drawn emoji on disk. Avatars stay, because the server sends them only once. Returns the
/// freed bytes of the files.
#[tauri::command]
pub async fn clear_caches(app: AppHandle) -> Res<u64> {
    let dir = data_dir(&app)?.join("emoji");
    crate::link_preview::clear_cache();
    tauri::async_runtime::spawn_blocking(move || clear_drawn(&dir))
        .await
        .map_err(|e| ChordError::io("the clear task failed", e))
}

/// Delete the stored messages of the open account. The page must reload after it.
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Res<usize> {
    let client = state.client()?;
    tauri::async_runtime::spawn_blocking(move || {
        let guard = lock(&client.avatars);
        delete_history(guard.0.conn(), guard.1)
    })
    .await
    .map_err(|e| ChordError::io("the clear task failed", e))?
    .map_err(|e| ChordError::io("cannot clear the history", e))
}

/// The features that the server and the account advertise in disco#info, sorted. The list
/// is empty while the client is offline.
#[tauri::command]
pub async fn server_features(state: State<'_, AppState>) -> Res<Vec<String>> {
    Ok(state.handle()?.server_features().await?)
}

/// The version of the app and the system that it runs on.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CHORD_VERSION"),
        commit: env!("CHORD_COMMIT"),
        build: crate::updates::current_build(),
        channel: crate::updates::BUILD_CHANNEL,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chord_core::store::{Store, queries};

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chord-advanced-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_folder_has_size_zero() {
        assert_eq!(tree_size(Path::new("/no/such/folder/here")), 0);
        assert_eq!(file_size(Path::new("/no/such/file")), 0);
    }

    #[test]
    fn the_database_size_adds_the_journal_files() {
        let dir = temp_dir("dbsize");
        let db = dir.join("a@b.c.sqlite3");
        std::fs::write(&db, [0u8; 100]).unwrap();
        std::fs::write(dir.join("a@b.c.sqlite3-wal"), [0u8; 20]).unwrap();
        std::fs::write(dir.join("other.sqlite3"), [0u8; 999]).unwrap();
        assert_eq!(database_size(&db), 120);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_drawn_emoji_keeps_the_packs() {
        let dir = temp_dir("drawn");
        let pack = dir.join("fluent");
        std::fs::create_dir_all(pack.join(DRAWN_DIR)).unwrap();
        std::fs::write(pack.join(DRAWN_DIR).join("1f600-64.png"), [0u8; 50]).unwrap();
        std::fs::write(pack.join("1f600.svg"), [0u8; 10]).unwrap();
        assert_eq!(drawn_folders(&dir).len(), 1);
        assert_eq!(clear_drawn(&dir), 50);
        assert!(pack.join("1f600.svg").exists());
        assert!(!pack.join(DRAWN_DIR).exists());
        assert_eq!(clear_drawn(&dir), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_the_history_keeps_other_accounts() {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        let mine = queries::ensure_account(conn, "alice@chord.localhost").unwrap();
        let other = queries::ensure_account(conn, "bob@chord.localhost").unwrap();
        for (account, key) in [(mine, "m1"), (mine, "m2"), (other, "o1")] {
            conn.execute(
                "INSERT INTO messages (account_id, key_kind, key, direction, peer, sender, body, timestamp)
                 VALUES (?1, 'origin-id', ?2, 'in', 'c@chord.localhost', 'c@chord.localhost', 'hi', 1)",
                params![account, key],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sync_cursors (account_id, archive, newest_id) VALUES (?1, ?2, 'x')",
                params![account, key],
            )
            .unwrap();
        }
        assert_eq!(delete_history(conn, mine).unwrap(), 2);
        let count = |table: &str, account: i64| -> i64 {
            conn.query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE account_id = ?1"),
                params![account],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(count("messages", mine), 0);
        assert_eq!(count("sync_cursors", mine), 0);
        assert_eq!(count("messages", other), 1);
        assert_eq!(count("sync_cursors", other), 1);
    }

    #[test]
    fn avatar_bytes_are_zero_with_no_avatars() {
        let store = Store::open_in_memory().unwrap();
        let account = queries::ensure_account(store.conn(), "alice@chord.localhost").unwrap();
        assert_eq!(avatar_bytes(store.conn(), account), 0);
    }
}
