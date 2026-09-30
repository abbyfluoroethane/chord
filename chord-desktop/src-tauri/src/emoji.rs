//! Emoji packs: the images that Chord draws for emoji, in place of the system font.
//!
//! Each pack is an Iconify JSON set from npm (`@iconify-json/*`). Twemoji is inside the
//! app and installs on first use. Noto and Fluent download once from the npm registry
//! when the user picks them. The download has a fixed URL and a fixed SHA-512, so a
//! changed file fails.
//!
//! An install writes one SVG file for each emoji: `<app data>/emoji/<pack>/<hex>.svg`.
//! `<hex>` is the code points in lower-case hex, at least 4 digits, joined with `-`,
//! without U+FE0F. The `chord-emoji` URI scheme serves these files to `<img>`, which
//! never runs a script in an SVG.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Manager};

use crate::error::{ChordError, Res};

pub const SCHEME: &str = "chord-emoji";

/// The most bytes of a downloaded pack: 32 MB. Fluent is about 14 MB.
const MAX_DOWNLOAD_BYTES: usize = 32 * 1024 * 1024;
/// The most bytes of one JSON file in a pack: 160 MB. Fluent icons.json is about 104 MB.
const MAX_JSON_BYTES: u64 = 160 * 1024 * 1024;

/// One emoji pack.
pub struct Pack {
    pub id: &'static str,
    /// The npm tarball.
    url: &'static str,
    /// The npm `dist.integrity` of the tarball.
    integrity: &'static str,
    /// The tarball inside the app, if the pack ships with it.
    bundled: Option<&'static [u8]>,
}

pub const PACKS: [Pack; 3] = [
    Pack {
        id: "twemoji",
        url: "https://registry.npmjs.org/@iconify-json/twemoji/-/twemoji-1.2.5.tgz",
        integrity: "sha512-uKpuIEV0v6K5BW3Mjdyl+XKFVAbbcPxAgifKvEMtZoUZB5+YiY5zaMm2uNNCxyXzAWU9yNLlj41WU6/mvgALsw==",
        bundled: Some(include_bytes!("../resources/twemoji-1.2.5.tgz")),
    },
    Pack {
        id: "noto",
        url: "https://registry.npmjs.org/@iconify-json/noto/-/noto-1.2.9.tgz",
        integrity: "sha512-DQiuXENbun41ch+XPSV55H9FaWKpHTyz46YIKtSaKYD4fIDDzL1tR6H4np/T38jfrTAeI3rcKF0mqHYeZVAuaw==",
        bundled: None,
    },
    Pack {
        id: "fluent",
        url: "https://registry.npmjs.org/@iconify-json/fluent-emoji/-/fluent-emoji-1.2.7.tgz",
        integrity: "sha512-D8G6bKAyIsyxP1rzZtIWr/gg/fO4Rg1+DyCWlfqKK4h8W+4Em/Y0P6QYgJUQTlUcagzJvSyg+9OBv4hz4Hc/QA==",
        bundled: None,
    },
];

fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

/// The marker file of a complete install. It holds the tarball URL.
const DONE: &str = ".installed";

fn packs_dir(app: &AppHandle) -> Res<PathBuf> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("emoji"))
        .map_err(|e| ChordError::io("find the app data folder", e))
}

fn is_installed(dir: &Path, p: &Pack) -> bool {
    std::fs::read_to_string(dir.join(p.id).join(DONE)).is_ok_and(|s| s.trim() == p.url)
}

// ---------------------------------------------------------------- the pack data

#[derive(Deserialize)]
struct IconSet {
    icons: HashMap<String, Icon>,
    #[serde(default)]
    aliases: HashMap<String, Alias>,
    #[serde(default = "default_size")]
    width: f64,
    #[serde(default = "default_size")]
    height: f64,
}

fn default_size() -> f64 {
    16.0
}

#[derive(Deserialize)]
struct Icon {
    body: String,
    width: Option<f64>,
    height: Option<f64>,
    left: Option<f64>,
    top: Option<f64>,
}

