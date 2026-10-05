//! Receive an OMEMO message. `chat` calls `intake` before it stores a message, for live
//! messages, carbons, and archive results.
//!
//! `intake` finds the key for this device, opens it with the stored session (or a key
//! exchange), saves the session, decrypts the payload, parses the XEP-0420 envelope, and
//! checks it against the stanza. The result is a new message for `chat` to store: the
//! decrypted body and payloads, and from the outer stanza only the parts that are safe
//! outside (the ids, hints, chat states, markers, receipts, and the delay). Everything else
//! in the outer stanza is dropped: the server could add it.
//!
//! A message that cannot be read gives a placeholder with a short body, and the row gets
//! `encryption = omemo:2:failed`. The fallback body of the sender never shows. We do not
//! try again: the ratchet state moved, or the message was not for this device.
//!
//! An empty OMEMO message (no payload) only updates the session. No row comes of it.

use jid::BareJid;
use rusqlite::params;
use xmpp_parsers::delay::Delay;
use xmpp_parsers::message::{Lang, Message, MessageType};
use xmpp_parsers::minidom::Element;

use super::store::{self, TrustLevel};
use super::xml::{Encrypted, Envelope};
use super::{crypto, device, send, trust};
use crate::features::Ctx;
use crate::features::chat::MessageIds;
use crate::features::muc;

const NS_SID: &str = "urn:xmpp:sid:0";

/// The body of a message that this device cannot read.
pub(crate) const CANNOT_READ: &str = "Encrypted message that this device cannot read.";
/// The body of a message from a device that the user does not trust.
const FROM_DISTRUSTED: &str = "Encrypted message from a device that you do not trust.";

/// Where a message comes from.
#[derive(Clone, Copy)]
pub(crate) enum Source<'a> {
    /// A live message or a carbon.
    Live,
    /// An archive result (MAM) with this archive id.
    Archive(&'a str),
}

/// What `chat` does with a message.
pub(crate) enum Intake {
    /// Not an OMEMO message. Store the message as it is.
    Plain,
    /// The decrypted message. Store it with `encryption = omemo:2`.
    Decrypted(Box<Message>),
    /// The decrypted message of a device that has no trust decision yet. Store it with
    /// `encryption = omemo:2:untrusted`, so that the UI can warn.
    DecryptedUntrusted(Box<Message>),
    /// A placeholder for a message that we cannot read. Store it with
    /// `encryption = omemo:2:failed`.
    Failed(Box<Message>),
    /// Nothing to store: an empty message, or one that we have.
    Done,
}

/// How much of the message the store has already.
enum Stored {
    No,
    /// A row has the stanza-id. Nothing to do.
    ByStanzaId,
    /// A row has the origin-id: our own message, or a copy. `chat` gives it the stanza-id.
    ByOriginId,
}

