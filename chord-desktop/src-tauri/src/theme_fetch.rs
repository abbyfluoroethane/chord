//! Fetch the CSS of a linked theme. The webview cannot do this itself, because its CSP
//! blocks other hosts. The fetch uses the same client as the link previews: public
//! addresses only, and every redirect hop is checked.

use reqwest::header::{ACCEPT, CONTENT_TYPE};
use url::Url;

use crate::error::{ChordError, Res};
use crate::link_preview::{client, validate_url};

/// The most bytes of a theme that we read: 256 KB.
const MAX_THEME_BYTES: usize = 256 * 1024;

fn fail(message: impl Into<String>) -> ChordError {
    ChordError::new("themeFetch", message)
}

/// True if the content type can hold CSS. A missing type is fine. Raw GitHub sends
/// `text/plain`, and some hosts send `application/octet-stream`.
fn text_type(content_type: Option<&str>) -> bool {
    let Some(value) = content_type else {
        return true;
    };
    let mime = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    (mime.starts_with("text/") && mime != "text/html")
        || matches!(mime.as_str(), "application/octet-stream" | "application/css")
}

/// Check a URL for a theme: https only, and the usual public-address checks.
fn check_url(text: &str) -> Res<Url> {
    let url = Url::parse(text.trim())
        .map_err(|e| ChordError::invalid(format!("not a URL ({text:?}): {e}")))?;
    if url.scheme() != "https" {
        return Err(ChordError::invalid("a theme link must start with https://"));
    }
    validate_url(&url)?;
    Ok(url)
}

/// Turn the bytes of a body into text. Fails for a body that is not UTF-8.
fn decode(body: Vec<u8>) -> Res<String> {
    String::from_utf8(body).map_err(|_| fail("the theme is not UTF-8 text"))
}

/// Download the CSS at `url`. The body must be text and 256 KB at most.
#[tauri::command]
pub async fn theme_fetch(url: String) -> Res<String> {
    let parsed = check_url(&url)?;
    let mut response = client()?
        .get(parsed)
        .header(ACCEPT, "text/css,text/plain;q=0.9,*/*;q=0.1")
        .send()
        .await
        .map_err(|e| fail(format!("cannot load the theme: {e}")))?;
    if !response.status().is_success() {
        return Err(fail(format!("the server answered {}", response.status())));
    }
    if response.url().scheme() != "https" {
        return Err(fail("the link redirects to a page without https"));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if !text_type(content_type.as_deref()) {
        return Err(fail("the link is not a CSS file"));
    }
    let too_big = || fail("the theme is larger than 256 KB");
    if response
        .content_length()
        .is_some_and(|n| n > MAX_THEME_BYTES as u64)
    {
        return Err(too_big());
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| fail(format!("cannot read the theme: {e}")))?
    {
        if body.len() + chunk.len() > MAX_THEME_BYTES {
            return Err(too_big());
        }
        body.extend_from_slice(&chunk);
    }
    decode(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_types_pass() {
        for t in [
            Some("text/css"),
            Some("text/css; charset=utf-8"),
            Some("text/plain; charset=utf-8"),
            Some("application/octet-stream"),
            Some("TEXT/CSS"),
            None,
        ] {
            assert!(text_type(t), "{t:?}");
        }
    }

    #[test]
    fn other_types_fail() {
        for t in [
            "text/html",
            "text/html; charset=utf-8",
            "image/png",
            "application/json",
            "application/zip",
        ] {
            assert!(!text_type(Some(t)), "{t}");
        }
    }

    #[test]
    fn only_https_passes() {
        assert!(check_url("https://example.com/a.css").is_ok());
        assert!(check_url("http://example.com/a.css").is_err());
        assert!(check_url("file:///etc/passwd").is_err());
        assert!(check_url("https://user:pw@example.com/a.css").is_err());
        assert!(check_url("https://127.0.0.1/a.css").is_err());
        assert!(check_url("https://[::1]/a.css").is_err());
        assert!(check_url("nonsense").is_err());
    }

    #[test]
    fn decode_needs_utf8() {
        assert_eq!(decode("a { }".as_bytes().to_vec()).unwrap(), "a { }");
        assert!(decode(vec![0xff, 0xfe, 0x00]).is_err());
    }
}
