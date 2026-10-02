//! The commands that touch the user's files: send a file, save an image.
//!
//! The file dialog runs here, in Rust, and the UI never sees a path. A script in the page
//! cannot ask to read or write an arbitrary path: it can only ask for a dialog, and the
//! user picks the file. The dialog plugin has no permission for the page (see
//! capabilities/default.json). The gate functions below add two more rules: they never
//! follow a symlink, and they check the size and the type on the open file handle, so a
//! swap between the check and the read gets no chance.
//!
//! A file dropped on the window is the other way in. The OS gives its path to Rust in the
//! drag-drop event (`Dropped::remember`), and `upload_dropped` accepts only such a path,
//! once. A page that invents a path gets a refusal.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::{MAX_UPLOAD_BYTES, file_name, guess_content_type};
use crate::error::{ChordError, Res};
use crate::link_preview::download_image;
use crate::state::{AppState, lock};

/// The paths that the user dropped on the window and that no upload has used yet.
#[derive(Default)]
pub struct Dropped(Mutex<HashSet<PathBuf>>);

/// A drop holds a few files. The set forgets older drops when a new drop comes.
const DROP_LIMIT: usize = 64;

impl Dropped {
    /// Keep the paths of a new drop. Older paths are forgotten.
    pub fn remember(&self, paths: &[PathBuf]) {
        let mut set = lock(&self.0);
        set.clear();
        set.extend(paths.iter().take(DROP_LIMIT).cloned());
    }

    /// Accept a path if the user dropped it, and forget it. The path can be used once.
    fn take(&self, path: &Path) -> Res<()> {
        if lock(&self.0).remove(path) {
            Ok(())
        } else {
            Err(ChordError::invalid(
                "the file was not dropped on the window",
            ))
        }
    }
}

/// The extensions that `save_image` writes. The save dialog can offer any name.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg"];

/// Open `path` and refuse a symlink. On Unix the open itself refuses it (`O_NOFOLLOW`),
/// so no race is left. Elsewhere a check follows the open.
fn open_no_follow(path: &Path, options: &mut OpenOptions) -> std::io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::other("the path is a symlink"));
    }
    Ok(file)
}

fn open_error(e: &std::io::Error, what: &str) -> ChordError {
    #[cfg(unix)]
    if e.raw_os_error() == Some(libc::ELOOP) {
        return ChordError::invalid("the path is a symlink");
    }
    ChordError::io(what, e)
}

/// Open the file that the user picked. One open, no symlink, a regular file, and the size
/// cap on the handle. The core streams the upload from this handle and never sees the path,
/// so a swap of the path after the check has no effect. The core sends as many bytes as the
/// handle had at the check: a file that grows is cut there, and one that shrinks fails.
fn open_picked(path: &Path) -> Res<File> {
    let file = open_no_follow(path, OpenOptions::new().read(true))
        .map_err(|e| open_error(&e, "cannot read the file"))?;
    let meta = file
        .metadata()
        .map_err(|e| ChordError::io("cannot read the file", e))?;
    if !meta.is_file() {
        return Err(ChordError::invalid("the path is not a file"));
    }
    if meta.len() > MAX_UPLOAD_BYTES {
        return Err(ChordError::invalid(format!(
            "the file is {} bytes, the limit is {MAX_UPLOAD_BYTES}",
            meta.len()
        )));
    }
    Ok(file)
}

/// Write the image that the user chose a place for. Only an image extension, and never
/// through a symlink at the target.
fn write_image(path: &Path, data: &[u8]) -> Res<()> {
    let ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if !ok {
        return Err(ChordError::invalid(
            "the file name must end in an image extension such as .png or .jpg",
        ));
    }
    if !path.is_absolute() {
        return Err(ChordError::invalid("the save path must be a file path"));
    }
    // Open without truncation, check the handle, then truncate: a device or a file behind a
    // symlink at the target must not be damaged.
    let mut file = open_no_follow(path, OpenOptions::new().write(true).create(true))
        .map_err(|e| open_error(&e, "save the image"))?;
    let meta = file
        .metadata()
        .map_err(|e| ChordError::io("save the image", e))?;
    if !meta.is_file() {
        return Err(ChordError::invalid("the path is not a file"));
    }
    file.set_len(0)
        .and_then(|()| file.write_all(data))
        .map_err(|e| ChordError::io("save the image", e))
}

/// Show the file dialog, let the user pick files, and upload each one (XEP-0363) to `to`.
/// Returns the URLs. An empty list means the user cancelled.
#[tauri::command]
pub async fn upload_files(
    app: AppHandle,
    state: State<'_, AppState>,
    to: String,
) -> Res<Vec<String>> {
    let handle = state.handle()?;
    let to: chord_core::jid::Jid = to
        .parse()
        .map_err(|e| ChordError::invalid(format!("not a JID ({to:?}): {e}")))?;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Send a file")
            .blocking_pick_files()
    })
    .await
    .map_err(|e| ChordError::io("the file dialog failed", e))?;
    let mut urls = Vec::new();
    for path in picked.unwrap_or_default() {
        let path = path
            .into_path()
            .map_err(|e| ChordError::io("cannot use the file", e))?;
        let name = file_name(&path)?;
        let file = tauri::async_runtime::spawn_blocking(move || open_picked(&path))
            .await
            .map_err(|e| ChordError::io("the read task failed", e))??;
        let content_type = guess_content_type(&name).to_owned();
        urls.push(
            handle
                .upload_file(to.clone(), name, content_type, file)
                .await?,
        );
    }
    Ok(urls)
}

