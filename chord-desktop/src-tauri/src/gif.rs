//! GIF search with the KLIPY API (https://klipy.com). Tenor closed its API in June 2026.
//!
//! The requests go from Rust, not from the webview, so the API key stays out of the page
//! and every answer is checked here. The search text goes to KLIPY: the UI has a switch
//! to turn the GIF picker off. Chord sends no user ID, and asks for no ads.
//!
//! The API key comes from `CHORD_KLIPY_KEY`, at run time or at build time. Without a key,
//! the search fails with the code `gifUnavailable`. A key in the app can be read by anyone
//! who has the app (BRIDGESECURITY-07). So a build can set `CHORD_GIF_PROXY` to the https
//! address of a small server that holds the key and limits the rate. The app then sends
//! `{proxy}/gifs/{action}` with no key, and ships no key at all.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{ChordError, Res};

const API_ROOT: &str = "https://api.klipy.com/api/v1/";
/// The most bytes of one API answer: 2 MB.
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const PER_PAGE: u32 = 24;
const MAX_QUERY_CHARS: usize = 100;
const USER_AGENT: &str = "Chord/0.1 (GIF search)";

/// One file of a GIF, in one size.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GifFile {
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// One GIF: a small file for the picker grid and a large one to send.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Gif {
    pub slug: String,
    pub title: String,
    pub preview: GifFile,
    pub full: GifFile,
}

/// One page of results.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GifPage {
    pub items: Vec<Gif>,
    pub has_next: bool,
}

// ---------------------------------------------------------------- the answer

#[derive(Deserialize)]
struct Answer {
    result: bool,
    data: Option<RawPage>,
}

#[derive(Deserialize)]
struct RawPage {
    #[serde(default)]
    data: Vec<RawGif>,
    #[serde(default)]
    has_next: bool,
}

#[derive(Deserialize)]
struct RawGif {
    #[serde(rename = "type")]
    kind: Option<String>,
    slug: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    file: BTreeMap<String, RawFormats>,
}

#[derive(Deserialize)]
struct RawFormats {
    gif: Option<RawMedia>,
}

#[derive(Deserialize)]
struct RawMedia {
    url: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

/// Only an https URL with a host and no user info.
fn safe_media_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}

impl RawGif {
    /// The first size in `order` that has a safe GIF file.
    fn file(&self, order: [&str; 4]) -> Option<GifFile> {
        order.into_iter().find_map(|size| {
            let media = self.file.get(size)?.gif.as_ref()?;
            safe_media_url(&media.url).then(|| GifFile {
                url: media.url.clone(),
                width: media.width,
                height: media.height,
            })
        })
    }

    fn into_gif(self) -> Option<Gif> {
        // Chord asks for no ads, and drops any that come.
        if self.kind.as_deref().is_some_and(|k| k != "gif") {
            return None;
        }
        let slug = self.slug.clone()?;
        let preview = self.file(["sm", "xs", "md", "hd"])?;
        let full = self.file(["md", "hd", "sm", "xs"])?;
        Some(Gif {
            slug,
            title: self.title.chars().take(200).collect(),
            preview,
            full,
        })
    }
}

/// Read one API answer.
pub fn parse_page(bytes: &[u8]) -> Res<GifPage> {
    let answer: Answer = serde_json::from_slice(bytes).map_err(|e| {
        ChordError::new(
            "gif",
            format!("KLIPY sent an answer that Chord cannot read: {e}"),
        )
    })?;
    if !answer.result {
        return Err(ChordError::new("gif", "KLIPY could not do the search"));
    }
    let page = answer.data.unwrap_or(RawPage {
        data: Vec::new(),
        has_next: false,
    });
    Ok(GifPage {
        items: page.data.into_iter().filter_map(RawGif::into_gif).collect(),
        has_next: page.has_next,
    })
}

// ---------------------------------------------------------------- the request

/// Where the requests go, and the key to put in the path (none for a proxy). A proxy wins.
fn target() -> Res<(Url, Option<String>)> {
    let proxy = std::env::var("CHORD_GIF_PROXY")
        .ok()
        .or_else(|| option_env!("CHORD_GIF_PROXY").map(str::to_owned))
        .filter(|p| !p.trim().is_empty());
    match proxy {
        Some(p) => Ok((proxy_root(&p)?, None)),
        None => Ok((
            Url::parse(API_ROOT).expect("a valid static URL"),
            Some(api_key()?),
        )),
    }
}

/// The root of a proxy: an https URL with no user info.
fn proxy_root(text: &str) -> Res<Url> {
    let url = Url::parse(text.trim())
        .ok()
        .filter(|u| u.scheme() == "https" && u.host().is_some())
        .filter(|u| u.username().is_empty() && u.password().is_none())
        .ok_or_else(|| ChordError::new("gifUnavailable", "CHORD_GIF_PROXY is not an https URL"))?;
    Ok(url)
}

fn api_key() -> Res<String> {
    std::env::var("CHORD_KLIPY_KEY")
        .ok()
        .or_else(|| option_env!("CHORD_KLIPY_KEY").map(str::to_owned))
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            ChordError::new(
                "gifUnavailable",
                "GIF search has no KLIPY key in this build",
            )
        })
}

fn client() -> Res<&'static reqwest::Client> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(c) = CLIENT.get() {
        return Ok(c);
    }
    let built = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ChordError::new("gif", format!("cannot start the client: {e}")))?;
    Ok(CLIENT.get_or_init(|| built))
}

