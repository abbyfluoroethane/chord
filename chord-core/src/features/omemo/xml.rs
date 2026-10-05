//! The XML of OMEMO 2 (XEP-0384) and of the XEP-0420 envelope. No cryptography here.
//!
//! - `DeviceList` and `BundleXml` read and write the PEP payloads.
//! - `Encrypted` reads and writes the `<encrypted/>` element of a message. Keys and
//!   payloads are raw bytes. The crypto layer fills them.
//! - `wrap` and `Encrypted::find` put the element in a message and take it out again.
//! - `Envelope` is the plain text that OMEMO encrypts (XEP-0420, with the OMEMO profile).
//! - The `*_iq` functions build the PEP requests. The `parse_*` functions read the answers
//!   and the events.
//!
//! No function here panics on bad input. A parse error is an `Error`.

// Nothing calls this module yet.
#![allow(dead_code)]

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use jid::{BareJid, Jid};
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field};
use xmpp_parsers::date::DateTime;
use xmpp_parsers::eme::ExplicitMessageEncryption;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::{Lang, Message};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::pubsub::event::Payload;
use xmpp_parsers::pubsub::pubsub::{Item, Items, PubSub, Publish, PublishOptions};
use xmpp_parsers::pubsub::{ItemId, NodeName};

pub const NS: &str = "urn:xmpp:omemo:2";
pub const NODE_DEVICES: &str = "urn:xmpp:omemo:2:devices";
pub const NODE_BUNDLES: &str = "urn:xmpp:omemo:2:bundles";
/// The caps feature that asks for the device lists of our contacts.
pub const NOTIFY_DEVICES: &str = "urn:xmpp:omemo:2:devices+notify";

const NS_SCE: &str = "urn:xmpp:sce:1";
const NS_EME: &str = "urn:xmpp:eme:0";
const NS_HINTS: &str = "urn:xmpp:hints";
const PUBLISH_OPTIONS: &str = "http://jabber.org/protocol/pubsub#publish-options";

/// The item id of the one item on the device list node.
pub const ITEM_CURRENT: &str = "current";

/// The body of an encrypted message, for clients that cannot read OMEMO.
pub const FALLBACK_BODY: &str =
    "This message is encrypted with OMEMO, and your client cannot read it.";

/// The most devices that we read from one list.
pub const MAX_DEVICES: usize = 1000;
/// The most one-time prekeys that we read from one bundle.
pub const MAX_PREKEYS: usize = 1000;
/// The most key elements that we read from one message.
pub const MAX_KEYS: usize = 4000;
/// The largest envelope that we parse, in bytes.
pub const MAX_ENVELOPE_BYTES: usize = 1024 * 1024;
/// The longest random padding, in characters.
pub const MAX_RPAD: usize = 200;
/// A `<time/>` that is later than this many milliseconds after now is wrong.
pub const MAX_FUTURE_MS: i64 = 5 * 60 * 1000;

