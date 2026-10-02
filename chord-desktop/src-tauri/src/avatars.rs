//! The `chord-avatar` URI scheme: `<img src>` shows a stored avatar.
//!
//! The path holds one key. The key is either an image hash (40 hex characters, the id
//! that the view items carry) or an owner (a JID, or `service/node` for a space). The
//! UI builds the URL with `avatarUrl` in src/lib/chord/avatars.ts.
//!
//! The scheme reads the database with its own connection. It never waits for the actor,
//! and it works offline. The scheme serves only png, jpeg, gif and webp. It reads the type
//! from the bytes of the image, never from the type that a peer gave. A page in the scheme
//! runs no script and loads nothing (a `Content-Security-Policy` with a sandbox).

use std::sync::{Arc, Mutex};

use chord_core::features::avatars;
use chord_core::store::Store;
use percent_encoding::percent_decode_str;
use rusqlite::{OptionalExtension, params};
use tauri::Manager;
use tauri::http::{Response, StatusCode, header};

use crate::state::{AppState, lock};

pub const SCHEME: &str = "chord-avatar";

/// The longest side of a served avatar, in pixels. The page shows an avatar at 128 pixels
/// or less, so 256 stays sharp on a high-density screen. A bigger image is shrunk, and the
/// webview then decodes fewer pixels.
const MAX_AVATAR_SIDE: u32 = 256;

/// An avatar image and its type.
#[derive(Debug, PartialEq, Eq)]
pub struct Image {
    pub mime: String,
    pub data: Vec<u8>,
    /// True if the key was a hash: the content behind it never changes.
    pub immutable: bool,
}