/// The API URL for `action`. The key is a path segment, so it is never in a log line.
/// A proxy has no key.
fn endpoint(root: &Url, key: Option<&str>, action: &str) -> Url {
    let mut url = root.clone();
    {
        let mut path = url
            .path_segments_mut()
            .expect("an https URL has path segments");
        path.pop_if_empty();
        if let Some(key) = key {
            path.push(key);
        }
        path.push("gifs").push(action);
    }
    url
}

/// The error for a failed request. It never shows the URL, which holds the key.
fn request_error(error: reqwest::Error) -> ChordError {
    let what = if error.is_timeout() {
        "timed out"
    } else {
        "failed"
    };
    ChordError::new("gif", format!("the GIF search {what}"))
}

async fn get_page(query: &str, page: u32) -> Res<GifPage> {
    let (root, key) = target()?;
    let query: String = query.trim().chars().take(MAX_QUERY_CHARS).collect();
    let action = if query.is_empty() {
        "trending"
    } else {
        "search"
    };
    let mut url = endpoint(&root, key.as_deref(), action);
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("page", &page.max(1).to_string())
            .append_pair("per_page", &PER_PAGE.to_string())
            .append_pair("format_filter", "gif");
        if !query.is_empty() {
            pairs.append_pair("q", &query);
        }
    }
    let mut response = client()?.get(url).send().await.map_err(request_error)?;
    match response.status().as_u16() {
        200..=299 => {}
        401 | 403 => {
            return Err(ChordError::new(
                "gifUnavailable",
                "KLIPY refused the API key",
            ));
        }
        429 => {
            return Err(ChordError::new(
                "gif",
                "too many GIF searches: wait a moment",
            ));
        }
        code => return Err(ChordError::new("gif", format!("KLIPY answered {code}"))),
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(request_error)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(ChordError::new("gif", "the KLIPY answer is too big"));
        }
        body.extend_from_slice(&chunk);
    }
    parse_page(&body)
}

/// Search GIFs, or get the trending ones for an empty query. `page` starts at 1.
#[tauri::command]
pub async fn gif_search(query: String, page: u32) -> Res<GifPage> {
    get_page(&query, page).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(url: &str) -> String {
        format!(r#"{{"gif":{{"url":"{url}","width":200,"height":100}}}}"#)
    }

    #[test]
    fn a_page_keeps_gifs_and_drops_ads_and_unsafe_urls() {
        let good = format!(
            r#"{{"type":"gif","slug":"wave-1","title":"Wave","file":{{"sm":{},"md":{}}}}}"#,
            media("https://static.klipy.com/sm.gif"),
            media("https://static.klipy.com/md.gif")
        );
        let ad = format!(
            r#"{{"type":"ad","slug":"ad-1","title":"Ad","file":{{"sm":{}}}}}"#,
            media("https://ads.example/a.gif")
        );
        let http = format!(
            r#"{{"type":"gif","slug":"plain-1","title":"Plain","file":{{"sm":{}}}}}"#,
            media("http://static.klipy.com/sm.gif")
        );
        let json =
            format!(r#"{{"result":true,"data":{{"data":[{good},{ad},{http}],"has_next":true}}}}"#);
        let page = parse_page(json.as_bytes()).unwrap();
        assert!(page.has_next);
        assert_eq!(page.items.len(), 1);
        let gif = &page.items[0];
        assert_eq!(gif.slug, "wave-1");
        assert_eq!(gif.preview.url, "https://static.klipy.com/sm.gif");
        assert_eq!(gif.full.url, "https://static.klipy.com/md.gif");
        assert_eq!((gif.full.width, gif.full.height), (200, 100));
    }

    #[test]
    fn a_failed_answer_is_an_error() {
        assert!(parse_page(br#"{"result":false}"#).is_err());
        assert!(parse_page(b"not json").is_err());
    }

    /// The real KLIPY API. It needs a key: run it with dev/klipy/.env loaded and
    /// `cargo test -p chord-desktop klipy_live -- --ignored`.
    #[test]
    #[ignore = "needs CHORD_KLIPY_KEY and the network"]
    fn klipy_live() {
        for query in ["", "wave"] {
            let page = tauri::async_runtime::block_on(get_page(query, 1)).unwrap();
            assert!(!page.items.is_empty(), "no GIFs for {query:?}");
            for gif in &page.items {
                assert!(gif.full.url.starts_with("https://"));
                assert!(gif.full.url.ends_with(".gif"), "{}", gif.full.url);
                assert!(gif.preview.width > 0 && gif.preview.height > 0);
            }
        }
    }

    #[test]
    fn the_key_is_a_path_segment() {
        let root = Url::parse(API_ROOT).unwrap();
        let url = endpoint(&root, Some("a/b c"), "search");
        assert_eq!(
            url.as_str(),
            "https://api.klipy.com/api/v1/a%2Fb%20c/gifs/search"
        );
    }

    #[test]
    fn a_proxy_gets_no_key() {
        let root = proxy_root("https://gif.example.org/v1/").unwrap();
        assert_eq!(
            endpoint(&root, None, "trending").as_str(),
            "https://gif.example.org/v1/gifs/trending"
        );
        let root = proxy_root("https://gif.example.org").unwrap();
        assert_eq!(
            endpoint(&root, None, "search").as_str(),
            "https://gif.example.org/gifs/search"
        );
    }

    #[test]
    fn a_proxy_needs_https_and_no_user_info() {
        for bad in [
            "http://gif.example.org",
            "https://u:p@gif.example.org",
            "ftp://gif.example.org",
            "gif.example.org",
            "",
        ] {
            assert!(proxy_root(bad).is_err(), "{bad}");
        }
    }
}
