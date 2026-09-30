//! Message retraction (XEP-0424).
//!
//! A retraction keeps the row. It sets `retracted_at`, clears the text, and removes the
//! reactions, so the timeline can show "retracted". The `id` is the `id` attribute of the
//! message in a chat and the stanza-id in a room.
//!
//! A moderator of a room can retract the message of another occupant (XEP-0425). The room
//! sends a message from its bare JID with the `<retract/>` and a `<moderated/>` element.
//! We apply it only if it comes from the room itself. `moderate_message` asks a room to
//! moderate a message.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::params;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;

use super::corrections::{NOW_MS, new_message, original_id, own_message, same_sender};
use super::message_ext::{self, Incoming};
use super::{Ctx, Pending, muc, orphans};
use crate::actor::{ClientError, ClientHandle};
use crate::store::queries::{self, MessageKind, MessageRow};
use crate::views::{ChannelScope, ViewKey};

type Reply = oneshot::Sender<Result<(), ClientError>>;

const NS_RETRACT: &str = "urn:xmpp:message-retract:1";
/// The older form: `<apply-to xmlns="urn:xmpp:fasten:0"><retract xmlns=...:0/></apply-to>`.
const NS_RETRACT_OLD: &str = "urn:xmpp:message-retract:0";
const NS_FASTEN: &str = "urn:xmpp:fasten:0";
/// XEP-0425 moderation, in the current and the older form.
const NS_MODERATE: &str = "urn:xmpp:message-moderate:1";
const NS_MODERATE_OLD: &str = "urn:xmpp:message-moderate:0";
const NS_FALLBACK: &str = "urn:xmpp:fallback:0";
const NS_HINTS: &str = "urn:xmpp:hints";
const FALLBACK_BODY: &str =
    "This person attempted to retract a previous message, but it's unsupported by your client.";

/// A command from the public API.
pub(crate) enum Command {
    Retract {
        item_id: String,
        reply: Reply,
    },
    Moderate {
        item_id: String,
        reason: Option<String>,
        reply: Reply,
    },
}

