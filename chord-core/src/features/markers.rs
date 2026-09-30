//! Chat markers (XEP-0333): read state and unread counts. Also XEP-0184 receipts.
//!
//! Every outgoing message is `<markable/>`. A `<displayed/>` marker from the peer moves
//! our outgoing messages to `displayed`. A `<displayed/>` marker from our own account (a
//! carbon from another device, or the echo of a room) moves the read position. The unread
//! count of the channel list comes from `read_state.last_read` (a `messages.id`).
//!
//! A `mark_read` while offline sends its marker at the next session (`on_connected`).
//! `read_state.marker_sent` holds the newest message that a marker went out for.
//!
//! XEP-0184 receipts: a 1:1 message of ours carries `<request/>` (`request_payload`). We
//! answer a `<request/>` with `<received/>` (`after_store`) only for a live message in a
//! 1:1 chat, from a contact who sees our presence (subscription `from` or `both`). A
//! stranger gets none, because the answer would show that we are online. A message from the
//! archive or with a delay, a message of a room, and a private message of a room get none.
//! `mark_read` sends a `<displayed/>` marker, because the user asks for it.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;

use super::message_ext::{self, Incoming};
use super::{Ctx, FeatureCommand, muc, new_id};
use crate::actor::{ClientError, ClientHandle};
use crate::store::Store;
use crate::store::queries::{Direction, MessageKind, StoredMessage};
use crate::views::{ChannelScope, ViewKey};

/// XEP-0333 namespace.
pub const NS_MARKERS: &str = "urn:xmpp:chat-markers:0";
/// XEP-0184 namespace.
const NS_RECEIPTS: &str = "urn:xmpp:receipts";
/// XEP-0334 namespace.
const NS_HINTS: &str = "urn:xmpp:hints";

type Reply = oneshot::Sender<Result<(), ClientError>>;

/// An attribute name for `Element::builder`. Only constant names go in.
fn nc(name: &str) -> NcName {
    NcName::try_from(name.to_owned()).expect("a valid attribute name")
}

/// A command from the public API.
pub(crate) enum Command {
    MarkRead { peer: String, reply: Reply },
    MarkUnread { item_id: String, reply: Reply },
}

impl ClientHandle {
    /// Mark the chat or room `peer` as read up to its newest incoming message. This moves
    /// the read position, which resets the unread count, and sends a `<displayed/>`
    /// marker (XEP-0333) for that message. It sends no marker if that message was read
    /// already.
    pub async fn mark_read(&self, peer: BareJid) -> Result<(), ClientError> {
        self.mark_read_peer(peer.to_string()).await
    }

    /// Like `mark_read`, for the private messages between us and the occupant `nick` of
    /// `room`. The marker goes to the room outbox, so it waits for the join.
    pub async fn mark_read_private(&self, room: BareJid, nick: String) -> Result<(), ClientError> {
        self.mark_read_peer(format!("{room}/{nick}")).await
    }

