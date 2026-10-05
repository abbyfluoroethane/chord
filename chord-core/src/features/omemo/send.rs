//! Send a message with OMEMO.
//!
//! `outgoing` is the one door for a 1:1 message. If OMEMO is off for the chat, or the
//! message has nothing secret, the message goes out as it is. Otherwise it waits for the
//! device lists and bundles (see `jobs`), then `encrypt_and_send` builds the XEP-0420
//! envelope, encrypts it once, encrypts the key for each trusted device of the peer and of
//! our own account, and sends the `<encrypted/>` message.
//!
//! Plain text never goes out for a message with secret parts. If there is no device, no
//! trusted device, or no keys, the message fails: its row gets `failed_at`, and the actor
//! emits `ClientEvent::OmemoSendFailed`.

use jid::{BareJid, Jid};
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::minidom::Element;

use super::jobs::{self, Change, Held, OnSent};
use super::store;
use super::xml::{Encrypted, Envelope, KeyEntry, RecipientKeys};
use super::{crypto, trust};
use crate::actor::{ClientError, ClientEvent};
use crate::features::{Ctx, new_id};
use crate::store::queries;

/// The namespaces of the parts that stay outside the envelope. The server or the other
/// client needs them without the key: the ids, the hints, chat states, markers, receipts,
/// and the time of a delayed message. Everything else is secret.
const OUTSIDE: &[&str] = &[
    "urn:xmpp:sid:0",
    "urn:xmpp:hints",
    "http://jabber.org/protocol/chatstates",
    "urn:xmpp:chat-markers:0",
    "urn:xmpp:receipts",
    "urn:xmpp:delay",
    "jabber:x:delay",
    "urn:xmpp:eme:0",
];

/// True if a payload element may stay outside the envelope.
pub(super) fn is_outside(el: &Element) -> bool {
    OUTSIDE.contains(&el.ns().as_str())
}

/// True if the message has no body and no payload that must be secret.
fn nothing_secret(message: &Message) -> bool {
    message.bodies.is_empty()
        && message.subjects.is_empty()
        && message.thread.is_none()
        && message.payloads.iter().all(is_outside)
}

/// Send a message of a 1:1 chat. Returns an error if the message cannot go out now and
/// plain text is not an option. `Ok` means that the message went out, or that it waits for
/// the keys of the peer and will go out or fail later (with an event).
pub(crate) fn outgoing(
    ctx: &mut Ctx<'_>,
    peer: &BareJid,
    message: Message,
) -> Result<(), ClientError> {
    if !super::is_on(ctx, peer) || nothing_secret(&message) {
        ctx.send(message);
        return Ok(());
    }
    precheck(ctx, peer)?;
    jobs::submit_send(ctx, message, peer.clone(), None);
    Ok(())
}

/// Send a change of an earlier message of a 1:1 chat: a correction, a retraction, or a
/// reaction. `target_id` is the id of that message and `target_encrypted` says that its row
/// has an encryption mark. `on_sent` makes the local change. It runs when the message goes
/// out: at once for plain text, or later for an encrypted message. If the send fails, it
/// does not run, and `OmemoSendFailed` names the target.
///
/// A change of an encrypted message never goes out as plain text, also when OMEMO is off
/// for the chat now.
pub(crate) fn outgoing_change(
    ctx: &mut Ctx<'_>,
    peer: &BareJid,
    message: Message,
    target_id: String,
    target_encrypted: bool,
    on_sent: OnSent,
) -> Result<(), ClientError> {
    if !super::is_on(ctx, peer) {
        if target_encrypted {
            return Err(ClientError::Invalid(
                "This message is encrypted. Turn OMEMO on for the chat to change it.".into(),
            ));
        }
        ctx.send(message);
        on_sent(ctx);
        return Ok(());
    }
    if nothing_secret(&message) {
        ctx.send(message);
        on_sent(ctx);
        return Ok(());
    }
    precheck(ctx, peer)?;
    let change = Change { target_id, on_sent };
    jobs::submit_send(ctx, message, peer.clone(), Some(change));
    Ok(())
}

