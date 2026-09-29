//! Spawn and timers. A thin wrapper over tokio in phase 1.
// Phase 2 adds a wasm version of this module.

#[cfg(feature = "native-session")]
use core::future::Future;

/// Spawns a task on the tokio runtime. The task runs detached.
#[cfg(feature = "native-session")]
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    drop(tokio::spawn(future));
}

/// Waits for the given duration.
#[cfg(feature = "native-session")]
pub async fn sleep(duration: core::time::Duration) {
    tokio::time::sleep(duration).await;
}

/// Give up on an upload after this long.
#[cfg(feature = "native-session")]
const HTTP_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(600);

/// Run an HTTP PUT. It follows no redirect, so a server cannot move the upload to a
/// plain http URL. Any 2xx status is a success.
#[cfg(feature = "native-session")]
pub async fn http_put(
    url: &str,
    headers: &[(String, String)],
    content_type: &str,
    body: Vec<u8>,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(core::time::Duration::from_secs(30))
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("cannot build the HTTP client: {e}"))?;
    let mut request = client
        .put(url)
        .header(reqwest::header::CONTENT_TYPE, content_type);
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let response = request
        .body(body)
        .send()
        .await
        .map_err(|e| format!("the upload failed: {e}"))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("the upload server answered {status}"))
    }
}

/// Give up on a download after this long.
#[cfg(feature = "native-session")]
const DOWNLOAD_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(60);

/// Accept an https URL, or an http URL to `localhost` or `127.0.0.1` (a dev server).
/// This is the rule of the upload path. A URL with user info is refused.
#[cfg(feature = "native-session")]
fn check_download_url(url: &str) -> Result<(), String> {
    let bad = || format!("unsafe download URL: {url}");
    let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // No user info: it can hide the real host.
    if authority.is_empty() || authority.contains('@') {
        return Err(bad());
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    let local = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1";
    if scheme.eq_ignore_ascii_case("https") || (scheme.eq_ignore_ascii_case("http") && local) {
        Ok(())
    } else {
        Err(bad())
    }
}

/// Check that a chunk fits under the cap. `have` is the size so far.
#[cfg(feature = "native-session")]
fn check_size(have: usize, chunk: usize, max_bytes: usize) -> Result<usize, String> {
    match have.checked_add(chunk) {
        Some(total) if total <= max_bytes => Ok(total),
        _ => Err(format!("the download is bigger than {max_bytes} bytes")),
    }
}

/// Run an HTTP GET and return the body. It follows no redirect. The download stops
/// with an error when the body grows over `max_bytes`.
#[cfg(feature = "native-session")]
pub async fn http_get(url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    check_download_url(url)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(core::time::Duration::from_secs(15))
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| format!("cannot build the HTTP client: {e}"))?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("the download failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the server answered {status}"));
    }
    // Refuse early when the server announces a big body.
    if let Some(length) = response.content_length() {
        check_size(0, usize::try_from(length).unwrap_or(usize::MAX), max_bytes)?;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("the download failed: {e}"))?
    {
        check_size(body.len(), chunk.len(), max_bytes)?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(all(test, feature = "native-session"))]
mod tests {
    use super::*;

    #[test]
    fn size_cap_is_enforced() {
        assert_eq!(check_size(0, 10, 10), Ok(10));
        assert_eq!(check_size(5, 5, 10), Ok(10));
        assert!(check_size(5, 6, 10).is_err());
        assert!(check_size(usize::MAX, 1, usize::MAX).is_err());
    }

    #[test]
    fn download_urls_follow_the_upload_rule() {
        for ok in [
            "https://a.example/x.png",
            "http://localhost:5280/x",
            "http://127.0.0.1/x",
        ] {
            assert!(check_download_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://a.example/x",
            "http://localhost@evil.example/x",
            "http://localhost.evil.example/x",
            "ftp://a.example/x",
            "https:///x",
            "nonsense",
        ] {
            assert!(check_download_url(bad).is_err(), "{bad}");
        }
    }
}