/// The size of a public key (Curve25519 or Ed25519) and of a prekey, in bytes.
pub const KEY_LEN: usize = 32;
/// The size of the signature of the signed prekey, in bytes.
pub const SIG_LEN: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The element has the wrong name or namespace. The text says which one we expect.
    WrongElement(&'static str),
    /// A required element or attribute is missing.
    Missing(&'static str),
    /// A number is not valid. The text names the attribute.
    BadNumber(&'static str),
    /// A device id is 0 or too big.
    BadId(&'static str),
    BadBase64(&'static str),
    BadLength {
        field: &'static str,
        expected: usize,
        got: usize,
    },
    BadJid(&'static str),
    BadTime,
    /// The envelope is not valid XML, not UTF-8, or too big.
    BadXml(String),
    /// The answer or event has no item with a payload.
    NoItem,
    /// A field of the envelope does not match the stanza.
    Mismatch(&'static str),
    /// The envelope has no `<to/>` in a group chat.
    MissingTo,
    /// The `<time/>` of the envelope is too far in the future.
    FutureTime,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::WrongElement(what) => write!(f, "expected the element {what}"),
            Error::Missing(what) => write!(f, "{what} is missing"),
            Error::BadNumber(what) => write!(f, "{what} is not a number"),
            Error::BadId(what) => write!(f, "{what} is not a valid id"),
            Error::BadBase64(what) => write!(f, "{what} is not valid base64"),
            Error::BadLength {
                field,
                expected,
                got,
            } => write!(f, "{field} has {got} bytes, expected {expected}"),
            Error::BadJid(what) => write!(f, "{what} is not a valid JID"),
            Error::BadTime => write!(f, "the time stamp is not valid"),
            Error::BadXml(why) => write!(f, "bad envelope: {why}"),
            Error::NoItem => write!(f, "no item with a payload"),
            Error::Mismatch(what) => write!(f, "{what} of the envelope does not match"),
            Error::MissingTo => write!(f, "the envelope has no to in a group chat"),
            Error::FutureTime => write!(f, "the time of the envelope is in the future"),
        }
    }
}

impl std::error::Error for Error {}

// Helpers.

/// Read a device id. XEP-0384 says: "a randomly generated integer between 1 and 2^31 - 1
/// (the positive numbers of a signed 32 bit integer, without 0)". We follow that text and
/// not the wider range of a u32, because other clients keep ids in a signed integer. A
/// bigger id is an error.
fn parse_id(text: &str, what: &'static str) -> Result<u32, Error> {
    let n: u32 = text.trim().parse().map_err(|_| Error::BadNumber(what))?;
    if n == 0 || n > i32::MAX as u32 {
        return Err(Error::BadId(what));
    }
    Ok(n)
}

/// Read a prekey id or a signed prekey id. These need no limit but u32.
fn parse_key_id(text: &str, what: &'static str) -> Result<u32, Error> {
    text.trim().parse().map_err(|_| Error::BadNumber(what))
}

fn attr<'a>(el: &'a Element, name: &'static str) -> Result<&'a str, Error> {
    el.attr(name).ok_or(Error::Missing(name))
}

fn b64_decode(text: &str, what: &'static str) -> Result<Vec<u8>, Error> {
    // XML can wrap long text with white space.
    let clean: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    STANDARD
        .decode(clean.as_bytes())
        .map_err(|_| Error::BadBase64(what))
}

fn b64_len(text: &str, what: &'static str, len: usize) -> Result<Vec<u8>, Error> {
    let bytes = b64_decode(text, what)?;
    if bytes.len() != len {
        return Err(Error::BadLength {
            field: what,
            expected: len,
            got: bytes.len(),
        });
    }
    Ok(bytes)
}

/// An attribute name. Every call passes a fixed valid name.
fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

fn text_el(name: &str, ns: &str, text: String) -> Element {
    Element::builder(name, ns).append(text).build()
}

fn expect(el: &Element, name: &str, what: &'static str) -> Result<(), Error> {
    if el.is(name, NS) {
        Ok(())
    } else {
        Err(Error::WrongElement(what))
    }
}

// Device list.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: u32,
    pub label: Option<String>,
    pub labelsig: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceList {
    pub devices: Vec<Device>,
}

fn parse_device(el: &Element) -> Result<Device, Error> {
    let id = parse_id(attr(el, "id")?, "device id")?;
    let labelsig = match el.attr("labelsig") {
        Some(s) => Some(b64_decode(s, "labelsig")?),
        None => None,
    };
    Ok(Device {
        id,
        label: el.attr("label").map(str::to_owned),
        labelsig,
    })
}

impl TryFrom<&Element> for DeviceList {
    type Error = Error;

    /// A device element that is not valid is skipped, and so is a repeated id. The list
    /// stops at `MAX_DEVICES` devices.
    fn try_from(el: &Element) -> Result<Self, Error> {
        expect(el, "devices", "devices")?;
        let mut devices: Vec<Device> = Vec::new();
        for child in el.children().filter(|c| c.is("device", NS)) {
            if devices.len() >= MAX_DEVICES {
                break;
            }
            let Ok(device) = parse_device(child) else {
                continue;
            };
            if devices.iter().all(|d| d.id != device.id) {
                devices.push(device);
            }
        }
        Ok(DeviceList { devices })
    }
}

impl From<&DeviceList> for Element {
    fn from(list: &DeviceList) -> Element {
        let mut root = Element::builder("devices", NS).build();
        for d in &list.devices {
            let mut b = Element::builder("device", NS).attr(nc("id"), d.id.to_string());
            if let Some(label) = &d.label {
                b = b.attr(nc("label"), label.as_str());
            }
            if let Some(sig) = &d.labelsig {
                b = b.attr(nc("labelsig"), STANDARD.encode(sig));
            }
            root.append_child(b.build());
        }
        root
    }
}

// Bundle.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleXml {
    pub spk_id: u32,
    pub spk: Vec<u8>,
    pub spks: Vec<u8>,
    pub ik: Vec<u8>,
    pub prekeys: Vec<(u32, Vec<u8>)>,
}

fn child<'a>(el: &'a Element, name: &'static str) -> Result<&'a Element, Error> {
    el.get_child(name, NS).ok_or(Error::Missing(name))
}

impl TryFrom<&Element> for BundleXml {
    type Error = Error;

    /// Any bad field is an error. A bundle with more than `MAX_PREKEYS` prekeys keeps
    /// the first ones.
    fn try_from(el: &Element) -> Result<Self, Error> {
        expect(el, "bundle", "bundle")?;
        let spk_el = child(el, "spk")?;
        let spk_id = parse_key_id(attr(spk_el, "id")?, "spk id")?;
        let spk = b64_len(&spk_el.text(), "spk", KEY_LEN)?;
        let spks = b64_len(&child(el, "spks")?.text(), "spks", SIG_LEN)?;
        let ik = b64_len(&child(el, "ik")?.text(), "ik", KEY_LEN)?;
        let mut prekeys = Vec::new();
        for pk in child(el, "prekeys")?.children().filter(|c| c.is("pk", NS)) {
            if prekeys.len() >= MAX_PREKEYS {
                break;
            }
            let id = parse_key_id(attr(pk, "id")?, "pk id")?;
            prekeys.push((id, b64_len(&pk.text(), "pk", KEY_LEN)?));
        }
        Ok(BundleXml {
            spk_id,
            spk,
            spks,
            ik,
            prekeys,
        })
    }
}

impl From<&BundleXml> for Element {
    fn from(b: &BundleXml) -> Element {
        let mut prekeys = Element::builder("prekeys", NS).build();
        for (id, key) in &b.prekeys {
            prekeys.append_child(
                Element::builder("pk", NS)
                    .attr(nc("id"), id.to_string())
                    .append(STANDARD.encode(key))
                    .build(),
            );
        }
        Element::builder("bundle", NS)
            .append(
                Element::builder("spk", NS)
                    .attr(nc("id"), b.spk_id.to_string())
                    .append(STANDARD.encode(&b.spk))
                    .build(),
            )
            .append(text_el("spks", NS, STANDARD.encode(&b.spks)))
            .append(text_el("ik", NS, STANDARD.encode(&b.ik)))
            .append(prekeys)
            .build()
    }
}

// The encrypted element.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEntry {
    pub rid: u32,
    /// True for a key exchange message (X3DH data and the first ratchet message).
    pub kex: bool,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientKeys {
    pub jid: BareJid,
    pub keys: Vec<KeyEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encrypted {
    /// The device id of the sender.
    pub sid: u32,
    pub keys: Vec<RecipientKeys>,
    /// `None` for an empty OMEMO message.
    pub payload: Option<Vec<u8>>,
}

fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_key(el: &Element) -> Result<KeyEntry, Error> {
    let kex = match el.attr("kex") {
        None => false,
        Some(v) => parse_bool(v).ok_or(Error::BadNumber("kex"))?,
    };
    Ok(KeyEntry {
        rid: parse_id(attr(el, "rid")?, "rid")?,
        kex,
        data: b64_decode(&el.text(), "key")?,
    })
}

impl Encrypted {
    /// The key for the device `rid` of the account `jid`.
    pub fn key_for(&self, jid: &BareJid, rid: u32) -> Option<&KeyEntry> {
        self.keys
            .iter()
            .filter(|k| &k.jid == jid)
            .flat_map(|k| k.keys.iter())
            .find(|k| k.rid == rid)
    }

    /// The `<encrypted/>` element of a message, if it has one.
    pub fn find(message: &Message) -> Option<Result<Encrypted, Error>> {
        message
            .payloads
            .iter()
            .find(|p| p.is("encrypted", NS))
            .map(Encrypted::try_from)
    }
}

impl TryFrom<&Element> for Encrypted {
    type Error = Error;

    /// A bad `<keys/>` or `<key/>` is skipped: one broken entry for another device must
    /// not hide the key for us. A bad `sid` or payload is an error. The parser keeps
    /// the first `MAX_KEYS` key elements.
    fn try_from(el: &Element) -> Result<Self, Error> {
        expect(el, "encrypted", "encrypted")?;
        let header = child(el, "header")?;
        let sid = parse_id(attr(header, "sid")?, "sid")?;
        let mut keys = Vec::new();
        let mut total = 0;
        for group in header.children().filter(|c| c.is("keys", NS)) {
            let Some(jid) = group.attr("jid").and_then(|j| j.parse::<BareJid>().ok()) else {
                continue;
            };
            let mut entries = Vec::new();
            for key in group.children().filter(|c| c.is("key", NS)) {
                if total >= MAX_KEYS {
                    break;
                }
                if let Ok(entry) = parse_key(key) {
                    entries.push(entry);
                    total += 1;
                }
            }
            keys.push(RecipientKeys { jid, keys: entries });
        }
        let payload = match el.get_child("payload", NS) {
            Some(p) => Some(b64_decode(&p.text(), "payload")?),
            None => None,
        };
        Ok(Encrypted { sid, keys, payload })
    }
}

impl From<&Encrypted> for Element {
    fn from(e: &Encrypted) -> Element {
        let mut header = Element::builder("header", NS)
            .attr(nc("sid"), e.sid.to_string())
            .build();
        for group in &e.keys {
            let mut keys = Element::builder("keys", NS)
                .attr(nc("jid"), group.jid.to_string())
                .build();
            for k in &group.keys {
                let mut b = Element::builder("key", NS).attr(nc("rid"), k.rid.to_string());
                if k.kex {
                    b = b.attr(nc("kex"), "true");
                }
                keys.append_child(b.append(STANDARD.encode(&k.data)).build());
            }
            header.append_child(keys);
        }
        let mut root = Element::builder("encrypted", NS).append(header).build();
        if let Some(payload) = &e.payload {
            root.append_child(text_el("payload", NS, STANDARD.encode(payload)));
        }
        root
    }
}

/// Put `encrypted` in `message`. This also adds the XEP-0380 hint, the XEP-0334 `<store/>`
/// hint, and the fallback body. An older copy of these elements is removed first.
pub fn wrap(message: &mut Message, encrypted: &Encrypted) {
    message.payloads.retain(|p| {
        !(p.is("encrypted", NS) || p.is("encryption", NS_EME) || p.is("store", NS_HINTS))
    });
    message.payloads.push(encrypted.into());
    message.payloads.push(
        ExplicitMessageEncryption {
            namespace: NS.to_owned(),
            name: Some("OMEMO".to_owned()),
        }
        .into(),
    );
    message
        .payloads
        .push(Element::builder("store", NS_HINTS).build());
    message.bodies.clear();
    message
        .bodies
        .insert(Lang(String::new()), FALLBACK_BODY.to_owned());
}

// The XEP-0420 envelope.

#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    /// The payload elements, for example `<body xmlns='jabber:client'/>`.
    pub content: Vec<Element>,
    pub rpad: String,
    pub time: Option<DateTime>,
    pub from: Option<Jid>,
    pub to: Option<Jid>,
}

/// Make random padding of 0 to `MAX_RPAD` characters from `rng_bytes`. The first byte sets
/// the length. The other bytes pick the characters. The padding only hides the length of
/// the message. It is not a secret, so a plain byte slice is enough. The caller takes the
/// bytes from the OS. An empty slice gives an empty padding.
pub fn new_rpad(rng_bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let Some(&first) = rng_bytes.first() else {
        return String::new();
    };
    let len = first as usize * (MAX_RPAD + 1) / 256;
    (0..len)
        .map(|i| {
            let b = rng_bytes
                .get(1 + i)
                .copied()
                .unwrap_or_else(|| first.wrapping_add(i as u8));
            ALPHABET[b as usize % ALPHABET.len()] as char
        })
        .collect()
}

impl Envelope {
    pub fn to_element(&self) -> Element {
        let mut content = Element::builder("content", NS_SCE).build();
        for c in &self.content {
            content.append_child(c.clone());
        }
        let mut root = Element::builder("envelope", NS_SCE)
            .append(content)
            .append(text_el("rpad", NS_SCE, self.rpad.clone()))
            .build();
        if let Some(time) = &self.time {
            root.append_child(
                Element::builder("time", NS_SCE)
                    .attr(nc("stamp"), time.0.to_rfc3339())
                    .build(),
            );
        }
        if let Some(to) = &self.to {
            root.append_child(
                Element::builder("to", NS_SCE)
                    .attr(nc("jid"), to.to_string())
                    .build(),
            );
        }
        if let Some(from) = &self.from {
            root.append_child(
                Element::builder("from", NS_SCE)
                    .attr(nc("jid"), from.to_string())
                    .build(),
            );
        }
        root
    }

    /// The UTF-8 XML that OMEMO encrypts.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        // Writing to a Vec does not fail.
        let _ = self.to_element().write_to(&mut out);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Envelope, Error> {
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::BadXml("too big".into()));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Error::BadXml("not UTF-8".into()))?;
        let el: Element = text.parse().map_err(|e| Error::BadXml(format!("{e}")))?;
        if !el.is("envelope", NS_SCE) {
            return Err(Error::WrongElement("envelope"));
        }
        let content = el
            .get_child("content", NS_SCE)
            .ok_or(Error::Missing("content"))?
            .children()
            .cloned()
            .collect();
        let rpad = el
            .get_child("rpad", NS_SCE)
            .ok_or(Error::Missing("rpad"))?
            .text();
        let time = match el.get_child("time", NS_SCE) {
            Some(t) => Some(
                attr(t, "stamp")?
                    .parse::<DateTime>()
                    .map_err(|_| Error::BadTime)?,
            ),
            None => None,
        };
        let jid_of = |name: &'static str| -> Result<Option<Jid>, Error> {
            match el.get_child(name, NS_SCE) {
                Some(e) => Ok(Some(
                    attr(e, "jid")?.parse().map_err(|_| Error::BadJid(name))?,
                )),
                None => Ok(None),
            }
        };
        Ok(Envelope {
            content,
            rpad,
            time,
            from: jid_of("from")?,
            to: jid_of("to")?,
        })
    }

    /// Check the envelope against the stanza that carried it. Compare the bare JIDs.
    /// `from` must match when present. A group chat needs `to`, and it must match.
    /// A `time` more than `MAX_FUTURE_MS` after `now_ms` is wrong.
    pub fn check(
        &self,
        stanza_from: &Jid,
        stanza_to: Option<&Jid>,
        groupchat: bool,
        now_ms: i64,
    ) -> Result<(), Error> {
        if let Some(from) = &self.from
            && from.to_bare() != stanza_from.to_bare()
        {
            return Err(Error::Mismatch("from"));
        }
        match (&self.to, stanza_to) {
            (Some(to), Some(stanza_to)) if to.to_bare() != stanza_to.to_bare() => {
                return Err(Error::Mismatch("to"));
            }
            (None, _) if groupchat => return Err(Error::MissingTo),
            _ => {}
        }
        if let Some(time) = &self.time
            && time.0.timestamp_millis() > now_ms.saturating_add(MAX_FUTURE_MS)
        {
            return Err(Error::FutureTime);
        }
        Ok(())
    }
}