    /// Mark the message with the timeline id `item_id` and all later messages of its chat
    /// or room as unread. The read position moves to the row before that message. This
    /// works offline, changes the channel list, and sends nothing to the server.
    ///
    /// Fails with `Invalid` if the message is unknown.
    pub async fn mark_unread(&self, item_id: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Markers(Command::MarkUnread {
            item_id,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    async fn mark_read_peer(&self, peer: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Markers(Command::MarkRead { peer, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::MarkRead { peer, reply } => {
            let _ = reply.send(mark_read(ctx, &peer));
        }
        Command::MarkUnread { item_id, reply } => {
            let _ = reply.send(mark_unread(ctx, &item_id));
        }
    }
}

/// A command while no session is up, with the store. `mark_read` moves the read position
/// and sends no marker now. `on_connected` sends it in the next session. The caller must
/// mark the channel list as changed.
pub(crate) fn offline_with_store(store: &Store, account_id: i64, command: Command) {
    match command {
        Command::MarkRead { peer, reply } => {
            let result = newest_incoming(store, account_id, &peer)
                .and_then(|newest| match newest {
                    Some(n) => set_last_read(store, account_id, &peer, n.rowid, false),
                    None => Ok(()),
                })
                .map_err(|e| ClientError::Invalid(format!("store: {e}")));
            let _ = reply.send(result);
        }
        Command::MarkUnread { item_id, reply } => {
            let result = unread_from(store, account_id, &item_id).map(|_| ());
            let _ = reply.send(result);
        }
    }
}

/// Move the read position of the chat of `item_id` to the row before that message. Returns
/// the peer and the kind of the message. The caller marks the channel list as changed.
fn unread_from(
    store: &Store,
    account_id: i64,
    item_id: &str,
) -> Result<(String, MessageKind), ClientError> {
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    let row = crate::store::queries::find_by_timeline_id(store.conn(), account_id, item_id)
        .map_err(store_error)?
        .ok_or_else(|| ClientError::Invalid("unknown message".to_owned()))?;
    let before: i64 = store
        .conn()
        .query_row(
            "SELECT COALESCE(MAX(id), 0) FROM messages
             WHERE account_id = ?1 AND peer = ?2 AND id < ?3",
            params![account_id, row.peer, row.rowid],
            |r| r.get(0),
        )
        .map_err(store_error)?;
    // A marker is not out for this position: we send none for a move back. Setting
    // `marker_sent` keeps `on_connected` quiet and lets a later `mark_read` send its own.
    store
        .conn()
        .execute(
            "INSERT INTO read_state (account_id, peer, last_read, marker_sent)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT (account_id, peer) DO UPDATE SET
                last_read = excluded.last_read, marker_sent = excluded.marker_sent",
            params![account_id, row.peer, before],
        )
        .map_err(store_error)?;
    Ok((row.peer, row.kind))
}

fn mark_unread(ctx: &mut Ctx<'_>, item_id: &str) -> Result<(), ClientError> {
    let (peer, kind) = unread_from(ctx.store, ctx.account_id, item_id)?;
    changed_channel_list(ctx, &peer, kind);
    Ok(())
}

/// What a marker or receipt says.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Received,
    Displayed,
}

impl Level {
    fn status(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Displayed => "displayed",
        }
    }
}

/// The marker or receipt in a message: its level and the id of the message it names.
fn marker_of(message: &Message) -> Option<(Level, &str, bool)> {
    message.payloads.iter().find_map(|p| {
        let id = p.attr("id")?;
        match (p.name(), p.ns().as_str()) {
            ("displayed", NS_MARKERS) => Some((Level::Displayed, id, false)),
            ("received" | "acknowledged", NS_MARKERS) => Some((Level::Received, id, false)),
            // A XEP-0184 receipt names exactly one message.
            ("received", NS_RECEIPTS) => Some((Level::Received, id, true)),
            _ => None,
        }
    })
}

/// A message that only carries a chat marker. Returns true if this module applied it, so that no new
/// timeline row appears.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    let Some((level, id, receipt)) = marker_of(incoming.message) else {
        return false;
    };
    // A marker with a body is a normal message too. Apply it, and let the caller store it.
    let handled = incoming.message.bodies.is_empty();
    let peer = incoming.peer;
    let target =
        match crate::store::queries::find_message(ctx.store.conn(), ctx.account_id, peer, id) {
            Ok(Some(row)) => row,
            Ok(None) => {
                log::debug!("marker for {id} in {peer}: no such message");
                return handled;
            }
            Err(e) => {
                ctx.store_error("find a marked message", e);
                return handled;
            }
        };
    let result = match incoming.direction {
        Direction::In if receipt => {
            // Only a chat message can get a receipt, and only for one of ours.
            if incoming.kind == MessageKind::Chat && target.direction == Direction::Out {
                set_status(ctx, peer, Level::Received, target.rowid, true)
            } else {
                Ok(false)
            }
        }
        // A marker from the peer of a 1:1 chat (or an occupant of a private chat) names
        // one of our messages. One room member who read a message says nothing about the
        // others, so a room keeps the `sent` state (XEP-0333, 5).
        Direction::In
            if incoming.kind == MessageKind::Chat && target.direction == Direction::Out =>
        {
            set_status(ctx, peer, level, target.rowid, false)
        }
        Direction::In => Ok(false),
        // A marker from our own account only counts when it says displayed, and when it
        // names a message that we received.
        Direction::Out
            if level == Level::Displayed && !receipt && target.direction == Direction::In =>
        {
            set_last_read(ctx.store, ctx.account_id, peer, target.rowid, true).map(|()| true)
        }
        Direction::Out => Ok(false),
    };
    match result {
        Ok(true) => {
            if let Some(key) = message_ext::timeline_key(peer) {
                ctx.changed(key);
            }
            if incoming.direction == Direction::Out {
                changed_channel_list(ctx, peer, incoming.kind);
            }
        }
        Ok(false) => {}
        Err(e) => ctx.store_error("apply a marker", e),
    }
    handled
}

/// Raise the status of our outgoing messages in `peer`. With `only`, only the message
/// with this row id. Otherwise all up to and including it. Never lowers a status.
/// Returns true if a row changed.
fn set_status(
    ctx: &Ctx<'_>,
    peer: &str,
    level: Level,
    rowid: i64,
    only: bool,
) -> rusqlite::Result<bool> {
    let lower = match level {
        Level::Received => "status = 'sent'",
        Level::Displayed => "status != 'displayed'",
    };
    let range = if only { "id = ?4" } else { "id <= ?4" };
    let changed = ctx.store.conn().execute(
        &format!(
            "UPDATE messages SET status = ?1
             WHERE account_id = ?2 AND peer = ?3 AND direction = 'out' AND {range} AND {lower}"
        ),
        params![level.status(), ctx.account_id, peer, rowid],
    )?;
    Ok(changed > 0)
}

/// Move the read position of `peer` to `rowid`. Never moves it back. With `marker_sent`,
/// a displayed marker for `rowid` is out already (we sent it, or another device did).
fn set_last_read(
    store: &Store,
    account_id: i64,
    peer: &str,
    rowid: i64,
    marker_sent: bool,
) -> rusqlite::Result<()> {
    store.conn().execute(
        "INSERT INTO read_state (account_id, peer, last_read, marker_sent)
         VALUES (?1, ?2, ?3, CASE WHEN ?4 THEN ?3 END)
         ON CONFLICT (account_id, peer) DO UPDATE SET
            last_read = MAX(last_read, excluded.last_read),
            marker_sent = CASE WHEN ?4
                THEN MAX(COALESCE(marker_sent, 0), excluded.last_read) ELSE marker_sent END",
        params![account_id, peer, rowid, marker_sent],
    )?;
    Ok(())
}

fn changed_channel_list(ctx: &mut Ctx<'_>, peer: &str, kind: MessageKind) {
    match (kind, BareJid::new(peer)) {
        // `mark_room` also marks the channel list of each space of the room.
        (MessageKind::Groupchat, Ok(room)) => muc::mark_room(ctx, &room),
        _ => ctx.changed(ViewKey::ChannelList(ChannelScope::Home)),
    }
}

/// The newest incoming message of `peer`, with the ids that a marker can name.
struct Newest {
    rowid: i64,
    kind: MessageKind,
    message_id: Option<String>,
    origin_id: Option<String>,
    stanza_id: Option<String>,
}

fn newest_incoming(store: &Store, account_id: i64, peer: &str) -> rusqlite::Result<Option<Newest>> {
    store
        .conn()
        .prepare_cached(
            "SELECT id, kind, message_id, origin_id, stanza_id FROM messages
             WHERE account_id = ?1 AND peer = ?2 AND direction = 'in'
             ORDER BY id DESC LIMIT 1",
        )?
        .query_row(params![account_id, peer], |row| newest_from_row(row, 0))
        .optional()
}

/// Read a `Newest` from the columns `id, kind, message_id, origin_id, stanza_id`, which
/// start at `first`.
fn newest_from_row(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Newest> {
    let kind: String = row.get(first + 1)?;
    Ok(Newest {
        rowid: row.get(first)?,
        kind: if kind == "groupchat" {
            MessageKind::Groupchat
        } else {
            MessageKind::Chat
        },
        message_id: row.get(first + 2)?,
        origin_id: row.get(first + 3)?,
        stanza_id: row.get(first + 4)?,
    })
}

/// `peer` is the stored peer: a bare JID, or room@service/nick for private messages.
fn mark_read(ctx: &mut Ctx<'_>, peer: &str) -> Result<(), ClientError> {
    let to: Jid = peer
        .parse()
        .map_err(|e| ClientError::Invalid(format!("bad peer {peer}: {e}")))?;
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    let Some(newest) = newest_incoming(ctx.store, ctx.account_id, peer).map_err(store_error)?
    else {
        return Ok(());
    };
    let state: Option<(i64, Option<i64>)> = ctx
        .store
        .conn()
        .query_row(
            "SELECT last_read, marker_sent FROM read_state WHERE account_id = ?1 AND peer = ?2",
            params![ctx.account_id, peer],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(store_error)?;
    let last_read = state.map(|s| s.0);
    let marker_sent = state.and_then(|s| s.1);
    // A marker that waits (we marked the chat as read while offline) goes out now.
    if marker_sent.is_some_and(|n| n >= newest.rowid) {
        return Ok(());
    }
    if last_read.is_none_or(|n| n < newest.rowid) {
        set_last_read(ctx.store, ctx.account_id, peer, newest.rowid, false).map_err(store_error)?;
        changed_channel_list(ctx, peer, newest.kind);
    }
    send_marker(ctx, peer, &to, &newest);
    Ok(())
}

/// Send a displayed marker for `newest`, and record that it went out. A marker that cannot
/// go out (a room that we left) stays pending. The read position moves on its own.
fn send_marker(ctx: &mut Ctx<'_>, peer: &str, to: &Jid, newest: &Newest) {
    // In a room the marker names the stanza-id. In a chat it names the `id` attribute.
    let (reference, mut message) = match newest.kind {
        MessageKind::Groupchat => (newest.stanza_id.clone(), Message::groupchat(to.clone())),
        MessageKind::Chat => (
            newest.message_id.clone().or(newest.origin_id.clone()),
            Message::chat(to.clone()),
        ),
    };
    let Some(reference) = reference else {
        log::debug!("newest message of {peer} has no id for a marker");
        // Nothing can name this message. Do not try again.
        mark_sent(ctx, peer, newest.rowid);
        return;
    };
    message.id = Some(Id(new_id()));
    message.payloads.push(
        Element::builder("displayed", NS_MARKERS)
            .attr(nc("id"), reference.as_str())
            .build(),
    );
    message
        .payloads
        .push(Element::builder("store", NS_HINTS).build());
    match message_ext::send_to_peer(ctx, newest.kind, to, message) {
        Ok(()) => mark_sent(ctx, peer, newest.rowid),
        Err(e) => log::debug!("no displayed marker to {peer}: {e}"),
    }
}

fn mark_sent(ctx: &mut Ctx<'_>, peer: &str, rowid: i64) {
    if let Err(e) = ctx.store.conn().execute(
        "UPDATE read_state SET marker_sent = MAX(COALESCE(marker_sent, 0), ?3)
         WHERE account_id = ?1 AND peer = ?2",
        params![ctx.account_id, peer, rowid],
    ) {
        ctx.store_error("store a marker state", e);
    }
}

/// A new session is up: send the displayed markers that wait. That is a read position
/// (`last_read`, an incoming message) that is newer than the last marker that we sent.
/// This happens when the user reads a chat while offline. A room gets its marker through
/// the room outbox, after the join. We send none to a room that we do not join.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    let pending = ctx
        .store
        .conn()
        .prepare_cached(
            "SELECT rs.peer, m.id, m.kind, m.message_id, m.origin_id, m.stanza_id
             FROM read_state rs JOIN messages m
               ON m.account_id = rs.account_id AND m.id = rs.last_read
             WHERE rs.account_id = ?1 AND m.direction = 'in'
               AND (rs.marker_sent IS NULL OR rs.marker_sent < rs.last_read)
             ORDER BY rs.peer",
        )
        .and_then(|mut stmt| {
            stmt.query_map(params![ctx.account_id], |row| {
                Ok((row.get::<_, String>(0)?, newest_from_row(row, 1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        });
    let pending = match pending {
        Ok(pending) => pending,
        Err(e) => {
            ctx.store_error("read the pending markers", e);
            return;
        }
    };
    for (peer, newest) in pending {
        let Ok(to) = peer.parse::<Jid>() else {
            continue;
        };
        // A room, or a private message of a room, needs a join that runs or is done.
        // Otherwise the marker would force a join.
        if (newest.kind == MessageKind::Groupchat || to.resource().is_some())
            && !muc::is_joined_or_joining(ctx, &to.to_bare())
        {
            log::debug!("marker for {peer} waits: the room is not joined");
            continue;
        }
        send_marker(ctx, &peer, &to, &newest);
    }
}

/// A new message is in the store. Answer its XEP-0184 `<request/>` when the rules in the
/// module text allow it.
pub(crate) fn after_store(
    ctx: &mut Ctx<'_>,
    incoming: &Incoming<'_>,
    _stored: &StoredMessage,
    live: bool,
) {
    let message = incoming.message;
    if !live
        || incoming.direction != Direction::In
        || incoming.kind != MessageKind::Chat
        || message_ext::is_private(incoming.peer)
        || message.type_ == MessageType::Groupchat
        || super::chat::delay_ms(message).is_some()
        || !message
            .payloads
            .iter()
            .any(|p| p.is("request", NS_RECEIPTS))
    {
        return;
    }
    // The receipt names the `id` attribute of the message (XEP-0184, section 5.1).
    let Some(id) = message.id.as_ref().filter(|id| !id.0.is_empty()) else {
        return;
    };
    let Ok(to) = incoming.sender.parse::<Jid>() else {
        return;
    };
    // Only a contact who sees our presence: a stranger would learn that we are online.
    let sees_us = ctx
        .store
        .conn()
        .query_row(
            "SELECT 1 FROM contacts WHERE account_id = ?1 AND jid = ?2
               AND subscription IN ('from', 'both')",
            params![ctx.account_id, to.to_bare().as_str()],
            |_| Ok(()),
        )
        .optional();
    match sees_us {
        Ok(Some(())) => {}
        Ok(None) => return,
        Err(e) => {
            ctx.store_error("read the roster for a receipt", e);
            return;
        }
    }
    let mut receipt = Message::chat(to);
    receipt.id = Some(Id(new_id()));
    receipt.payloads.push(
        Element::builder("received", NS_RECEIPTS)
            .attr(nc("id"), id.0.as_str())
            .build(),
    );
    receipt
        .payloads
        .push(Element::builder("store", NS_HINTS).build());
    log::debug!("receipt for {} to {}", id.0, incoming.sender);
    ctx.send(receipt);
}

/// The XEP-0184 `<request/>` for a message of ours in a 1:1 chat.
pub(crate) fn request_payload() -> Element {
    Element::builder("request", NS_RECEIPTS).build()
}

/// Payloads for every outgoing message: `<markable/>`.
pub(crate) fn outgoing_payloads(_ctx: &mut Ctx<'_>) -> Vec<Element> {
    vec![Element::builder("markable", NS_MARKERS).build()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::store::queries::{KeyKind, MessageExtras, NewMessage, insert_message};
    use xmpp_parsers::stanza::Stanza;

    const PEER: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    fn peer() -> String {
        PEER.to_owned()
    }

    fn put(h: &Harness, key: &str, dir: Direction, kind: MessageKind, peer: &str) -> i64 {
        let stanza = kind == MessageKind::Groupchat;
        let new = NewMessage {
            kind,
            key_kind: if stanza {
                KeyKind::StanzaId
            } else {
                KeyKind::OriginId
            },
            key,
            direction: dir,
            peer,
            sender: if dir == Direction::Out {
                "alice@chord.localhost"
            } else {
                "bob@chord.localhost/x"
            },
            body: "hi",
            timestamp: None,
            extras: MessageExtras {
                message_id: Some(key.into()),
                ..Default::default()
            },
        };
        insert_message(h.store.conn(), h.account_id, &new)
            .unwrap()
            .unwrap()
            .rowid
    }

    fn marker(name: &str, ns: &str, id: &str) -> Message {
        let mut m = Message::new(None);
        m.payloads
            .push(Element::builder(name, ns).attr(nc("id"), id).build());
        m
    }

    fn deliver(
        h: &mut Harness,
        m: &Message,
        dir: Direction,
        kind: MessageKind,
        peer: &str,
    ) -> bool {
        let sender = "x";
        let incoming = Incoming {
            message: m,
            kind,
            direction: dir,
            peer,
            sender,
            timestamp: None,
        };
        h.with_ctx(|ctx| on_message(ctx, &incoming))
    }

    fn statuses(h: &Harness) -> Vec<String> {
        let mut stmt = h
            .store
            .conn()
            .prepare("SELECT status FROM messages WHERE direction = 'out' ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn last_read(h: &Harness, peer: &str) -> Option<i64> {
        h.store
            .conn()
            .query_row(
                "SELECT last_read FROM read_state WHERE peer = ?1",
                [peer],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
    }

    fn contact(h: &Harness, jid: &str, subscription: &str) {
        h.store
            .conn()
            .execute(
                "INSERT INTO contacts (account_id, jid, subscription) VALUES (?1, ?2, ?3)",
                params![h.account_id, jid, subscription],
            )
            .unwrap();
    }

    fn requesting(id: Option<&str>) -> Message {
        let mut m = Message::chat(None).with_body("".into(), "hi".into());
        m.id = id.map(|i| Id(i.into()));
        m.payloads
            .push(Element::builder("request", NS_RECEIPTS).build());
        m
    }

    /// Run `after_store` for an incoming message. Returns the stanzas that went out.
    fn receipts(
        h: &mut Harness,
        m: &Message,
        live: bool,
        kind: MessageKind,
        peer: &str,
        sender: &str,
    ) -> Vec<Stanza> {
        let incoming = Incoming {
            message: m,
            kind,
            direction: Direction::In,
            peer,
            sender,
            timestamp: None,
        };
        let stored = StoredMessage {
            rowid: 1,
            kind,
            key_kind: crate::store::queries::KeyKind::OriginId,
            key: "k".into(),
            direction: Direction::In,
            peer: peer.into(),
            sender: sender.into(),
            body: "hi".into(),
            timestamp: 0,
        };
        h.with_ctx(|ctx| after_store(ctx, &incoming, &stored, live));
        h.take_sent()
    }

    const FROM: &str = "bob@chord.localhost/laptop";

    #[test]
    fn a_request_from_a_contact_gets_a_receipt() {
        let mut h = Harness::new();
        contact(&h, PEER, "both");
        let sent = receipts(
            &mut h,
            &requesting(Some("m1")),
            true,
            MessageKind::Chat,
            PEER,
            FROM,
        );
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.to, Some(Jid::new(FROM).unwrap()));
        let (level, id, receipt) = marker_of(m).unwrap();
        assert!(level == Level::Received && receipt);
        assert_eq!(id, "m1");
        // A contact that sees our presence, with no subscription of ours, counts too.
        let mut h = Harness::new();
        contact(&h, PEER, "from");
        let sent = receipts(
            &mut h,
            &requesting(Some("m1")),
            true,
            MessageKind::Chat,
            PEER,
            FROM,
        );
        assert_eq!(sent.len(), 1);
    }

    #[test]
    fn no_receipt_for_a_stranger_or_a_contact_who_cannot_see_us() {
        let mut h = Harness::new();
        let m = requesting(Some("m1"));
        assert!(receipts(&mut h, &m, true, MessageKind::Chat, PEER, FROM).is_empty());
        contact(&h, PEER, "to");
        assert!(receipts(&mut h, &m, true, MessageKind::Chat, PEER, FROM).is_empty());
    }

    #[test]
    fn no_receipt_for_history_rooms_delays_or_a_message_without_request() {
        let mut h = Harness::new();
        contact(&h, PEER, "both");
        let m = requesting(Some("m1"));
        // From the archive.
        assert!(receipts(&mut h, &m, false, MessageKind::Chat, PEER, FROM).is_empty());
        // A room message.
        let mut g = requesting(Some("m1"));
        g.type_ = MessageType::Groupchat;
        assert!(receipts(&mut h, &g, true, MessageKind::Groupchat, ROOM, PEER).is_empty());
        // A private message of a room.
        let private = format!("{ROOM}/bob");
        assert!(receipts(&mut h, &m, true, MessageKind::Chat, &private, &private).is_empty());
        // A delayed copy (offline storage).
        let mut d = requesting(Some("m1"));
        d.payloads.push(
            Element::builder("delay", "urn:xmpp:delay")
                .attr(nc("stamp"), "2026-01-01T00:00:00Z")
                .build(),
        );
        assert!(receipts(&mut h, &d, true, MessageKind::Chat, PEER, FROM).is_empty());
        // No request, and no id.
        let plain = Message::chat(None).with_body("".into(), "hi".into());
        assert!(receipts(&mut h, &plain, true, MessageKind::Chat, PEER, FROM).is_empty());
        assert!(
            receipts(
                &mut h,
                &requesting(None),
                true,
                MessageKind::Chat,
                PEER,
                FROM
            )
            .is_empty()
        );
    }

    #[test]
    fn the_request_payload_is_a_receipts_request() {
        assert!(request_payload().is("request", NS_RECEIPTS));
    }

    #[test]
    fn adds_markable() {
        let mut h = Harness::new();
        let payloads = h.with_ctx(outgoing_payloads);
        assert!(payloads[0].is("markable", NS_MARKERS));
    }

    #[test]
    fn displayed_from_the_peer_marks_earlier_messages() {
        let mut h = Harness::new();
        let p = peer();
        put(&h, "a", Direction::Out, MessageKind::Chat, PEER);
        put(&h, "b", Direction::Out, MessageKind::Chat, PEER);
        put(&h, "c", Direction::Out, MessageKind::Chat, PEER);
        let m = marker("displayed", NS_MARKERS, "b");
        assert!(deliver(&mut h, &m, Direction::In, MessageKind::Chat, &p));
        assert_eq!(statuses(&h), ["displayed", "displayed", "sent"]);
        assert!(
            h.take_dirty()
                .contains(&ViewKey::Timeline(BareJid::new(PEER).unwrap()))
        );
        // A received marker never lowers a status.
        let m = marker("received", NS_MARKERS, "c");
        deliver(&mut h, &m, Direction::In, MessageKind::Chat, &p);
        assert_eq!(statuses(&h), ["displayed", "displayed", "received"]);
        let m = marker("received", NS_MARKERS, "a");
        deliver(&mut h, &m, Direction::In, MessageKind::Chat, &p);
        assert_eq!(statuses(&h), ["displayed", "displayed", "received"]);
    }

    #[test]
    fn a_receipt_marks_one_message() {
        let mut h = Harness::new();
        let p = peer();
        put(&h, "a", Direction::Out, MessageKind::Chat, PEER);
        put(&h, "b", Direction::Out, MessageKind::Chat, PEER);
        let m = marker("received", NS_RECEIPTS, "b");
        assert!(deliver(&mut h, &m, Direction::In, MessageKind::Chat, &p));
        assert_eq!(statuses(&h), ["sent", "received"]);
    }

    #[test]
    fn unknown_target_is_dropped() {
        let mut h = Harness::new();
        let p = peer();
        let m = marker("displayed", NS_MARKERS, "nope");
        assert!(deliver(&mut h, &m, Direction::In, MessageKind::Chat, &p));
        assert!(h.take_dirty().is_empty());
    }

    #[test]
    fn own_displayed_moves_the_read_position_forward_only() {
        let mut h = Harness::new();
        let p = peer();
        put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        let b = put(&h, "b", Direction::In, MessageKind::Chat, PEER);
        let m = marker("displayed", NS_MARKERS, "b");
        assert!(deliver(&mut h, &m, Direction::Out, MessageKind::Chat, &p));
        assert_eq!(last_read(&h, PEER), Some(b));
        assert!(
            h.take_dirty()
                .contains(&ViewKey::ChannelList(ChannelScope::Home))
        );
        let m = marker("displayed", NS_MARKERS, "a");
        deliver(&mut h, &m, Direction::Out, MessageKind::Chat, &p);
        assert_eq!(last_read(&h, PEER), Some(b));
    }

    #[test]
    fn room_marker_from_another_occupant_leaves_own_messages_alone() {
        let mut h = Harness::new();
        let room = BareJid::new(ROOM).unwrap();
        put(&h, "s1", Direction::Out, MessageKind::Groupchat, ROOM);
        put(&h, "s2", Direction::In, MessageKind::Groupchat, ROOM);
        let m = marker("displayed", NS_MARKERS, "s2");
        assert!(deliver(
            &mut h,
            &m,
            Direction::In,
            MessageKind::Groupchat,
            room.as_str()
        ));
        assert_eq!(statuses(&h), ["sent"]);
        assert_eq!(last_read(&h, ROOM), None);
    }

    #[test]
    fn a_marker_for_a_message_of_the_peer_changes_nothing() {
        let mut h = Harness::new();
        let p = peer();
        put(&h, "a", Direction::Out, MessageKind::Chat, PEER);
        put(&h, "b", Direction::In, MessageKind::Chat, PEER);
        // The peer names its own message: it cannot mark ours.
        let m = marker("displayed", NS_MARKERS, "b");
        deliver(&mut h, &m, Direction::In, MessageKind::Chat, &p);
        assert_eq!(statuses(&h), ["sent"]);
        assert!(h.take_dirty().is_empty());
    }

    #[test]
    fn own_displayed_for_our_own_message_is_ignored() {
        let mut h = Harness::new();
        let p = peer();
        put(&h, "a", Direction::Out, MessageKind::Chat, PEER);
        let m = marker("displayed", NS_MARKERS, "a");
        deliver(&mut h, &m, Direction::Out, MessageKind::Chat, &p);
        assert_eq!(last_read(&h, PEER), None);
    }

    #[test]
    fn mark_read_sets_position_and_sends_a_marker() {
        let mut h = Harness::new();
        let p = peer();
        put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        let b = put(&h, "b", Direction::In, MessageKind::Chat, PEER);
        h.with_ctx(|ctx| mark_read(ctx, &p)).unwrap();
        assert_eq!(last_read(&h, PEER), Some(b));
        assert!(
            h.take_dirty()
                .contains(&ViewKey::ChannelList(ChannelScope::Home))
        );
        let sent = h.take_sent();
        let Stanza::Message(m) = &sent[0] else {
            panic!()
        };
        assert_eq!(
            marker_of(m).map(|(l, id, _)| (l == Level::Displayed, id)),
            Some((true, "b"))
        );
        assert!(m.payloads.iter().any(|p| p.is("store", NS_HINTS)));
        // A second call has nothing new to report.
        h.with_ctx(|ctx| mark_read(ctx, &p)).unwrap();
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn mark_read_in_a_room_names_the_stanza_id() {
        let mut h = Harness::new();
        let room = BareJid::new(ROOM).unwrap();
        put(&h, "s9", Direction::In, MessageKind::Groupchat, ROOM);
        h.state.muc.nicks.insert(room.clone(), "alice".into());
        h.with_ctx(|ctx| mark_read(ctx, room.as_str())).unwrap();
        let sent = h.take_sent();
        let Stanza::Message(m) = &sent[0] else {
            panic!()
        };
        assert_eq!(m.type_, xmpp_parsers::message::MessageType::Groupchat);
        assert_eq!(marker_of(m).map(|(_, id, _)| id), Some("s9"));
    }

    #[test]
    fn mark_read_offline_moves_the_position_only() {
        let h = Harness::new();
        let b = put(&h, "b", Direction::In, MessageKind::Chat, PEER);
        let (reply, mut answer) = oneshot::channel();
        offline_with_store(
            &h.store,
            h.account_id,
            Command::MarkRead {
                peer: peer(),
                reply,
            },
        );
        assert!(matches!(answer.try_recv(), Ok(Some(Ok(())))));
        assert_eq!(last_read(&h, PEER), Some(b));
    }

    fn item(rowid: i64) -> String {
        crate::views::timeline::item_id(rowid)
    }

    #[test]
    fn mark_unread_moves_the_position_to_the_row_before() {
        let mut h = Harness::new();
        let a = put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        let b = put(&h, "b", Direction::In, MessageKind::Chat, PEER);
        let c = put(&h, "c", Direction::In, MessageKind::Chat, PEER);
        put(
            &h,
            "other",
            Direction::In,
            MessageKind::Chat,
            "eve@chord.localhost",
        );
        h.with_ctx(|ctx| mark_read(ctx, PEER)).unwrap();
        assert_eq!(last_read(&h, PEER), Some(c));
        h.take_sent();
        h.take_dirty();
        h.with_ctx(|ctx| mark_unread(ctx, &item(b))).unwrap();
        assert_eq!(last_read(&h, PEER), Some(a));
        assert!(
            h.take_dirty()
                .contains(&ViewKey::ChannelList(ChannelScope::Home))
        );
        // Nothing goes to the server, not now and not at the next session.
        assert!(h.take_sent().is_empty());
        h.with_ctx(on_connected);
        assert!(h.take_sent().is_empty());
        // Reading the chat again moves the position and sends a marker.
        h.with_ctx(|ctx| mark_read(ctx, PEER)).unwrap();
        assert_eq!(last_read(&h, PEER), Some(c));
        assert_eq!(h.take_sent().len(), 1);
    }

    #[test]
    fn mark_unread_on_the_first_message_resets_to_zero() {
        let mut h = Harness::new();
        let a = put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        h.with_ctx(|ctx| mark_read(ctx, PEER)).unwrap();
        h.with_ctx(|ctx| mark_unread(ctx, &item(a))).unwrap();
        assert_eq!(last_read(&h, PEER), Some(0));
    }

    #[test]
    fn mark_unread_in_a_room_marks_the_room_view() {
        let mut h = Harness::new();
        let room = BareJid::new(ROOM).unwrap();
        let a = put(&h, "s1", Direction::In, MessageKind::Groupchat, ROOM);
        h.take_dirty();
        h.with_ctx(|ctx| mark_unread(ctx, &item(a))).unwrap();
        assert_eq!(last_read(&h, ROOM), Some(0));
        assert!(h.take_dirty().contains(&ViewKey::Timeline(room)));
    }

    #[test]
    fn mark_unread_of_an_unknown_message_fails() {
        let mut h = Harness::new();
        assert!(h.with_ctx(|ctx| mark_unread(ctx, "m:999")).is_err());
        assert!(h.with_ctx(|ctx| mark_unread(ctx, "junk")).is_err());
    }

    #[test]
    fn mark_unread_works_offline() {
        let h = Harness::new();
        put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        let b = put(&h, "b", Direction::In, MessageKind::Chat, PEER);
        offline_mark(&h, PEER);
        let (reply, mut answer) = oneshot::channel();
        offline_with_store(
            &h.store,
            h.account_id,
            Command::MarkUnread {
                item_id: item(b),
                reply,
            },
        );
        assert!(matches!(answer.try_recv(), Ok(Some(Ok(())))));
        assert_eq!(last_read(&h, PEER), Some(b - 1));
    }

    #[test]
    fn displayed_markers_in_a_private_message() {
        let mut h = Harness::new();
        let peer = format!("{ROOM}/bob");
        let room = BareJid::new(ROOM).unwrap();
        put(&h, "a", Direction::Out, MessageKind::Chat, &peer);
        let b = put(&h, "b", Direction::In, MessageKind::Chat, &peer);
        // The occupant read our message.
        let m = marker("displayed", NS_MARKERS, "a");
        assert!(deliver(&mut h, &m, Direction::In, MessageKind::Chat, &peer));
        assert_eq!(statuses(&h), ["displayed"]);
        assert!(
            h.take_dirty()
                .contains(&ViewKey::PrivateTimeline(room.clone(), "bob".into()))
        );
        // We read theirs: the marker goes to the occupant.
        h.state.muc.nicks.insert(room, "alice".into());
        h.with_ctx(|ctx| mark_read(ctx, &peer)).unwrap();
        assert_eq!(last_read(&h, &peer), Some(b));
        let sent = h.take_sent();
        let Stanza::Message(m) = &sent[0] else {
            panic!()
        };
        assert_eq!(m.type_, xmpp_parsers::message::MessageType::Chat);
        assert_eq!(m.to, Some(Jid::new(&peer).unwrap()));
        assert_eq!(marker_of(m).map(|(_, id, _)| id), Some("b"));
    }

    fn marker_sent(h: &Harness, peer: &str) -> Option<i64> {
        h.store
            .conn()
            .query_row(
                "SELECT marker_sent FROM read_state WHERE peer = ?1",
                [peer],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn offline_mark(h: &Harness, peer: &str) {
        let (reply, mut answer) = oneshot::channel();
        offline_with_store(
            &h.store,
            h.account_id,
            Command::MarkRead {
                peer: peer.into(),
                reply,
            },
        );
        assert!(matches!(answer.try_recv(), Ok(Some(Ok(())))));
    }

    #[test]
    fn a_marker_from_an_offline_read_goes_out_once_on_connect() {
        let mut h = Harness::new();
        put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        let b = put(&h, "b", Direction::In, MessageKind::Chat, PEER);
        offline_mark(&h, PEER);
        assert_eq!(marker_sent(&h, PEER), None);
        h.with_ctx(on_connected);
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.to, Some(Jid::new(PEER).unwrap()));
        assert_eq!(marker_of(m).map(|(_, id, _)| id), Some("b"));
        assert_eq!(marker_sent(&h, PEER), Some(b));
        // The next session has nothing to send.
        h.with_ctx(on_connected);
        assert!(h.take_sent().is_empty());
        // A newer read moves the marker on.
        let c = put(&h, "c", Direction::In, MessageKind::Chat, PEER);
        offline_mark(&h, PEER);
        h.with_ctx(on_connected);
        assert_eq!(h.take_sent().len(), 1);
        assert_eq!(marker_sent(&h, PEER), Some(c));
    }

    #[test]
    fn a_marker_from_another_device_needs_no_marker_of_ours() {
        let mut h = Harness::new();
        put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        let m = marker("displayed", NS_MARKERS, "a");
        assert!(deliver(&mut h, &m, Direction::Out, MessageKind::Chat, PEER));
        h.with_ctx(on_connected);
        assert!(h.take_sent().is_empty());
        // A read position on an outgoing message needs none either.
        let out = put(&h, "o", Direction::Out, MessageKind::Chat, PEER);
        h.store
            .conn()
            .execute(
                "UPDATE read_state SET last_read = ?1, marker_sent = NULL",
                [out],
            )
            .unwrap();
        h.with_ctx(on_connected);
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn a_room_marker_waits_for_the_join() {
        let mut h = Harness::new();
        let room = BareJid::new(ROOM).unwrap();
        let s = put(&h, "s1", Direction::In, MessageKind::Groupchat, ROOM);
        offline_mark(&h, ROOM);
        // No join runs for this room: the marker stays pending.
        h.with_ctx(on_connected);
        assert!(h.take_sent().is_empty());
        assert_eq!(marker_sent(&h, ROOM), None);
        // The room is joined: the marker goes out.
        h.state.muc.nicks.insert(room, "alice".into());
        h.with_ctx(on_connected);
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.type_, xmpp_parsers::message::MessageType::Groupchat);
        assert_eq!(marker_of(m).map(|(_, id, _)| id), Some("s1"));
        assert_eq!(marker_sent(&h, ROOM), Some(s));
    }

    #[test]
    fn mark_read_sends_a_marker_that_waits() {
        let mut h = Harness::new();
        put(&h, "a", Direction::In, MessageKind::Chat, PEER);
        offline_mark(&h, PEER);
        h.with_ctx(|ctx| mark_read(ctx, PEER)).unwrap();
        assert_eq!(h.take_sent().len(), 1);
    }
}
