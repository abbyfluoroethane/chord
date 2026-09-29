//! Local UI settings, for example the circle folders. Core has no part in them.
//!
//! The UI owns the shape. Rust only checks that the value is JSON and small. The file is
//! `settings.json` in the app config directory. A write goes to a temporary file first,
//! then a rename replaces the old file, so a crash cannot leave half a file.

use std::path::Path;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::error::{ChordError, Res};

/// The most bytes of a settings file: 256 KB.
pub const MAX_SETTINGS_BYTES: usize = 256 * 1024;

const FILE: &str = "settings.json";

/// Turn the value into file bytes. Fails if the file would be too big.
pub fn validate(value: &Value) -> Res<Vec<u8>> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| ChordError::new("settings", format!("the settings are not JSON: {e}")))?;
    if bytes.len() > MAX_SETTINGS_BYTES {
        return Err(ChordError::new(
            "settings",
            format!(
                "the settings are {} bytes, the limit is {MAX_SETTINGS_BYTES}",
                bytes.len()
            ),
        ));
    }
    Ok(bytes)
}

/// Read the settings from `dir`. A missing or damaged file gives an empty object.
pub fn read_from(dir: &Path) -> Value {
    let empty = || Value::Object(serde_json::Map::new());
    let path = dir.join(FILE);
    let Ok(meta) = std::fs::metadata(&path) else {
        return empty();
    };
    if meta.len() > MAX_SETTINGS_BYTES as u64 {
        log::warn!("settings file is too big: ignored");
        return empty();
    }
    match std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(value) => value,
        Err(e) => {
            log::warn!("settings file cannot be read: {e}");
            empty()
        }
    }
}

/// Write the settings to `dir`.
pub fn write_to(dir: &Path, value: &Value) -> Res<()> {
    let bytes = validate(value)?;
    std::fs::create_dir_all(dir).map_err(|e| ChordError::io("cannot create the config dir", e))?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| ChordError::io("cannot write the settings", e))?;
    std::fs::rename(&tmp, dir.join(FILE))
        .map_err(|e| ChordError::io("cannot replace the settings", e))
}

fn config_dir(app: &AppHandle) -> Res<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .map_err(|e| ChordError::io("no config dir", e))
}

/// The saved settings, or `{}`.
#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Res<Value> {
    let dir = config_dir(&app)?;
    Ok(read_from(&dir))
}

/// Replace the settings. The value can be any JSON up to 256 KB.
#[tauri::command]
pub async fn set_settings(app: AppHandle, value: Value) -> Res<()> {
    validate(&value)?;
    let dir = config_dir(&app)?;
    write_to(&dir, &value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chord-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn small_json_passes() {
        let bytes = validate(&json!({"folders": [{"name": "Work", "spaces": []}]})).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn big_json_fails() {
        let big = json!({"blob": "x".repeat(MAX_SETTINGS_BYTES)});
        let error = validate(&big).unwrap_err();
        assert_eq!(error.code, "settings");
        assert!(error.message.contains("limit"));
    }

    #[test]
    fn the_limit_is_exact() {
        // {"a":"..."} has 8 bytes around the text.
        let fits = json!({"a": "x".repeat(MAX_SETTINGS_BYTES - 8)});
        assert_eq!(validate(&fits).unwrap().len(), MAX_SETTINGS_BYTES);
        let over = json!({"a": "x".repeat(MAX_SETTINGS_BYTES - 7)});
        assert!(validate(&over).is_err());
    }

    #[test]
    fn write_then_read_returns_the_value() {
        let dir = temp_dir("roundtrip");
        assert_eq!(read_from(&dir), json!({}), "no file gives an empty object");
        let value = json!({"theme": "dark", "n": [1, 2]});
        write_to(&dir, &value).unwrap();
        assert_eq!(read_from(&dir), value);
        std::fs::write(dir.join(FILE), b"{not json").unwrap();
        assert_eq!(
            read_from(&dir),
            json!({}),
            "a damaged file gives an empty object"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