fn is_hash(key: &str) -> bool {
    key.len() == 40 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Find the image for `key`. An avatar with no data yet is not found.
pub fn lookup(store: &Store, account_id: i64, key: &str) -> Option<Image> {
    if is_hash(key) {
        let hash = key.to_ascii_lowercase();
        let row: Option<(Option<String>, Vec<u8>)> = store
            .conn()
            .query_row(
                "SELECT mime, data FROM avatars
                 WHERE account_id = ?1 AND hash = ?2 AND data IS NOT NULL LIMIT 1",
                params![account_id, hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .ok()?;
        let (mime, data) = row?;
        return Some(Image {
            mime: mime.unwrap_or_default(),
            data,
            immutable: true,
        });
    }
    let avatar = avatars::load_key(store, account_id, key).ok()??;
    Some(Image {
        mime: avatar.mime.unwrap_or_default(),
        data: avatar.data?,
        immutable: false,
    })
}

/// Build the response for one request path, for example `/amy%40example.org`.
pub fn respond(store: &Mutex<(Store, i64)>, path: &str) -> Response<Vec<u8>> {
    let key = percent_decode_str(path.trim_start_matches('/')).decode_utf8_lossy();
    let image = {
        let guard = lock(store);
        lookup(&guard.0, guard.1, &key)
    };
    let Some(image) = image else {
        return status(StatusCode::NOT_FOUND);
    };
    // An avatar comes from another user. Never serve it as a page or a script, and never
    // as SVG. The type comes from the bytes: the stored type is what a peer wrote.
    let Some(mime) = avatars::sniff_mime(&image.data) else {
        return status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    };
    let (mime, data) = match crate::thumb::shrink(&image.data, MAX_AVATAR_SIDE) {
        Some(small) => (small.mime, small.bytes),
        None => (mime, image.data),
    };
    let cache = if image.immutable {
        "max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache)
        .header("X-Content-Type-Options", "nosniff")
        // Opened as a page, an avatar can run nothing and load nothing.
        .header("Content-Security-Policy", "default-src 'none'; sandbox")
        // The profile card reads the pixels for its banner colour. A canvas can do that
        // only for an image with CORS.
        .header("Access-Control-Allow-Origin", "*")
        .body(data)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(Vec::new())
        .unwrap_or_default()
}

/// Register the scheme. It answers "not found" while no account is open.
pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(SCHEME, |ctx, request, responder| {
        let store = ctx
            .app_handle()
            .try_state::<AppState>()
            .and_then(|state| state.client().ok())
            .map(|client| Arc::clone(&client.avatars));
        let path = request.uri().path().to_owned();
        tauri::async_runtime::spawn_blocking(move || {
            let response = match store {
                Some(store) => respond(&store, &path),
                None => status(StatusCode::NOT_FOUND),
            };
            responder.respond(response);
        });
    })
}

#[cfg(test)]
mod tests {
    use chord_core::store::queries;

    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    /// owner, hash, mime, data.
    type Row<'a> = (&'a str, &'a str, Option<&'a str>, Option<&'a [u8]>);

    fn store_with(rows: &[Row<'_>]) -> (Store, i64) {
        let store = Store::open_in_memory().unwrap();
        let account_id = queries::ensure_account(store.conn(), "amy@example.org").unwrap();
        for (owner, hash, mime, data) in rows {
            store
                .conn()
                .execute(
                    "INSERT INTO avatars (account_id, owner, hash, mime, data)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![account_id, owner, hash, mime, data],
                )
                .unwrap();
        }
        (store, account_id)
    }

    #[test]
    fn finds_an_image_by_owner_and_by_hash() {
        let png: &[u8] = &[1, 2, 3];
        let (store, id) = store_with(&[("bob@example.org", HASH, Some("image/png"), Some(png))]);
        let by_owner = lookup(&store, id, "bob@example.org").unwrap();
        assert_eq!(by_owner.data, png);
        assert!(!by_owner.immutable);
        let by_hash = lookup(&store, id, &HASH.to_uppercase()).unwrap();
        assert_eq!(by_hash.mime, "image/png");
        assert!(by_hash.immutable);
        assert!(lookup(&store, id, "nobody@example.org").is_none());
    }

    #[test]
    fn a_space_owner_has_a_slash() {
        let (store, id) = store_with(&[(
            "pubsub.example.org/space1",
            HASH,
            Some("image/png"),
            Some(&[9]),
        )]);
        assert!(lookup(&store, id, "pubsub.example.org/space1").is_some());
    }

    #[test]
    fn an_avatar_without_data_is_not_found() {
        let (store, id) = store_with(&[("bob@example.org", HASH, Some("image/png"), None)]);
        assert!(lookup(&store, id, "bob@example.org").is_none());
        assert!(lookup(&store, id, HASH).is_none());
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n1234";

    #[test]
    fn the_response_serves_images_only() {
        let (store, id) = store_with(&[
            ("bob@example.org", HASH, Some("image/png"), Some(PNG)),
            (
                "eve@example.org",
                "f".repeat(40).as_str(),
                Some("text/html"),
                Some(b"<html>"),
            ),
        ]);
        let store = Mutex::new((store, id));
        let ok = respond(&store, "/bob%40example.org");
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/png");
        // The profile card reads the pixels of the avatar for its banner colour.
        assert_eq!(ok.headers()["Access-Control-Allow-Origin"], "*");
        assert_eq!(
            ok.headers()["Content-Security-Policy"],
            "default-src 'none'; sandbox"
        );
        assert_eq!(ok.body(), &PNG.to_vec());
        assert_eq!(
            respond(&store, "/eve%40example.org").status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        assert_eq!(respond(&store, "/none").status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn svg_is_never_served_whatever_the_stored_type() {
        let svg: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg'><script>1</script></svg>";
        let (store, id) = store_with(&[
            ("a@example.org", HASH, Some("image/svg+xml"), Some(svg)),
            // A peer that calls an SVG a PNG.
            (
                "b@example.org",
                "e".repeat(40).as_str(),
                Some("image/png"),
                Some(svg),
            ),
        ]);
        let store = Mutex::new((store, id));
        for owner in ["a", "b"] {
            let response = respond(&store, &format!("/{owner}%40example.org"));
            assert_eq!(
                response.status(),
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "{owner}"
            );
        }
    }

    #[test]
    fn the_type_comes_from_the_bytes() {
        // A JPEG that the peer called a GIF is served as a JPEG.
        let jpeg: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 0, 0];
        let (store, id) = store_with(&[("a@example.org", HASH, Some("image/gif"), Some(jpeg))]);
        let store = Mutex::new((store, id));
        let response = respond(&store, "/a%40example.org");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    }

    #[test]
    fn a_big_avatar_is_served_smaller() {
        let mut png = std::io::Cursor::new(Vec::new());
        let mut seed = 7u32;
        image::RgbaImage::from_fn(512, 512, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let b = seed.to_be_bytes();
            image::Rgba([b[0], b[1], b[2], 255])
        })
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let png = png.into_inner();
        let (store, id) = store_with(&[("a@example.org", HASH, Some("image/png"), Some(&png))]);
        let store = Mutex::new((store, id));
        let response = respond(&store, "/a%40example.org");
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.body().len() < png.len());
        let small = image::load_from_memory(response.body()).unwrap();
        assert_eq!((small.width(), small.height()), (256, 256));
    }
}