#[derive(Deserialize)]
struct Alias {
    parent: String,
}

/// Check the tarball against the npm integrity string (`sha512-<base64>`).
fn verify(bytes: &[u8], integrity: &str) -> Res<()> {
    let want = integrity
        .strip_prefix("sha512-")
        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
        .ok_or_else(|| ChordError::new("emoji", "the pack has a bad integrity value"))?;
    if Sha512::digest(bytes).as_slice() == want.as_slice() {
        Ok(())
    } else {
        Err(ChordError::new(
            "emoji",
            "the downloaded pack does not match its checksum",
        ))
    }
}

/// Read `package/icons.json` and `package/chars.json` from the tarball.
fn read_tarball(bytes: &[u8]) -> Res<(IconSet, HashMap<String, String>)> {
    let bad =
        |what: &str, e: &dyn std::fmt::Display| ChordError::new("emoji", format!("{what}: {e}"));
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut icons = None;
    let mut chars = None;
    for entry in archive
        .entries()
        .map_err(|e| bad("cannot read the pack", &e))?
    {
        let entry = entry.map_err(|e| bad("cannot read the pack", &e))?;
        let name = entry
            .path()
            .map_err(|e| bad("cannot read the pack", &e))?
            .to_string_lossy()
            .into_owned();
        if name != "package/icons.json" && name != "package/chars.json" {
            continue;
        }
        let mut json = Vec::new();
        entry
            .take(MAX_JSON_BYTES)
            .read_to_end(&mut json)
            .map_err(|e| bad("cannot read the pack", &e))?;
        if name.ends_with("icons.json") {
            icons = Some(
                serde_json::from_slice::<IconSet>(&json).map_err(|e| bad("bad icons.json", &e))?,
            );
        } else {
            chars = Some(
                serde_json::from_slice::<HashMap<String, String>>(&json)
                    .map_err(|e| bad("bad chars.json", &e))?,
            );
        }
    }
    match (icons, chars) {
        (Some(i), Some(c)) => Ok((i, c)),
        _ => Err(ChordError::new("emoji", "the pack has no emoji data")),
    }
}

const TONES: [&str; 5] = ["medium-light", "medium-dark", "light", "medium", "dark"];

/// Code points by icon name, from the Twemoji names. The Fluent set has no code point
/// map, and its names put the skin tone last without "skin-tone": "waving-hand-light",
/// "man-bald-light". Twemoji says "waving-hand-light-skin-tone" and
/// "man-light-skin-tone-bald". About 96% of the Fluent names match. The rest fall back to
/// Twemoji in the UI.
fn codes_by_name() -> Res<HashMap<String, String>> {
    let (_, chars) = read_tarball(PACKS[0].bundled.unwrap_or_default())?;
    let mut by_name = HashMap::new();
    for (hex, name) in chars {
        by_name
            .entry(name.replace("-skin-tone", ""))
            .or_insert_with(|| hex.clone());
        for tone in TONES {
            let mark = format!("-{tone}-skin-tone");
            if name.contains(&mark) && !name.ends_with(&mark) {
                let moved = format!("{}-{tone}", name.replacen(&mark, "", 1));
                by_name.entry(moved).or_insert_with(|| hex.clone());
                break;
            }
        }
        by_name.entry(name).or_insert(hex);
    }
    Ok(by_name)
}