/// Open the OMEMO part of `message`, if it has one.
pub(crate) fn intake(ctx: &mut Ctx<'_>, message: &Message, source: Source<'_>) -> Intake {
    let Some(parsed) = Encrypted::find(message) else {
        return Intake::Plain;
    };
    if !matches!(message.type_, MessageType::Chat | MessageType::Normal) {
        return Intake::Plain;
    }
    let Some(from) = &message.from else {
        return Intake::Plain;
    };
    let sender = from.to_bare();
    // A message in a room is not 1:1. OMEMO for rooms comes later.
    if muc::is_room(ctx, &sender) || message.payloads.iter().any(|p| p.is("x", muc::NS_MUC_USER)) {
        return Intake::Plain;
    }
    // The origin-id outside the envelope is not signed, so for a message of another
    // sender it is not used before the decrypt. The one inside is used after it.
    let outer_origin = (sender == *ctx.account)
        .then(|| MessageIds::of(message, ctx.account).origin_id)
        .flatten();
    match already_stored(ctx, message, source, &sender, outer_origin) {
        Stored::No => {}
        Stored::ByStanzaId => return Intake::Done,
        Stored::ByOriginId => return Intake::Failed(Box::new(placeholder(message, CANNOT_READ))),
    }
    let enc = match parsed {
        Ok(enc) => enc,
        Err(e) => {
            return failed(
                message,
                &format!("bad <encrypted/>: {e}"),
                CANNOT_READ,
                true,
            );
        }
    };
    let has_payload = enc.payload.is_some();
    let Some(engine) = &ctx.state.omemo.engine else {
        return failed(message, "no keys loaded", CANNOT_READ, has_payload);
    };
    let this = engine.keys.device_id();
    if sender == *ctx.account && enc.sid == this {
        // The copy of a message that this device sent. It has no key for this device.
        return failed(
            message,
            "a copy of our own message",
            CANNOT_READ,
            has_payload,
        );
    }
    let Some(entry) = enc.key_for(ctx.account, this) else {
        return failed(message, "no key for this device", CANNOT_READ, has_payload);
    };
    let session = store::load_session(
        ctx.store,
        &engine.sealer,
        ctx.account_id,
        sender.as_str(),
        enc.sid,
    )
    .ok()
    .flatten();
    let had_session = session.is_some();
    let opened = match crypto::decrypt_key(
        &engine.keys,
        session.as_ref().map(|s| s.as_slice()),
        &entry.data,
        entry.kex,
    ) {
        Ok(opened) => opened,
        // We read this message before.
        Err(crypto::OpenError::Duplicate) => return Intake::Done,
        Err(crypto::OpenError::Other(e)) => {
            return failed(
                message,
                &format!("the key does not open: {e}"),
                CANNOT_READ,
                has_payload,
            );
        }
    };
    let kex = entry.kex;

    // Open the payload and check the envelope first. Nothing about the sender device is
    // trusted or stored before the envelope fits the stanza (XEP-0420).
    let envelope = match &enc.payload {
        None => None,
        Some(payload) => {
            match open_envelope(ctx, &opened.material, payload, from, message.to.as_ref()) {
                Ok(envelope) => Some(envelope),
                Err(why) => {
                    // The ratchet did decrypt, so the message key is used up: keep the
                    // ratchet state and remove the used prekey. But do not learn the key
                    // or change any trust. If the key of the device is not the one that we
                    // know, the session comes from an unknown key: do not save it, or it
                    // could replace the session of the real device.
                    let save = !matches!(
                        trust::peek_key(ctx, &sender, enc.sid, &opened.remote_identity_key),
                        trust::KeyChange::Replaced
                    );
                    commit_ratchet(ctx, &sender, enc.sid, &opened, save);
                    return failed(message, &why, CANNOT_READ, true);
                }
            }
        }
    };

    // The key of the sender device: decide the trust before the session is saved, because
    // a changed key drops the old session.
    let (_, level) = trust::learn_key(ctx, &sender, enc.sid, &opened.remote_identity_key);
    commit_ratchet(ctx, &sender, enc.sid, &opened, true);

    // Answer a device only if we know its owner: our own account, a contact, or a peer that
    // we have a chat with. A stranger gets no empty message.
    let may_answer = matches!(source, Source::Live)
        && store::is_known_peer(
            ctx.store,
            ctx.account_id,
            ctx.account.as_str(),
            sender.as_str(),
        );
    // A new session from a key exchange: answer with an empty message, so that the other
    // device sees that the session works (XEP-0384, 4.3). History gets no answer.
    let mut answered = false;
    if kex && !had_session && may_answer {
        send::send_empty(ctx, &sender, enc.sid);
        answered = true;
    }
    // A heartbeat (XEP-0384, section 6): the sender used 53 messages or more on one ratchet
    // key without an answer. An empty message to that device moves the ratchet on. One
    // heartbeat for each ratchet key. A key exchange answer above is already such a message.
    if let Some(ratchet_key) = opened.heartbeat_ratchet_key.clone()
        && may_answer
    {
        let seen = (sender.clone(), enc.sid);
        let known = ctx.state.omemo.heartbeats.get(&seen) == Some(&ratchet_key);
        if !known {
            ctx.state.omemo.heartbeats.insert(seen, ratchet_key);
            if !answered {
                send::send_empty(ctx, &sender, enc.sid);
            }
        }
    }

    let Some(envelope) = envelope else {
        return Intake::Done;
    };
    if level == TrustLevel::Distrusted {
        return failed(message, "from a distrusted device", FROM_DISTRUSTED, true);
    }
    // The id inside the envelope is the one that the sender signed with the key. Check the
    // store with it: the id outside can come from the server.
    if let Some(inner) = envelope_origin_id(&envelope) {
        let again =
            |ctx: &Ctx<'_>| already_stored(ctx, message, source, &sender, Some(inner.clone()));
        match again(ctx) {
            Stored::No => {}
            Stored::ByStanzaId => return Intake::Done,
            Stored::ByOriginId if sender == *ctx.account => {
                return Intake::Failed(Box::new(placeholder(message, CANNOT_READ)));
            }
            Stored::ByOriginId => return Intake::Done,
        }
    }
    match merged(message, envelope, source) {
        Some(decrypted) if level == TrustLevel::Undecided => {
            Intake::DecryptedUntrusted(Box::new(decrypted))
        }
        Some(decrypted) => Intake::Decrypted(Box::new(decrypted)),
        None => failed(message, "the content does not parse", CANNOT_READ, true),
    }
}