// PEP requests and answers.

fn publish_iq(node: &str, item: Item, mut fields: Vec<Field>) -> Iq {
    fields.push(Field::text_single("pubsub#access_model", "open"));
    let form = DataForm::new(DataFormType::Submit, PUBLISH_OPTIONS, fields);
    Iq::from_set(
        "",
        PubSub::Publish {
            publish: Publish {
                node: NodeName(node.to_owned()),
                items: vec![item],
            },
            publish_options: Some(PublishOptions { form: Some(form) }),
        },
    )
}

/// Publish our device list. XEP-0384 asks for the access model `open` and no more.
pub fn publish_devices_iq(list: &DeviceList) -> Iq {
    let item = Item {
        id: Some(ItemId(ITEM_CURRENT.to_owned())),
        publisher: None,
        payload: Some(list.into()),
    };
    publish_iq(NODE_DEVICES, item, vec![])
}

/// Publish the bundle of our device `device_id`. The node must keep one item for each
/// device, so the options ask for `max` items.
pub fn publish_bundle_iq(device_id: u32, bundle: &BundleXml) -> Iq {
    let item = Item {
        id: Some(ItemId(device_id.to_string())),
        publisher: None,
        payload: Some(bundle.into()),
    };
    publish_iq(
        NODE_BUNDLES,
        item,
        vec![Field::text_single("pubsub#max_items", "max")],
    )
}

