//! Link previews: fetch a page from this computer and read its Open Graph tags.
//!
//! The site sees the IP address of the user, so the UI has a switch for it. The fetch
//! never reaches a private network. A custom DNS resolver drops every address that is
//! not public, so a DNS rebinding trick cannot bypass the check. A URL with an IP
//! literal is checked before the request, because the resolver does not see it. Each
//! redirect hop goes through the same URL check.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::redirect::Policy;
use scraper::{Html, Selector};
use serde::Serialize;
use url::{Host, Url};

use crate::error::{ChordError, Res};

/// The most bytes of a page that we read: 512 KB.
const MAX_BODY_BYTES: usize = 512 * 1024;
const MAX_REDIRECTS: usize = 3;
/// The largest image that `save_image` writes.
const MAX_IMAGE_BYTES: u64 = 50 * 1024 * 1024;
/// The largest preview image that `link_image` sends to the page: 8 MB.
const MAX_PREVIEW_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 400;
const CACHE_ENTRIES: usize = 256;
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);
const USER_AGENT: &str = "Chord/0.1 (link preview)";
/// The most page fetches at one time. A room with many URLs must not open many sockets.
const MAX_PARALLEL_FETCHES: usize = 4;

/// What the UI shows for one link. Every field but `url` can be missing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    /// The URL after the redirects.
    pub url: String,
    pub site_name: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// An absolute http or https URL.
    pub image: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
}

// ---------------------------------------------------------------- the IP filter

// The ranges live in chord-core, which the space avatar download shares.
#[cfg(test)]
use chord_core::ip_filter::is_public_ip;
use chord_core::ip_filter::{is_public_v4, is_public_v6, resolve_public};

/// Check a URL before the request. Only http and https, no user info, and an IP literal
/// must be public. A domain is checked by the resolver.
pub fn validate_url(url: &Url) -> Res<()> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ChordError::invalid(
            "a link preview needs an http or https URL",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ChordError::invalid(
            "a link preview URL cannot have user info",
        ));
    }
    match url.host() {
        None => Err(ChordError::invalid("the URL has no host")),
        Some(Host::Ipv4(ip)) if !is_public_v4(ip) => Err(not_public()),
        Some(Host::Ipv6(ip)) if !is_public_v6(ip) => Err(not_public()),
        Some(_) => Ok(()),
    }
}

fn not_public() -> ChordError {
    ChordError::invalid("the URL points to a private address")
}

/// The DNS resolver of the client. It drops the addresses that are not public.
struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let public = resolve_public(&host).await?;
            let addrs: Addrs = Box::new(public.into_iter());
            Ok(addrs)
        })
    }
}

// ---------------------------------------------------------------- the body

/// What the content type tells us to do with the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Html,
    Image,
    Other,
}

