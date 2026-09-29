//! The `chord-avatar` URI scheme: `<img src>` shows a stored avatar.
//!
//! The path holds one key. The key is either an image hash (40 hex characters, the id
//! that the view items carry) or an owner (a JID, or `service/node` for a space). The
//! UI builds the URL with `avatarUrl` in src/lib/chord/avatars.ts.
//!
//! The scheme reads the database with its own connection. It never waits for the actor,
//! and it works offline. The scheme serves only `image/*` types.

use std::sync::{Arc, Mutex};

use chord_core::features::avatars;
use chord_core::store::Store;
use percent_encoding::percent_decode_str;
use rusqlite::{OptionalExtension, params};
use tauri::Manager;
use tauri::http::{Response, StatusCode, header};

use crate::state::{AppState, lock};

pub const SCHEME: &str = "chord-avatar";

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
    // An avatar comes from another user. Never serve it as a page or a script.
    if !image.mime.starts_with("image/") {
        return status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let cache = if image.immutable {
        "max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, image.mime)
        .header(header::CACHE_CONTROL, cache)
        .header("X-Content-Type-Options", "nosniff")
        .body(image.data)
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

    #[test]
    fn the_response_serves_images_only() {
        let (store, id) = store_with(&[
            ("bob@example.org", HASH, Some("image/png"), Some(&[1])),
            (
                "eve@example.org",
                "f".repeat(40).as_str(),
                Some("text/html"),
                Some(&[2]),
            ),
        ]);
        let store = Mutex::new((store, id));
        let ok = respond(&store, "/bob%40example.org");
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(ok.body(), &vec![1]);
        assert_eq!(
            respond(&store, "/eve%40example.org").status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        assert_eq!(respond(&store, "/none").status(), StatusCode::NOT_FOUND);
    }
}