impl ClientHandle {
    /// Retract one of our own messages (XEP-0424). `item_id` is the `TimelineItem::id` of
    /// the message. The row shows as retracted at once. A message with the status `failed`
    /// is hidden from the timeline instead, and nothing goes out.
    ///
    /// Fails with `Invalid` for a message from another sender, for a room that we have not
    /// joined, and for a room message that has no stanza-id yet.
    pub async fn retract_message(&self, item_id: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Retraction(Command::Retract {
            item_id,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Ask the room to retract a message of another occupant (XEP-0425). `item_id` is the
    /// `TimelineItem::id` of a room message. The row shows as retracted when the room
    /// announces the moderation, so the answer only says that the room accepted the request.
    ///
    /// Fails with `Invalid` for a message that is not in a room that we joined, and for a
    /// message that has no stanza-id. Fails with `Server` if the room refuses, for example
    /// because we are no moderator.
    pub async fn moderate_message(
        &self,
        item_id: String,
        reason: Option<String>,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Retraction(Command::Moderate {
            item_id,
            reason,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Retract { item_id, reply } => {
            let _ = reply.send(retract(ctx, &item_id));
        }
        Command::Moderate {
            item_id,
            reason,
            reply,
        } => moderate(ctx, &item_id, reason, reply),
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Retract { reply, .. } | Command::Moderate { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// An attribute name. Each caller passes a literal that is a valid XML name.
fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

/// Send a retraction of an own message and mark its row.
fn retract(ctx: &mut Ctx<'_>, item_id: &str) -> Result<(), ClientError> {
    let (row, peer) = own_message(ctx, item_id)?;
    if discard_failed(ctx, &row) {
        return Ok(());
    }
    let reference = match row.kind {
        MessageKind::Groupchat => row.stanza_id.clone(),
        MessageKind::Chat => retraction_id(&row).map(str::to_owned),
    }
    .ok_or_else(|| ClientError::Invalid("the message has no id to retract yet".into()))?;
    let mut message = new_message(&row, &peer).with_body("".into(), FALLBACK_BODY.into());
    message.payloads.push(
        Element::builder("retract", NS_RETRACT)
            .attr(nc("id"), reference)
            .build(),
    );
    message.payloads.push(
        Element::builder("fallback", NS_FALLBACK)
            .attr(nc("for"), NS_RETRACT)
            .build(),
    );
    message
        .payloads
        .push(Element::builder("store", NS_HINTS).build());
    message_ext::send_to_peer(ctx, row.kind, &peer, message)?;
    apply(ctx, &row, None);
    Ok(())
}

/// The id that a retraction of a chat message names (XEP-0424, section 6): the origin-id
/// when the row has one, else the `id` attribute. A message that another device of ours
/// sent can have two different values.
fn retraction_id(row: &MessageRow) -> Option<&str> {
    row.origin_id.as_deref().or_else(|| original_id(row))
}

/// A message that failed (RFC 6121, 8.5) never arrived, so there is nothing to retract.
/// Mark the row as retracted here only, and the timeline hides it. The row stays, so a
/// copy of the message in the archive finds it and does not add the message again. The
/// retry action of a frontend sends the text again and then discards the failed row this
/// way. Returns true if the row was failed.
fn discard_failed(ctx: &mut Ctx<'_>, row: &MessageRow) -> bool {
    let failed = ctx.store.conn().query_row(
        "SELECT failed_at IS NOT NULL FROM messages WHERE account_id = ?1 AND id = ?2",
        params![ctx.account_id, row.rowid],
        |r| r.get::<_, bool>(0),
    );
    match failed {
        Ok(true) => {
            apply(ctx, row, None);
            true
        }
        Ok(false) => false,
        Err(e) => {
            ctx.store_error("read a failed message", e);
            false
        }
    }
}

/// Send the XEP-0425 request that asks a room to retract a message. `reply` gets the
/// answer of the room.
fn moderate(ctx: &mut Ctx<'_>, item_id: &str, reason: Option<String>, reply: Reply) {
    let result = (|| {
        let row = queries::find_by_timeline_id(ctx.store.conn(), ctx.account_id, item_id)
            .map_err(|e| ClientError::Invalid(format!("store: {e}")))?
            .ok_or_else(|| ClientError::Invalid(format!("no message {item_id}")))?;
        if row.kind != MessageKind::Groupchat {
            return Err(ClientError::Invalid(
                "only a room message can be moderated".into(),
            ));
        }
        let room = BareJid::new(&row.peer)
            .map_err(|e| ClientError::Invalid(format!("bad peer {}: {e}", row.peer)))?;
        if !muc::knows_room(ctx, &room) {
            return Err(ClientError::Invalid(format!("not in the room {room}")));
        }
        let stanza_id = row
            .stanza_id
            .clone()
            .ok_or_else(|| ClientError::Invalid("the message has no stanza-id yet".into()))?;
        let mut payload = Element::builder("moderate", NS_MODERATE)
            .attr(nc("id"), stanza_id)
            .append(Element::builder("retract", NS_RETRACT).build())
            .build();
        if let Some(reason) = reason.filter(|r| !r.is_empty()) {
            payload.append_child(
                Element::builder("reason", NS_MODERATE)
                    .append(reason)
                    .build(),
            );
        }
        Ok((room, payload))
    })();
    match result {
        Ok((room, payload)) => {
            let iq = Iq::Set {
                from: None,
                to: Some(Jid::from(room.clone())),
                id: String::new(),
                payload,
            };
            // A moderator must be in the room. The IQ waits while the join runs.
            let then = Pending::Muc(muc::Pending::Moderate(reply));
            muc::request_in_room(ctx, &room, iq, then);
        }
        Err(e) => {
            let _ = reply.send(Err(e));
        }
    }
}

/// Mark a message as retracted, clear its text, and remove its reactions.
fn apply(ctx: &mut Ctx<'_>, row: &MessageRow, timestamp: Option<i64>) {
    let conn = ctx.store.conn();
    let result = conn
        .execute(
            &format!(
                "UPDATE messages SET retracted_at = COALESCE(?3, {NOW_MS}), body = '',
                    edited_body = ''
                 WHERE account_id = ?1 AND id = ?2"
            ),
            params![ctx.account_id, row.rowid, timestamp],
        )
        .and_then(|_| {
            conn.execute(
                "DELETE FROM reactions WHERE account_id = ?1 AND message = ?2",
                params![ctx.account_id, row.rowid],
            )
        });
    if let Err(e) = result {
        ctx.store_error("store a retraction", e);
        return;
    }
    if let Some(key) = message_ext::timeline_key(&row.peer) {
        ctx.changed(key);
    }
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
}

/// The id of the message that a stanza retracts, in the current or the older form.
fn retracted_id(message: &Message) -> Option<String> {
    message.payloads.iter().find_map(|p| {
        if p.is("retract", NS_RETRACT) {
            return p.attr("id").map(str::to_owned);
        }
        if p.is("apply-to", NS_FASTEN) && p.has_child("retract", NS_RETRACT_OLD) {
            return p.attr("id").map(str::to_owned);
        }
        None
    })
}

/// The id of the message that a room moderated, in the current or the older form.
fn moderated_id(message: &Message) -> Option<String> {
    message.payloads.iter().find_map(|p| {
        // XEP-0425 0.3: <retract id=...><moderated by=.../></retract>
        if p.is("retract", NS_RETRACT) && p.has_child("moderated", NS_MODERATE) {
            return p.attr("id").map(str::to_owned);
        }
        // Older: <apply-to id=...><moderated><retract/></moderated></apply-to>
        if p.is("apply-to", NS_FASTEN)
            && let Some(moderated) = p.get_child("moderated", NS_MODERATE_OLD)
            && moderated.has_child("retract", NS_RETRACT_OLD)
        {
            return p.attr("id").map(str::to_owned);
        }
        None
    })
}

/// A message from a room that says that a moderator retracted a message (XEP-0425).
/// Only the room itself may say so: the sender must be the bare JID of the room, and
/// the target must be a room message. Returns true if the message is a moderation, so that
/// no new timeline row appears.
pub(crate) fn on_moderation(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    let Some(id) = moderated_id(incoming.message) else {
        return false;
    };
    if incoming.kind != MessageKind::Groupchat || incoming.sender != incoming.peer {
        log::debug!(
            "moderation from {} ignored: not from the room",
            incoming.sender
        );
        return true;
    }
    match queries::find_message(ctx.store.conn(), ctx.account_id, incoming.peer, &id) {
        Ok(Some(row)) if row.kind == MessageKind::Groupchat => {
            if !row.retracted {
                apply(ctx, &row, incoming.timestamp);
            }
        }
        Ok(_) => log::debug!("moderation of {id} dropped: no such room message"),
        Err(e) => ctx.store_error("find the message to moderate", e),
    }
    true
}

/// A message that retracts an earlier message. Returns true if this module handled it, so
/// that no new timeline row appears.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    let Some(id) = retracted_id(incoming.message) else {
        return false;
    };
    let row = match queries::find_message(ctx.store.conn(), ctx.account_id, incoming.peer, &id) {
        Ok(Some(row)) => row,
        Ok(None) => {
            orphans::stash(ctx, incoming, &id, orphans::Change::Retract);
            return true;
        }
        Err(e) => {
            ctx.store_error("find the message to retract", e);
            return true;
        }
    };
    if !same_sender(&row, incoming.sender) {
        log::debug!(
            "retraction from {} for another sender ignored",
            incoming.sender
        );
        return true;
    }
    if !row.retracted {
        apply(ctx, &row, incoming.timestamp);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::corrections::tests::store_row;
    use crate::features::testing::Harness;
    use crate::store::queries::Direction;
    use jid::{BareJid, Jid};
    use xmpp_parsers::stanza::Stanza;

    const BOB: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    fn retraction(id: &str) -> Message {
        let mut m = Message::chat(None);
        m.payloads.push(
            Element::builder("retract", NS_RETRACT)
                .attr(nc("id"), id)
                .build(),
        );
        m
    }

    fn deliver(h: &mut Harness, m: &Message, sender: &str) -> bool {
        let incoming = Incoming {
            message: m,
            kind: MessageKind::Chat,
            direction: Direction::In,
            peer: BOB,
            sender,
            timestamp: None,
        };
        h.with_ctx(|ctx| on_message(ctx, &incoming))
    }

    fn row(h: &Harness, item: &str) -> MessageRow {
        queries::find_by_timeline_id(h.store.conn(), h.account_id, item)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn retraction_clears_body_and_reactions() {
        let mut h = Harness::new();
        let item = store_row(&h, MessageKind::Chat, Direction::In, BOB, BOB, "m1", None);
        let rowid = row(&h, &item).rowid;
        h.store
            .conn()
            .execute(
                "INSERT INTO reactions (account_id, message, sender, emojis)
                 VALUES (?1, ?2, ?3, '[\"x\"]')",
                params![h.account_id, rowid, BOB],
            )
            .unwrap();
        assert!(deliver(
            &mut h,
            &retraction("m1"),
            "bob@chord.localhost/phone"
        ));
        let r = row(&h, &item);
        assert!(r.retracted);
        assert_eq!(r.body, "");
        let n: i64 = h
            .store
            .conn()
            .query_row("SELECT COUNT(*) FROM reactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        assert!(
            h.take_dirty()
                .contains(&ViewKey::Timeline(BareJid::new(BOB).unwrap()))
        );
    }

    #[test]
    fn retraction_from_another_sender_or_of_nothing_changes_nothing() {
        let mut h = Harness::new();
        let item = store_row(&h, MessageKind::Chat, Direction::In, BOB, BOB, "m1", None);
        assert!(deliver(&mut h, &retraction("m1"), "eve@chord.localhost"));
        assert!(!row(&h, &item).retracted);
        assert!(deliver(&mut h, &retraction("nope"), BOB));
    }

    #[test]
    fn older_form_is_accepted() {
        let mut h = Harness::new();
        let item = store_row(&h, MessageKind::Chat, Direction::In, BOB, BOB, "m1", None);
        let mut m = Message::chat(None);
        m.payloads.push(
            Element::builder("apply-to", NS_FASTEN)
                .attr(nc("id"), "m1")
                .append(Element::builder("retract", NS_RETRACT_OLD).build())
                .build(),
        );
        assert!(deliver(&mut h, &m, BOB));
        assert!(row(&h, &item).retracted);
    }

    #[test]
    fn outgoing_retraction_in_a_chat() {
        let mut h = Harness::new();
        let item = store_row(
            &h,
            MessageKind::Chat,
            Direction::Out,
            BOB,
            "alice@chord.localhost",
            "m1",
            None,
        );
        h.with_ctx(|ctx| retract(ctx, &item)).unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.to, Some(Jid::new(BOB).unwrap()));
        assert_eq!(retracted_id(m).as_deref(), Some("m1"));
        assert!(m.payloads.iter().any(|p| p.is("fallback", NS_FALLBACK)));
        assert!(m.payloads.iter().any(|p| p.is("store", NS_HINTS)));
        assert_eq!(m.get_best_body(vec![]).unwrap().1, FALLBACK_BODY);
        let r = row(&h, &item);
        assert!(r.retracted);
        assert_eq!(r.body, "");
    }

    #[test]
    fn outgoing_retraction_in_a_chat_uses_the_origin_id() {
        let mut h = Harness::new();
        let item = store_row(
            &h,
            MessageKind::Chat,
            Direction::Out,
            BOB,
            "alice@chord.localhost",
            "m1",
            None,
        );
        // Another device sent it with an `id` that differs from the origin-id.
        h.store
            .conn()
            .execute("UPDATE messages SET message_id = 'other-id'", [])
            .unwrap();
        h.with_ctx(|ctx| retract(ctx, &item)).unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(retracted_id(m).as_deref(), Some("m1"));
    }

    #[test]
    fn outgoing_retraction_in_a_room_needs_the_stanza_id() {
        let mut h = Harness::new();
        let sender = format!("{ROOM}/alice");
        let room = BareJid::new(ROOM).unwrap();
        h.state.muc.nicks.insert(room, "alice".into());
        let no_sid = store_row(
            &h,
            MessageKind::Groupchat,
            Direction::Out,
            ROOM,
            &sender,
            "m1",
            None,
        );
        assert!(h.with_ctx(|ctx| retract(ctx, &no_sid)).is_err());
        assert!(!row(&h, &no_sid).retracted);
        let item = store_row(
            &h,
            MessageKind::Groupchat,
            Direction::Out,
            ROOM,
            &sender,
            "m2",
            Some("s2"),
        );
        h.with_ctx(|ctx| retract(ctx, &item)).unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(retracted_id(m).as_deref(), Some("s2"));
        assert!(row(&h, &item).retracted);
    }

    #[test]
    fn retraction_in_a_private_message() {
        let mut h = Harness::new();
        let peer = format!("{ROOM}/bob");
        let sender = format!("{ROOM}/alice");
        h.state
            .muc
            .nicks
            .insert(BareJid::new(ROOM).unwrap(), "alice".into());
        // An outgoing retraction goes to the occupant.
        let item = store_row(
            &h,
            MessageKind::Chat,
            Direction::Out,
            &peer,
            &sender,
            "m1",
            None,
        );
        h.with_ctx(|ctx| retract(ctx, &item)).unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.to, Some(Jid::new(&peer).unwrap()));
        assert_eq!(retracted_id(m).as_deref(), Some("m1"));
        assert!(row(&h, &item).retracted);
        // An incoming one needs the same occupant.
        let item = store_row(
            &h,
            MessageKind::Chat,
            Direction::In,
            &peer,
            &peer,
            "m2",
            None,
        );
        let m = retraction("m2");
        let inc = |h: &mut Harness, sender: &str| {
            let incoming = Incoming {
                message: &m,
                kind: MessageKind::Chat,
                direction: Direction::In,
                peer: &peer,
                sender,
                timestamp: None,
            };
            h.with_ctx(|ctx| on_message(ctx, &incoming))
        };
        assert!(inc(&mut h, &format!("{ROOM}/carol")));
        assert!(!row(&h, &item).retracted);
        assert!(inc(&mut h, &peer));
        assert!(row(&h, &item).retracted);
        assert!(h.take_dirty().contains(&ViewKey::PrivateTimeline(
            BareJid::new(ROOM).unwrap(),
            "bob".into()
        )));
    }

    fn moderation(id: &str) -> Message {
        let mut m = Message::new(None);
        m.payloads.push(
            Element::builder("retract", NS_RETRACT)
                .attr(nc("id"), id)
                .append(
                    Element::builder("moderated", NS_MODERATE)
                        .attr(nc("by"), format!("{ROOM}/carol"))
                        .build(),
                )
                .append(
                    Element::builder("reason", NS_RETRACT)
                        .append("spam")
                        .build(),
                )
                .build(),
        );
        m
    }

    fn moderated(h: &mut Harness, m: &Message, kind: MessageKind, sender: &str) -> bool {
        let incoming = Incoming {
            message: m,
            kind,
            direction: Direction::In,
            peer: ROOM,
            sender,
            timestamp: Some(7),
        };
        h.with_ctx(|ctx| on_moderation(ctx, &incoming))
    }

    #[test]
    fn the_room_can_retract_a_message_of_an_occupant() {
        let mut h = Harness::new();
        let bob = format!("{ROOM}/bob");
        let item = store_row(
            &h,
            MessageKind::Groupchat,
            Direction::In,
            ROOM,
            &bob,
            "m1",
            Some("s1"),
        );
        let room = BareJid::new(ROOM).unwrap();
        assert!(moderated(
            &mut h,
            &moderation("s1"),
            MessageKind::Groupchat,
            ROOM
        ));
        let r = row(&h, &item);
        assert!(r.retracted);
        assert_eq!(r.body, "");
        assert!(h.take_dirty().contains(&ViewKey::Timeline(room)));
    }

    #[test]
    fn a_moderation_from_an_occupant_is_ignored() {
        let mut h = Harness::new();
        let bob = format!("{ROOM}/bob");
        let item = store_row(
            &h,
            MessageKind::Groupchat,
            Direction::In,
            ROOM,
            &bob,
            "m1",
            Some("s1"),
        );
        // An occupant who pretends to be the room.
        let eve = format!("{ROOM}/eve");
        assert!(moderated(
            &mut h,
            &moderation("s1"),
            MessageKind::Groupchat,
            &eve
        ));
        assert!(!row(&h, &item).retracted);
        // A chat message is no room message.
        assert!(moderated(
            &mut h,
            &moderation("s1"),
            MessageKind::Chat,
            ROOM
        ));
        assert!(!row(&h, &item).retracted);
        // A retraction with no moderation is not for this function.
        assert!(!moderated(
            &mut h,
            &retraction("s1"),
            MessageKind::Groupchat,
            ROOM
        ));
        // An unknown target is dropped.
        assert!(moderated(
            &mut h,
            &moderation("nope"),
            MessageKind::Groupchat,
            ROOM
        ));
    }

    #[test]
    fn the_older_moderation_form_is_accepted() {
        let mut h = Harness::new();
        let bob = format!("{ROOM}/bob");
        let item = store_row(
            &h,
            MessageKind::Groupchat,
            Direction::In,
            ROOM,
            &bob,
            "m1",
            Some("s1"),
        );
        let mut m = Message::new(None);
        m.payloads.push(
            Element::builder("apply-to", NS_FASTEN)
                .attr(nc("id"), "s1")
                .append(
                    Element::builder("moderated", NS_MODERATE_OLD)
                        .append(Element::builder("retract", NS_RETRACT_OLD).build())
                        .build(),
                )
                .build(),
        );
        assert!(moderated(&mut h, &m, MessageKind::Groupchat, ROOM));
        assert!(row(&h, &item).retracted);
    }

    #[test]
    fn moderate_sends_the_request_and_passes_the_answer() {
        let mut h = Harness::new();
        let bob = format!("{ROOM}/bob");
        let item = store_row(
            &h,
            MessageKind::Groupchat,
            Direction::In,
            ROOM,
            &bob,
            "m1",
            Some("s1"),
        );
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| moderate(ctx, &item, Some("spam".into()), reply));
        // We are not in the room.
        assert!(matches!(
            answer.try_recv(),
            Ok(Some(Err(ClientError::Invalid(_))))
        ));
        h.state
            .muc
            .nicks
            .insert(BareJid::new(ROOM).unwrap(), "alice".into());
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| moderate(ctx, &item, Some("spam".into()), reply));
        let sent = h.take_sent();
        let [Stanza::Iq(Iq::Set { to, payload, .. })] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(to, &Some(Jid::new(ROOM).unwrap()));
        assert!(payload.is("moderate", NS_MODERATE));
        assert_eq!(payload.attr("id"), Some("s1"));
        assert!(payload.has_child("retract", NS_RETRACT));
        assert_eq!(
            payload.get_child("reason", NS_MODERATE).map(|r| r.text()),
            Some("spam".to_owned())
        );
        h.answer(
            |p| matches!(p, Pending::Muc(muc::Pending::Moderate(_))),
            None,
        );
        assert!(matches!(answer.try_recv(), Ok(Some(Ok(())))));
        // The row changes when the room announces the moderation.
        assert!(!row(&h, &item).retracted);
    }
}