fn kind_of(content_type: Option<&str>) -> Kind {
    let mime = content_type
        .and_then(|c| c.split(';').next())
        .map(|m| m.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match mime.as_str() {
        "text/html" | "application/xhtml+xml" => Kind::Html,
        m if m.starts_with("image/") => Kind::Image,
        _ => Kind::Other,
    }
}

/// Add `chunk` to `buf` up to `max` bytes in all. Returns true when the buffer is full
/// and the caller must stop reading.
fn append_capped(buf: &mut Vec<u8>, chunk: &[u8], max: usize) -> bool {
    let room = max.saturating_sub(buf.len());
    buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
    buf.len() >= max
}

// ---------------------------------------------------------------- the metadata

/// Trim, collapse the white space, and cap at `max` characters. None if nothing is left.
fn clean(text: &str, max: usize) -> Option<String> {
    let mut out = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.is_empty() {
        return None;
    }
    if out.chars().count() > max {
        out = out.chars().take(max - 1).collect::<String>() + "…";
    }
    Some(out)
}

fn absolute_image(value: &str, base: &Url) -> Option<String> {
    let url = base.join(value.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

/// Read the preview from a page. `base` is the final URL of the page.
fn extract(html: &str, base: &Url) -> Option<LinkPreview> {
    let doc = Html::parse_document(html);
    let meta_sel = Selector::parse("meta").ok()?;
    let title_sel = Selector::parse("title").ok()?;

    // The first tag wins for each key. `property` is Open Graph, `name` is Twitter and HTML.
    let mut meta: HashMap<String, String> = HashMap::new();
    for el in doc.select(&meta_sel) {
        let v = el.value();
        let key = v.attr("property").or_else(|| v.attr("name"));
        if let (Some(key), Some(content)) = (key, v.attr("content")) {
            meta.entry(key.trim().to_ascii_lowercase())
                .or_insert_with(|| content.to_owned());
        }
    }
    let first = |keys: &[&str]| keys.iter().find_map(|k| meta.get(*k)).map(String::as_str);

    let page_title = doc
        .select(&title_sel)
        .next()
        .map(|t| t.text().collect::<String>());
    let title = first(&["og:title", "twitter:title"])
        .and_then(|t| clean(t, MAX_TITLE_CHARS))
        .or_else(|| page_title.and_then(|t| clean(&t, MAX_TITLE_CHARS)));
    let description = first(&["og:description", "twitter:description", "description"])
        .and_then(|d| clean(d, MAX_DESCRIPTION_CHARS));
    let image = first(&[
        "og:image",
        "og:image:url",
        "twitter:image",
        "twitter:image:src",
    ])
    .and_then(|i| absolute_image(i, base));
    let dimension = |key: &str| {
        image
            .as_ref()
            .and_then(|_| meta.get(key))
            .and_then(|n| n.trim().parse::<u32>().ok())
            .filter(|n| *n > 0)
    };
    let (image_width, image_height) = (dimension("og:image:width"), dimension("og:image:height"));

    if title.is_none() && description.is_none() && image.is_none() {
        return None;
    }
    let site_name = first(&["og:site_name"])
        .and_then(|s| clean(s, MAX_TITLE_CHARS))
        .or_else(|| base.host_str().map(str::to_owned));
    Some(LinkPreview {
        url: base.to_string(),
        site_name,
        title,
        description,
        image,
        image_width,
        image_height,
    })
}

// ---------------------------------------------------------------- the cache

/// A small cache with an age limit. The oldest entry leaves first when it is full.
struct Cache {
    map: HashMap<String, (Instant, Option<LinkPreview>)>,
    order: VecDeque<String>,
}

impl Cache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// `Some(entry)` on a hit, where the entry can be `None` (a cached miss).
    fn get(&mut self, key: &str, now: Instant) -> Option<Option<LinkPreview>> {
        match self.map.get(key) {
            Some((at, value)) if now.saturating_duration_since(*at) < CACHE_TTL => {
                Some(value.clone())
            }
            Some(_) => {
                self.remove(key);
                None
            }
            None => None,
        }
    }

    fn remove(&mut self, key: &str) {
        self.map.remove(key);
        self.order.retain(|k| k != key);
    }

    fn put(&mut self, key: String, value: Option<LinkPreview>, now: Instant) {
        self.remove(&key);
        while self.order.len() >= CACHE_ENTRIES {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, (now, value));
    }
}

struct Shared {
    cache: Mutex<Cache>,
    /// One lock for each URL that is in flight, so two requests fetch it once.
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared {
        cache: Mutex::new(Cache::new()),
        inflight: Mutex::new(HashMap::new()),
    })
}

/// The permits for the page fetches.
fn fetch_permits() -> &'static tokio::sync::Semaphore {
    static PERMITS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    PERMITS.get_or_init(|| tokio::sync::Semaphore::new(MAX_PARALLEL_FETCHES))
}

fn cached(key: &str) -> Option<Option<LinkPreview>> {
    shared().cache.lock().ok()?.get(key, Instant::now())
}

// ---------------------------------------------------------------- the fetch

/// The client for the page fetch: 10 seconds in all.
pub(crate) fn client() -> Res<&'static reqwest::Client> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    shared_client(&CLIENT, Duration::from_secs(10))
}

/// The client for the image download: the same filters, and more time for a big file.
fn download_client() -> Res<&'static reqwest::Client> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    shared_client(&CLIENT, Duration::from_secs(120))
}