/// Upload a file that the user dropped on the window. `path` must come from the drop event:
/// Rust remembers those paths and refuses every other one. Returns the URL.
#[tauri::command]
pub async fn upload_dropped(
    state: State<'_, AppState>,
    dropped: State<'_, Dropped>,
    to: String,
    path: String,
) -> Res<String> {
    let handle = state.handle()?;
    let to: chord_core::jid::Jid = to
        .parse()
        .map_err(|e| ChordError::invalid(format!("not a JID ({to:?}): {e}")))?;
    let path = PathBuf::from(path);
    dropped.take(&path)?;
    let name = file_name(&path)?;
    let file = tauri::async_runtime::spawn_blocking(move || open_picked(&path))
        .await
        .map_err(|e| ChordError::io("the read task failed", e))??;
    let content_type = guess_content_type(&name).to_owned();
    Ok(handle.upload_file(to, name, content_type, file).await?)
}

/// The biggest pasted file: 25 MB. The page sends the bytes through the IPC channel in one
/// piece, so the cap is lower than the one of a file from the disk.
pub const MAX_PASTE_BYTES: usize = 25 * 1024 * 1024;

/// The file extension that we give a pasted file of this media type. `None` for a type that
/// we do not know: the file is then a plain binary.
fn paste_extension(media_type: &str) -> Option<&'static str> {
    Some(match media_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" => "wav",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        _ => return None,
    })
}

/// The name and the media type of a pasted file. The page has no say in the name: Rust
/// makes it from the media type and the time. An unknown type becomes a plain binary.
fn pasted_identity(media_type: &str, unix_secs: u64) -> (String, &'static str) {
    let media_type = media_type.trim().to_ascii_lowercase();
    let known = paste_extension(&media_type);
    let ext = known.unwrap_or("bin");
    let content_type = if known.is_some() {
        guess_content_type(&format!("x.{ext}"))
    } else {
        "application/octet-stream"
    };
    (format!("pasted-{unix_secs}.{ext}"), content_type)
}

/// The name and the type of a file that the page sends as bytes. A file from the file
/// dialog keeps its own name: only the last path part, without control characters, at
/// most 200 characters. The type comes from the extension of that name. A paste has no
/// name, so it gets a generated one (`pasted_identity`).
fn upload_identity(name: Option<&str>, media_type: &str, unix_secs: u64) -> (String, &'static str) {
    let clean: String = name
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() || clean.chars().all(|c| c == '.') {
        return pasted_identity(media_type, unix_secs);
    }
    (clean.to_owned(), guess_content_type(clean))
}

/// Decode a percent-encoded header value (the page uses `encodeURIComponent`). A bad
/// escape stays as it is, and a byte sequence that is no UTF-8 gets replacement characters.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).and_then(|b| hex(*b)),
                bytes.get(i + 2).and_then(|b| hex(*b)),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Upload a file that the user pasted into the composer: the page has the bytes and no
/// path. The body of the request is the raw bytes, and the headers have `to` and `type`
/// (percent-encoded). Returns the URL.
#[tauri::command]
pub async fn upload_pasted(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Res<String> {
    let handle = state.handle()?;
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(percent_decode)
            .ok_or_else(|| ChordError::invalid(format!("the request has no {name} header")))
    };
    let to = header("to")?;
    let to: chord_core::jid::Jid = to
        .parse()
        .map_err(|e| ChordError::invalid(format!("not a JID ({to:?}): {e}")))?;
    let media_type = header("type").unwrap_or_default();
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(ChordError::invalid("the pasted file must be raw bytes"));
    };
    if bytes.is_empty() {
        return Err(ChordError::invalid("the pasted file is empty"));
    }
    if bytes.len() > MAX_PASTE_BYTES {
        return Err(ChordError::invalid(format!(
            "the pasted file is {} bytes, the limit is {MAX_PASTE_BYTES}",
            bytes.len()
        )));
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = header("name").ok();
    let (name, content_type) = upload_identity(name.as_deref(), &media_type, secs);
    Ok(handle
        .upload(to, name, content_type.to_owned(), bytes.clone())
        .await?)
}