/// Decrypt the payload and read the XEP-0420 envelope, and check it against the stanza.
/// `<from/>` is only a SHOULD for the sender (XEP-0384, 5.5.1), so a missing one passes
/// and goes to the log.
fn open_envelope(
    ctx: &Ctx<'_>,
    material: &[u8],
    payload: &[u8],
    from: &jid::Jid,
    to: Option<&jid::Jid>,
) -> Result<Envelope, String> {
    let plain = crypto::decrypt_payload(material, payload)
        .map_err(|e| format!("the payload does not open: {e}"))?;
    let envelope = Envelope::from_bytes(&plain).map_err(|e| format!("bad envelope: {e}"))?;
    let now = super::now_ms(ctx.store);
    envelope
        .check(from, to, false, now)
        .map_err(|e| format!("envelope check: {e}"))?;
    if envelope.from.is_none() {
        log::info!("the OMEMO envelope from {from} has no <from/>");
    }
    Ok(envelope)
}

/// Save the ratchet state after a decrypt that worked, and remove the used prekey. The
/// message key is used up, so this runs even when the message is then refused. With `save`
/// false the session is not saved (see the caller).
fn commit_ratchet(
    ctx: &mut Ctx<'_>,
    sender: &BareJid,
    device: u32,
    opened: &crypto::OpenedKey,
    save: bool,
) {
    if save
        && let Some(engine) = &ctx.state.omemo.engine
        && let Err(e) = store::save_session(
            ctx.store,
            &engine.sealer,
            ctx.account_id,
            sender.as_str(),
            device,
            &opened.session,
        )
    {
        ctx.store_error("save an OMEMO session", e);
    }
    if let Some(prekey) = opened.used_prekey {
        device::after_used_prekey(ctx, prekey);
    }
}

