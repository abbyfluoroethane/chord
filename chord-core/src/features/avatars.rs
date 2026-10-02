//! User avatars (XEP-0084).
//!
//! - A PEP event on `urn:xmpp:avatar:metadata` stores the hash and the type of the avatar
//!   of its owner. If we lack the image, we fetch it from the data node of the owner and
//!   check its SHA-1 against the hash.
//! - An info with only a URL (no data node copy) is downloaded with `http_get`: https and
//!   public hosts only, a size cap, the SHA-1 check, and the type from the bytes.
//! - The type of the image is read from its bytes (`sniff_mime`): png, jpeg, gif or webp.
//!   Anything else (an SVG, for example) is dropped.
//! - An empty `<metadata/>`, a retract, a purge, or a deleted node removes the avatar.
//! - At each new session we fetch our own metadata. Our caps ask for `+notify`, so the
//!   metadata of our contacts arrives as events.
//! - `ClientHandle::set_avatar` publishes the data and then the metadata, both with the
//!   access model `open`. XEP-0084 leaves the access model to PEP, where the default is
//!   `presence`. Chord uses `open`, so that members of a room who are not contacts
//!   can also see the avatar.
//! - XEP-0153: a presence of a contact can carry the SHA-1 of its vCard photo. If the
//!   contact has no XEP-0084 avatar and we lack that image, we fetch the vCard (XEP-0054)
//!   from the bare JID of the contact, check the SHA-1, and store the photo. A XEP-0084
//!   avatar always wins. Only a contact that we see (subscription `to` or `both`) can
//!   make us fetch (a person with an open 1:1 chat also can, through the queue below),
//!   and a hash that failed once is not tried again in the session. An
//!   empty `<photo/>` removes only a photo that we took from a vCard in this session.
//!   `on_presence` returns false, so the roster also sees the presence.
//! - `ClientHandle::refresh_avatar` asks for XEP-0084 first. If the owner has no metadata
//!   node, or the metadata is empty, it fetches the vCard of the bare JID and stores its
//!   photo. A XEP-0084 avatar always wins.
//! - `set_avatar` and `remove_avatar` also update the PHOTO of our own vCard, so that
//!   XEP-0153 clients see the change. The vCard step reads our vCard first and keeps all
//!   its other fields. After the update we send a presence with `vcard-temp:x:update`.
//!   A failure of the vCard step is logged and does not fail `set_avatar`.
//!   `set_vcard_photo` and `remove_vcard_photo` change only the vCard.
//!
//! - Room occupants: `on_occupant` runs for each room presence. If the room gives the real
//!   JID of the occupant, the avatar of that bare JID is fetched as for a contact, once
//!   in a session, and the views find it by the real JID. Otherwise the XEP-0153 hash of
//!   the room presence decides. Then the vCard comes from the full JID `room/nick`, and
//!   the owner of the avatar is that full JID. A hash that we have is not fetched again.
//!   The requests wait in a queue, and only `MAX_IN_FLIGHT` run at one time.
//!
//! The functions that store and check images also serve the space avatars (`spaces.rs`).
//! The owner of a space avatar is `service/node`.

use std::collections::{HashSet, VecDeque};

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};
use sha1::{Digest, Sha1};
use xmpp_parsers::avatar::{Data, Info, Metadata};
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field};
use xmpp_parsers::hashes::Sha1HexAttribute;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::presence::{Presence, Type as PresenceType};
use xmpp_parsers::pubsub::event::Payload;
use xmpp_parsers::pubsub::pubsub::{Item, Items, PubSub, Publish, PublishOptions};
use xmpp_parsers::pubsub::{ItemId, NodeName};
use xmpp_parsers::stanza_error::{DefinedCondition, StanzaError};
use xmpp_parsers::vcard::{Binval, Photo, Type as PhotoType, VCard, VCardQuery};
use xmpp_parsers::vcard_update::{Photo as UpdatePhoto, VCardUpdate};

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending};
use crate::actor::{ClientError, ClientHandle};
use crate::store::Store;
use crate::views::{ChannelScope, ViewKey};

pub const NODE_METADATA: &str = "urn:xmpp:avatar:metadata";
pub const NODE_DATA: &str = "urn:xmpp:avatar:data";
const PUBLISH_OPTIONS: &str = "http://jabber.org/protocol/pubsub#publish-options";

/// The largest image that we accept or publish, in bytes. Avatars are small.
pub const MAX_AVATAR_BYTES: usize = 1024 * 1024;

/// The largest image that we publish, in bytes. The UI resizes to 256 by 256 pixels, and
/// that fits in a few tens of KiB as PNG or JPEG. A small item also passes the item size
/// limit of a server (XEP-0084 4).
pub const MAX_PUBLISH_BYTES: usize = 64 * 1024;

/// The avatar of one owner, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Avatar {
    /// SHA-1 of the image, in lower case hex. It is the XEP-0084 id.
    pub hash: String,
    pub mime: Option<String>,
    /// `None` until the image has arrived.
    pub data: Option<Vec<u8>>,
}

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The images that we are fetching now, as (owner, hash).
    fetching: HashSet<(BareJid, String)>,
    /// Owners whose avatar came from XEP-0084 in this session. XEP-0153 does not replace it.
    pep: HashSet<BareJid>,
    /// Owners whose avatar we took from a vCard in this session.
    vcard: HashSet<BareJid>,
    /// Vcard photos that failed, as (owner, hash). We do not ask for them again.
    vcard_failed: HashSet<(BareJid, String)>,
    /// Real JIDs of occupants that we asked for in this session.
    occupant_asked: HashSet<BareJid>,
    /// Occupant vCard photos that run now, as (full JID, hash).
    occupant_fetching: HashSet<(String, String)>,
    /// Occupant vCard photos that failed. We do not ask for them again.
    occupant_failed: HashSet<(String, String)>,
    /// Requests for occupants that wait for a free place.
    queue: VecDeque<Job>,
    /// The queued requests that have gone out and have no answer yet.
    in_flight: usize,
}

/// The most occupant requests that run at one time.
const MAX_IN_FLIGHT: usize = 3;
/// The most entries in each set that remembers an owner for the whole session. A big
/// room can show many occupants in a session that lasts for days. A full set starts again
/// from empty: the cost is one more request for an owner that we saw before.
const MAX_REMEMBERED: usize = 4096;

/// Add `item` to `set`. A full set starts again from empty. Returns true if `item` is new.
fn remember<T: std::hash::Hash + Eq>(set: &mut HashSet<T>, item: T) -> bool {
    if set.len() >= MAX_REMEMBERED && !set.contains(&item) {
        set.clear();
    }
    set.insert(item)
}

/// The most occupant requests that wait. A larger room gets no more avatars in a session.
const MAX_QUEUED: usize = 256;

/// One request for an occupant.
#[derive(Debug)]
enum Job {
    /// The XEP-0084 metadata of a real JID. No metadata leads to the vCard.
    Metadata(BareJid),
    /// The vCard photo of a bare JID that has an open 1:1 chat but is not a contact.
    VCard { owner: BareJid, hash: String },
    /// The vCard of an occupant with no real JID.
    OccupantVCard { owner: Jid, hash: String },
}

type Reply = oneshot::Sender<Result<(), ClientError>>;

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The metadata node of `owner`. `reply` waits for the end of the whole fetch.
    Metadata {
        owner: BareJid,
        reply: Option<Reply>,
    },
    /// The data item of `owner` with this hash.
    Data {
        owner: BareJid,
        hash: String,
        reply: Option<Reply>,
    },
    /// We published our data item. The metadata comes next.
    PublishData { image: Image, reply: Reply },
    /// The vCard of `owner` (XEP-0153). With a `hash` we check the photo against it.
    /// Without a `hash` (a refresh) we take the hash from the photo. `reply` waits.
    VCard {
        owner: BareJid,
        hash: Option<String>,
        reply: Option<Reply>,
    },
    /// Our own vCard, before we change its photo. `image` is `None` to remove the photo.
    /// If `strict` is false, an error is logged and the reply is `Ok`.
    OwnVCard {
        image: Option<Image>,
        reply: Reply,
        strict: bool,
    },
    /// We set our vCard. `hash` goes in our presence.
    PublishVCard {
        hash: Option<String>,
        reply: Reply,
        strict: bool,
    },
    /// We published our metadata.
    PublishMetadata { image: Image, reply: Reply },
    /// We published an empty metadata element.
    Unpublish { reply: Reply },
    /// The vCard of an occupant, from its full JID. We check the photo against `hash`.
    OccupantVCard { owner: Jid, hash: String },
    /// A request from the queue. Its answer frees a place, and the next request goes out.
    Queued(Box<Pending>),
}

/// An image that `set_avatar` publishes.
#[derive(Debug)]
pub(crate) struct Image {
    hash: String,
    mime: String,
    data: Vec<u8>,
    width: u16,
    height: u16,
}

/// A command from the public API.
pub(crate) enum Command {
    Get {
        owner: BareJid,
        reply: oneshot::Sender<Result<Option<Avatar>, ClientError>>,
    },
    Refresh {
        owner: BareJid,
        reply: Reply,
    },
    Remove {
        reply: Reply,
    },
    /// Change only the photo of our vCard. `None` removes it.
    VCardPhoto {
        image: Option<(String, Vec<u8>)>,
        reply: Reply,
    },
    Set {
        mime: String,
        data: Vec<u8>,
        width: u16,
        height: u16,
        reply: Reply,
    },
}