fn fetch_iq(to: Option<&BareJid>, node: &str, item_id: &str) -> Iq {
    let items = Items {
        max_items: None,
        node: NodeName(node.to_owned()),
        subid: None,
        items: vec![Item {
            id: Some(ItemId(item_id.to_owned())),
            publisher: None,
            payload: None,
        }],
    };
    let iq = Iq::from_get("", PubSub::Items(items));
    match to {
        Some(jid) => iq.with_to(Jid::from(jid.clone())),
        None => iq,
    }
}

/// Fetch the device list of `to`. `None` means our own account.
pub fn fetch_devices_iq(to: Option<&BareJid>) -> Iq {
    fetch_iq(to, NODE_DEVICES, ITEM_CURRENT)
}

/// Fetch one bundle of `to`. `None` means our own account.
pub fn fetch_bundle_iq(to: Option<&BareJid>, device_id: u32) -> Iq {
    fetch_iq(to, NODE_BUNDLES, &device_id.to_string())
}

/// The payload of the newest item of an answer to a fetch. `result` is the `<pubsub/>`
/// element of the IQ result.
fn result_payload(result: &Element) -> Result<Element, Error> {
    let pubsub = PubSub::try_from(result.clone()).map_err(|_| Error::WrongElement("pubsub"))?;
    match pubsub {
        PubSub::Items(items) => items
            .items
            .into_iter()
            .rev()
            .find_map(|i| i.payload)
            .ok_or(Error::NoItem),
        _ => Err(Error::WrongElement("pubsub items")),
    }
}

/// The payload of the newest published item of an event.
fn event_payload(event: &Payload) -> Result<Element, Error> {
    match event {
        Payload::Items { published, .. } => published
            .iter()
            .rev()
            .find_map(|i| i.payload.clone())
            .ok_or(Error::NoItem),
        _ => Err(Error::NoItem),
    }
}