fn shared_client(
    cell: &'static OnceLock<reqwest::Client>,
    timeout: Duration,
) -> Res<&'static reqwest::Client> {
    if let Some(c) = cell.get() {
        return Ok(c);
    }
    let policy = Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error("too many redirects")
        } else if let Err(e) = validate_url(attempt.url()) {
            attempt.error(e.message)
        } else {
            attempt.follow()
        }
    });
    let built = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .redirect(policy)
        // A proxy would do its own DNS lookup and skip the filter.
        .no_proxy()
        .dns_resolver(Arc::new(PublicOnly))
        .build()
        .map_err(|e| ChordError::new("linkPreview", format!("cannot start the client: {e}")))?;
    Ok(cell.get_or_init(|| built))
}

fn fetch_error(error: reqwest::Error) -> ChordError {
    ChordError::new("linkPreview", format!("cannot load the page: {error}"))
}

/// Get the page and read it. `Ok(None)` means the page has no preview.
async fn fetch(url: Url) -> Res<Option<LinkPreview>> {
    let mut response = client()?
        .get(url)
        .header(ACCEPT, "text/html")
        .send()
        .await
        .map_err(fetch_error)?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let base = response.url().clone();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    match kind_of(content_type.as_deref()) {
        Kind::Other => Ok(None),
        Kind::Image => Ok(Some(LinkPreview {
            url: base.to_string(),
            image: Some(base.to_string()),
            ..LinkPreview::default()
        })),
        Kind::Html => {
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(fetch_error)? {
                if append_capped(&mut body, &chunk, MAX_BODY_BYTES) {
                    break;
                }
            }
            Ok(extract(&String::from_utf8_lossy(&body), &base))
        }
    }
}

/// The preview of one link, or `None` if the page has none. Failures to connect are an
/// error and are not cached.
#[tauri::command]
pub async fn link_preview(url: String) -> Res<Option<LinkPreview>> {
    let parsed = Url::parse(url.trim())
        .map_err(|e| ChordError::invalid(format!("not a URL ({url:?}): {e}")))?;
    validate_url(&parsed)?;
    let key = parsed.to_string();
    if let Some(hit) = cached(&key) {
        return Ok(hit);
    }
    let lock = {
        let mut inflight = shared().inflight.lock().map_err(|_| poisoned())?;
        inflight.entry(key.clone()).or_default().clone()
    };
    let _guard = lock.lock().await;
    // Another request for the same URL can have finished while we waited.
    let result = match cached(&key) {
        Some(hit) => Ok(hit),
        None => {
            // The permit lives until the fetch ends. The semaphore is never closed.
            let _permit = fetch_permits().acquire().await.map_err(|_| poisoned())?;
            fetch(parsed).await.inspect(|value| {
                if let Ok(mut cache) = shared().cache.lock() {
                    cache.put(key.clone(), value.clone(), Instant::now());
                }
            })
        }
    };
    if let Ok(mut inflight) = shared().inflight.lock() {
        inflight.remove(&key);
    }
    result
}

/// True if `total` bytes so far plus a chunk of `chunk` bytes stay within `max`.
fn within_cap(total: u64, chunk: usize, max: u64) -> bool {
    total.checked_add(chunk as u64).is_some_and(|n| n <= max)
}

/// The first size of a download buffer. A known length gives one exact allocation, so the
/// buffer never doubles and copies. A wrong or missing length is safe: the buffer then grows
/// as before. The size never goes over `max`.
pub(crate) fn initial_capacity(content_length: Option<u64>, max: u64) -> usize {
    content_length
        .map_or(0, |n| n.min(max))
        .try_into()
        .unwrap_or(0)
}

/// Download the image at `url` and return its bytes. The download uses the same filters as
/// the previews: only public addresses, checked redirects. It fails for a file that is not
/// an image, and for one over 50 MB. The caller decides where the bytes go (files.rs).
pub async fn download_image(url: &str) -> Res<Vec<u8>> {
    download_capped(url, MAX_IMAGE_BYTES)
        .await
        .map(|(bytes, _)| bytes)
}

