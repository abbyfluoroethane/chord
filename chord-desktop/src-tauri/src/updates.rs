//! The app updates itself from the release repository. See docs/updates.md.
//!
//! The page picks the channel. Rust reads the manifest of that channel for this platform, and
//! takes an update only when its build number is higher than the build of this app: never a
//! downgrade. A `dev` build does not update itself.
//!
//! On Linux Chord is a Flatpak: the Flatpak portal does the updates (src/flatpak.rs), never
//! the Tauri updater. A Linux build outside Flatpak (a dev build) has no updater.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::error::{ChordError, Res};
use crate::state::lock;

/// The manifest of each channel and platform. `{{target}}` and `{{arch}}` are filled in by the
/// updater: `linux`, `windows` or `darwin`, and `x86_64` or `aarch64`.
const MANIFEST_BASE: &str =
    "https://raw.githubusercontent.com/abbyfluoroethane/chord-desktop/main/channels";

/// The longest wait for the manifest.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// A progress message at most once per this many bytes, when the size is unknown.
const PROGRESS_STEP: u64 = 256 * 1024;

/// The channel that this build came from: `stable`, `beta`, `nightly` or `dev`.
pub const BUILD_CHANNEL: &str = env!("CHORD_CHANNEL");

/// The build number of this app (see docs/updates.md).
pub fn current_build() -> i64 {
    env!("CHORD_BUILD").parse().unwrap_or(0)
}

/// The event that tells the page about an update that the Flatpak portal found.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const UPDATE_EVENT: &str = "update-available";

/// How this install updates: `tauri` (Windows, macOS), `flatpak`, or `none`.
pub fn updater_kind() -> &'static str {
    if let Some(info) = crate::flatpak::instance() {
        return if crate::flatpak::can_update(info) {
            "flatpak"
        } else {
            "none"
        };
    }
    if cfg!(target_os = "linux") || BUILD_CHANNEL == "dev" {
        "none"
    } else {
        "tauri"
    }
}

/// The update found by the last check. Install takes it from here.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

/// An update for the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// The version without the build number, for example `0.3.0-beta.2`. Empty for a Flatpak
    /// update: the portal gives no version.
    pub version: String,
    pub build: i64,
    pub channel: Option<String>,
    pub commit: Option<String>,
    /// Markdown.
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub release_url: Option<String>,
    /// The new version is already installed (a Flatpak updated by the system): only a
    /// restart is needed.
    pub installed: bool,
}

/// The download progress of an install.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    /// The size of the download, when the server sends it.
    pub total: Option<u64>,
}

/// The channel names that the page may send.
fn parse_channel(channel: &str) -> Option<&'static str> {
    ["stable", "beta", "nightly"]
        .into_iter()
        .find(|c| *c == channel)
}

/// The manifest URL of a channel, with the updater's `{{target}}` and `{{arch}}`.
fn manifest_url(channel: &str) -> String {
    format!("{MANIFEST_BASE}/{channel}/{{{{target}}}}-{{{{arch}}}}.json")
}

/// The build number in the build metadata of a version: `b1612345` gives 1612345. The
/// metadata can have more parts, split by dots.
fn build_from_meta(meta: &str) -> Option<i64> {
    meta.split('.').find_map(|part| {
        let digits = part.strip_prefix('b')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    })
}

/// The build number of a version text, for example `0.3.0-beta.2+b1612345`.
fn build_from_version(version: &str) -> Option<i64> {
    build_from_meta(version.split_once('+')?.1)
}

/// The version without its build metadata: `0.3.0-beta.2+b1612345` gives `0.3.0-beta.2`.
fn without_build(version: &str) -> &str {
    version.split_once('+').map_or(version, |(core, _)| core)
}

/// Is the remote build newer than this one? A version without a build number never is.
fn is_newer(remote_meta: &str, current: i64) -> bool {
    build_from_meta(remote_meta).is_some_and(|remote| remote > current)
}

