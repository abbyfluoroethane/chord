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

/// The size of one chunk that the upload reads from a file.
#[cfg(feature = "native-session")]
const CHUNK: usize = 256 * 1024;

/// A body that a thread fills from a file, one chunk at a time. A few chunks wait in the
/// channel, so a slow server holds the reader back and the file is never all in memory.
#[cfg(feature = "native-session")]
struct FileChunks(tokio::sync::mpsc::Receiver<Result<Vec<u8>, std::io::Error>>);

#[cfg(feature = "native-session")]
impl futures_core::Stream for FileChunks {
    type Item = Result<Vec<u8>, std::io::Error>;

    fn poll_next(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

/// Read exactly `size` bytes of `file` in chunks, send them to `tx`, and return their
/// SHA-256. A file that ends early or cannot be read is an error.
#[cfg(feature = "native-session")]
fn read_chunks(
    file: std::fs::File,
    size: u64,
    tx: &tokio::sync::mpsc::Sender<Result<Vec<u8>, std::io::Error>>,
) -> Result<Vec<u8>, String> {
    use sha2::Digest;
    use std::io::Read;
    let mut file = file.take(size);
    let mut hasher = sha2::Sha256::new();
    let mut total = 0u64;
    loop {
        let mut chunk = Vec::new();
        let n = (&mut file)
            .take(CHUNK as u64)
            .read_to_end(&mut chunk)
            .map_err(|e| format!("cannot read the file: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk);
        total += n as u64;
        if tx.blocking_send(Ok(chunk)).is_err() {
            return Err("the upload stopped".into());
        }
    }
    if total != size {
        let e = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        let _ = tx.blocking_send(Err(e));
        return Err("the file got shorter while it was read".into());
    }
    Ok(hasher.finalize().to_vec())
}

/// Run an HTTP PUT. It follows no redirect, so a server cannot move the upload to a
/// plain http URL. Any 2xx status is a success. Returns the SHA-256 of the body, which
/// it computes as the bytes go out: one pass, and a file is never all in memory.
#[cfg(feature = "native-session")]
pub(crate) async fn http_put(
    url: &str,
    headers: &[(String, String)],
    content_type: &str,
    source: crate::features::upload::Source,
    size: u64,
) -> Result<Vec<u8>, String> {
    use crate::features::upload::Source;
    use sha2::Digest;
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
    let (body, reader) = match source {
        Source::Memory(data) => {
            let digest = sha2::Sha256::digest(&data).to_vec();
            (reqwest::Body::from(data), Err(digest))
        }
        Source::File(file) => {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            let reader = tokio::task::spawn_blocking(move || read_chunks(file, size, &tx));
            // A stream has no length: name it, or the server gets a chunked body.
            request = request.header(reqwest::header::CONTENT_LENGTH, size);
            (reqwest::Body::wrap_stream(FileChunks(rx)), Ok(reader))
        }
    };
    let sent = request.body(body).send().await;
    let response = match sent {
        Ok(response) => response,
        Err(e) => {
            // A file that changed under us is the cause, not the broken body.
            if let Ok(reader) = reader
                && let Ok(Err(reason)) = reader.await
                && !reason.starts_with("the upload stopped")
            {
                return Err(reason);
            }
            return Err(format!("the upload failed: {e}"));
        }
    };
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the upload server answered {status}"));
    }
    match reader {
        // The server can answer before it has read all bytes. Do not wait for the reader.
        Ok(reader) => {
            match tokio::time::timeout(core::time::Duration::from_secs(5), reader).await {
                Ok(Ok(digest)) => digest,
                Ok(Err(e)) => Err(format!("the read task failed: {e}")),
                Err(_) => Err("the server answered before it got the whole file".into()),
            }
        }
        Err(digest) => Ok(digest),
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

    /// A one-shot HTTP server on localhost. It reads one request, answers `status`, and
    /// returns the headers (lowercase) and the body of the request through the thread.
    fn serve_once(status: u16) -> (String, std::thread::JoinHandle<(String, Vec<u8>)>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/put",
            listener.local_addr().unwrap().port()
        );
        let thread = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut data = Vec::new();
            let mut buf = [0u8; 8192];
            let end = loop {
                let n = sock.read(&mut buf).unwrap();
                data.extend_from_slice(&buf[..n]);
                if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
            let length: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .map_or(0, |v| v.trim().parse().unwrap());
            let mut body = data[end..].to_vec();
            while body.len() < length {
                let n = sock.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
            }
            let reply =
                format!("HTTP/1.1 {status} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            let _ = sock.write_all(reply.as_bytes());
            (head, body)
        });
        (url, thread)
    }

    /// A file in the temp dir with `len` bytes of a pattern. The test removes it.
    fn scratch_file(tag: &str, len: usize) -> (std::path::PathBuf, Vec<u8>) {
        let data: Vec<u8> = (0..len).map(|i| (i * 7 % 253) as u8).collect();
        let path = std::env::temp_dir().join(format!("chord-put-{tag}-{}", std::process::id()));
        std::fs::write(&path, &data).unwrap();
        (path, data)
    }

    #[tokio::test]
    async fn a_file_streams_with_its_length_and_its_hash() {
        use crate::features::upload::Source;
        use sha2::Digest;
        // More than one chunk, and not a multiple of the chunk size.
        let (path, data) = scratch_file("stream", CHUNK * 3 + 17);
        let (url, server) = serve_once(201);
        let file = std::fs::File::open(&path).unwrap();
        let digest = http_put(
            &url,
            &[],
            "text/plain",
            Source::File(file),
            data.len() as u64,
        )
        .await
        .unwrap();
        let (head, body) = server.join().unwrap();
        assert_eq!(body, data);
        assert!(
            head.contains(&format!("content-length: {}", data.len())),
            "{head}"
        );
        assert!(!head.contains("transfer-encoding"), "{head}");
        assert_eq!(digest, sha2::Sha256::digest(&data).to_vec());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_memory_body_has_a_hash_too() {
        use crate::features::upload::Source;
        use sha2::Digest;
        let (url, server) = serve_once(200);
        let digest = http_put(&url, &[], "text/plain", Source::Memory(b"abc".to_vec()), 3)
            .await
            .unwrap();
        assert_eq!(server.join().unwrap().1, b"abc");
        assert_eq!(digest, sha2::Sha256::digest(b"abc").to_vec());
    }

    #[tokio::test]
    async fn a_file_that_got_shorter_fails_the_upload() {
        use crate::features::upload::Source;
        let (path, data) = scratch_file("short", 1000);
        let (url, server) = serve_once(200);
        let file = std::fs::File::open(&path).unwrap();
        // The caller says 5000 bytes, the file has 1000.
        let result = http_put(&url, &[], "text/plain", Source::File(file), 5000).await;
        assert_eq!(
            result.unwrap_err(),
            "the file got shorter while it was read"
        );
        drop(server);
        let _ = std::fs::remove_file(&path);
        assert_eq!(data.len(), 1000);
    }

    #[tokio::test]
    async fn an_error_status_fails_the_upload() {
        use crate::features::upload::Source;
        let (url, server) = serve_once(500);
        let result = http_put(&url, &[], "text/plain", Source::Memory(vec![1]), 1).await;
        assert!(result.unwrap_err().contains("500"));
        let _ = server.join();
    }

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