/// A file name for a `chars.json` key: lower-case hex parts, 1 to 16 of them.
fn is_hex_key(key: &str) -> bool {
    let parts: Vec<&str> = key.split('-').collect();
    (1..=16).contains(&parts.len())
        && parts.iter().all(|p| {
            (1..=6).contains(&p.len())
                && p.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

/// One SVG document for an icon of the set.
fn svg(set: &IconSet, icon: &Icon) -> String {
    let w = icon.width.unwrap_or(set.width);
    let h = icon.height.unwrap_or(set.height);
    let left = icon.left.unwrap_or(0.0);
    let top = icon.top.unwrap_or(0.0);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{left} {top} {w} {h}" width="{w}" height="{h}">{}</svg>"#,
        icon.body
    )
}

/// Write the pack to `<dir>/<id>`: first to a new folder, then a rename, so a failed
/// install leaves the old one as it was.
fn install_from(dir: &Path, p: &Pack, bytes: &[u8]) -> Res<()> {
    verify(bytes, p.integrity)?;
    let (set, mut chars) = read_tarball(bytes)?;
    if chars.is_empty() {
        let by_name = codes_by_name()?;
        chars = set
            .icons
            .keys()
            .filter_map(|name| Some((by_name.get(name)?.clone(), name.clone())))
            .collect();
    }
    let io = |what: &str, e: std::io::Error| ChordError::io(what, e);
    std::fs::create_dir_all(dir).map_err(|e| io("make the emoji folder", e))?;
    let temp = dir.join(format!(".{}-new", p.id));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).map_err(|e| io("make the emoji folder", e))?;
    for (key, name) in &chars {
        if !is_hex_key(key) {
            continue;
        }
        let icon = set
            .icons
            .get(name)
            .or_else(|| set.aliases.get(name).and_then(|a| set.icons.get(&a.parent)));
        if let Some(icon) = icon {
            std::fs::write(temp.join(format!("{key}.svg")), svg(&set, icon))
                .map_err(|e| io("write an emoji", e))?;
        }
    }
    std::fs::write(temp.join(DONE), p.url).map_err(|e| io("write an emoji", e))?;
    let target = dir.join(p.id);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&temp, &target).map_err(|e| io("move the emoji folder", e))
}

/// One install at a time, so two requests do not write the same folder.
static INSTALL: Mutex<()> = Mutex::new(());

/// Install a pack from its bundled tarball. Only Twemoji has one.
fn install_bundled(dir: &Path, p: &Pack) -> Res<()> {
    let bytes = p
        .bundled
        .ok_or_else(|| ChordError::new("emoji", "this pack is not in the app"))?;
    let _guard = INSTALL
        .lock()
        .map_err(|_| ChordError::new("emoji", "an install failed"))?;
    if is_installed(dir, p) {
        return Ok(());
    }
    install_from(dir, p, bytes)
}