/// The page view of an update. The extra fields come from the raw manifest.
fn info_of(version: &str, raw: &Value) -> UpdateInfo {
    let text = |key: &str| raw.get(key).and_then(Value::as_str).map(str::to_owned);
    UpdateInfo {
        version: without_build(version).to_owned(),
        build: build_from_version(version)
            .or_else(|| raw.get("build").and_then(Value::as_i64))
            .unwrap_or(0),
        channel: text("channel"),
        commit: text("commit"),
        notes: text("notes"),
        pub_date: text("pub_date"),
        release_url: text("release_url"),
        installed: false,
    }
}

fn update_error(e: impl std::fmt::Display) -> ChordError {
    ChordError::new("update", e.to_string())
}

/// Look for an update in `channel`. Resolves to null when there is none, when the channel has
/// no manifest yet, or when this install has no updater. In a Flatpak the branch is the
/// channel, so `channel` does not count there: the update monitor of the portal answers.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    pending: State<'_, Pending>,
    channel: String,
) -> Res<Option<UpdateInfo>> {
    let channel = parse_channel(&channel)
        .ok_or_else(|| ChordError::invalid(format!("no update channel {channel}")))?;
    match updater_kind() {
        #[cfg(target_os = "linux")]
        "flatpak" => return crate::flatpak::check(&app).await,
        "tauri" => {}
        _ => return Ok(None),
    }
    let url = manifest_url(channel)
        .parse()
        .map_err(|e| ChordError::invalid(format!("bad manifest URL: {e}")))?;
    let current = current_build();
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(update_error)?
        .timeout(CHECK_TIMEOUT)
        // The build number decides, not the version: a user who moves to a slower channel
        // keeps the newer build.
        .version_comparator(move |_, release| is_newer(release.version.build.as_str(), current))
        .build()
        .map_err(update_error)?;
    let found = match updater.check().await {
        Ok(found) => found,
        // No manifest (HTTP 404): the channel has no build yet.
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => None,
        Err(e) => return Err(update_error(e)),
    };
    let Some(mut update) = found else {
        *lock(&pending.0) = None;
        return Ok(None);
    };
    let info = info_of(&update.version, &update.raw_json);
    // The signature names the version that the release was built with, which has no build
    // number. The updater compares the two before it installs.
    update.version = info.version.clone();
    *lock(&pending.0) = Some(update);
    Ok(Some(info))
}

/// Download and install the update from the last check. The progress goes to `on_progress`.
/// On Windows the installer closes the app. Elsewhere, call `update_restart` after it.
#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    pending: State<'_, Pending>,
    on_progress: Channel<UpdateProgress>,
) -> Res<()> {
    match updater_kind() {
        // The portal counts in operations, not bytes: each operation is 100.
        #[cfg(target_os = "linux")]
        "flatpak" => {
            return crate::flatpak::install(&app, |downloaded, total| {
                let _ = on_progress.send(UpdateProgress {
                    downloaded,
                    total: Some(total),
                });
            })
            .await;
        }
        "tauri" => {}
        _ => return Err(ChordError::new("noUpdate", "this install has no updater")),
    }
    // Only the Flatpak path needs the handle.
    let _ = &app;
    let update = lock(&pending.0)
        .clone()
        .ok_or_else(|| ChordError::new("noUpdate", "no update to install: check first"))?;
    let mut downloaded: u64 = 0;
    let mut sent: Option<u64> = None;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                // One message per percent, or per step when the size is unknown.
                let mark = match total {
                    Some(total) if total > 0 => downloaded * 100 / total,
                    _ => downloaded / PROGRESS_STEP,
                };
                if sent != Some(mark) {
                    sent = Some(mark);
                    let _ = on_progress.send(UpdateProgress { downloaded, total });
                }
            },
            || {},
        )
        .await
        .map_err(update_error)?;
    *lock(&pending.0) = None;
    Ok(())
}