/// The errors that we know before we ask the server.
fn precheck(ctx: &Ctx<'_>, peer: &BareJid) -> Result<(), ClientError> {
    if ctx.state.omemo.engine.is_none() {
        let why = ctx.state.omemo.problem.clone().unwrap_or_default();
        return Err(ClientError::Invalid(format!("OMEMO is not ready: {why}")));
    }
    let rows = store::list_devices(ctx.store, ctx.account_id, peer.as_str()).unwrap_or_default();
    let active: Vec<_> = rows.iter().filter(|r| r.active).collect();
    // We can only say "no trusted device" when we know every key.
    if !active.is_empty() && active.iter().all(|r| r.identity_key.is_some()) {
        let any = active.iter().any(|r| {
            r.identity_key.as_ref().is_some_and(|ik| {
                trust::level_for(ctx.store, ctx.account_id, peer.as_str(), ik)
                    .is_ok_and(trust::usable)
            })
        });
        if !any {
            return Err(ClientError::Invalid(no_trusted(peer)));
        }
    }
    Ok(())
}

fn no_trusted(peer: &BareJid) -> String {
    format!("No trusted OMEMO device of {peer}.")
}

/// Fail a message: mark its row, and tell the frontends. `id` is the `id` attribute of the
/// message.
pub(crate) fn fail(ctx: &mut Ctx<'_>, peer: &BareJid, id: Option<&str>, reason: &str) {
    if let Some(id) = id
        && let Err(e) = queries::mark_failed(ctx.store.conn(), ctx.account_id, peer.as_str(), id)
    {
        ctx.store_error("mark a message as failed", e);
    }
    log::info!("OMEMO message to {peer} failed: {reason}");
    super::timeline_changed(ctx, peer);
    ctx.emit(ClientEvent::OmemoSendFailed {
        peer: peer.clone(),
        message_id: id.unwrap_or_default().to_owned(),
        reason: reason.to_owned(),
    });
}

/// Fail a held message. A change of an earlier message has no row of its own: the failure
/// names the target, and the target row stays as it is.
pub(super) fn fail_held(ctx: &mut Ctx<'_>, held: Held, reason: &str) {
    match held.change {
        Some(change) => {
            log::info!(
                "OMEMO change of {} for {} failed: {reason}",
                change.target_id,
                held.peer
            );
            super::timeline_changed(ctx, &held.peer);
            ctx.emit(ClientEvent::OmemoSendFailed {
                peer: held.peer,
                message_id: change.target_id,
                reason: reason.to_owned(),
            });
        }
        None => {
            let id = held.message.id.as_ref().map(|i| i.0.as_str());
            fail(ctx, &held.peer, id, reason);
        }
    }
}

/// A device that gets a key.
struct Target {
    jid: BareJid,
    device: u32,
}

/// The devices of `jid` that may get a message key: active, key known, trust Blind or
/// Verified. Our own device is not in the list.
fn targets(ctx: &Ctx<'_>, jid: &BareJid, this: u32) -> Vec<Target> {
    let rows = store::list_devices(ctx.store, ctx.account_id, jid.as_str()).unwrap_or_default();
    rows.into_iter()
        .filter(|r| r.active && !(*jid == *ctx.account && r.device_id == this))
        .filter(|r| {
            r.identity_key.as_ref().is_some_and(|ik| {
                trust::level_for(ctx.store, ctx.account_id, jid.as_str(), ik)
                    .is_ok_and(trust::usable)
            })
        })
        .map(|r| Target {
            jid: jid.clone(),
            device: r.device_id,
        })
        .collect()
}

/// The active devices of `jid` whose key we do not know. They get no key: we have no
/// bundle for them. Our own device is not in the list.
fn unknown_key_devices(ctx: &Ctx<'_>, jid: &BareJid, this: u32) -> Vec<u32> {
    store::list_devices(ctx.store, ctx.account_id, jid.as_str())
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.active && r.identity_key.is_none())
        .filter(|r| !(*jid == *ctx.account && r.device_id == this))
        .map(|r| r.device_id)
        .collect()
}