/// Download an image of at most `max` bytes. Returns the bytes and the media type.
async fn download_capped(url: &str, max: u64) -> Res<(Vec<u8>, String)> {
    let parsed = Url::parse(url.trim())
        .map_err(|e| ChordError::invalid(format!("not a URL ({url:?}): {e}")))?;
    validate_url(&parsed)?;
    let mut response = download_client()?
        .get(parsed)
        .header(ACCEPT, "image/*")
        .send()
        .await
        .map_err(fetch_error)?;
    if !response.status().is_success() {
        return Err(ChordError::new(
            "linkPreview",
            format!("the server answered {}", response.status()),
        ));
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if kind_of(content_type.as_deref()) != Kind::Image {
        return Err(ChordError::invalid("the link is not an image"));
    }
    let too_big = || {
        ChordError::invalid(format!(
            "the image is larger than {} MB",
            max / (1024 * 1024)
        ))
    };
    if response.content_length().is_some_and(|n| n > max) {
        return Err(too_big());
    }
    let mut body: Vec<u8> = Vec::with_capacity(initial_capacity(response.content_length(), max));
    while let Some(chunk) = response.chunk().await.map_err(fetch_error)? {
        if !within_cap(body.len() as u64, chunk.len(), max) {
            return Err(too_big());
        }
        body.extend_from_slice(&chunk);
    }
    let mime = essence(content_type.as_deref());
    Ok((body, mime))
}

/// The media type of a Content-Type header, in lower case and with no parameters.
fn essence(content_type: Option<&str>) -> String {
    content_type
        .and_then(|c| c.split(';').next())
        .map(|m| m.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

/// The image types that the UI may show from a link: no SVG, which can hold a script
/// when a page opens it on its own.
fn is_preview_image_type(mime: &str) -> bool {
    matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

/// `bytes` as a `data:` URL, or `None` if the type is not one that the UI shows. The text
/// grows in one allocation of the final size: no second copy of a big string.
fn data_url(mime: &str, bytes: &[u8]) -> Option<String> {
    use base64::Engine;
    if !is_preview_image_type(mime) {
        return None;
    }
    let prefix = format!("data:{mime};base64,");
    let mut out = String::with_capacity(prefix.len() + bytes.len().div_ceil(3) * 4);
    out.push_str(&prefix);
    base64::engine::general_purpose::STANDARD.encode_string(bytes, &mut out);
    Some(out)
}

/// The preview image of a link, as a `data:` URL. The webview never loads the address
/// from the page itself: that would skip the filter for private networks and would send
/// the IP address of the user to a host that the sender of the link picked. The download
/// has the same filters as the page fetch, takes 8 MB at most, and accepts PNG, JPEG,
/// GIF and WebP.
#[tauri::command]
pub async fn link_image(url: String) -> Res<String> {
    let (bytes, mime) = download_capped(&url, MAX_PREVIEW_IMAGE_BYTES).await?;
    data_url(&mime, &bytes).ok_or_else(|| ChordError::invalid("the image type is not supported"))
}

fn poisoned() -> ChordError {
    ChordError::new("linkPreview", "the preview state is damaged")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn a_data_url_uses_one_allocation() {
        let bytes = vec![7u8; 1000];
        let url = data_url("image/png", &bytes).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        // The capacity is the exact final size, so the string never grew.
        assert_eq!(url.capacity(), url.len());
        assert!(data_url("image/svg+xml", &bytes).is_none());
    }

    #[test]
    fn the_buffer_size_follows_the_length_and_the_cap() {
        assert_eq!(initial_capacity(None, 100), 0);
        assert_eq!(initial_capacity(Some(40), 100), 40);
        assert_eq!(initial_capacity(Some(4000), 100), 100);
    }

    #[test]
    fn public_addresses_pass() {
        for s in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.0.1",
            "172.32.0.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public_ip(ip(s)), "{s} must pass");
        }
    }

    #[test]
    fn private_v4_ranges_fail() {
        for s in [
            "127.0.0.1",
            "127.255.255.254",
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "224.0.0.1",
            "239.255.255.255",
            "255.255.255.255",
            "192.0.2.1",
            "198.51.100.7",
            "203.0.113.9",
            "198.18.0.1",
            "240.0.0.1",
        ] {
            assert!(!is_public_ip(ip(s)), "{s} must fail");
        }
    }

    #[test]
    fn private_v6_ranges_fail() {
        for s in [
            "::",
            "::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "febf::1",
            "fec0::1",
            "ff02::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(ip(s)), "{s} must fail");
        }
    }

    #[test]
    fn mapped_and_nat64_addresses_follow_their_v4() {
        for s in [
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.3",
            "::ffff:192.168.0.1",
            "::ffff:169.254.1.1",
            "::ffff:100.64.0.1",
            "::ffff:0.0.0.0",
            "64:ff9b::7f00:1",
            "64:ff9b::a00:1",
        ] {
            assert!(!is_public_ip(ip(s)), "{s} must fail");
        }
        assert!(is_public_ip(ip("64:ff9b::808:808")));
    }

    #[test]
    fn urls_need_http_and_no_user_info() {
        assert!(validate_url(&url("https://example.org/a?b=c")).is_ok());
        assert!(validate_url(&url("http://example.org:8080/")).is_ok());
        assert!(validate_url(&url("http://8.8.8.8/")).is_ok());
        assert!(validate_url(&url("http://[2606:4700:4700::1111]/")).is_ok());
        for bad in [
            "ftp://example.org/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hi",
            "https://user@example.org/",
            "https://user:pw@example.org/",
            "http://127.0.0.1/",
            "http://10.0.0.1:8080/",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/",
            "http://[::ffff:192.168.1.1]/",
            "http://[fd00::1]/",
            "http://2130706433/",
        ] {
            assert!(validate_url(&url(bad)).is_err(), "{bad} must fail");
        }
    }

    #[test]
    fn the_content_type_picks_the_action() {
        assert_eq!(kind_of(Some("text/html")), Kind::Html);
        assert_eq!(kind_of(Some("Text/HTML; charset=UTF-8")), Kind::Html);
        assert_eq!(kind_of(Some("application/xhtml+xml")), Kind::Html);
        assert_eq!(kind_of(Some("image/png")), Kind::Image);
        assert_eq!(kind_of(Some("image/svg+xml; x=y")), Kind::Image);
        assert_eq!(kind_of(Some("application/json")), Kind::Other);
        assert_eq!(kind_of(Some("text/plain")), Kind::Other);
        assert_eq!(kind_of(None), Kind::Other);
    }

    #[test]
    fn a_download_stops_at_the_image_cap() {
        assert!(within_cap(0, 10, 10));
        assert!(!within_cap(1, 10, 10));
        assert!(within_cap(MAX_IMAGE_BYTES - 5, 5, MAX_IMAGE_BYTES));
        assert!(!within_cap(MAX_IMAGE_BYTES, 1, MAX_IMAGE_BYTES));
        assert!(!within_cap(u64::MAX, 1, MAX_IMAGE_BYTES));
    }

    #[test]
    fn only_raster_images_become_data_urls() {
        assert_eq!(
            data_url("image/png", b"abc").as_deref(),
            Some("data:image/png;base64,YWJj")
        );
        for mime in ["image/svg+xml", "text/html", "image/x-icon", ""] {
            assert_eq!(data_url(mime, b"abc"), None, "{mime}");
        }
        assert_eq!(
            essence(Some("Image/PNG; charset=binary")),
            "image/png".to_string()
        );
    }

    #[test]
    fn link_image_refuses_bad_urls() {
        let run = |u: &str| tauri::async_runtime::block_on(link_image(u.to_owned()));
        assert!(run("http://127.0.0.1/a.png").is_err());
        assert!(run("http://[::1]/a.png").is_err());
        assert!(run("file:///etc/passwd").is_err());
        assert!(run("https://user:pw@example.org/a.png").is_err());
    }

    #[test]
    fn download_image_refuses_bad_urls() {
        let run = |u: &str| tauri::async_runtime::block_on(download_image(u));
        assert!(run("http://127.0.0.1/a.png").is_err());
        assert!(run("file:///etc/passwd").is_err());
    }

    #[test]
    fn the_body_stops_at_the_cap() {
        let mut buf = Vec::new();
        assert!(!append_capped(&mut buf, b"abc", 8));
        assert_eq!(buf, b"abc");
        assert!(!append_capped(&mut buf, b"defg", 8));
        assert_eq!(buf.len(), 7);
        assert!(append_capped(&mut buf, b"hijkl", 8));
        assert_eq!(buf, b"abcdefgh");
        assert!(
            append_capped(&mut buf, b"more", 8),
            "a full buffer stays full"
        );
        assert_eq!(buf.len(), 8);
        let mut exact = Vec::new();
        assert!(append_capped(&mut exact, b"12345678", 8));
    }

    #[test]
    fn open_graph_tags_win() {
        let html = r#"<html><head>
            <title>Page title</title>
            <meta property="og:title" content="OG title">
            <meta name="twitter:title" content="Twitter title">
            <meta property="og:description" content="OG description">
            <meta name="description" content="Plain description">
            <meta property="og:site_name" content="Example">
            <meta property="og:image" content="https://cdn.example.org/a.png">
            <meta property="og:image:width" content="1200">
            <meta property="og:image:height" content="630">
            </head><body></body></html>"#;
        let p = extract(html, &url("https://example.org/post")).unwrap();
        assert_eq!(p.url, "https://example.org/post");
        assert_eq!(p.title.as_deref(), Some("OG title"));
        assert_eq!(p.description.as_deref(), Some("OG description"));
        assert_eq!(p.site_name.as_deref(), Some("Example"));
        assert_eq!(p.image.as_deref(), Some("https://cdn.example.org/a.png"));
        assert_eq!((p.image_width, p.image_height), (Some(1200), Some(630)));
    }

    #[test]
    fn twitter_and_title_are_fallbacks() {
        let html = r#"<head><title> Only  title </title>
            <meta name="twitter:description" content="Tw desc">
            <meta name="twitter:image" content="https://i.example.org/t.jpg"></head>"#;
        let p = extract(html, &url("https://example.org/")).unwrap();
        assert_eq!(p.title.as_deref(), Some("Only title"));
        assert_eq!(p.description.as_deref(), Some("Tw desc"));
        assert_eq!(p.image.as_deref(), Some("https://i.example.org/t.jpg"));
        let html = r#"<title>T</title><meta name="twitter:title" content="TT">
            <meta name="description" content="Meta desc">"#;
        let p = extract(html, &url("https://example.org/")).unwrap();
        assert_eq!(p.title.as_deref(), Some("TT"));
        assert_eq!(p.description.as_deref(), Some("Meta desc"));
    }

    #[test]
    fn the_site_name_falls_back_to_the_host() {
        let p = extract("<title>T</title>", &url("https://blog.example.org/x")).unwrap();
        assert_eq!(p.site_name.as_deref(), Some("blog.example.org"));
    }

    #[test]
    fn a_relative_image_uses_the_final_url() {
        let base = url("https://example.org/a/b/post");
        let rel = |v: &str| {
            let html = format!(r#"<title>T</title><meta property="og:image" content="{v}">"#);
            extract(&html, &base).unwrap().image
        };
        assert_eq!(
            rel("/img/x.png").as_deref(),
            Some("https://example.org/img/x.png")
        );
        assert_eq!(
            rel("x.png").as_deref(),
            Some("https://example.org/a/b/x.png")
        );
        assert_eq!(
            rel("//cdn.example.org/x.png").as_deref(),
            Some("https://cdn.example.org/x.png")
        );
        assert_eq!(rel("javascript:alert(1)"), None);
        assert_eq!(rel("data:image/png;base64,AAAA"), None);
    }

    #[test]
    fn white_space_and_length_are_cleaned() {
        assert_eq!(clean("  a \n\t b   c ", 50).as_deref(), Some("a b c"));
        assert_eq!(clean(" \n ", 50), None);
        let long = "x".repeat(500);
        let title = clean(&long, MAX_TITLE_CHARS).unwrap();
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert!(title.ends_with('…'));
        assert_eq!(clean(&"y".repeat(200), 200).unwrap().chars().count(), 200);
        let html = format!(
            r#"<meta property="og:title" content="{long}"><meta property="og:description" content="{long}">"#
        );
        let p = extract(&html, &url("https://example.org/")).unwrap();
        assert_eq!(p.title.unwrap().chars().count(), MAX_TITLE_CHARS);
        assert_eq!(
            p.description.unwrap().chars().count(),
            MAX_DESCRIPTION_CHARS
        );
    }

    #[test]
    fn entities_are_decoded_and_empty_pages_give_none() {
        let p = extract(
            r#"<meta property="og:title" content="Tom &amp; Jerry &#39;s">"#,
            &url("https://example.org/"),
        )
        .unwrap();
        assert_eq!(p.title.as_deref(), Some("Tom & Jerry 's"));
        assert_eq!(
            extract("<html><body>hi</body></html>", &url("https://example.org/")),
            None
        );
    }

    #[test]
    fn image_size_needs_an_image() {
        let html = r#"<title>T</title><meta property="og:image:width" content="800">"#;
        let p = extract(html, &url("https://example.org/")).unwrap();
        assert_eq!(p.image_width, None);
    }

    #[test]
    fn the_json_is_camel_case() {
        let json = serde_json::to_value(LinkPreview {
            url: "https://a.b/".into(),
            site_name: Some("A".into()),
            image_width: Some(5),
            ..LinkPreview::default()
        })
        .unwrap();
        assert_eq!(json["siteName"], "A");
        assert_eq!(json["imageWidth"], 5);
        assert!(json["title"].is_null());
    }

    #[test]
    fn the_cache_expires_and_evicts_the_oldest() {
        let t0 = Instant::now();
        let mut cache = Cache::new();
        assert_eq!(cache.get("a", t0), None);
        cache.put("a".into(), None, t0);
        assert_eq!(cache.get("a", t0), Some(None), "a miss is cached too");
        assert_eq!(
            cache.get("a", t0 + CACHE_TTL),
            None,
            "one hour is the limit"
        );
        assert!(cache.map.is_empty() && cache.order.is_empty());
        for n in 0..CACHE_ENTRIES + 10 {
            cache.put(format!("k{n}"), None, t0);
        }
        assert_eq!(cache.map.len(), CACHE_ENTRIES);
        assert_eq!(cache.order.len(), CACHE_ENTRIES);
        assert_eq!(cache.get("k0", t0), None);
        assert_eq!(cache.get("k10", t0), Some(None));
        cache.put("k10".into(), None, t0);
        assert_eq!(
            cache.order.len(),
            CACHE_ENTRIES,
            "a second put does not grow it"
        );
    }

    #[test]
    fn embedded_private_v4_in_v6_is_not_public() {
        use std::str::FromStr;
        for text in [
            "::127.0.0.1",
            "::10.0.0.1",
            "2002:c0a8:0101::1",
            "2002:7f00:0001::",
        ] {
            let ip = IpAddr::from_str(text).unwrap();
            assert!(!is_public_ip(ip), "{text}");
        }
        // A 6to4 address of a public IPv4 host stays public.
        assert!(is_public_ip(IpAddr::from_str("2002:0808:0808::1").unwrap()));
    }

    #[test]
    fn the_extra_ranges_and_odd_hosts_are_refused() {
        for text in [
            "2001::1",
            "64:ff9b:1::1",
            "100::1",
            "192.88.99.1",
            "192.31.196.1",
        ] {
            assert!(!is_public_ip(ip(text)), "{text}");
        }
        // A bracketed mapped loopback.
        assert!(validate_url(&url("http://[::ffff:7f00:1]/")).is_err());
        assert!(validate_url(&url("http://[::ffff:127.0.0.1]/")).is_err());
        assert!(validate_url(&url("http://[2001::1]/")).is_err());
    }

    #[tokio::test]
    async fn a_trailing_dot_host_does_not_reach_the_loopback() {
        assert!(resolve_public("localhost.").await.is_err());
        assert!(link_preview("http://localhost./".into()).await.is_err());
    }

    #[test]
    fn the_fetches_have_a_cap_of_four() {
        assert_eq!(fetch_permits().available_permits(), MAX_PARALLEL_FETCHES);
        assert_eq!(MAX_PARALLEL_FETCHES, 4);
    }
}