/// Start the app again, so the installed update runs. In a Flatpak the portal starts the
/// newest installed version, then the app quits.
#[tauri::command]
pub async fn update_restart(app: AppHandle) -> Res<()> {
    #[cfg(target_os = "linux")]
    if updater_kind() == "flatpak" {
        return crate::flatpak::restart(&app).await;
    }
    app.restart();
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_manifest_url_has_the_channel_and_the_updater_variables() {
        assert_eq!(
            manifest_url("beta"),
            "https://raw.githubusercontent.com/abbyfluoroethane/chord-desktop/main/channels/beta/{{target}}-{{arch}}.json"
        );
        // The url crate encodes the braces. The updater replaces both forms.
        let url: url::Url = manifest_url("nightly").parse().unwrap();
        assert!(url.as_str().contains("/channels/nightly/"));
    }

    #[test]
    fn only_the_three_channels_are_accepted() {
        assert_eq!(parse_channel("stable"), Some("stable"));
        assert_eq!(parse_channel("beta"), Some("beta"));
        assert_eq!(parse_channel("nightly"), Some("nightly"));
        assert_eq!(parse_channel("dev"), None);
        assert_eq!(parse_channel("../x"), None);
    }

    #[test]
    fn the_build_comes_from_the_metadata() {
        assert_eq!(build_from_meta("b1612345"), Some(1_612_345));
        assert_eq!(build_from_meta("dev.b42"), Some(42));
        assert_eq!(build_from_meta("dev"), None);
        assert_eq!(build_from_meta("b"), None);
        assert_eq!(build_from_meta("b12x"), None);
        assert_eq!(build_from_meta(""), None);
        assert_eq!(build_from_version("0.3.0-beta.2+b1612345"), Some(1_612_345));
        assert_eq!(build_from_version("0.3.0"), None);
    }

    #[test]
    fn the_version_loses_its_build_for_the_page() {
        assert_eq!(without_build("0.3.0-beta.2+b1612345"), "0.3.0-beta.2");
        assert_eq!(without_build("0.3.0"), "0.3.0");
    }

    #[test]
    fn only_a_higher_build_is_an_update() {
        assert!(is_newer("b101", 100));
        assert!(!is_newer("b100", 100));
        // A slower channel with an older build: no downgrade.
        assert!(!is_newer("b99", 100));
        // No build number: never an update.
        assert!(!is_newer("", 100));
    }

    #[test]
    fn this_build_has_a_number_and_a_channel() {
        assert!(current_build() >= 0);
        assert!(["stable", "beta", "nightly", "dev"].contains(&BUILD_CHANNEL));
    }

    #[test]
    fn linux_outside_flatpak_has_no_updater() {
        if cfg!(target_os = "linux") && crate::flatpak::instance().is_none() {
            assert_eq!(updater_kind(), "none");
        }
    }

    #[test]
    fn the_info_reads_the_extra_manifest_fields() {
        let raw = json!({
            "version": "0.3.0-beta.2+b1612345",
            "build": 1_612_345,
            "channel": "beta",
            "commit": "09c83fb",
            "pub_date": "2026-10-04T06:00:00Z",
            "release_url": "https://example.org/r",
            "notes": "Fixes",
        });
        let info = info_of("0.3.0-beta.2+b1612345", &raw);
        assert_eq!(
            info,
            UpdateInfo {
                version: "0.3.0-beta.2".into(),
                build: 1_612_345,
                channel: Some("beta".into()),
                commit: Some("09c83fb".into()),
                notes: Some("Fixes".into()),
                pub_date: Some("2026-10-04T06:00:00Z".into()),
                release_url: Some("https://example.org/r".into()),
                installed: false,
            }
        );
        // Without the build in the version, the build field counts.
        assert_eq!(info_of("0.3.0", &json!({ "build": 7 })).build, 7);
    }
}