/// The keys are here: encrypt `held` and send it, or fail it.
pub(super) fn encrypt_and_send(ctx: &mut Ctx<'_>, held: Held) {
    match build(ctx, &held.message, &held.peer) {
        Ok(out) => {
            ctx.send(out);
            if let Some(change) = held.change {
                (change.on_sent)(ctx);
            }
        }
        Err(reason) => fail_held(ctx, held, &reason),
    }
}

/// The `<encrypted/>` message for `message`, or the reason why there is none.
fn build(ctx: &mut Ctx<'_>, message: &Message, peer: &BareJid) -> Result<Message, String> {
    let peer = peer.clone();
    let Some(engine) = &ctx.state.omemo.engine else {
        let why = ctx.state.omemo.problem.clone().unwrap_or_default();
        return Err(format!("OMEMO is not ready: {why}"));
    };
    let this = engine.keys.device_id();
    let own = ctx.account.clone();

    let peer_rows =
        store::list_devices(ctx.store, ctx.account_id, peer.as_str()).unwrap_or_default();
    if !peer_rows.iter().any(|r| r.active) {
        return Err(format!("{peer} has no OMEMO device."));
    }
    let peer_targets = targets(ctx, &peer, this);
    if peer_targets.is_empty() {
        return Err(no_trusted(&peer));
    }
    // A chat with ourselves has one list of devices.
    let own_targets = if peer == own {
        Vec::new()
    } else {
        targets(ctx, &own, this)
    };

    // The envelope: the secret parts of the message.
    let secret: Vec<Element> = secret_parts(message);
    let envelope = Envelope {
        content: secret,
        rpad: super::xml::new_rpad(&super::random_bytes::<201>()),
        time: super::now_rfc3339(ctx.store).parse().ok(),
        from: Some(Jid::from(own.clone())),
        to: Some(Jid::from(peer.clone())),
    };
    let (material, payload) = match crypto::encrypt_payload(&envelope.to_bytes()) {
        Ok(done) => done,
        Err(e) => return Err(format!("Encryption failed: {e}")),
    };

    let mut groups = Vec::new();
    let mut peer_keys = 0;
    let mut skipped: Vec<(BareJid, Vec<u32>)> = Vec::new();
    for (n, (jid, list)) in [(&peer, peer_targets), (&own, own_targets)]
        .into_iter()
        .enumerate()
    {
        let mut entries = Vec::new();
        // Active devices with no key that we know (no bundle came) are left out too.
        let mut left_out = unknown_key_devices(ctx, jid, this);
        for target in list {
            match key_for(ctx, &target, &material) {
                Some(entry) => entries.push(entry),
                None => left_out.push(target.device),
            }
        }
        if !left_out.is_empty() {
            skipped.push((jid.clone(), left_out));
        }
        if n == 0 {
            peer_keys = entries.len();
        }
        if !entries.is_empty() {
            groups.push(RecipientKeys {
                jid: jid.clone(),
                keys: entries,
            });
        }
    }
    if peer_keys == 0 {
        return Err(format!("Chord has no way to encrypt for {peer} now."));
    }
    // Some devices got no key (no bundle, or the key changed). They cannot read this
    // message. Tell the frontends.
    for (jid, device_ids) in skipped {
        log::info!("no OMEMO key for devices {device_ids:?} of {jid}");
        ctx.emit(ClientEvent::OmemoDevicesSkipped {
            peer: jid,
            device_ids,
        });
    }
    let encrypted = Encrypted {
        sid: this,
        keys: groups,
        payload: Some(payload),
    };
    let mut out = Message::new(message.to.clone());
    out.type_ = MessageType::Chat;
    out.id = message.id.clone();
    out.payloads = message
        .payloads
        .iter()
        .filter(|p| is_outside(p))
        .cloned()
        .collect();
    super::xml::wrap(&mut out, &encrypted);
    Ok(out)
}