/// Read a device list from the `<pubsub/>` payload of an IQ result.
pub fn parse_devices_result(result: &Element) -> Result<DeviceList, Error> {
    DeviceList::try_from(&result_payload(result)?)
}

/// Read a bundle from the `<pubsub/>` payload of an IQ result.
pub fn parse_bundle_result(result: &Element) -> Result<BundleXml, Error> {
    BundleXml::try_from(&result_payload(result)?)
}

/// Read a device list from a PEP event on the devices node.
pub fn parse_devices_event(event: &Payload) -> Result<DeviceList, Error> {
    DeviceList::try_from(&event_payload(event)?)
}

/// Read a bundle from a PEP event on the bundles node.
pub fn parse_bundle_event(event: &Payload) -> Result<BundleXml, Error> {
    BundleXml::try_from(&event_payload(event)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn b64(n: usize, fill: u8) -> String {
        STANDARD.encode(vec![fill; n])
    }

    fn bundle() -> BundleXml {
        BundleXml {
            spk_id: 1,
            spk: vec![1; 32],
            spks: vec![2; 64],
            ik: vec![3; 32],
            prekeys: vec![(1, vec![4; 32]), (7, vec![5; 32])],
        }
    }

    fn encrypted() -> Encrypted {
        Encrypted {
            sid: 27183,
            keys: vec![
                RecipientKeys {
                    jid: "juliet@capulet.lit".parse().unwrap(),
                    keys: vec![KeyEntry {
                        rid: 31415,
                        kex: false,
                        data: vec![1, 2, 3],
                    }],
                },
                RecipientKeys {
                    jid: "romeo@montague.lit".parse().unwrap(),
                    keys: vec![
                        KeyEntry {
                            rid: 1337,
                            kex: false,
                            data: vec![4],
                        },
                        KeyEntry {
                            rid: 12321,
                            kex: true,
                            data: vec![5, 6],
                        },
                    ],
                },
            ],
            payload: Some(vec![9, 9, 9]),
        }
    }

    #[test]
    fn device_list_xep_example() {
        let list = DeviceList::try_from(&el("<devices xmlns='urn:xmpp:omemo:2'>\
            <device id='12345' label='Dino on Lenovo Thinkpad T495' labelsig='b64/encoded/data' />\
            <device id='4223' />\
            <device id='31415' label='Conversations on Pixel 3' labelsig='b64/encoded/data' />\
            </devices>"))
        .unwrap();
        assert_eq!(list.devices.len(), 3);
        assert_eq!(list.devices[1].id, 4223);
        assert_eq!(list.devices[1].label, None);
        assert_eq!(
            list.devices[0].label.as_deref(),
            Some("Dino on Lenovo Thinkpad T495")
        );
        assert!(list.devices[0].labelsig.is_some());
    }

    #[test]
    fn device_list_round_trip() {
        let list = DeviceList {
            devices: vec![
                Device {
                    id: 5,
                    label: Some("Phone".into()),
                    labelsig: Some(vec![1; 64]),
                },
                Device {
                    id: i32::MAX as u32,
                    label: None,
                    labelsig: None,
                },
            ],
        };
        let element = Element::from(&list);
        assert!(element.is("devices", NS));
        assert_eq!(DeviceList::try_from(&element).unwrap(), list);
        // Through text too.
        let text = String::from(&element);
        assert_eq!(DeviceList::try_from(&el(&text)).unwrap(), list);
    }

    #[test]
    fn device_list_skips_bad_entries_and_duplicates() {
        let list = DeviceList::try_from(&el("<devices xmlns='urn:xmpp:omemo:2'>\
            <device id='0'/><device id='-4'/><device id='2147483648'/>\
            <device id='4294967296'/><device id='abc'/><device/>\
            <device id='7' labelsig='!!!'/><device id='8'/><device id='8' label='x'/>\
            <other id='9'/></devices>"))
        .unwrap();
        assert_eq!(list.devices.len(), 1);
        assert_eq!(list.devices[0].id, 8);
        assert_eq!(list.devices[0].label, None);
    }

    #[test]
    fn device_list_is_capped() {
        let mut xml = String::from("<devices xmlns='urn:xmpp:omemo:2'>");
        for i in 1..=(MAX_DEVICES + 50) {
            xml.push_str(&format!("<device id='{i}'/>"));
        }
        xml.push_str("</devices>");
        let list = DeviceList::try_from(&el(&xml)).unwrap();
        assert_eq!(list.devices.len(), MAX_DEVICES);
    }

    #[test]
    fn device_list_wrong_element() {
        assert!(DeviceList::try_from(&el("<devices xmlns='urn:x:y'/>")).is_err());
        assert!(DeviceList::try_from(&el("<list xmlns='urn:xmpp:omemo:2'/>")).is_err());
    }

    #[test]
    fn bundle_round_trip() {
        let b = bundle();
        let element = Element::from(&b);
        assert_eq!(BundleXml::try_from(&element).unwrap(), b);
        let text = String::from(&element);
        assert_eq!(BundleXml::try_from(&el(&text)).unwrap(), b);
    }

    fn bundle_xml(spk: usize, spks: usize, ik: usize, pk: usize) -> Element {
        el(&format!(
            "<bundle xmlns='urn:xmpp:omemo:2'><spk id='1'>{}</spk><spks>{}</spks>\
             <ik>{}</ik><prekeys><pk id='1'>{}</pk></prekeys></bundle>",
            b64(spk, 1),
            b64(spks, 2),
            b64(ik, 3),
            b64(pk, 4)
        ))
    }

    #[test]
    fn bundle_checks_lengths() {
        assert!(BundleXml::try_from(&bundle_xml(32, 64, 32, 32)).is_ok());
        for (a, b, c, d) in [
            (31, 64, 32, 32),
            (32, 32, 32, 32),
            (32, 64, 33, 32),
            (32, 64, 32, 0),
        ] {
            assert!(matches!(
                BundleXml::try_from(&bundle_xml(a, b, c, d)),
                Err(Error::BadLength { .. })
            ));
        }
    }

    #[test]
    fn bundle_bad_base64_and_missing_parts() {
        let bad = el(&format!(
            "<bundle xmlns='urn:xmpp:omemo:2'><spk id='1'>%%%</spk><spks>{}</spks>\
             <ik>{}</ik><prekeys/></bundle>",
            b64(64, 2),
            b64(32, 3)
        ));
        assert_eq!(BundleXml::try_from(&bad), Err(Error::BadBase64("spk")));
        let no_id = el(&format!(
            "<bundle xmlns='urn:xmpp:omemo:2'><spk>{}</spk></bundle>",
            b64(32, 1)
        ));
        assert!(BundleXml::try_from(&no_id).is_err());
        let no_prekeys = el("<bundle xmlns='urn:xmpp:omemo:2'/>");
        assert!(BundleXml::try_from(&no_prekeys).is_err());
    }

    #[test]
    fn bundle_prekeys_are_capped() {
        let mut b = bundle();
        b.prekeys = (0..(MAX_PREKEYS as u32 + 20))
            .map(|i| (i, vec![1; 32]))
            .collect();
        let parsed = BundleXml::try_from(&Element::from(&b)).unwrap();
        assert_eq!(parsed.prekeys.len(), MAX_PREKEYS);
    }

    #[test]
    fn encrypted_xep_example() {
        let e = Encrypted::try_from(&el("<encrypted xmlns='urn:xmpp:omemo:2'>\
            <header sid='27183'>\
            <keys jid='juliet@capulet.lit'><key rid='31415'>YWJj</key></keys>\
            <keys jid='romeo@montague.lit'><key rid='1337'>YWJj</key>\
            <key kex='true' rid='12321'>YWJj</key></keys>\
            </header><payload>YWJj</payload></encrypted>"))
        .unwrap();
        assert_eq!(e.sid, 27183);
        let romeo: BareJid = "romeo@montague.lit".parse().unwrap();
        assert!(e.key_for(&romeo, 12321).unwrap().kex);
        assert!(!e.key_for(&romeo, 1337).unwrap().kex);
        assert_eq!(e.payload.as_deref(), Some(&b"abc"[..]));
        assert!(e.key_for(&romeo, 31415).is_none());
    }

    #[test]
    fn encrypted_round_trip() {
        let e = encrypted();
        let element = Element::from(&e);
        assert_eq!(Encrypted::try_from(&element).unwrap(), e);
        let text = String::from(&element);
        assert_eq!(Encrypted::try_from(&el(&text)).unwrap(), e);
        let empty = Encrypted {
            payload: None,
            ..encrypted()
        };
        assert_eq!(Encrypted::try_from(&Element::from(&empty)).unwrap(), empty);
    }

    #[test]
    fn encrypted_kex_values() {
        let parse = |kex: &str| {
            Encrypted::try_from(&el(&format!(
                "<encrypted xmlns='urn:xmpp:omemo:2'><header sid='1'>\
                 <keys jid='a@b.c'><key rid='2' kex='{kex}'>AA==</key></keys></header></encrypted>"
            )))
            .unwrap()
        };
        let jid: BareJid = "a@b.c".parse().unwrap();
        assert!(parse("true").key_for(&jid, 2).unwrap().kex);
        assert!(parse("1").key_for(&jid, 2).unwrap().kex);
        assert!(!parse("false").key_for(&jid, 2).unwrap().kex);
        assert!(!parse("0").key_for(&jid, 2).unwrap().kex);
        // A wrong value skips the key.
        assert!(parse("yes").key_for(&jid, 2).is_none());
    }

    #[test]
    fn encrypted_bad_input() {
        let bad = [
            // No header.
            "<encrypted xmlns='urn:xmpp:omemo:2'/>",
            // No sid.
            "<encrypted xmlns='urn:xmpp:omemo:2'><header/></encrypted>",
            // Zero, negative, overflow, too big for 31 bits.
            "<encrypted xmlns='urn:xmpp:omemo:2'><header sid='0'/></encrypted>",
            "<encrypted xmlns='urn:xmpp:omemo:2'><header sid='-1'/></encrypted>",
            "<encrypted xmlns='urn:xmpp:omemo:2'><header sid='99999999999'/></encrypted>",
            "<encrypted xmlns='urn:xmpp:omemo:2'><header sid='2147483648'/></encrypted>",
            // Bad payload.
            "<encrypted xmlns='urn:xmpp:omemo:2'><header sid='1'/><payload>@@</payload></encrypted>",
        ];
        for xml in bad {
            assert!(Encrypted::try_from(&el(xml)).is_err(), "{xml}");
        }
    }

    #[test]
    fn encrypted_skips_bad_keys() {
        let e = Encrypted::try_from(&el("<encrypted xmlns='urn:xmpp:omemo:2'><header sid='1'>\
            <keys jid='a@b.c'><key rid='0'>AA==</key><key>AA==</key><key rid='3'>@@</key>\
            <key rid='4'>AA==</key></keys><keys><key rid='5'>AA==</key></keys>\
            <keys jid='not a jid@@'><key rid='6'>AA==</key></keys></header></encrypted>"))
        .unwrap();
        assert_eq!(e.keys.len(), 1);
        assert_eq!(e.keys[0].keys.len(), 1);
        assert_eq!(e.keys[0].keys[0].rid, 4);
    }

    #[test]
    fn encrypted_keys_are_capped() {
        let mut xml =
            String::from("<encrypted xmlns='urn:xmpp:omemo:2'><header sid='1'><keys jid='a@b.c'>");
        for i in 1..=(MAX_KEYS + 10) {
            xml.push_str(&format!("<key rid='{i}'>AA==</key>"));
        }
        xml.push_str("</keys></header></encrypted>");
        let e = Encrypted::try_from(&el(&xml)).unwrap();
        assert_eq!(e.keys[0].keys.len(), MAX_KEYS);
    }

    #[test]
    fn wrap_and_find() {
        let e = encrypted();
        let mut m = Message::chat(None).with_body(Lang(String::new()), "secret".into());
        assert!(Encrypted::find(&m).is_none());
        wrap(&mut m, &e);
        assert_eq!(Encrypted::find(&m).unwrap().unwrap(), e);
        assert_eq!(
            m.bodies.get(&Lang(String::new())).map(String::as_str),
            Some(FALLBACK_BODY)
        );
        assert_eq!(m.bodies.len(), 1);
        let eme = m
            .payloads
            .iter()
            .find_map(|p| ExplicitMessageEncryption::try_from(p.clone()).ok())
            .unwrap();
        assert_eq!(eme.namespace, NS);
        assert_eq!(eme.name.as_deref(), Some("OMEMO"));
        assert!(m.payloads.iter().any(|p| p.is("store", NS_HINTS)));
        // A second wrap does not double the elements.
        wrap(&mut m, &e);
        assert_eq!(m.payloads.len(), 3);
        // The message survives a trip through XML.
        let again = Message::try_from(Element::from(m.clone())).unwrap();
        assert_eq!(Encrypted::find(&again).unwrap().unwrap(), e);
    }

    #[test]
    fn find_reports_bad_element() {
        let mut m = Message::chat(None);
        m.payloads.push(el("<encrypted xmlns='urn:xmpp:omemo:2'/>"));
        assert!(Encrypted::find(&m).unwrap().is_err());
    }

    fn jid(s: &str) -> Jid {
        s.parse().unwrap()
    }

    fn envelope() -> Envelope {
        Envelope {
            content: vec![el("<body xmlns='jabber:client'>Hello &amp; bye</body>")],
            rpad: "C1DHN9HK-9A25".into(),
            time: Some("2004-01-25T06:05:00+01:00".parse().unwrap()),
            from: Some(jid("opportunity@mars.planet")),
            to: Some(jid("missioncontrol@houston.nasa.gov")),
        }
    }

    #[test]
    fn envelope_round_trip() {
        let e = envelope();
        let bytes = e.to_bytes();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            text.starts_with("<envelope xmlns='urn:xmpp:sce:1'>"),
            "{text}"
        );
        assert_eq!(Envelope::from_bytes(&bytes).unwrap(), e);
        let min = Envelope {
            content: vec![],
            rpad: String::new(),
            time: None,
            from: None,
            to: None,
        };
        assert_eq!(Envelope::from_bytes(&min.to_bytes()).unwrap(), min);
    }

    #[test]
    fn envelope_xep_example() {
        let e = Envelope::from_bytes(
            b"<envelope xmlns='urn:xmpp:sce:1'><content>\
              <body xmlns='jabber:client'>Hi</body>\
              <x xmlns='jabber:x:oob'><url>https://example.org/</url></x></content>\
              <time stamp='2004-01-25T06:05:00+01:00'/>\
              <to jid='missioncontrol@houston.nasa.gov'/>\
              <from jid='opportunity@mars.planet'/>\
              <rpad>C1DHN9HK-9A25tSmwK4hU!Jji9%GKYK^syIlHJT9TnI4</rpad></envelope>",
        )
        .unwrap();
        assert_eq!(e.content.len(), 2);
        assert_eq!(e.rpad.len(), 44);
        assert!(e.time.is_some());
    }

    #[test]
    fn envelope_bad_input() {
        for xml in [
            &b"not xml"[..],
            b"<envelope xmlns='urn:x:y'/>",
            // No rpad.
            b"<envelope xmlns='urn:xmpp:sce:1'><content/></envelope>",
            // No content.
            b"<envelope xmlns='urn:xmpp:sce:1'><rpad>x</rpad></envelope>",
            b"<envelope xmlns='urn:xmpp:sce:1'><content/><rpad/><time stamp='soon'/></envelope>",
            b"<envelope xmlns='urn:xmpp:sce:1'><content/><rpad/><from jid='@@'/></envelope>",
            b"<envelope xmlns='urn:xmpp:sce:1'><content/><rpad/><to/></envelope>",
            &[0xff, 0xfe],
        ] {
            assert!(Envelope::from_bytes(xml).is_err());
        }
        let big = vec![b' '; MAX_ENVELOPE_BYTES + 1];
        assert!(Envelope::from_bytes(&big).is_err());
    }

    #[test]
    fn envelope_checks() {
        let from = jid("opportunity@mars.planet/rover");
        let to = jid("missioncontrol@houston.nasa.gov");
        let now = envelope().time.unwrap().0.timestamp_millis();
        let e = envelope();
        assert_eq!(e.check(&from, Some(&to), false, now), Ok(()));
        assert_eq!(e.check(&from, Some(&to), true, now), Ok(()));
        // The stanza has no to: nothing to compare.
        assert_eq!(e.check(&from, None, false, now), Ok(()));
        assert_eq!(
            e.check(&jid("eve@evil.example"), Some(&to), false, now),
            Err(Error::Mismatch("from"))
        );
        assert_eq!(
            e.check(&from, Some(&jid("eve@evil.example")), false, now),
            Err(Error::Mismatch("to"))
        );
        let no_to = Envelope {
            to: None,
            ..envelope()
        };
        assert_eq!(no_to.check(&from, Some(&to), false, now), Ok(()));
        assert_eq!(
            no_to.check(&from, Some(&to), true, now),
            Err(Error::MissingTo)
        );
        // A time 5 minutes ahead is fine. A time 6 minutes ahead is not.
        assert_eq!(
            e.check(&from, Some(&to), false, now - MAX_FUTURE_MS),
            Ok(())
        );
        assert_eq!(
            e.check(&from, Some(&to), false, now - MAX_FUTURE_MS - 1000),
            Err(Error::FutureTime)
        );
        // An extreme clock value does not overflow.
        assert!(e.check(&from, Some(&to), false, i64::MAX).is_ok());
        assert!(e.check(&from, Some(&to), false, i64::MIN).is_err());
    }

    #[test]
    fn rpad_length_and_alphabet() {
        assert_eq!(new_rpad(&[]), "");
        assert_eq!(new_rpad(&[0, 1, 2]), "");
        assert_eq!(new_rpad(&[255]).len(), MAX_RPAD);
        for first in 0..=255u8 {
            let pad = new_rpad(&[first, 7, 8, 9]);
            assert!(pad.len() <= MAX_RPAD);
            assert!(
                pad.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            );
        }
        assert_eq!(new_rpad(&[10, 0, 1]), new_rpad(&[10, 0, 1]));
    }

    fn set_payload(iq: Iq) -> Element {
        match iq {
            Iq::Set { payload, .. } => payload,
            other => panic!("not a set: {other:?}"),
        }
    }

    #[test]
    fn publish_devices_request() {
        let list = DeviceList {
            devices: vec![Device {
                id: 3,
                label: None,
                labelsig: None,
            }],
        };
        let PubSub::Publish {
            publish,
            publish_options,
        } = PubSub::try_from(set_payload(publish_devices_iq(&list))).unwrap()
        else {
            panic!("not a publish");
        };
        assert_eq!(publish.node.0, NODE_DEVICES);
        assert_eq!(publish.items[0].id.as_ref().unwrap().0, "current");
        let el = publish.items[0].payload.clone().unwrap();
        assert_eq!(DeviceList::try_from(&el).unwrap(), list);
        let form = publish_options.unwrap().form.unwrap();
        let has = |var: &str, val: &str| {
            form.fields
                .iter()
                .any(|f| f.var.as_deref() == Some(var) && f.values == [val])
        };
        assert!(has("pubsub#access_model", "open"));
        assert!(
            !form
                .fields
                .iter()
                .any(|f| f.var.as_deref() == Some("pubsub#max_items"))
        );
    }

    #[test]
    fn publish_bundle_request() {
        let b = bundle();
        let PubSub::Publish {
            publish,
            publish_options,
        } = PubSub::try_from(set_payload(publish_bundle_iq(31415, &b))).unwrap()
        else {
            panic!("not a publish");
        };
        assert_eq!(publish.node.0, NODE_BUNDLES);
        assert_eq!(publish.items[0].id.as_ref().unwrap().0, "31415");
        let form = publish_options.unwrap().form.unwrap();
        let has = |var: &str, val: &str| {
            form.fields
                .iter()
                .any(|f| f.var.as_deref() == Some(var) && f.values == [val])
        };
        assert!(has("pubsub#access_model", "open"));
        assert!(has("pubsub#max_items", "max"));
    }

    #[test]
    fn fetch_requests() {
        let jid: BareJid = "juliet@capulet.lit".parse().unwrap();
        let iq = fetch_devices_iq(Some(&jid));
        assert_eq!(iq.to().cloned(), Some(Jid::from(jid.clone())));
        let Iq::Get { payload: e, .. } = iq else {
            panic!("not a get");
        };
        let PubSub::Items(items) = PubSub::try_from(e).unwrap() else {
            panic!("not items");
        };
        assert_eq!(items.node.0, NODE_DEVICES);
        assert_eq!(items.items[0].id.as_ref().unwrap().0, "current");

        let iq = fetch_bundle_iq(None, 99);
        assert_eq!(iq.to().cloned(), None);
        let Iq::Get { payload: e, .. } = iq else {
            panic!("not a get");
        };
        let PubSub::Items(items) = PubSub::try_from(e).unwrap() else {
            panic!("not items");
        };
        assert_eq!(items.node.0, NODE_BUNDLES);
        assert_eq!(items.items[0].id.as_ref().unwrap().0, "99");
    }

    fn items_result(node: &str, payload: Option<Element>) -> Element {
        PubSub::Items(Items {
            max_items: None,
            node: NodeName(node.to_owned()),
            subid: None,
            items: vec![Item {
                id: None,
                publisher: None,
                payload,
            }],
        })
        .into()
    }

    fn event(payload: Option<Element>) -> Payload {
        Payload::Items {
            node: NodeName(NODE_DEVICES.to_owned()),
            published: vec![xmpp_parsers::pubsub::event::Item {
                id: None,
                publisher: None,
                payload,
            }],
            retracted: vec![],
        }
    }

    #[test]
    fn parse_results_and_events() {
        let list = DeviceList {
            devices: vec![Device {
                id: 8,
                label: Some("x".into()),
                labelsig: None,
            }],
        };
        let result = items_result(NODE_DEVICES, Some((&list).into()));
        assert_eq!(parse_devices_result(&result).unwrap(), list);
        assert_eq!(
            parse_devices_event(&event(Some((&list).into()))).unwrap(),
            list
        );
        // A bundle parser refuses a device list.
        assert!(parse_bundle_result(&result).is_err());
        let b = bundle();
        let result = items_result(NODE_BUNDLES, Some((&b).into()));
        assert_eq!(parse_bundle_result(&result).unwrap(), b);
        assert_eq!(parse_bundle_event(&event(Some((&b).into()))).unwrap(), b);
        // No payload, a purge, and not a pubsub element.
        assert_eq!(
            parse_devices_result(&items_result(NODE_DEVICES, None)),
            Err(Error::NoItem)
        );
        assert_eq!(parse_devices_event(&event(None)), Err(Error::NoItem));
        let purge = Payload::Purge {
            node: NodeName(NODE_DEVICES.to_owned()),
        };
        assert_eq!(parse_devices_event(&purge), Err(Error::NoItem));
        assert!(parse_devices_result(&el("<iq xmlns='jabber:client'/>")).is_err());
    }
}