/// Download the image at `url`, ask the user where to save it, and write it there. Returns
/// false if the user cancelled. `name` is the suggested file name.
#[tauri::command]
pub async fn save_image(app: AppHandle, url: String, name: Option<String>) -> Res<bool> {
    let data = download_image(&url).await?;
    let suggested = name
        .as_deref()
        .and_then(|n| Path::new(n).file_name())
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("image")
        .to_owned();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Save image")
            .set_file_name(suggested)
            .blocking_save_file()
    })
    .await
    .map_err(|e| ChordError::io("the file dialog failed", e))?;
    let Some(path) = picked else {
        return Ok(false);
    };
    let path = path
        .into_path()
        .map_err(|e| ChordError::io("cannot use the path", e))?;
    tauri::async_runtime::spawn_blocking(move || write_image(&path, &data))
        .await
        .map_err(|e| ChordError::io("save the image", e))??;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that the test removes at the end.
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("chord-files-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_picked_file_is_opened_as_a_handle() {
        use std::io::Read;
        let dir = Scratch::new("read");
        let file = dir.0.join("a.txt");
        std::fs::write(&file, b"hello").unwrap();
        let mut text = String::new();
        open_picked(&file)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn a_header_value_is_percent_decoded() {
        assert_eq!(
            percent_decode("amy%40chat.example%2Fweb"),
            "amy@chat.example/web"
        );
        assert_eq!(percent_decode("image%2Fpng"), "image/png");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("%C3%A9"), "\u{e9}");
    }

    #[test]
    fn a_pasted_file_gets_a_generated_name_and_a_known_type() {
        assert_eq!(
            pasted_identity("image/png", 1700000000),
            ("pasted-1700000000.png".to_owned(), "image/png")
        );
        assert_eq!(pasted_identity(" IMAGE/JPEG ", 5).0, "pasted-5.jpg");
        // The page cannot smuggle a path or a script type through the media type.
        let (name, kind) = pasted_identity("../../x; text/html", 7);
        assert_eq!(
            (name.as_str(), kind),
            ("pasted-7.bin", "application/octet-stream")
        );
        assert_eq!(pasted_identity("", 7).0, "pasted-7.bin");
    }

    #[test]
    fn a_picked_file_keeps_its_name_without_a_path() {
        assert_eq!(
            upload_identity(Some("report.pdf"), "application/pdf", 5),
            ("report.pdf".to_owned(), "application/pdf")
        );
        assert_eq!(
            upload_identity(Some("../../etc/x.png"), "", 5),
            ("x.png".to_owned(), "image/png")
        );
        assert_eq!(upload_identity(Some("C:\\a\\b.txt"), "", 5).0, "b.txt");
        assert_eq!(
            upload_identity(Some("a\nb.html"), "", 5),
            ("ab.html".to_owned(), "application/octet-stream")
        );
        // No name, a blank name or only dots: a generated name.
        assert_eq!(upload_identity(None, "image/png", 5).0, "pasted-5.png");
        assert_eq!(
            upload_identity(Some("  "), "image/png", 5).0,
            "pasted-5.png"
        );
        assert_eq!(upload_identity(Some(".."), "", 5).0, "pasted-5.bin");
    }

    #[test]
    fn only_a_dropped_path_passes_and_only_once() {
        let dropped = Dropped::default();
        let path = PathBuf::from("/home/amy/photo.png");
        assert_eq!(dropped.take(&path).unwrap_err().code, "invalid");
        dropped.remember(std::slice::from_ref(&path));
        assert_eq!(
            dropped.take(Path::new("/etc/passwd")).unwrap_err().code,
            "invalid"
        );
        assert!(dropped.take(&path).is_ok());
        assert!(dropped.take(&path).is_err());
    }

    #[test]
    fn a_new_drop_replaces_the_old_one() {
        let dropped = Dropped::default();
        dropped.remember(&[PathBuf::from("/a")]);
        dropped.remember(&[PathBuf::from("/b")]);
        assert!(dropped.take(Path::new("/a")).is_err());
        assert!(dropped.take(Path::new("/b")).is_ok());
    }

    #[test]
    fn a_missing_file_or_a_directory_is_refused() {
        let error = open_picked(Path::new("/definitely/not/here")).unwrap_err();
        assert_eq!(error.code, "io");
        let dir = Scratch::new("dir");
        assert_eq!(open_picked(&dir.0).unwrap_err().code, "invalid");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_refused_for_reading() {
        let dir = Scratch::new("link");
        let secret = dir.0.join("secret");
        std::fs::write(&secret, b"key").unwrap();
        let link = dir.0.join("link");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        assert_eq!(open_picked(&link).unwrap_err().code, "invalid");
    }

    #[test]
    fn an_image_is_written_to_an_image_name() {
        let dir = Scratch::new("write");
        let file = dir.0.join("a.PNG");
        std::fs::write(&file, b"old old old").unwrap();
        write_image(&file, b"new").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
    }

    #[test]
    fn other_extensions_and_relative_paths_are_refused() {
        let dir = Scratch::new("ext");
        assert!(write_image(&dir.0.join("a.sh"), b"x").is_err());
        assert!(write_image(&dir.0.join("noext"), b"x").is_err());
        assert!(!dir.0.join("a.sh").exists());
        assert!(write_image(Path::new("relative.png"), b"x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_refused_for_writing() {
        let dir = Scratch::new("wlink");
        let victim = dir.0.join("victim.png");
        std::fs::write(&victim, b"keep").unwrap();
        let link = dir.0.join("link.png");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        assert_eq!(write_image(&link, b"x").unwrap_err().code, "invalid");
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    }
}