/// The origin-id in the content of the envelope.
fn envelope_origin_id(envelope: &Envelope) -> Option<String> {
    envelope
        .content
        .iter()
        .find(|c| c.is("origin-id", NS_SID))
        .and_then(|c| c.attr("id"))
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// A message that we cannot read. An empty OMEMO message makes no row, so it is `Done`.
fn failed(outer: &Message, why: &str, text: &str, has_payload: bool) -> Intake {
    log::info!("cannot read an OMEMO message from {:?}: {why}", outer.from);
    if has_payload {
        Intake::Failed(Box::new(placeholder(outer, text)))
    } else {
        Intake::Done
    }
}

/// A message with the parts of `outer` that are safe outside the envelope: no body, and no
/// payload that the sender or the server could have added to change a message.
fn base(outer: &Message) -> Message {
    let mut m = Message::new(outer.to.clone());
    m.from = outer.from.clone();
    m.id = outer.id.clone();
    m.type_ = outer.type_.clone();
    m.payloads = outer
        .payloads
        .iter()
        // The fallback body of the sender shows nothing real, so its XEP-0380 hint goes too.
        .filter(|p| send::is_outside(p) && !p.is("encryption", "urn:xmpp:eme:0"))
        .cloned()
        .collect();
    m
}

fn placeholder(outer: &Message, text: &str) -> Message {
    let mut m = base(outer);
    m.bodies.insert(Lang(String::new()), text.to_owned());
    m
}

/// The decrypted message: `base` and the content of the envelope. A `stanza-id` in the
/// content is dropped: only our server may add one. An `origin-id` in the content wins
/// over the one outside, and the `<time/>` of the envelope wins over the delay outside:
/// the envelope is the part that the sender protected.
fn merged(outer: &Message, envelope: Envelope, source: Source<'_>) -> Option<Message> {
    let mut m = base(outer);
    let time = envelope.time.clone();
    let mut wrapper = Element::builder("message", "jabber:client").build();
    for child in envelope.content {
        if !child.is("stanza-id", NS_SID) {
            wrapper.append_child(child);
        }
    }
    let inner = Message::try_from(wrapper).ok()?;
    if inner.payloads.iter().any(|p| p.is("origin-id", NS_SID)) {
        m.payloads.retain(|p| !p.is("origin-id", NS_SID));
    }
    m.bodies = inner.bodies;
    m.subjects = inner.subjects;
    m.thread = inner.thread;
    m.payloads.extend(inner.payloads);
    if let Some(stamp) = time {
        // A live message has a delay only when the server held it. Then the time of the
        // sender replaces it. History has the time of the archive outside, so it gets the
        // time of the sender in every case.
        let has_delay =
            |p: &Element| p.is("delay", "urn:xmpp:delay") || p.is("x", "jabber:x:delay");
        if matches!(source, Source::Archive(_)) || m.payloads.iter().any(has_delay) {
            m.payloads.retain(|p| !has_delay(p));
            m.payloads.push(Element::from(Delay {
                from: None,
                stamp,
                data: None,
            }));
        }
    }
    Some(m)
}

/// Look for the message in the store. `origin_id` is the id to trust: the one that comes
/// from outside only counts for a message of our own account.
fn already_stored(
    ctx: &Ctx<'_>,
    message: &Message,
    source: Source<'_>,
    sender: &BareJid,
    origin_id: Option<String>,
) -> Stored {
    let ids = MessageIds::of(message, ctx.account);
    let stanza_id = match source {
        Source::Archive(id) => Some(id.to_owned()),
        Source::Live => ids.stanza_id.clone(),
    };
    let exists = |sql: &str, args: &[&dyn rusqlite::ToSql]| {
        ctx.store.conn().query_row(sql, args, |_| Ok(())).is_ok()
    };
    if let Some(stanza_id) = stanza_id
        && exists(
            "SELECT 1 FROM messages WHERE account_id = ?1 AND stanza_id = ?2",
            params![ctx.account_id, stanza_id],
        )
    {
        return Stored::ByStanzaId;
    }
    let peer = if sender == ctx.account {
        message.to.as_ref().map(|to| to.to_bare())
    } else {
        Some(sender.clone())
    };
    if let (Some(origin_id), Some(peer)) = (origin_id, peer)
        && exists(
            "SELECT 1 FROM messages WHERE account_id = ?1 AND origin_id = ?2 AND peer = ?3",
            params![ctx.account_id, origin_id, peer.as_str()],
        )
    {
        return Stored::ByOriginId;
    }
    Stored::No
}
