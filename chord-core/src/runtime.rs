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