/// Encrypt the key material for one device, and save the new session. `None` if the device
/// has no session and no bundle, or if the library refuses.
fn key_for(ctx: &mut Ctx<'_>, target: &Target, material: &[u8]) -> Option<KeyEntry> {
    let account_id = ctx.account_id;
    let omemo = &mut ctx.state.omemo;
    let engine = omemo.engine.as_ref()?;
    let session = store::load_session(
        ctx.store,
        &engine.sealer,
        account_id,
        target.jid.as_str(),
        target.device,
    )
    .ok()
    .flatten();
    let bundle = match &session {
        Some(_) => None,
        None => omemo.bundles.remove(&(target.jid.clone(), target.device)),
    };
    // A cached bundle can be older than the key that we know for the device (the device came
    // back with a new key, and the trust was decided for the new one). Do not pair them.
    // The bundle is gone from the cache, so the next send asks for a new one.
    if let Some(b) = &bundle {
        let known = store::list_devices(ctx.store, account_id, target.jid.as_str())
            .ok()
            .and_then(|rows| rows.into_iter().find(|d| d.device_id == target.device))
            .and_then(|d| d.identity_key);
        if known.as_deref() != Some(b.ik.as_slice()) {
            log::info!(
                "the bundle of device {} of {} does not match its key: left out",
                target.device,
                target.jid
            );
            return None;
        }
    }
    if session.is_none() && bundle.is_none() {
        log::info!(
            "no session and no bundle for device {} of {}",
            target.device,
            target.jid
        );
        return None;
    }
    let sealed = match crypto::encrypt_key(
        &engine.keys,
        session.as_ref().map(|s| s.as_slice()),
        bundle.as_ref(),
        material,
    ) {
        Ok(sealed) => sealed,
        Err(e) => {
            log::warn!(
                "cannot encrypt for device {} of {}: {e}",
                target.device,
                target.jid
            );
            return None;
        }
    };
    if let Err(e) = store::save_session(
        ctx.store,
        &engine.sealer,
        account_id,
        target.jid.as_str(),
        target.device,
        &sealed.session,
    ) {
        // Without the new session, the next message would use an old ratchet state.
        ctx.store_error("save an OMEMO session", e);
        return None;
    }
    Some(KeyEntry {
        rid: target.device,
        kex: sealed.kex,
        data: sealed.data,
    })
}

/// The children of the envelope `<content/>`: the bodies and every payload that is not on
/// the outside list.
fn secret_parts(message: &Message) -> Vec<Element> {
    let mut inner = Message::new(None);
    inner.bodies = message.bodies.clone();
    inner.subjects = message.subjects.clone();
    inner.thread = message.thread.clone();
    // The origin-id goes inside too (and stays outside for servers and other clients). The
    // receiver prefers the one inside: it is the one that the envelope protects.
    inner.payloads = message
        .payloads
        .iter()
        .filter(|p| !is_outside(p) || p.is("origin-id", "urn:xmpp:sid:0"))
        .cloned()
        .collect();
    Element::from(inner).children().cloned().collect()
}

/// Send an empty OMEMO message to one device of `to`. It moves no content. It finishes a
/// key exchange: the other device sees an answer on the new session (XEP-0384, 4.3).
pub(super) fn send_empty(ctx: &mut Ctx<'_>, to: &BareJid, device: u32) {
    let Some(engine) = &ctx.state.omemo.engine else {
        return;
    };
    let this = engine.keys.device_id();
    let target = Target {
        jid: to.clone(),
        device,
    };
    // No new session from a bundle here: the empty message answers a session that exists.
    let has_session = store::load_session(
        ctx.store,
        &engine.sealer,
        ctx.account_id,
        to.as_str(),
        device,
    )
    .ok()
    .flatten()
    .is_some();
    if !has_session {
        return;
    }
    let Some(entry) = key_for(ctx, &target, &crypto::empty_material()) else {
        return;
    };
    let encrypted = Encrypted {
        sid: this,
        keys: vec![RecipientKeys {
            jid: to.clone(),
            keys: vec![entry],
        }],
        payload: None,
    };
    let mut message = Message::new(Some(Jid::from(to.clone())));
    message.type_ = MessageType::Chat;
    message.id = Some(Id(new_id()));
    message.payloads.push(Element::from(&encrypted));
    message
        .payloads
        .push(Element::builder("store", "urn:xmpp:hints").build());
    ctx.send(message);
}
