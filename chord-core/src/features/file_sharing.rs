//! Stateless file sharing (XEP-0447) with file metadata (XEP-0446).
//!
//! An upload sends `<file-sharing>` with the name, size, media type and SHA-256 hash of
//! the file and its HTTP source, next to the XEP-0066 link and the URL in the body. Old
//! clients keep reading those. An incoming message gets its metadata read, and stored with
//! the message, so that the UI can show the file name and size.

use xmpp_parsers::hashes::{Algo, Hash};
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;

use crate::store::queries::FileMeta;

pub const NS_SFS: &str = "urn:xmpp:sfs:0";
const NS_FILE: &str = "urn:xmpp:file:metadata:0";
const NS_HASHES: &str = "urn:xmpp:hashes:2";
const NS_URL_DATA: &str = "http://jabber.org/protocol/url-data";

/// The longest file name that we keep. A peer can send any text.
const NAME_LIMIT: usize = 255;

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

/// The `<file-sharing>` element for a file that is at `url`.
pub(crate) fn build(meta: &FileMeta, url: &str) -> Element {
    let mut file = Element::builder("file", NS_FILE);
    if let Some(media_type) = &meta.media_type {
        file = file.append(Element::builder("media-type", NS_FILE).append(media_type.as_str()));
    }
    if let Some(name) = &meta.name {
        file = file.append(Element::builder("name", NS_FILE).append(name.as_str()));
    }
    if let Some(size) = meta.size {
        file = file.append(Element::builder("size", NS_FILE).append(size.to_string()));
    }
    if let Some(hash) = &meta.sha256 {
        file = file.append(
            Element::builder("hash", NS_HASHES)
                .attr(nc("algo"), "sha-256")
                .append(hash.as_str()),
        );
    }
    let source = Element::builder("url-data", NS_URL_DATA).attr(nc("target"), url);
    Element::builder("file-sharing", NS_SFS)
        .append(file)
        .append(Element::builder("sources", NS_SFS).append(source))
        .build()
}

/// The base64 text of a SHA-256 digest, as XEP-0300 has it.
pub(crate) fn sha256_base64(digest: &[u8]) -> String {
    Hash::new(Algo::Sha_256, digest.to_vec()).to_base64()
}

/// The metadata and the first source URL of the `<file-sharing>` of a message.
pub(crate) fn parse(message: &Message) -> Option<(FileMeta, Option<String>)> {
    let sfs = message
        .payloads
        .iter()
        .find(|p| p.is("file-sharing", NS_SFS))?;
    let file = sfs.get_child("file", NS_FILE);
    let text = |name: &str| {
        let t = file?.get_child(name, NS_FILE)?.text();
        let t = t.trim();
        (!t.is_empty()).then(|| t.to_owned())
    };
    let meta = FileMeta {
        name: text("name")
            .map(|n| clean_name(&n))
            .filter(|n| !n.is_empty()),
        size: text("size").and_then(|s| s.parse().ok()),
        media_type: text("media-type"),
        sha256: file.and_then(|f| {
            f.children()
                .find(|h| h.is("hash", NS_HASHES) && h.attr("algo") == Some("sha-256"))
                .map(|h| h.text().trim().to_owned())
        }),
    };
    let url = sfs
        .get_child("sources", NS_SFS)
        .into_iter()
        .flat_map(|s| s.children())
        .filter(|u| u.is("url-data", NS_URL_DATA))
        .filter_map(|u| u.attr("target"))
        .find(|t| t.starts_with("https://") || t.starts_with("http://"))
        .map(str::to_owned);
    Some((meta, url))
}

/// A file name for display: the last path part, without control characters, cut short.
fn clean_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    last.chars()
        .filter(|c| !c.is_control())
        .take(NAME_LIMIT)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> FileMeta {
        FileMeta {
            name: Some("cat.png".into()),
            size: Some(1234),
            media_type: Some("image/png".into()),
            sha256: Some(sha256_base64(&[7; 32])),
        }
    }

    fn message_with(payload: Element) -> Message {
        let mut m = Message::chat(None);
        m.payloads.push(payload);
        m
    }

    #[test]
    fn build_has_metadata_hash_and_source() {
        let e = build(&meta(), "https://up.example/get/cat.png");
        assert!(e.is("file-sharing", NS_SFS));
        let file = e.get_child("file", NS_FILE).unwrap();
        assert_eq!(file.get_child("name", NS_FILE).unwrap().text(), "cat.png");
        assert_eq!(file.get_child("size", NS_FILE).unwrap().text(), "1234");
        assert_eq!(
            file.get_child("media-type", NS_FILE).unwrap().text(),
            "image/png"
        );
        let hash = file.get_child("hash", NS_HASHES).unwrap();
        assert_eq!(hash.attr("algo"), Some("sha-256"));
        assert_eq!(hash.text(), sha256_base64(&[7; 32]));
        let source = e
            .get_child("sources", NS_SFS)
            .unwrap()
            .get_child("url-data", NS_URL_DATA)
            .unwrap();
        assert_eq!(
            source.attr("target"),
            Some("https://up.example/get/cat.png")
        );
    }

    #[test]
    fn what_we_build_we_parse() {
        let url = "https://up.example/get/cat.png";
        let (got, got_url) = parse(&message_with(build(&meta(), url))).unwrap();
        assert_eq!(got, meta());
        assert_eq!(got_url.as_deref(), Some(url));
    }

    #[test]
    fn parses_a_stanza_from_another_client() {
        let xml = "<file-sharing xmlns='urn:xmpp:sfs:0' disposition='inline'>\
            <file xmlns='urn:xmpp:file:metadata:0'><media-type>video/mp4</media-type>\
            <name>../../evil/clip.mp4</name><size>99</size>\
            <hash xmlns='urn:xmpp:hashes:2' algo='sha-1'>AAAA</hash></file>\
            <sources><url-data xmlns='http://jabber.org/protocol/url-data' target='ftp://x/y'/>\
            <url-data xmlns='http://jabber.org/protocol/url-data' target='https://h/clip.mp4'/>\
            </sources></file-sharing>";
        let (meta, url) = parse(&message_with(xml.parse().unwrap())).unwrap();
        assert_eq!(meta.name.as_deref(), Some("clip.mp4"));
        assert_eq!(meta.size, Some(99));
        assert_eq!(meta.media_type.as_deref(), Some("video/mp4"));
        // Only SHA-256 is kept.
        assert_eq!(meta.sha256, None);
        assert_eq!(url.as_deref(), Some("https://h/clip.mp4"));
    }

    #[test]
    fn bad_values_are_dropped() {
        let xml = "<file-sharing xmlns='urn:xmpp:sfs:0'>\
            <file xmlns='urn:xmpp:file:metadata:0'><size>big</size><name>  </name></file>\
            </file-sharing>";
        let (meta, url) = parse(&message_with(xml.parse().unwrap())).unwrap();
        assert_eq!(meta, FileMeta::default());
        assert_eq!(url, None);
    }

    #[test]
    fn a_message_without_sfs_has_none() {
        assert!(parse(&Message::chat(None)).is_none());
    }
}