impl ClientHandle {
    /// The stored avatar of `owner`, or `None`. It reads the store, so it does not wait
    /// for the server. It is a feature command, so it needs a session: offline it returns
    /// `NotConnected`. A frontend that needs avatars offline calls `avatars::load` with
    /// its own `Store`.
    pub async fn avatar(&self, owner: BareJid) -> Result<Option<Avatar>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Avatars(Command::Get { owner, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Ask the server for the avatar of `owner`, and store it. Use it for a user that
    /// is not a contact, for whom no PEP events arrive. Returns when the image is stored.
    /// If the owner has no XEP-0084 avatar, it uses the photo of the vCard.
    pub async fn refresh_avatar(&self, owner: BareJid) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Avatars(Command::Refresh { owner, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Stop showing our avatar: publish an empty metadata element (XEP-0084, 4.2).
    pub async fn remove_avatar(&self) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Avatars(Command::Remove { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Set the PHOTO of our vCard (XEP-0054) and tell our contacts with a presence
    /// (XEP-0153). It does not touch XEP-0084. `set_avatar` does both.
    pub async fn set_vcard_photo(&self, mime: String, data: Vec<u8>) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Avatars(Command::VCardPhoto {
            image: Some((mime, data)),
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Remove the PHOTO of our vCard. The other fields stay.
    pub async fn remove_vcard_photo(&self) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Avatars(Command::VCardPhoto {
            image: None,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Publish our avatar. `mime` is the image type, for example `image/png`.
    pub async fn set_avatar(
        &self,
        mime: String,
        data: Vec<u8>,
        width: u16,
        height: u16,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Avatars(Command::Set {
            mime,
            data,
            width,
            height,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    fetch_metadata(ctx, ctx.account.clone(), None);
}

// Store access.

/// The stored avatar of `owner`.
pub fn load(store: &Store, account_id: i64, owner: &BareJid) -> rusqlite::Result<Option<Avatar>> {
    load_key(store, account_id, owner.as_str())
}

/// The stored avatar of an owner key: a bare JID, or `service/node` for a space.
pub fn load_key(store: &Store, account_id: i64, owner: &str) -> rusqlite::Result<Option<Avatar>> {
    store
        .conn()
        .query_row(
            "SELECT hash, mime, data FROM avatars WHERE account_id = ?1 AND owner = ?2",
            params![account_id, owner],
            |row| {
                Ok(Avatar {
                    hash: row.get(0)?,
                    mime: row.get(1)?,
                    data: row.get(2)?,
                })
            },
        )
        .optional()
}

/// What `store_metadata` found.
pub(crate) struct Stored {
    pub changed: bool,
    pub needs_data: bool,
}

/// Store the hash and the type. The data stays if the hash is the same. Otherwise it goes.
pub(crate) fn store_metadata(
    store: &Store,
    account_id: i64,
    owner: &str,
    hash: &str,
    mime: Option<&str>,
) -> rusqlite::Result<Stored> {
    let conn = store.conn();
    match load_key(store, account_id, owner)? {
        Some(old) if old.hash == hash => {
            let changed = old.mime.as_deref() != mime;
            if changed {
                conn.execute(
                    "UPDATE avatars SET mime = ?3 WHERE account_id = ?1 AND owner = ?2",
                    params![account_id, owner, mime],
                )?;
            }
            Ok(Stored {
                changed,
                needs_data: old.data.is_none(),
            })
        }
        _ => {
            conn.execute(
                "INSERT OR REPLACE INTO avatars (account_id, owner, hash, mime, data)
                 VALUES (?1, ?2, ?3, ?4, NULL)",
                params![account_id, owner, hash, mime],
            )?;
            Ok(Stored {
                changed: true,
                needs_data: true,
            })
        }
    }
}

/// Store the image, if the stored hash is still `hash`.
pub(crate) fn store_data(
    store: &Store,
    account_id: i64,
    owner: &str,
    hash: &str,
    data: &[u8],
) -> rusqlite::Result<bool> {
    let n = store.conn().execute(
        "UPDATE avatars SET data = ?4 WHERE account_id = ?1 AND owner = ?2 AND hash = ?3",
        params![account_id, owner, hash, data],
    )?;
    Ok(n > 0)
}

pub(crate) fn remove(store: &Store, account_id: i64, owner: &str) -> rusqlite::Result<bool> {
    let n = store.conn().execute(
        "DELETE FROM avatars WHERE account_id = ?1 AND owner = ?2",
        params![account_id, owner],
    )?;
    Ok(n > 0)
}

/// SHA-1 of `data`, in lower case hex.
pub fn sha1_hex(data: &[u8]) -> String {
    hex(&Sha1::digest(data))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Check the size of an image and its SHA-1 against `hash` (lower case hex).
pub(crate) fn verify_image(hash: &str, data: &[u8]) -> Result<(), ClientError> {
    if data.len() > MAX_AVATAR_BYTES {
        return Err(ClientError::Invalid("avatar is too big".into()));
    }
    if sha1_hex(data) != hash {
        return Err(ClientError::Invalid("avatar hash mismatch".into()));
    }
    Ok(())
}

/// The image type, from the first bytes of the image. XEP-0153 says to trust the data
/// and not the `TYPE` hint. Only png, jpeg, gif and webp: never SVG. The avatar scheme of
/// the desktop app serves nothing else.
pub fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn mark_changed(ctx: &mut Ctx<'_>, owner: &BareJid) {
    ctx.changed(ViewKey::Timeline(owner.clone()));
    ctx.changed(ViewKey::MemberList(owner.clone()));
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
    // The rooms where the owner is an occupant with this real JID.
    let rooms: Vec<String> = ctx
        .store
        .conn()
        .prepare_cached("SELECT room FROM occupants WHERE account_id = ?1 AND real_jid = ?2")
        .and_then(|mut stmt| {
            stmt.query_map(params![ctx.account_id, owner.as_str()], |row| row.get(0))?
                .collect()
        })
        .unwrap_or_default();
    for room in rooms.iter().filter_map(|r| BareJid::new(r).ok()) {
        mark_room(ctx, &room);
    }
}

fn mark_room(ctx: &mut Ctx<'_>, room: &BareJid) {
    ctx.changed(ViewKey::Timeline(room.clone()));
    ctx.changed(ViewKey::MemberList(room.clone()));
}

/// A short text for an error answer.
fn describe(error: &StanzaError) -> String {
    match error.texts.values().next() {
        Some(text) => format!("{:?}: {text}", error.defined_condition),
        None => format!("{:?}", error.defined_condition),
    }
}

fn done(reply: Option<Reply>, result: Result<(), ClientError>) {
    if let Some(reply) = reply {
        let _ = reply.send(result);
    }
}

// Events and requests.

/// A PEP event on the avatar metadata node of `owner`.
pub(crate) fn on_metadata_event(ctx: &mut Ctx<'_>, owner: &BareJid, payload: Payload) {
    match payload {
        Payload::Items {
            published,
            retracted,
            ..
        } => {
            // The newest item is the last one.
            match published.iter().rev().find_map(|i| i.payload.clone()) {
                Some(element) => apply_metadata(ctx, owner, element, None),
                None if !retracted.is_empty() => remove_avatar(ctx, owner),
                None => {}
            }
        }
        Payload::Purge { .. } | Payload::Delete { .. } => remove_avatar(ctx, owner),
        Payload::Configuration { .. } | Payload::Subscription { .. } => {}
    }
}

fn remove_avatar(ctx: &mut Ctx<'_>, owner: &BareJid) {
    ctx.state.avatars.pep.remove(owner);
    ctx.state.avatars.vcard.remove(owner);
    match remove(ctx.store, ctx.account_id, owner.as_str()) {
        Ok(true) => mark_changed(ctx, owner),
        Ok(false) => {}
        Err(e) => ctx.store_error("remove an avatar", e),
    }
}

/// Handle one metadata element of `owner`. `reply` waits for the image, if one is needed.
fn apply_metadata(ctx: &mut Ctx<'_>, owner: &BareJid, element: Element, reply: Option<Reply>) {
    let metadata = match Metadata::try_from(element) {
        Ok(metadata) => metadata,
        Err(e) => {
            log::warn!("bad avatar metadata from {owner}: {e}");
            done(
                reply,
                Err(ClientError::Invalid("bad avatar metadata".into())),
            );
            return;
        }
    };
    // An info with a URL is an extra copy. Prefer an image that the data node holds. If
    // every info has a URL, the image comes from the URL (XEP-0084 4.2).
    let info = metadata
        .infos
        .iter()
        .find(|i| i.url.is_none())
        .or(metadata.infos.first());
    let Some(info) = info else {
        no_metadata(ctx, owner, reply);
        return;
    };
    let hash = info.id.to_hex();
    // XEP-0084 wins over a vCard photo from now on.
    ctx.state.avatars.pep.insert(owner.clone());
    ctx.state.avatars.vcard.remove(owner);
    let stored = match store_metadata(
        ctx.store,
        ctx.account_id,
        owner.as_str(),
        &hash,
        Some(&info.type_),
    ) {
        Ok(stored) => stored,
        Err(e) => {
            ctx.store_error("store avatar metadata", e);
            done(reply, Err(ClientError::Invalid("store error".into())));
            return;
        }
    };
    if stored.changed {
        mark_changed(ctx, owner);
    }
    if !stored.needs_data {
        done(reply, Ok(()));
        return;
    }
    if info.bytes as usize > MAX_AVATAR_BYTES {
        log::warn!(
            "avatar of {owner} is too big ({} bytes). Not fetched.",
            info.bytes
        );
        done(reply, Err(ClientError::Invalid("avatar is too big".into())));
        return;
    }
    if !ctx
        .state
        .avatars
        .fetching
        .insert((owner.clone(), hash.clone()))
    {
        // A fetch of the same image runs already.
        done(reply, Ok(()));
        return;
    }
    if let Some(url) = info.url.clone() {
        ctx.download_avatar(UserDownload {
            owner: owner.clone(),
            hash,
            url,
            reply,
        });
        return;
    }
    let items = Items {
        max_items: None,
        node: NodeName(NODE_DATA.to_owned()),
        subid: None,
        items: vec![Item {
            id: Some(ItemId(hash.clone())),
            publisher: None,
            payload: None,
        }],
    };
    let iq = Iq::from_get("", PubSub::Items(items)).with_to(Jid::from(owner.clone()));
    ctx.request(
        iq,
        FeaturePending::Avatars(Pending::Data {
            owner: owner.clone(),
            hash,
            reply,
        }),
    );
}

/// Ask for the newest metadata item of `owner`. The own account has no `to`.
fn fetch_metadata(ctx: &mut Ctx<'_>, owner: BareJid, reply: Option<Reply>) {
    let mut items = Items::new(NODE_METADATA);
    items.max_items = Some(1);
    let mut iq = Iq::from_get("", PubSub::Items(items));
    if owner != *ctx.account {
        iq = iq.with_to(Jid::from(owner.clone()));
    }
    ctx.request(
        iq,
        FeaturePending::Avatars(Pending::Metadata { owner, reply }),
    );
}

/// The owner has no XEP-0084 avatar. A refresh (`reply` is set) asks for the vCard photo.
/// Other cases remove the avatar. The stored row stays until the vCard answers, so a
/// photo does not flicker.
fn no_metadata(ctx: &mut Ctx<'_>, owner: &BareJid, reply: Option<Reply>) {
    if reply.is_none() || owner == ctx.account {
        remove_avatar(ctx, owner);
        done(reply, Ok(()));
        return;
    }
    // XEP-0084 no longer wins for this owner.
    ctx.state.avatars.pep.remove(owner);
    if !ctx
        .state
        .avatars
        .fetching
        .insert((owner.clone(), String::new()))
    {
        // A refresh of this vCard runs already.
        done(reply, Ok(()));
        return;
    }
    // XEP-0153: the request goes to the bare JID.
    let iq = Iq::from_get("", VCardQuery).with_to(Jid::from(owner.clone()));
    ctx.request(
        iq,
        FeaturePending::Avatars(Pending::VCard {
            owner: owner.clone(),
            hash: None,
            reply,
        }),
    );
}

/// The items of a pubsub result.
fn result_items(payload: Option<Element>) -> Option<Vec<Item>> {
    match PubSub::try_from(payload?).ok()? {
        PubSub::Items(items) => Some(items.items),
        _ => None,
    }
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Queued(inner) => {
            let state = &mut ctx.state.avatars;
            state.in_flight = state.in_flight.saturating_sub(1);
            on_response(ctx, *inner, response);
            pump(ctx);
        }
        Pending::OccupantVCard { owner, hash } => {
            let key = (owner.to_string(), hash.clone());
            ctx.state.avatars.occupant_fetching.remove(&key);
            let result = match response {
                IqResponse::Result(payload) => accept_occupant_vcard(ctx, &owner, &hash, payload),
                IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            if let Err(e) = result {
                log::debug!("vCard photo of {owner}: {e}");
                if !matches!(e, ClientError::NotConnected) {
                    remember(&mut ctx.state.avatars.occupant_failed, key);
                }
            }
        }
        Pending::Metadata { owner, reply } => match response {
            IqResponse::Result(payload) => {
                let items = result_items(payload).unwrap_or_default();
                match items.into_iter().rev().find_map(|i| i.payload) {
                    Some(element) => apply_metadata(ctx, &owner, element, reply),
                    None => no_metadata(ctx, &owner, reply),
                }
            }
            IqResponse::Error(e) => {
                if matches!(
                    e.defined_condition,
                    DefinedCondition::ItemNotFound
                        | DefinedCondition::ServiceUnavailable
                        | DefinedCondition::FeatureNotImplemented
                ) {
                    // The owner has no metadata node: no XEP-0084 avatar.
                    no_metadata(ctx, &owner, reply);
                } else {
                    log::debug!("avatar metadata of {owner}: {}", describe(&e));
                    done(reply, Err(ClientError::Server(describe(&e))));
                }
            }
            IqResponse::Lost => done(reply, Err(ClientError::NotConnected)),
        },
        Pending::Data { owner, hash, reply } => {
            ctx.state
                .avatars
                .fetching
                .remove(&(owner.clone(), hash.clone()));
            match response {
                IqResponse::Result(payload) => {
                    let result = accept_data(ctx, &owner, &hash, payload);
                    done(reply, result);
                }
                IqResponse::Error(e) => {
                    log::debug!("avatar data of {owner}: {}", describe(&e));
                    done(reply, Err(ClientError::Server(describe(&e))));
                }
                IqResponse::Lost => done(reply, Err(ClientError::NotConnected)),
            }
        }
        Pending::VCard { owner, hash, reply } => {
            let key = hash.clone().unwrap_or_default();
            ctx.state.avatars.fetching.remove(&(owner.clone(), key));
            let result = match response {
                IqResponse::Result(payload) => accept_vcard(ctx, &owner, hash.as_deref(), payload),
                IqResponse::Error(e) => {
                    log::debug!("vCard of {owner}: {}", describe(&e));
                    if hash.is_none() && e.defined_condition == DefinedCondition::ItemNotFound {
                        // A refresh: the owner has no vCard, so no photo.
                        remove_avatar(ctx, &owner);
                        Ok(())
                    } else {
                        Err(ClientError::Server(describe(&e)))
                    }
                }
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            if let Err(e) = &result {
                log::debug!("vCard photo of {owner}: {e}");
                // After a lost session the next presence can start the fetch again.
                if let (Some(hash), false) = (hash, matches!(e, ClientError::NotConnected)) {
                    remember(&mut ctx.state.avatars.vcard_failed, (owner, hash));
                }
            }
            done(reply, result);
        }
        Pending::OwnVCard {
            image,
            reply,
            strict,
        } => match response {
            IqResponse::Result(payload) => {
                let vcard = payload
                    .and_then(|p| VCard::try_from(p).ok())
                    .unwrap_or(VCard {
                        photo: None,
                        payloads: vec![],
                    });
                publish_vcard(ctx, vcard, image, reply, strict);
            }
            // Some servers answer item-not-found for a vCard that does not exist.
            IqResponse::Error(e) if e.defined_condition == DefinedCondition::ItemNotFound => {
                let vcard = VCard {
                    photo: None,
                    payloads: vec![],
                };
                publish_vcard(ctx, vcard, image, reply, strict);
            }
            IqResponse::Error(e) => vcard_failed(reply, strict, ClientError::Server(describe(&e))),
            IqResponse::Lost => vcard_failed(reply, strict, ClientError::NotConnected),
        },
        Pending::PublishVCard {
            hash,
            reply,
            strict,
        } => match response {
            IqResponse::Result(_) => {
                ctx.send(vcard_presence(hash.as_deref()));
                let _ = reply.send(Ok(()));
            }
            IqResponse::Error(e) => vcard_failed(reply, strict, ClientError::Server(describe(&e))),
            IqResponse::Lost => vcard_failed(reply, strict, ClientError::NotConnected),
        },
        Pending::PublishData { image, reply } => match response {
            IqResponse::Result(_) => publish_metadata(ctx, image, reply),
            IqResponse::Error(e) => {
                let _ = reply.send(Err(ClientError::Server(describe(&e))));
            }
            IqResponse::Lost => {
                let _ = reply.send(Err(ClientError::NotConnected));
            }
        },
        Pending::Unpublish { reply } => match response {
            IqResponse::Result(_) => {
                let account = ctx.account.clone();
                remove_avatar(ctx, &account);
                start_vcard(ctx, None, reply, false);
            }
            IqResponse::Error(e) => {
                let _ = reply.send(Err(ClientError::Server(describe(&e))));
            }
            IqResponse::Lost => {
                let _ = reply.send(Err(ClientError::NotConnected));
            }
        },
        Pending::PublishMetadata { image, reply } => match response {
            IqResponse::Result(_) => {
                let account = ctx.account.clone();
                let (store, id) = (ctx.store, ctx.account_id);
                let key = account.as_str();
                let stored = store_metadata(store, id, key, &image.hash, Some(&image.mime))
                    .and_then(|_| store_data(store, id, key, &image.hash, &image.data));
                if let Err(e) = stored {
                    ctx.store_error("store our avatar", e);
                }
                mark_changed(ctx, &account);
                start_vcard(ctx, Some(image), reply, false);
            }
            IqResponse::Error(e) => {
                let _ = reply.send(Err(ClientError::Server(describe(&e))));
            }
            IqResponse::Lost => {
                let _ = reply.send(Err(ClientError::NotConnected));
            }
        },
    }
}

/// Check the image of a data result, and store it.
fn accept_data(
    ctx: &mut Ctx<'_>,
    owner: &BareJid,
    hash: &str,
    payload: Option<Element>,
) -> Result<(), ClientError> {
    let data = result_items(payload)
        .and_then(|items| items.into_iter().find_map(|i| i.payload))
        .and_then(|element| Data::try_from(element).ok())
        .map(|d| d.data);
    let Some(data) = data else {
        return Err(ClientError::Invalid("no avatar data in the answer".into()));
    };
    store_fetched(ctx, owner, hash, &data)
}

/// Check an image that we fetched for `owner` (size, SHA-1, and type from the bytes), and
/// store it. The type in the metadata is what the peer wrote, so the stored type is the
/// sniffed one. A file that is not png, jpeg, gif or webp (an SVG, for example) is dropped.
fn store_fetched(
    ctx: &mut Ctx<'_>,
    owner: &BareJid,
    hash: &str,
    data: &[u8],
) -> Result<(), ClientError> {
    if let Err(e) = verify_image(hash, data) {
        log::warn!("avatar data of {owner} is not valid ({e}). Dropped.");
        return Err(e);
    }
    let Some(mime) = sniff_mime(data) else {
        log::warn!("avatar data of {owner} is not a png, jpeg, gif or webp image. Dropped.");
        return Err(ClientError::Invalid("the avatar is not an image".into()));
    };
    let key = owner.as_str();
    let stored = store_metadata(ctx.store, ctx.account_id, key, hash, Some(mime))
        .and_then(|_| store_data(ctx.store, ctx.account_id, key, hash, data));
    match stored {
        Ok(true) => mark_changed(ctx, owner),
        // The owner changed the avatar while we fetched.
        Ok(false) => {}
        Err(e) => ctx.store_error("store avatar data", e),
    }
    Ok(())
}

/// An HTTP GET for the image of an avatar whose only info has a URL (XEP-0084 4.2). The
/// runtime runs it. It has the checks of a space avatar: https and public hosts only, a
/// size cap, the SHA-1 of the info, and the type from the bytes.
#[derive(Debug)]
pub(crate) struct UserDownload {
    pub owner: BareJid,
    /// The lower case SHA-1 hex of the image.
    pub hash: String,
    pub url: String,
    pub reply: Option<Reply>,
}

/// The result of a `UserDownload`.
#[derive(Debug)]
pub(crate) struct UserDownloadDone {
    pub request: UserDownload,
    pub result: Result<Vec<u8>, String>,
}

/// Run a download. Send the result to `done` as `Internal::AvatarDownloaded`.
#[cfg(feature = "native-session")]
pub(crate) fn start_download(
    request: UserDownload,
    done: futures_channel::mpsc::UnboundedSender<super::Internal>,
) {
    crate::runtime::spawn(async move {
        let result = crate::runtime::http_get(&request.url, MAX_AVATAR_BYTES).await;
        // An error means that the actor stopped. Nobody waits for the result.
        let _ = done.unbounded_send(super::Internal::AvatarDownloaded(UserDownloadDone {
            request,
            result,
        }));
    });
}

/// Without the native session there is no HTTP client yet. Fail at once.
#[cfg(not(feature = "native-session"))]
pub(crate) fn start_download(
    request: UserDownload,
    done: futures_channel::mpsc::UnboundedSender<super::Internal>,
) {
    let _ = done.unbounded_send(super::Internal::AvatarDownloaded(UserDownloadDone {
        request,
        result: Err("HTTP download is not available in this build".into()),
    }));
}

/// A download finished. Check the image and store it.
pub(crate) fn on_download_done(ctx: &mut Ctx<'_>, finished: UserDownloadDone) {
    let UserDownloadDone { request, result } = finished;
    let UserDownload {
        owner, hash, reply, ..
    } = request;
    ctx.state
        .avatars
        .fetching
        .remove(&(owner.clone(), hash.clone()));
    let result = match result {
        Ok(data) => store_fetched(ctx, &owner, &hash, &data),
        Err(e) => {
            log::warn!("avatar download for {owner}: {e}");
            Err(ClientError::Invalid(format!("the download failed: {e}")))
        }
    };
    done(reply, result);
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Get { owner, reply } => {
            let result = load(ctx.store, ctx.account_id, &owner)
                .map_err(|e| ClientError::Invalid(format!("store error: {e}")));
            let _ = reply.send(result);
        }
        Command::Refresh { owner, reply } => fetch_metadata(ctx, owner, Some(reply)),
        Command::VCardPhoto { image, reply } => {
            let image = match image {
                None => None,
                Some((mime, data)) => {
                    if data.is_empty()
                        || data.len() > MAX_PUBLISH_BYTES
                        || !mime.starts_with("image/")
                    {
                        let _ = reply.send(Err(ClientError::Invalid(
                            "a photo is an image of 1 byte to 64 KiB".into(),
                        )));
                        return;
                    }
                    Some(Image {
                        hash: sha1_hex(&data),
                        mime,
                        data,
                        width: 0,
                        height: 0,
                    })
                }
            };
            start_vcard(ctx, image, reply, true);
        }
        Command::Remove { reply } => {
            let item = Item::new(None, None, Some(Metadata { infos: vec![] }));
            let iq = publish_iq(NODE_METADATA, item);
            ctx.request_publish(iq, FeaturePending::Avatars(Pending::Unpublish { reply }));
        }
        Command::Set {
            mime,
            data,
            width,
            height,
            reply,
        } => {
            if data.is_empty() || data.len() > MAX_PUBLISH_BYTES || !mime.starts_with("image/") {
                let _ = reply.send(Err(ClientError::Invalid(
                    "an avatar is an image of 1 byte to 64 KiB".into(),
                )));
                return;
            }
            let image = Image {
                hash: sha1_hex(&data),
                mime,
                data,
                width,
                height,
            };
            let item = Item::new(
                Some(ItemId(image.hash.clone())),
                None,
                Some(Data {
                    data: image.data.clone(),
                }),
            );
            let iq = publish_iq(NODE_DATA, item);
            ctx.request_publish(
                iq,
                FeaturePending::Avatars(Pending::PublishData { image, reply }),
            );
        }
    }
}

/// Change the PHOTO of our vCard. We read our vCard first, so its other fields stay.
fn start_vcard(ctx: &mut Ctx<'_>, image: Option<Image>, reply: Reply, strict: bool) {
    // XEP-0054: a get without `to` asks for our own vCard.
    let iq = Iq::from_get("", VCardQuery);
    ctx.request(
        iq,
        FeaturePending::Avatars(Pending::OwnVCard {
            image,
            reply,
            strict,
        }),
    );
}

fn publish_vcard(
    ctx: &mut Ctx<'_>,
    mut vcard: VCard,
    image: Option<Image>,
    reply: Reply,
    strict: bool,
) {
    let hash = image.as_ref().map(|i| i.hash.clone());
    vcard.photo = image.map(|i| Photo {
        type_: PhotoType { data: i.mime },
        binval: Binval { data: i.data },
    });
    let iq = Iq::from_set("", vcard);
    ctx.request(
        iq,
        FeaturePending::Avatars(Pending::PublishVCard {
            hash,
            reply,
            strict,
        }),
    );
}

/// The vCard step failed. A step that is not strict follows a XEP-0084 change that worked.
fn vcard_failed(reply: Reply, strict: bool, error: ClientError) {
    if strict {
        let _ = reply.send(Err(error));
    } else {
        log::warn!("cannot update our vCard photo: {error}");
        let _ = reply.send(Ok(()));
    }
}

/// Our presence with the XEP-0153 update element. An empty `<photo/>` means no photo.
/// The initial presence in `presence.rs` uses it too, so each presence keeps the hash.
pub(crate) fn vcard_presence(hash: Option<&str>) -> Presence {
    let data = hash.and_then(|h| {
        let mut out = [0u8; 20];
        if h.len() != 40 {
            return None;
        }
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(h.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Some(out)
    });
    let update = VCardUpdate {
        photo: Some(UpdatePhoto { data }),
    };
    let mut presence = super::presence::initial();
    presence.payloads.push(update.into());
    presence
}

fn publish_metadata(ctx: &mut Ctx<'_>, image: Image, reply: Reply) {
    let Ok(id) = image.hash.parse::<Sha1HexAttribute>() else {
        let _ = reply.send(Err(ClientError::Invalid("bad hash".into())));
        return;
    };
    let metadata = Metadata {
        infos: vec![Info {
            bytes: image.data.len() as u32,
            width: Some(image.width),
            height: Some(image.height),
            id,
            type_: image.mime.clone(),
            url: None,
        }],
    };
    let item = Item::new(Some(ItemId(image.hash.clone())), None, Some(metadata));
    let iq = publish_iq(NODE_METADATA, item);
    ctx.request_publish(
        iq,
        FeaturePending::Avatars(Pending::PublishMetadata { image, reply }),
    );
}

/// A publish to our own PEP node, with the access model `open`.
fn publish_iq(node: &str, item: Item) -> Iq {
    let form = DataForm::new(
        DataFormType::Submit,
        PUBLISH_OPTIONS,
        vec![Field::text_single("pubsub#access_model", "open")],
    );
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

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Get { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::Refresh { reply, .. }
        | Command::Set { reply, .. }
        | Command::Remove { reply }
        | Command::VCardPhoto { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// A presence of a contact can carry the hash of its vCard photo (XEP-0153). Returns
/// false always: the roster must see the presence too.
pub(crate) fn on_presence(ctx: &mut Ctx<'_>, presence: &Presence) -> bool {
    on_vcard_hint(ctx, presence);
    false
}

fn on_vcard_hint(ctx: &mut Ctx<'_>, presence: &Presence) {
    if presence.type_ != PresenceType::None {
        return;
    }
    let Some(from) = &presence.from else {
        return;
    };
    let owner = from.to_bare();
    if owner == *ctx.account {
        return;
    }
    let Some(x) = presence
        .payloads
        .iter()
        .find(|p| p.is("x", xmpp_parsers::ns::VCARD_UPDATE))
    else {
        return;
    };
    let update = match VCardUpdate::try_from(x.clone()) {
        Ok(update) => update,
        Err(e) => {
            log::debug!("bad vcard update from {from}: {e}");
            return;
        }
    };
    // An empty update element means only that the client supports XEP-0153.
    let Some(photo) = update.photo else {
        return;
    };
    // The hash is a hint from a peer. Only a contact that we see, or a person with an open
    // 1:1 chat, can make us fetch. The second case waits in the queue, which has a limit.
    let contact = sees_contact(ctx, &owner);
    if !contact && !has_chat(ctx, &owner) {
        return;
    }
    let Some(hash) = photo.data.as_ref().map(|d| hex(d)) else {
        // The contact has no photo. Drop the one that we took from a vCard.
        if ctx.state.avatars.vcard.contains(&owner) {
            remove_avatar(ctx, &owner);
            mark_changed(ctx, &owner);
        }
        return;
    };
    let state = &ctx.state.avatars;
    if state.pep.contains(&owner)
        || state.fetching.contains(&(owner.clone(), hash.clone()))
        || state.vcard_failed.contains(&(owner.clone(), hash.clone()))
    {
        return;
    }
    match load(ctx.store, ctx.account_id, &owner) {
        Ok(Some(old)) if old.hash == hash && old.data.is_some() => return,
        Ok(_) => {}
        Err(e) => return ctx.store_error("read an avatar", e),
    }
    ctx.state
        .avatars
        .fetching
        .insert((owner.clone(), hash.clone()));
    if !contact {
        enqueue(ctx, Job::VCard { owner, hash });
        return;
    }
    // XEP-0153: the request goes to the bare JID.
    let iq = Iq::from_get("", VCardQuery).with_to(Jid::from(owner.clone()));
    ctx.request(
        iq,
        FeaturePending::Avatars(Pending::VCard {
            owner,
            hash: Some(hash),
            reply: None,
        }),
    );
}

/// True if we have a 1:1 chat with `owner`: a chat message, in or out.
fn has_chat(ctx: &Ctx<'_>, owner: &BareJid) -> bool {
    ctx.store
        .conn()
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM messages
             WHERE account_id = ?1 AND peer = ?2 AND kind = 'chat')",
            params![ctx.account_id, owner.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
}

/// A room presence of an occupant. `real` is the real bare JID, if the room gave it.
pub(crate) fn on_occupant(ctx: &mut Ctx<'_>, presence: &Presence, real: Option<BareJid>) {
    let Some(from) = &presence.from else {
        return;
    };
    if let Some(real) = real {
        if real == *ctx.account
            || ctx.state.avatars.pep.contains(&real)
            || !remember(&mut ctx.state.avatars.occupant_asked, real.clone())
        {
            return;
        }
        enqueue(ctx, Job::Metadata(real));
        return;
    }
    let Some(x) = presence
        .payloads
        .iter()
        .find(|p| p.is("x", xmpp_parsers::ns::VCARD_UPDATE))
    else {
        return;
    };
    let Ok(update) = VCardUpdate::try_from(x.clone()) else {
        return;
    };
    // An empty update element means only that the client supports XEP-0153.
    let Some(photo) = update.photo else {
        return;
    };
    let owner = from.to_string();
    let Some(hash) = photo.data.as_ref().map(|d| hex(d)) else {
        // The occupant has no photo. Drop the one that we stored.
        match remove(ctx.store, ctx.account_id, &owner) {
            Ok(true) => mark_room(ctx, &from.to_bare()),
            Ok(false) => {}
            Err(e) => ctx.store_error("remove an avatar", e),
        }
        return;
    };
    let key = (owner.clone(), hash.clone());
    let state = &ctx.state.avatars;
    if state.occupant_fetching.contains(&key) || state.occupant_failed.contains(&key) {
        return;
    }
    match load_key(ctx.store, ctx.account_id, &owner) {
        Ok(Some(old)) if old.hash == hash && old.data.is_some() => return,
        Ok(_) => {}
        Err(e) => return ctx.store_error("read an avatar", e),
    }
    ctx.state.avatars.occupant_fetching.insert(key);
    enqueue(
        ctx,
        Job::OccupantVCard {
            owner: from.clone(),
            hash,
        },
    );
}

fn enqueue(ctx: &mut Ctx<'_>, job: Job) {
    if ctx.state.avatars.queue.len() >= MAX_QUEUED {
        log::debug!("too many avatar requests for occupants: {job:?} dropped");
        match job {
            Job::OccupantVCard { owner, hash } => {
                ctx.state
                    .avatars
                    .occupant_fetching
                    .remove(&(owner.to_string(), hash));
            }
            Job::VCard { owner, hash } => {
                ctx.state.avatars.fetching.remove(&(owner, hash));
            }
            Job::Metadata(_) => {}
        }
        return;
    }
    ctx.state.avatars.queue.push_back(job);
    pump(ctx);
}

/// Send queued requests while there are free places.
fn pump(ctx: &mut Ctx<'_>) {
    while ctx.state.avatars.in_flight < MAX_IN_FLIGHT {
        let Some(job) = ctx.state.avatars.queue.pop_front() else {
            return;
        };
        ctx.state.avatars.in_flight += 1;
        let (iq, then) = match job {
            Job::Metadata(owner) => {
                let mut items = Items::new(NODE_METADATA);
                items.max_items = Some(1);
                let iq = Iq::from_get("", PubSub::Items(items)).with_to(Jid::from(owner.clone()));
                // Nobody waits for the answer. A missing node leads to the vCard.
                let (reply, _) = oneshot::channel();
                let reply = Some(reply);
                (iq, Pending::Metadata { owner, reply })
            }
            Job::VCard { owner, hash } => {
                // XEP-0153: the request goes to the bare JID.
                let iq = Iq::from_get("", VCardQuery).with_to(Jid::from(owner.clone()));
                let hash = Some(hash);
                (
                    iq,
                    Pending::VCard {
                        owner,
                        hash,
                        reply: None,
                    },
                )
            }
            Job::OccupantVCard { owner, hash } => {
                // XEP-0054: the request goes to the full JID. The room forwards it.
                let iq = Iq::from_get("", VCardQuery).with_to(owner.clone());
                (iq, Pending::OccupantVCard { owner, hash })
            }
        };
        ctx.request(iq, FeaturePending::Avatars(Pending::Queued(Box::new(then))));
    }
}

/// Check the photo of the vCard of an occupant against `hash`, and store it under the
/// full JID.
fn accept_occupant_vcard(
    ctx: &mut Ctx<'_>,
    owner: &Jid,
    hash: &str,
    payload: Option<Element>,
) -> Result<(), ClientError> {
    let photo = payload
        .and_then(|p| VCard::try_from(p).ok())
        .and_then(|v| v.photo)
        .ok_or_else(|| ClientError::Invalid("the vCard has no photo".into()))?;
    let data = photo.binval.data;
    verify_image(hash, &data)?;
    let mime = sniff_mime(&data)
        .map(str::to_owned)
        .ok_or_else(|| ClientError::Invalid("the photo is not an image".into()))?;
    let key = owner.to_string();
    store_metadata(ctx.store, ctx.account_id, &key, hash, Some(&mime))
        .and_then(|_| store_data(ctx.store, ctx.account_id, &key, hash, &data))
        .map_err(|e| {
            ctx.store_error("store an occupant photo", e);
            ClientError::Invalid("store error".into())
        })?;
    mark_room(ctx, &owner.to_bare());
    Ok(())
}

/// True if we see the presence of `owner`: a roster item with subscription `to` or `both`.
fn sees_contact(ctx: &Ctx<'_>, owner: &BareJid) -> bool {
    let subscription: Option<String> = ctx
        .store
        .conn()
        .query_row(
            "SELECT subscription FROM contacts WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, owner.as_str()],
            |row| row.get(0),
        )
        .optional()
        .unwrap_or(None);
    matches!(subscription.as_deref(), Some("to" | "both"))
}

/// Check the photo of a vCard result against `hash`, and store it. Without a `hash` (a
/// refresh) the hash is the SHA-1 of the photo, and a vCard without a photo removes the
/// avatar.
fn accept_vcard(
    ctx: &mut Ctx<'_>,
    owner: &BareJid,
    hash: Option<&str>,
    payload: Option<Element>,
) -> Result<(), ClientError> {
    let vcard = payload
        .and_then(|p| VCard::try_from(p).ok())
        .ok_or_else(|| ClientError::Invalid("no vCard in the answer".into()))?;
    let Some(photo) = vcard.photo else {
        if hash.is_none() {
            remove_avatar(ctx, owner);
            return Ok(());
        }
        return Err(ClientError::Invalid("the vCard has no photo".into()));
    };
    // XEP-0084 arrived while we fetched: it wins.
    if ctx.state.avatars.pep.contains(owner) {
        return Ok(());
    }
    let data = photo.binval.data;
    let hash = match hash {
        Some(hash) => hash.to_owned(),
        None => sha1_hex(&data),
    };
    verify_image(&hash, &data)?;
    let mime = sniff_mime(&data)
        .map(str::to_owned)
        .or_else(|| Some(photo.type_.data).filter(|t| t.starts_with("image/")))
        .ok_or_else(|| ClientError::Invalid("the photo is not an image".into()))?;
    let key = owner.as_str();
    let stored = store_metadata(ctx.store, ctx.account_id, key, &hash, Some(&mime))
        .and_then(|_| store_data(ctx.store, ctx.account_id, key, &hash, &data));
    match stored {
        Ok(_) => {
            ctx.state.avatars.vcard.insert(owner.clone());
            mark_changed(ctx, owner);
            Ok(())
        }
        Err(e) => {
            ctx.store_error("store a vCard photo", e);
            Err(ClientError::Invalid("store error".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::stanza_error::ErrorType;

    use super::*;
    use crate::features::testing::Harness;

    const BOB: &str = "bob@chord.localhost";

    fn bob() -> BareJid {
        BareJid::new(BOB).unwrap()
    }

    fn metadata_element(data: &[u8], mime: &str) -> Element {
        Metadata {
            infos: vec![Info {
                bytes: data.len() as u32,
                width: Some(8),
                height: Some(8),
                id: sha1_hex(data).parse().unwrap(),
                type_: mime.to_owned(),
                url: None,
            }],
        }
        .into()
    }

    fn event(payload: Option<Element>) -> Payload {
        Payload::Items {
            node: NodeName(NODE_METADATA.to_owned()),
            published: vec![xmpp_parsers::pubsub::event::Item {
                id: None,
                publisher: None,
                payload,
            }],
            retracted: vec![],
        }
    }

    fn data_result(bytes: &[u8]) -> Element {
        let item = Item::new(
            None,
            None,
            Some(Data {
                data: bytes.to_vec(),
            }),
        );
        PubSub::Items(Items {
            max_items: None,
            node: NodeName(NODE_DATA.to_owned()),
            subid: None,
            items: vec![item],
        })
        .into()
    }

    fn is_data(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Avatars(Pending::Data { .. }))
    }

    fn error(condition: DefinedCondition) -> StanzaError {
        StanzaError::new(ErrorType::Cancel, condition, "en", "no")
    }

    fn stored(h: &Harness) -> Option<Avatar> {
        load(&h.store, h.account_id, &bob()).unwrap()
    }

    fn deliver(h: &mut Harness, payload: Payload) {
        deliver_to(h, &bob(), payload);
    }

    fn deliver_to(h: &mut Harness, owner: &BareJid, payload: Payload) {
        h.with_ctx(|ctx| on_metadata_event(ctx, owner, payload));
    }

    #[test]
    fn metadata_event_stores_hash_and_asks_for_the_data() {
        let mut h = Harness::new();
        let image = b"\x89PNG\r\n\x1a\nimage bytes";
        deliver(&mut h, event(Some(metadata_element(image, "image/png"))));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.hash, sha1_hex(image));
        assert_eq!(avatar.mime.as_deref(), Some("image/png"));
        assert_eq!(avatar.data, None);
        let dirty = h.take_dirty();
        assert!(dirty.contains(&ViewKey::Timeline(bob())));
        assert!(dirty.contains(&ViewKey::MemberList(bob())));
        assert!(dirty.contains(&ViewKey::ChannelList(ChannelScope::Home)));
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to().unwrap().as_str(), BOB);
        let Iq::Get { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        let Ok(PubSub::Items(items)) = PubSub::try_from(payload.clone()) else {
            panic!("not an items get")
        };
        assert_eq!(items.node.0, NODE_DATA);
        assert_eq!(items.items[0].id.as_ref().unwrap().0, sha1_hex(image));
    }

    #[test]
    fn data_with_the_right_hash_is_stored() {
        let mut h = Harness::new();
        let image = b"\x89PNG\r\n\x1a\nimage bytes";
        deliver(&mut h, event(Some(metadata_element(image, "image/png"))));
        h.take_dirty();
        h.answer(is_data, Some(data_result(image)));
        assert_eq!(stored(&h).unwrap().data.as_deref(), Some(&image[..]));
        assert!(h.take_dirty().contains(&ViewKey::Timeline(bob())));
    }

    #[test]
    fn data_with_a_bad_hash_is_dropped() {
        let mut h = Harness::new();
        deliver(&mut h, event(Some(metadata_element(b"real", "image/png"))));
        h.answer(is_data, Some(data_result(b"forged")));
        assert_eq!(stored(&h).unwrap().data, None);
    }

    #[test]
    fn same_hash_with_data_is_not_fetched_again() {
        let mut h = Harness::new();
        let image = b"\x89PNG\r\n\x1a\nimage bytes";
        deliver(&mut h, event(Some(metadata_element(image, "image/png"))));
        h.answer(is_data, Some(data_result(image)));
        h.take_dirty();
        h.sent_iqs();
        deliver(&mut h, event(Some(metadata_element(image, "image/png"))));
        assert!(h.sent_iqs().is_empty());
        assert!(h.take_dirty().is_empty());
        assert!(stored(&h).unwrap().data.is_some());
    }

    #[test]
    fn a_new_hash_drops_the_old_data() {
        let mut h = Harness::new();
        deliver(&mut h, event(Some(metadata_element(b"one", "image/png"))));
        h.answer(is_data, Some(data_result(b"one")));
        deliver(&mut h, event(Some(metadata_element(b"two", "image/png"))));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.hash, sha1_hex(b"two"));
        assert_eq!(avatar.data, None);
    }

    #[test]
    fn a_fetch_that_runs_is_not_started_twice() {
        let mut h = Harness::new();
        let element = metadata_element(b"one", "image/png");
        deliver(&mut h, event(Some(element.clone())));
        deliver(&mut h, event(Some(element)));
        assert_eq!(h.sent_iqs().len(), 1);
    }

    #[test]
    fn empty_metadata_removes_the_avatar() {
        let mut h = Harness::new();
        deliver(&mut h, event(Some(metadata_element(b"one", "image/png"))));
        h.take_dirty();
        let empty: Element = Metadata { infos: vec![] }.into();
        deliver(&mut h, event(Some(empty)));
        assert_eq!(stored(&h), None);
        assert!(h.take_dirty().contains(&ViewKey::MemberList(bob())));
    }

    #[test]
    fn retract_and_node_delete_remove_the_avatar() {
        let mut h = Harness::new();
        deliver(&mut h, event(Some(metadata_element(b"one", "image/png"))));
        deliver(
            &mut h,
            Payload::Items {
                node: NodeName(NODE_METADATA.to_owned()),
                published: vec![],
                retracted: vec![ItemId("x".into())],
            },
        );
        assert_eq!(stored(&h), None);
        deliver(&mut h, event(Some(metadata_element(b"one", "image/png"))));
        deliver(
            &mut h,
            Payload::Delete {
                node: NodeName(NODE_METADATA.to_owned()),
                redirect: None,
            },
        );
        assert_eq!(stored(&h), None);
    }

    #[test]
    fn bad_metadata_changes_nothing() {
        let mut h = Harness::new();
        let junk: Element = "<foo xmlns='x'/>".parse().unwrap();
        deliver(&mut h, event(Some(junk)));
        assert_eq!(stored(&h), None);
        assert!(h.take_dirty().is_empty());
    }

    #[test]
    fn data_error_and_lost_keep_the_hash_and_allow_a_new_fetch() {
        let mut h = Harness::new();
        let element = metadata_element(b"one", "image/png");
        deliver(&mut h, event(Some(element.clone())));
        h.respond(
            is_data,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        assert_eq!(stored(&h).unwrap().data, None);
        deliver(&mut h, event(Some(element.clone())));
        h.respond(is_data, IqResponse::Lost);
        assert_eq!(stored(&h).unwrap().data, None);
        deliver(&mut h, event(Some(element)));
        assert_eq!(h.sent_iqs().len(), 3);
    }

    #[test]
    fn connect_fetches_our_metadata_from_our_own_account() {
        let mut h = Harness::new();
        h.with_ctx(on_connected);
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].to().is_none());
        let account = h.account.clone();
        let image = b"mine";
        let result: Element = PubSub::Items(Items {
            max_items: Some(1),
            node: NodeName(NODE_METADATA.to_owned()),
            subid: None,
            items: vec![Item {
                id: None,
                publisher: None,
                payload: Some(metadata_element(image, "image/png")),
            }],
        })
        .into();
        h.answer(
            |p| matches!(p, FeaturePending::Avatars(Pending::Metadata { .. })),
            Some(result),
        );
        let mine = load(&h.store, h.account_id, &account).unwrap().unwrap();
        assert_eq!(mine.hash, sha1_hex(image));
        assert_eq!(h.sent_iqs().len(), 1);
    }

    #[test]
    fn own_metadata_item_not_found_removes_the_row() {
        let mut h = Harness::new();
        let account = h.account.clone();
        let element = metadata_element(b"x", "image/png");
        deliver_to(&mut h, &account, event(Some(element)));
        h.with_ctx(on_connected);
        h.respond(
            |p| matches!(p, FeaturePending::Avatars(Pending::Metadata { .. })),
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        assert_eq!(load(&h.store, h.account_id, &account).unwrap(), None);
    }

    fn set_command(reply: Reply, mime: &str, data: Vec<u8>) -> Command {
        Command::Set {
            mime: mime.into(),
            data,
            width: 8,
            height: 8,
            reply,
        }
    }

    #[test]
    fn set_avatar_publishes_data_then_metadata_with_open_access() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        let image = b"png bytes".to_vec();
        h.with_ctx(|ctx| on_command(ctx, set_command(reply, "image/png", image.clone())));
        let sent = h.sent_iqs();
        let publish = |iq: &Iq| {
            let Iq::Set { payload, .. } = iq else {
                panic!("{iq:?}")
            };
            let Ok(PubSub::Publish {
                publish,
                publish_options,
            }) = PubSub::try_from(payload.clone())
            else {
                panic!("not a publish")
            };
            (publish, publish_options.unwrap().form.unwrap())
        };
        let (first, form) = publish(&sent[0]);
        assert_eq!(first.node.0, NODE_DATA);
        assert_eq!(first.items[0].id.as_ref().unwrap().0, sha1_hex(&image));
        let access = form
            .fields
            .iter()
            .find(|f| f.var.as_deref() == Some("pubsub#access_model"))
            .unwrap();
        assert_eq!(access.values, ["open"]);
        assert!(answer.try_recv().unwrap().is_none());

        h.answer(
            |p| matches!(p, FeaturePending::Avatars(Pending::PublishData { .. })),
            None,
        );
        let sent = h.sent_iqs();
        let (second, _) = publish(&sent[0]);
        assert_eq!(second.node.0, NODE_METADATA);
        h.answer(
            |p| matches!(p, FeaturePending::Avatars(Pending::PublishMetadata { .. })),
            None,
        );
        // The reply waits for the vCard step.
        assert!(answer.try_recv().unwrap().is_none());
        h.sent_iqs();
        h.answer(is_own_vcard, Some(own_vcard()));
        h.sent_iqs();
        h.answer(is_publish_vcard, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let account = h.account.clone();
        let mine = load(&h.store, h.account_id, &account).unwrap().unwrap();
        assert_eq!(mine.data, Some(image));
        assert!(h.take_dirty().contains(&ViewKey::MemberList(account)));
    }

    #[test]
    fn set_avatar_reports_server_errors_and_lost() {
        for (response, expected) in [
            (
                IqResponse::Error(error(DefinedCondition::Forbidden)),
                ClientError::Server("Forbidden: no".into()),
            ),
            (IqResponse::Lost, ClientError::NotConnected),
        ] {
            let mut h = Harness::new();
            let (reply, mut answer) = oneshot::channel();
            h.with_ctx(|ctx| on_command(ctx, set_command(reply, "image/png", vec![1, 2, 3])));
            h.respond(
                |p| matches!(p, FeaturePending::Avatars(Pending::PublishData { .. })),
                response,
            );
            assert_eq!(answer.try_recv().unwrap(), Some(Err(expected)));
        }
    }

    #[test]
    fn set_avatar_rejects_bad_input_and_offline_answers_not_connected() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, set_command(reply, "text/plain", vec![1])));
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        assert!(h.sent_iqs().is_empty());

        let (reply, mut answer) = oneshot::channel();
        offline(Command::Get {
            owner: bob(),
            reply,
        });
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn remove_publishes_empty_metadata_and_clears_our_row() {
        let mut h = Harness::new();
        let account = h.account.clone();
        deliver_to(
            &mut h,
            &account,
            event(Some(metadata_element(b"x", "image/png"))),
        );
        h.sent_iqs();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Remove { reply }));
        let sent = h.sent_iqs();
        let Iq::Set { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        let Ok(PubSub::Publish { publish, .. }) = PubSub::try_from(payload.clone()) else {
            panic!("not a publish")
        };
        let element = publish.items[0].payload.clone().unwrap();
        assert!(Metadata::try_from(element).unwrap().infos.is_empty());
        h.respond(
            |p| matches!(p, FeaturePending::Avatars(Pending::Unpublish { .. })),
            IqResponse::Lost,
        );
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
        assert!(load(&h.store, h.account_id, &account).unwrap().is_some());
    }

    #[test]
    fn get_reads_the_store() {
        let mut h = Harness::new();
        deliver(&mut h, event(Some(metadata_element(b"one", "image/png"))));
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Get {
                    owner: bob(),
                    reply,
                },
            )
        });
        let got = answer.try_recv().unwrap().unwrap().unwrap().unwrap();
        assert_eq!(got.hash, sha1_hex(b"one"));
    }

    // XEP-0153.

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nvcard image";

    fn base64(data: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let bits = chunk.iter().fold(0u32, |n, b| (n << 8) | u32::from(*b));
            let n = bits << (8 * (3 - chunk.len()));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn vcard_result(data: &[u8]) -> Element {
        format!(
            "<vCard xmlns='vcard-temp'><FN>Bob</FN><PHOTO><TYPE>image/jpeg</TYPE>\
             <BINVAL>{}</BINVAL></PHOTO></vCard>",
            base64(data)
        )
        .parse()
        .unwrap()
    }

    fn is_vcard(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Avatars(Pending::VCard { .. }))
    }

    fn contact(h: &Harness, jid: &str, subscription: &str) {
        h.store
            .conn()
            .execute(
                "INSERT OR REPLACE INTO contacts (account_id, jid, subscription)
                 VALUES (?1, ?2, ?3)",
                params![h.account_id, jid, subscription],
            )
            .unwrap();
    }

    fn presence_with(from: &str, x: &str) -> Presence {
        let mut p = Presence::new(PresenceType::None).with_from(Jid::new(from).unwrap());
        p.payloads.push(x.parse().unwrap());
        p
    }

    fn update(hash: &str) -> String {
        format!("<x xmlns='vcard-temp:x:update'><photo>{hash}</photo></x>")
    }

    fn hear(h: &mut Harness, presence: &Presence) -> bool {
        h.with_ctx(|ctx| on_presence(ctx, presence))
    }

    fn bob_phone() -> &'static str {
        "bob@chord.localhost/phone"
    }

    #[test]
    fn vcard_hash_of_a_contact_fetches_the_vcard_and_stores_the_photo() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        let hash = sha1_hex(PNG);
        // The presence still goes on to the roster.
        assert!(!hear(&mut h, &presence_with(bob_phone(), &update(&hash))));
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        // XEP-0153: the request goes to the bare JID.
        assert_eq!(sent[0].to().unwrap().as_str(), BOB);
        let Iq::Get { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        assert!(payload.is("vCard", "vcard-temp"));
        h.answer(is_vcard, Some(vcard_result(PNG)));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.hash, hash);
        // The image type comes from the data, not from the TYPE hint.
        assert_eq!(avatar.mime.as_deref(), Some("image/png"));
        assert_eq!(avatar.data.as_deref(), Some(PNG));
        assert!(h.take_dirty().contains(&ViewKey::MemberList(bob())));
        // The same hash again: nothing to fetch.
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn vcard_hash_in_upper_case_works() {
        let mut h = Harness::new();
        contact(&h, BOB, "to");
        let hash = sha1_hex(PNG).to_uppercase();
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        h.answer(is_vcard, Some(vcard_result(PNG)));
        assert_eq!(stored(&h).unwrap().hash, sha1_hex(PNG));
    }

    #[test]
    fn vcard_hint_is_ignored_when_it_does_not_come_from_a_contact_that_we_see() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        // No roster item.
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        // Only the contact sees us.
        contact(&h, BOB, "from");
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        contact(&h, BOB, "none");
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        // Our own account.
        hear(
            &mut h,
            &presence_with("alice@chord.localhost/other", &update(&hash)),
        );
        // Not an available presence.
        contact(&h, BOB, "both");
        let mut p = presence_with(bob_phone(), &update(&hash));
        p.type_ = PresenceType::Unavailable;
        hear(&mut h, &p);
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn vcard_hint_without_a_hash_fetches_nothing() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        for x in [
            "<x xmlns='vcard-temp:x:update'/>",
            "<x xmlns='vcard-temp:x:update'><photo/></x>",
            "<x xmlns='vcard-temp:x:update'><photo>not a hash</photo></x>",
            "<x xmlns='vcard-temp:x:update'><photo>abcd</photo></x>",
            "<x xmlns='other'><photo>a9993e364706816aba3e25717850c26c9cd0d89d</photo></x>",
        ] {
            hear(&mut h, &presence_with(bob_phone(), x));
        }
        hear(
            &mut h,
            &Presence::new(PresenceType::None).with_from(Jid::new(bob_phone()).unwrap()),
        );
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn xep_0084_wins_over_the_vcard_photo() {
        // The PEP avatar came first: the hint is ignored.
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        deliver(
            &mut h,
            event(Some(metadata_element(b"pep image", "image/png"))),
        );
        h.sent_iqs();
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        assert!(h.sent_iqs().is_empty());
        assert_eq!(stored(&h).unwrap().hash, sha1_hex(b"pep image"));

        // The vCard photo came first: the PEP avatar replaces it.
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        h.answer(is_vcard, Some(vcard_result(PNG)));
        h.sent_iqs();
        deliver(
            &mut h,
            event(Some(metadata_element(b"pep image", "image/png"))),
        );
        assert_eq!(stored(&h).unwrap().hash, sha1_hex(b"pep image"));
        assert_eq!(stored(&h).unwrap().data, None);
        // A later hint does not bring the vCard photo back.
        h.sent_iqs();
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn a_pep_avatar_that_arrives_during_the_vcard_fetch_wins() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        deliver(
            &mut h,
            event(Some(metadata_element(b"pep image", "image/png"))),
        );
        h.answer(is_vcard, Some(vcard_result(PNG)));
        assert_eq!(stored(&h).unwrap().hash, sha1_hex(b"pep image"));
    }

    #[test]
    fn a_vcard_fetch_that_runs_is_not_started_twice() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        let hash = sha1_hex(PNG);
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        hear(
            &mut h,
            &presence_with("bob@chord.localhost/laptop", &update(&hash)),
        );
        assert_eq!(h.sent_iqs().len(), 1);
    }

    #[test]
    fn a_new_vcard_hash_replaces_the_photo() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        h.answer(is_vcard, Some(vcard_result(PNG)));
        let other = b"\xff\xd8\xffjpeg image";
        hear(
            &mut h,
            &presence_with(bob_phone(), &update(&sha1_hex(other))),
        );
        h.answer(is_vcard, Some(vcard_result(other)));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.data.as_deref(), Some(&other[..]));
        assert_eq!(avatar.mime.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn a_photo_with_the_wrong_hash_is_dropped_and_not_asked_for_again() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        let hash = sha1_hex(PNG);
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        h.answer(is_vcard, Some(vcard_result(b"forged image")));
        assert_eq!(stored(&h), None);
        h.sent_iqs();
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn a_vcard_error_or_an_empty_vcard_is_not_asked_for_again_but_lost_is() {
        let hash = sha1_hex(PNG);
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        h.respond(
            is_vcard,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        assert_eq!(h.sent_iqs().len(), 1);

        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        h.answer(
            is_vcard,
            Some("<vCard xmlns='vcard-temp'/>".parse().unwrap()),
        );
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        assert_eq!(h.sent_iqs().len(), 1);

        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        h.respond(is_vcard, IqResponse::Lost);
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        assert_eq!(h.sent_iqs().len(), 2);
        assert_eq!(stored(&h), None);
    }

    #[test]
    fn a_photo_that_is_no_image_is_dropped() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        let text = b"plain text";
        hear(
            &mut h,
            &presence_with(bob_phone(), &update(&sha1_hex(text))),
        );
        let result: Element = format!(
            "<vCard xmlns='vcard-temp'><PHOTO><TYPE>text/plain</TYPE>\
             <BINVAL>{}</BINVAL></PHOTO></vCard>",
            base64(text)
        )
        .parse()
        .unwrap();
        h.answer(is_vcard, Some(result));
        assert_eq!(stored(&h), None);
    }

    #[test]
    fn an_unknown_format_uses_the_type_hint() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        let data = b"some other format";
        hear(
            &mut h,
            &presence_with(bob_phone(), &update(&sha1_hex(data))),
        );
        let result: Element = format!(
            "<vCard xmlns='vcard-temp'><PHOTO><TYPE>image/x-icon</TYPE>\
             <BINVAL>{}</BINVAL></PHOTO></vCard>",
            base64(data)
        )
        .parse()
        .unwrap();
        h.answer(is_vcard, Some(result));
        assert_eq!(stored(&h).unwrap().mime.as_deref(), Some("image/x-icon"));
    }

    #[test]
    fn an_empty_photo_removes_only_a_photo_that_came_from_a_vcard() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        h.answer(is_vcard, Some(vcard_result(PNG)));
        h.take_dirty();
        let empty = "<x xmlns='vcard-temp:x:update'><photo/></x>";
        hear(&mut h, &presence_with(bob_phone(), empty));
        assert_eq!(stored(&h), None);
        assert!(h.take_dirty().contains(&ViewKey::MemberList(bob())));

        // A XEP-0084 avatar stays.
        deliver(
            &mut h,
            event(Some(metadata_element(b"pep image", "image/png"))),
        );
        hear(&mut h, &presence_with(bob_phone(), empty));
        assert!(stored(&h).is_some());
    }

    // Refresh with the vCard fallback, and the vCard set.

    fn is_metadata(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Avatars(Pending::Metadata { .. }))
    }

    fn refresh(h: &mut Harness) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Refresh {
                    owner: bob(),
                    reply,
                },
            )
        });
        answer
    }

    fn metadata_result(element: Option<Element>) -> Element {
        PubSub::Items(Items {
            max_items: Some(1),
            node: NodeName(NODE_METADATA.to_owned()),
            subid: None,
            items: element
                .map(|payload| Item {
                    id: None,
                    publisher: None,
                    payload: Some(payload),
                })
                .into_iter()
                .collect(),
        })
        .into()
    }

    /// Check that the one IQ sent is a vCard get to bob.
    fn expect_vcard_get(h: &mut Harness) {
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0].to().unwrap().as_str(), BOB);
        let Iq::Get { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        assert!(payload.is("vCard", "vcard-temp"));
    }

    #[test]
    fn refresh_falls_back_to_the_vcard_when_the_node_is_missing() {
        let mut h = Harness::new();
        let mut answer = refresh(&mut h);
        h.sent_iqs();
        h.respond(
            is_metadata,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        expect_vcard_get(&mut h);
        assert!(answer.try_recv().unwrap().is_none());
        h.take_dirty();
        h.answer(is_vcard, Some(vcard_result(PNG)));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.hash, sha1_hex(PNG));
        assert_eq!(avatar.mime.as_deref(), Some("image/png"));
        assert_eq!(avatar.data.as_deref(), Some(PNG));
        assert!(h.take_dirty().contains(&ViewKey::MemberList(bob())));
    }

    #[test]
    fn refresh_falls_back_to_the_vcard_when_the_metadata_is_empty() {
        for result in [
            metadata_result(None),
            metadata_result(Some(Metadata { infos: vec![] }.into())),
        ] {
            let mut h = Harness::new();
            let mut answer = refresh(&mut h);
            h.sent_iqs();
            h.answer(is_metadata, Some(result));
            expect_vcard_get(&mut h);
            h.answer(is_vcard, Some(vcard_result(PNG)));
            assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
            assert_eq!(stored(&h).unwrap().data.as_deref(), Some(PNG));
        }
    }

    #[test]
    fn refresh_prefers_xep_0084_and_does_not_ask_for_the_vcard() {
        let mut h = Harness::new();
        let mut answer = refresh(&mut h);
        h.sent_iqs();
        let image = b"\x89PNG\r\n\x1a\npep image";
        h.answer(
            is_metadata,
            Some(metadata_result(Some(metadata_element(image, "image/png")))),
        );
        h.answer(is_data, Some(data_result(image)));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(h.sent_iqs().iter().all(|iq| {
            let Iq::Get { payload, .. } = iq else {
                return true;
            };
            !payload.is("vCard", "vcard-temp")
        }));
        assert_eq!(stored(&h).unwrap().hash, sha1_hex(image));
    }

    #[test]
    fn refresh_without_a_vcard_photo_removes_the_avatar() {
        let mut h = Harness::new();
        contact(&h, BOB, "both");
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        h.answer(is_vcard, Some(vcard_result(PNG)));
        h.sent_iqs();
        let mut answer = refresh(&mut h);
        h.sent_iqs();
        h.respond(
            is_metadata,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        // The old row stays until the vCard answers.
        assert!(stored(&h).is_some());
        h.answer(
            is_vcard,
            Some("<vCard xmlns='vcard-temp'/>".parse().unwrap()),
        );
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(stored(&h), None);
    }

    #[test]
    fn refresh_reports_other_errors_and_a_bad_vcard() {
        let mut h = Harness::new();
        let mut answer = refresh(&mut h);
        h.sent_iqs();
        h.respond(
            is_metadata,
            IqResponse::Error(error(DefinedCondition::Forbidden)),
        );
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server("Forbidden: no".into())))
        );
        assert!(h.sent_iqs().is_empty());

        // A vCard photo with a type that is no image fails the refresh.
        let mut h = Harness::new();
        let mut answer = refresh(&mut h);
        h.sent_iqs();
        h.respond(
            is_metadata,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        let text = b"plain text";
        let result: Element = format!(
            "<vCard xmlns='vcard-temp'><PHOTO><TYPE>text/plain</TYPE>\
             <BINVAL>{}</BINVAL></PHOTO></vCard>",
            base64(text)
        )
        .parse()
        .unwrap();
        h.answer(is_vcard, Some(result));
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        assert_eq!(stored(&h), None);
    }

    #[test]
    fn the_fetch_of_our_own_metadata_does_not_ask_for_the_vcard() {
        let mut h = Harness::new();
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.respond(
            is_metadata,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        assert!(h.sent_iqs().is_empty());
    }

    fn is_own_vcard(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Avatars(Pending::OwnVCard { .. }))
    }

    fn is_publish_vcard(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Avatars(Pending::PublishVCard { .. }))
    }

    fn own_vcard() -> Element {
        "<vCard xmlns='vcard-temp'><FN>Alice</FN><NICKNAME>al</NICKNAME>\
         <PHOTO><TYPE>image/png</TYPE><BINVAL>AAAA</BINVAL></PHOTO></vCard>"
            .parse()
            .unwrap()
    }

    fn sent_presence(h: &mut Harness) -> Vec<Presence> {
        h.take_sent()
            .into_iter()
            .filter_map(|s| match s {
                xmpp_parsers::stanza::Stanza::Presence(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    fn photo_hash(presence: &Presence) -> Option<Option<[u8; 20]>> {
        let x = presence
            .payloads
            .iter()
            .find(|p| p.is("x", xmpp_parsers::ns::VCARD_UPDATE))?;
        Some(VCardUpdate::try_from(x.clone()).unwrap().photo?.data)
    }

    #[test]
    fn set_vcard_photo_reads_the_vcard_keeps_its_fields_and_sends_a_presence() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        let image = PNG.to_vec();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::VCardPhoto {
                    image: Some(("image/png".into(), image.clone())),
                    reply,
                },
            )
        });
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        // A get without `to` reads our own vCard.
        assert!(sent[0].to().is_none());
        let Iq::Get { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        assert!(payload.is("vCard", "vcard-temp"));
        h.answer(is_own_vcard, Some(own_vcard()));

        let sent = h.sent_iqs();
        let Iq::Set { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        let vcard = VCard::try_from(payload.clone()).unwrap();
        let photo = vcard.photo.unwrap();
        assert_eq!(photo.binval.data, image);
        assert_eq!(photo.type_.data, "image/png");
        // The other fields stay.
        let names: Vec<_> = vcard.payloads.iter().map(|e| e.name().to_owned()).collect();
        assert_eq!(names, ["FN", "NICKNAME"]);
        assert!(answer.try_recv().unwrap().is_none());

        h.answer(is_publish_vcard, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let presences = sent_presence(&mut h);
        assert_eq!(presences.len(), 1);
        let hash = photo_hash(&presences[0]).unwrap().unwrap();
        assert_eq!(hex(&hash), sha1_hex(&image));
    }

    #[test]
    fn remove_vcard_photo_sends_an_empty_photo_element() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::VCardPhoto { image: None, reply }));
        h.sent_iqs();
        h.answer(is_own_vcard, Some(own_vcard()));
        let sent = h.sent_iqs();
        let Iq::Set { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        let vcard = VCard::try_from(payload.clone()).unwrap();
        assert!(vcard.photo.is_none());
        assert_eq!(vcard.payloads.len(), 2);
        h.answer(is_publish_vcard, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let presences = sent_presence(&mut h);
        assert_eq!(photo_hash(&presences[0]), Some(None));
    }

    #[test]
    fn a_missing_own_vcard_is_treated_as_empty() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::VCardPhoto {
                    image: Some(("image/png".into(), PNG.to_vec())),
                    reply,
                },
            )
        });
        h.sent_iqs();
        h.respond(
            is_own_vcard,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        let sent = h.sent_iqs();
        assert!(matches!(sent[0], Iq::Set { .. }));
        h.answer(is_publish_vcard, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn set_vcard_photo_rejects_bad_input_and_reports_errors() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::VCardPhoto {
                    image: Some(("text/plain".into(), vec![1])),
                    reply,
                },
            )
        });
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        assert!(h.sent_iqs().is_empty());

        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::VCardPhoto { image: None, reply }));
        h.sent_iqs();
        h.answer(is_own_vcard, Some(own_vcard()));
        h.sent_iqs();
        h.respond(
            is_publish_vcard,
            IqResponse::Error(error(DefinedCondition::NotAllowed)),
        );
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server("NotAllowed: no".into())))
        );
        assert!(sent_presence(&mut h).is_empty());
    }

    #[test]
    fn set_avatar_updates_the_vcard_last_and_ignores_a_vcard_error() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        let image = PNG.to_vec();
        h.with_ctx(|ctx| on_command(ctx, set_command(reply, "image/png", image.clone())));
        h.sent_iqs();
        h.answer(
            |p| matches!(p, FeaturePending::Avatars(Pending::PublishData { .. })),
            None,
        );
        h.sent_iqs();
        h.answer(
            |p| matches!(p, FeaturePending::Avatars(Pending::PublishMetadata { .. })),
            None,
        );
        // The reply waits for the vCard step.
        assert!(answer.try_recv().unwrap().is_none());
        let sent = h.sent_iqs();
        assert!(sent[0].to().is_none());
        h.answer(is_own_vcard, Some(own_vcard()));
        h.sent_iqs();
        h.respond(
            is_publish_vcard,
            IqResponse::Error(error(DefinedCondition::NotAllowed)),
        );
        // XEP-0084 worked, so `set_avatar` succeeds.
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(sent_presence(&mut h).is_empty());
    }

    #[test]
    fn remove_avatar_also_removes_the_vcard_photo() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Remove { reply }));
        h.sent_iqs();
        h.answer(
            |p| matches!(p, FeaturePending::Avatars(Pending::Unpublish { .. })),
            None,
        );
        assert!(answer.try_recv().unwrap().is_none());
        h.sent_iqs();
        h.answer(is_own_vcard, Some(own_vcard()));
        let sent = h.sent_iqs();
        let Iq::Set { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        assert!(VCard::try_from(payload.clone()).unwrap().photo.is_none());
        h.answer(is_publish_vcard, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn sha1_hex_matches_a_known_value() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    // Room occupants.

    const ROOM: &str = "dev@rooms.chord.localhost";

    fn room() -> BareJid {
        BareJid::new(ROOM).unwrap()
    }

    fn occupant_row(h: &Harness, nick: &str, real_jid: Option<&str>) {
        h.store
            .conn()
            .execute(
                "INSERT INTO occupants (account_id, room, nick, real_jid, affiliation, role)
                 VALUES (?1, ?2, ?3, ?4, 'none', 'participant')",
                params![h.account_id, ROOM, nick, real_jid],
            )
            .unwrap();
    }

    fn room_presence(nick: &str, x: Option<&str>) -> Presence {
        let mut p = Presence::new(PresenceType::None)
            .with_from(Jid::new(&format!("{ROOM}/{nick}")).unwrap());
        if let Some(x) = x {
            p.payloads.push(x.parse().unwrap());
        }
        p
    }

    fn occupant(h: &mut Harness, presence: &Presence, real: Option<&str>) {
        let real = real.map(|r| BareJid::new(r).unwrap());
        h.with_ctx(|ctx| on_occupant(ctx, presence, real));
    }

    fn is_queued_metadata(p: &FeaturePending) -> bool {
        matches!(
            p,
            FeaturePending::Avatars(Pending::Queued(inner))
                if matches!(**inner, Pending::Metadata { .. })
        )
    }

    fn is_queued_bare_vcard(p: &FeaturePending) -> bool {
        matches!(
            p,
            FeaturePending::Avatars(Pending::Queued(inner))
                if matches!(**inner, Pending::VCard { .. })
        )
    }

    fn is_queued_vcard(p: &FeaturePending) -> bool {
        matches!(
            p,
            FeaturePending::Avatars(Pending::Queued(inner))
                if matches!(**inner, Pending::OccupantVCard { .. })
        )
    }

    #[test]
    fn an_occupant_with_a_real_jid_gets_the_avatar_of_that_jid() {
        let mut h = Harness::new();
        occupant_row(&h, "bobby", Some(BOB));
        let image = b"\x89PNG\r\n\x1a\nimage bytes";
        occupant(&mut h, &room_presence("bobby", None), Some(BOB));
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        // The request goes to the real bare JID, as for a contact.
        assert_eq!(sent[0].to().unwrap().as_str(), BOB);
        h.answer(
            is_queued_metadata,
            Some(metadata_result(Some(metadata_element(image, "image/png")))),
        );
        h.answer(is_data, Some(data_result(image)));
        assert_eq!(stored(&h).unwrap().data.as_deref(), Some(&image[..]));
        // The views of the room refresh, not only the views of the owner.
        let dirty = h.take_dirty();
        assert!(dirty.contains(&ViewKey::Timeline(room())));
        assert!(dirty.contains(&ViewKey::MemberList(room())));
    }

    #[test]
    fn an_occupant_with_a_real_jid_and_no_metadata_falls_back_to_the_vcard() {
        let mut h = Harness::new();
        occupant_row(&h, "bobby", Some(BOB));
        occupant(&mut h, &room_presence("bobby", None), Some(BOB));
        h.sent_iqs();
        h.respond(
            is_queued_metadata,
            IqResponse::Error(error(DefinedCondition::ItemNotFound)),
        );
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to().unwrap().as_str(), BOB);
        h.answer(is_vcard, Some(vcard_result(PNG)));
        assert_eq!(stored(&h).unwrap().data.as_deref(), Some(PNG));
    }

    #[test]
    fn a_real_jid_is_asked_once_in_a_session_and_never_for_us() {
        let mut h = Harness::new();
        occupant(&mut h, &room_presence("bobby", None), Some(BOB));
        assert_eq!(h.sent_iqs().len(), 1);
        occupant(&mut h, &room_presence("bobby", None), Some(BOB));
        occupant(&mut h, &room_presence("bob2", None), Some(BOB));
        assert!(h.sent_iqs().is_empty());
        occupant(
            &mut h,
            &room_presence("me", None),
            Some(crate::features::testing::ACCOUNT),
        );
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn an_anonymous_occupant_gets_the_vcard_from_its_full_jid() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        occupant(&mut h, &room_presence("anon", Some(&update(&hash))), None);
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        // XEP-0054: the request goes to the occupant. The room forwards it.
        assert_eq!(sent[0].to().unwrap().as_str(), format!("{ROOM}/anon"));
        let Iq::Get { payload, .. } = &sent[0] else {
            panic!("{sent:?}")
        };
        assert!(payload.is("vCard", "vcard-temp"));
        h.answer(is_queued_vcard, Some(vcard_result(PNG)));
        let key = format!("{ROOM}/anon");
        let avatar = load_key(&h.store, h.account_id, &key).unwrap().unwrap();
        assert_eq!(avatar.hash, hash);
        assert_eq!(avatar.mime.as_deref(), Some("image/png"));
        assert_eq!(avatar.data.as_deref(), Some(PNG));
        let dirty = h.take_dirty();
        assert!(dirty.contains(&ViewKey::Timeline(room())));
        assert!(dirty.contains(&ViewKey::MemberList(room())));
    }

    #[test]
    fn a_known_or_running_or_failed_hash_is_not_fetched_again() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        let presence = room_presence("anon", Some(&update(&hash)));
        occupant(&mut h, &presence, None);
        // The fetch runs.
        occupant(&mut h, &presence, None);
        assert_eq!(h.sent_iqs().len(), 1);
        h.answer(is_queued_vcard, Some(vcard_result(PNG)));
        h.sent_iqs();
        // The image is stored.
        occupant(&mut h, &presence, None);
        assert!(h.sent_iqs().is_empty());
        // A new hash asks again, and a failure stops the next try for that hash.
        let other = room_presence("anon", Some(&update(&sha1_hex(b"other"))));
        occupant(&mut h, &other, None);
        assert_eq!(h.sent_iqs().len(), 1);
        h.respond(
            is_queued_vcard,
            IqResponse::Error(error(DefinedCondition::ServiceUnavailable)),
        );
        occupant(&mut h, &other, None);
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn an_empty_photo_removes_the_stored_photo_of_an_occupant() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        occupant(&mut h, &room_presence("anon", Some(&update(&hash))), None);
        h.answer(is_queued_vcard, Some(vcard_result(PNG)));
        h.take_dirty();
        occupant(
            &mut h,
            &room_presence("anon", Some("<x xmlns='vcard-temp:x:update'><photo/></x>")),
            None,
        );
        let key = format!("{ROOM}/anon");
        assert!(load_key(&h.store, h.account_id, &key).unwrap().is_none());
        assert!(h.take_dirty().contains(&ViewKey::MemberList(room())));
    }

    #[test]
    fn a_bad_photo_of_an_occupant_is_not_stored() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        occupant(&mut h, &room_presence("anon", Some(&update(&hash))), None);
        h.answer(
            is_queued_vcard,
            Some(vcard_result(b"\x89PNG\r\n\x1a\nforged")),
        );
        let key = format!("{ROOM}/anon");
        assert!(load_key(&h.store, h.account_id, &key).unwrap().is_none());
    }

    #[test]
    fn only_a_few_occupant_requests_run_at_one_time() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        for i in 0..10 {
            let nick = format!("anon{i}");
            occupant(&mut h, &room_presence(&nick, Some(&update(&hash))), None);
        }
        assert_eq!(h.sent_iqs().len(), MAX_IN_FLIGHT);
        // Each answer frees a place for the next request.
        h.answer(is_queued_vcard, Some(vcard_result(PNG)));
        assert_eq!(h.sent_iqs().len(), 1);
        h.respond(is_queued_vcard, IqResponse::Lost);
        assert_eq!(h.sent_iqs().len(), 1);
    }

    // The type from the bytes, and the avatars that have only a URL.

    const URL_IMAGE: &[u8] = b"\x89PNG\r\n\x1a\nurl image";
    const IMAGE_URL: &str = "https://example.org/a.png";

    fn url_metadata(data: &[u8], url: &str) -> Element {
        Metadata {
            infos: vec![Info {
                bytes: data.len() as u32,
                width: Some(8),
                height: Some(8),
                id: sha1_hex(data).parse().unwrap(),
                type_: "image/png".into(),
                url: Some(url.into()),
            }],
        }
        .into()
    }

    fn take_downloads(h: &mut Harness) -> Vec<UserDownload> {
        let mut found = Vec::new();
        for effect in std::mem::take(&mut h.effects) {
            match effect {
                crate::features::Effect::AvatarDownload { request } => found.push(request),
                other => h.effects.push(other),
            }
        }
        found
    }

    fn finish(h: &mut Harness, request: UserDownload, result: Result<Vec<u8>, String>) {
        h.with_ctx(|ctx| on_download_done(ctx, UserDownloadDone { request, result }));
    }

    #[test]
    fn data_that_is_not_an_image_is_dropped() {
        let mut h = Harness::new();
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg'><script>1</script></svg>";
        deliver(&mut h, event(Some(metadata_element(svg, "image/png"))));
        h.answer(is_data, Some(data_result(svg)));
        assert_eq!(stored(&h).unwrap().data, None);
    }

    #[test]
    fn the_stored_type_comes_from_the_bytes_not_from_the_metadata() {
        let mut h = Harness::new();
        let image = b"\x89PNG\r\n\x1a\nreally a png";
        deliver(
            &mut h,
            event(Some(metadata_element(image, "image/svg+xml"))),
        );
        h.answer(is_data, Some(data_result(image)));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.mime.as_deref(), Some("image/png"));
        assert_eq!(avatar.data.as_deref(), Some(&image[..]));
    }

    #[test]
    fn sniff_mime_knows_four_types_and_not_svg() {
        assert_eq!(sniff_mime(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
        assert_eq!(sniff_mime(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        assert_eq!(sniff_mime(b"GIF89a.."), Some("image/gif"));
        assert_eq!(sniff_mime(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            sniff_mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            None
        );
        assert_eq!(sniff_mime(b"<?xml version='1.0'?><svg/>"), None);
        assert_eq!(sniff_mime(b""), None);
    }

    #[test]
    fn an_info_with_only_a_url_starts_a_download() {
        let mut h = Harness::new();
        deliver(&mut h, event(Some(url_metadata(URL_IMAGE, IMAGE_URL))));
        // No data node request.
        assert!(h.sent_iqs().is_empty());
        let mut downloads = take_downloads(&mut h);
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].url, IMAGE_URL);
        assert_eq!(downloads[0].hash, sha1_hex(URL_IMAGE));
        assert_eq!(downloads[0].owner, bob());
        // The same event again: the download runs already.
        deliver(&mut h, event(Some(url_metadata(URL_IMAGE, IMAGE_URL))));
        assert!(take_downloads(&mut h).is_empty());
        h.take_dirty();
        finish(&mut h, downloads.remove(0), Ok(URL_IMAGE.to_vec()));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.data.as_deref(), Some(URL_IMAGE));
        assert_eq!(avatar.mime.as_deref(), Some("image/png"));
        assert!(h.take_dirty().contains(&ViewKey::Timeline(bob())));
    }

    #[test]
    fn an_info_with_a_data_copy_wins_over_a_url_info() {
        let mut h = Harness::new();
        let element: Element = Metadata {
            infos: vec![
                Info {
                    bytes: 3,
                    width: None,
                    height: None,
                    id: sha1_hex(URL_IMAGE).parse().unwrap(),
                    type_: "image/png".into(),
                    url: Some(IMAGE_URL.into()),
                },
                Info {
                    bytes: 3,
                    width: None,
                    height: None,
                    id: sha1_hex(PNG).parse().unwrap(),
                    type_: "image/png".into(),
                    url: None,
                },
            ],
        }
        .into();
        deliver(&mut h, event(Some(element)));
        assert!(take_downloads(&mut h).is_empty());
        assert_eq!(h.sent_iqs().len(), 1);
    }

    #[test]
    fn a_url_download_with_a_wrong_hash_a_wrong_type_or_an_error_is_dropped() {
        for result in [
            Ok(b"\x89PNG\r\n\x1a\nforged".to_vec()),
            Ok(b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec()),
            Ok(vec![0x89; MAX_AVATAR_BYTES + 1]),
            Err("the download failed".to_string()),
        ] {
            let mut h = Harness::new();
            deliver(&mut h, event(Some(url_metadata(URL_IMAGE, IMAGE_URL))));
            let request = take_downloads(&mut h).remove(0);
            finish(&mut h, request, result);
            assert_eq!(stored(&h).unwrap().data, None);
            // The fetch is over: a new event can start it again.
            deliver(&mut h, event(Some(url_metadata(URL_IMAGE, IMAGE_URL))));
            assert_eq!(take_downloads(&mut h).len(), 1);
        }
    }

    #[test]
    fn a_url_info_that_is_too_big_is_not_downloaded() {
        let mut h = Harness::new();
        let element: Element = Metadata {
            infos: vec![Info {
                bytes: (MAX_AVATAR_BYTES + 1) as u32,
                width: None,
                height: None,
                id: sha1_hex(URL_IMAGE).parse().unwrap(),
                type_: "image/png".into(),
                url: Some(IMAGE_URL.into()),
            }],
        }
        .into();
        deliver(&mut h, event(Some(element)));
        assert!(take_downloads(&mut h).is_empty());
    }

    #[test]
    fn a_refresh_waits_for_the_url_download() {
        let mut h = Harness::new();
        let mut answer = refresh(&mut h);
        h.sent_iqs();
        h.answer(
            is_metadata,
            Some(metadata_result(Some(url_metadata(URL_IMAGE, IMAGE_URL)))),
        );
        assert_eq!(answer.try_recv().unwrap(), None);
        let request = take_downloads(&mut h).remove(0);
        finish(&mut h, request, Ok(URL_IMAGE.to_vec()));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn the_publish_limit_is_64_kib() {
        let mut h = Harness::new();
        let mut big = PNG.to_vec();
        big.resize(MAX_PUBLISH_BYTES + 1, 0);
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, set_command(reply, "image/png", big.clone())));
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        assert!(h.sent_iqs().is_empty());
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::VCardPhoto {
                    image: Some(("image/png".into(), big)),
                    reply,
                },
            )
        });
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        assert!(h.sent_iqs().is_empty());
        // The limit itself passes.
        let mut fit = PNG.to_vec();
        fit.resize(MAX_PUBLISH_BYTES, 0);
        let (reply, _answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, set_command(reply, "image/png", fit)));
        assert_eq!(h.sent_iqs().len(), 1);
    }

    // XEP-0153 from a person with an open 1:1 chat.

    fn chat_message(h: &Harness, peer: &str) {
        h.store
            .conn()
            .execute(
                "INSERT INTO messages (account_id, key_kind, key, direction, peer, sender, body,
                                       timestamp, kind)
                 VALUES (?1, 'origin-id', ?2, 'in', ?3, ?3, 'hi', 1, 'chat')",
                params![h.account_id, format!("k-{peer}"), peer],
            )
            .unwrap();
    }

    #[test]
    fn a_person_with_an_open_chat_who_is_no_contact_gets_a_vcard_fetch() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        // No chat yet: nothing.
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        assert!(h.sent_iqs().is_empty());
        chat_message(&h, BOB);
        hear(&mut h, &presence_with(bob_phone(), &update(&hash)));
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to().unwrap().as_str(), BOB);
        h.answer(is_queued_bare_vcard, Some(vcard_result(PNG)));
        let avatar = stored(&h).unwrap();
        assert_eq!(avatar.hash, hash);
        assert_eq!(avatar.data.as_deref(), Some(PNG));
    }

    #[test]
    fn a_group_chat_message_does_not_open_a_chat() {
        let mut h = Harness::new();
        h.store
            .conn()
            .execute(
                "INSERT INTO messages (account_id, key_kind, key, direction, peer, sender, body,
                                       timestamp, kind)
                 VALUES (?1, 'origin-id', 'g', 'in', ?2, ?2, 'hi', 1, 'groupchat')",
                params![h.account_id, BOB],
            )
            .unwrap();
        hear(&mut h, &presence_with(bob_phone(), &update(&sha1_hex(PNG))));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn the_chat_peer_fetches_keep_the_queue_limit() {
        let mut h = Harness::new();
        let hash = sha1_hex(PNG);
        for i in 0..(MAX_IN_FLIGHT + MAX_QUEUED + 5) {
            let peer = format!("p{i}@chord.localhost");
            chat_message(&h, &peer);
            hear(
                &mut h,
                &presence_with(&format!("{peer}/phone"), &update(&hash)),
            );
        }
        // Only MAX_IN_FLIGHT went out. The queue took MAX_QUEUED. The rest were dropped.
        assert_eq!(h.sent_iqs().len(), MAX_IN_FLIGHT);
        assert_eq!(h.with_ctx(|ctx| ctx.state.avatars.queue.len()), MAX_QUEUED);
        // A dropped one can be asked again later.
        let last = format!("p{}@chord.localhost", MAX_IN_FLIGHT + MAX_QUEUED + 4);
        let last_jid = BareJid::new(&last).unwrap();
        assert!(!h.with_ctx(|ctx| {
            ctx.state
                .avatars
                .fetching
                .contains(&(last_jid, hash.clone()))
        }));
    }

    #[test]
    fn a_set_that_remembers_owners_stays_under_its_limit() {
        let mut set = HashSet::new();
        for i in 0..MAX_REMEMBERED * 3 {
            assert!(remember(&mut set, i));
            assert!(set.len() <= MAX_REMEMBERED);
        }
        // An entry that is in the set is not new, and a full set keeps it.
        let last = MAX_REMEMBERED * 3 - 1;
        assert!(!remember(&mut set, last));
        assert!(set.contains(&last));
    }
}
