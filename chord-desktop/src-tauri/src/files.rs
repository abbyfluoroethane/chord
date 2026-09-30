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

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::collections::HashSet;
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
            Err(ChordError::invalid("the file was not dropped on the window"))
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

/// Read the file that the user picked. One open, no symlink, a regular file, and the size
/// cap on the handle.
fn read_picked(path: &Path) -> Res<Vec<u8>> {
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
    // The file can still grow after the check: read at most one byte more than the cap.
    let mut data = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    file.take(MAX_UPLOAD_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| ChordError::io("cannot read the file", e))?;
    if data.len() as u64 > MAX_UPLOAD_BYTES {
        return Err(ChordError::invalid("the file is larger than the limit"));
    }
    Ok(data)
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
        let data = tauri::async_runtime::spawn_blocking(move || read_picked(&path))
            .await
            .map_err(|e| ChordError::io("the read task failed", e))??;
        let content_type = guess_content_type(&name).to_owned();
        urls.push(handle.upload(to.clone(), name, content_type, data).await?);
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
    let data = tauri::async_runtime::spawn_blocking(move || read_picked(&path))
        .await
        .map_err(|e| ChordError::io("the read task failed", e))??;
    let content_type = guess_content_type(&name).to_owned();
    Ok(handle.upload(to, name, content_type, data).await?)
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
    fn a_picked_file_is_read() {
        let dir = Scratch::new("read");
        let file = dir.0.join("a.txt");
        std::fs::write(&file, b"hello").unwrap();
        assert_eq!(read_picked(&file).unwrap(), b"hello");
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
        let error = read_picked(Path::new("/definitely/not/here")).unwrap_err();
        assert_eq!(error.code, "io");
        let dir = Scratch::new("dir");
        assert_eq!(read_picked(&dir.0).unwrap_err().code, "invalid");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_refused_for_reading() {
        let dir = Scratch::new("link");
        let secret = dir.0.join("secret");
        std::fs::write(&secret, b"key").unwrap();
        let link = dir.0.join("link");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        assert_eq!(read_picked(&link).unwrap_err().code, "invalid");
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