async fn download(p: &Pack) -> Res<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent("Chord/0.1 (emoji pack)")
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ChordError::new("emoji", format!("cannot start the client: {e}")))?;
    let fail = |e: reqwest::Error| ChordError::new("emoji", format!("the download failed: {e}"));
    let mut response = client.get(p.url).send().await.map_err(fail)?;
    if !response.status().is_success() {
        return Err(ChordError::new(
            "emoji",
            format!("the npm registry answered {}", response.status()),
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(fail)? {
        if body.len() + chunk.len() > MAX_DOWNLOAD_BYTES {
            return Err(ChordError::new("emoji", "the pack is too big"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackStatus {
    pub id: &'static str,
    pub installed: bool,
    /// True if the pack ships with the app. It needs no download.
    pub bundled: bool,
}

/// The packs and whether each one is on this computer.
#[tauri::command]
pub fn emoji_packs(app: AppHandle) -> Res<Vec<PackStatus>> {
    let dir = packs_dir(&app)?;
    Ok(PACKS
        .iter()
        .map(|p| PackStatus {
            id: p.id,
            installed: is_installed(&dir, p),
            bundled: p.bundled.is_some(),
        })
        .collect())
}

/// Install a pack: from the app for Twemoji, else a download from the npm registry.
#[tauri::command]
pub async fn emoji_pack_install(app: AppHandle, id: String) -> Res<()> {
    let p = pack(&id).ok_or_else(|| ChordError::invalid(format!("no emoji pack {id:?}")))?;
    let dir = packs_dir(&app)?;
    if is_installed(&dir, p) {
        return Ok(());
    }
    let bytes = match p.bundled {
        Some(b) => b.to_vec(),
        None => download(p).await?,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = INSTALL
            .lock()
            .map_err(|_| ChordError::new("emoji", "an install failed"))?;
        if is_installed(&dir, p) {
            return Ok(());
        }
        install_from(&dir, p, &bytes)
    })
    .await
    .map_err(|e| ChordError::new("emoji", format!("the install stopped: {e}")))?
}

// ---------------------------------------------------------------- the URI scheme

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(Vec::new())
        .unwrap_or_default()
}

/// The response for a path like `/twemoji/1f44b-1f3fb.svg`. Twemoji installs itself on
/// the first request.
pub fn respond(dir: &Path, path: &str) -> Response<Vec<u8>> {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    let (Some(id), Some(file)) = (parts.next(), parts.next()) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Some(p) = pack(id) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Some(key) = file.strip_suffix(".svg").filter(|k| is_hex_key(k)) else {
        return status(StatusCode::NOT_FOUND);
    };
    if !is_installed(dir, p) && (p.bundled.is_none() || install_bundled(dir, p).is_err()) {
        return status(StatusCode::NOT_FOUND);
    }
    match std::fs::read(dir.join(p.id).join(format!("{key}.svg"))) {
        Ok(data) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/svg+xml")
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .header("X-Content-Type-Options", "nosniff")
            .header(
                "Content-Security-Policy",
                "default-src 'none'; style-src 'unsafe-inline'",
            )
            .body(data)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

/// Register the scheme.
pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(SCHEME, |ctx, request, responder| {
        let dir = ctx
            .app_handle()
            .path()
            .app_data_dir()
            .map(|d| d.join("emoji"));
        let path = request.uri().path().to_owned();
        tauri::async_runtime::spawn_blocking(move || {
            let response = match dir {
                Ok(dir) => respond(&dir, &path),
                Err(_) => status(StatusCode::NOT_FOUND),
            };
            responder.respond(response);
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chord-emoji-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn hex_keys_are_strict() {
        assert!(is_hex_key("1f44b"));
        assert!(is_hex_key("1f44b-1f3fb"));
        assert!(is_hex_key("0030-20e3"));
        assert!(!is_hex_key("../etc"));
        assert!(!is_hex_key("1F44B"));
        assert!(!is_hex_key(""));
        assert!(!is_hex_key("1f44b--1f3fb"));
    }

    #[test]
    fn a_wrong_checksum_fails() {
        let p = &PACKS[0];
        assert!(verify(b"not the pack", p.integrity).is_err());
    }

    #[test]
    fn twemoji_installs_from_the_app_and_serves_svg() {
        let dir = temp_dir("twemoji");
        let response = respond(&dir, "/twemoji/1f44b.svg");
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(response.body().clone()).unwrap();
        assert!(
            body.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
            "{body}"
        );
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/svg+xml"
        );
        // A skin tone, a keycap, and a ZWJ sequence without U+FE0F.
        for key in ["1f44b-1f3fb", "0031-20e3", "1f3f3-200d-1f308"] {
            assert_eq!(
                respond(&dir, &format!("/twemoji/{key}.svg")).status(),
                StatusCode::OK,
                "{key}"
            );
        }
        assert_eq!(
            respond(&dir, "/twemoji/../x.svg").status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            respond(&dir, "/other/1f44b.svg").status(),
            StatusCode::NOT_FOUND
        );
        // A pack that needs a download is not there.
        assert_eq!(
            respond(&dir, "/noto/1f44b.svg").status(),
            StatusCode::NOT_FOUND
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Download and install Noto and Fluent from the npm registry. Run it with
    /// `cargo test -p chord-desktop emoji_download_live -- --ignored`.
    #[test]
    #[ignore = "needs the network"]
    fn emoji_download_live() {
        let dir = temp_dir("download");
        for p in &PACKS[1..] {
            let bytes = tauri::async_runtime::block_on(download(p)).unwrap();
            install_from(&dir, p, &bytes).unwrap();
            assert!(is_installed(&dir, p));
            let path = format!("/{}/1f44b-1f3fb.svg", p.id);
            assert_eq!(respond(&dir, &path).status(), StatusCode::OK, "{path}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
