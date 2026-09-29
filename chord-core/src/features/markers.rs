//! Chat markers (XEP-0333): read state and unread counts. Also XEP-0184 receipts.
//!
//! Every outgoing message is `<markable/>`. A `<displayed/>` marker from the peer moves
//! our outgoing messages to `displayed`. A `<displayed/>` marker from our own account (a
//! carbon from another device, or the echo of a room) moves the read position. The unread
//! count of the channel list comes from `read_state.last_read` (a `messages.id`).
//!
//! We never send `<received/>` markers on our own, for privacy. `mark_read` sends a
//! `<displayed/>` marker, because the user asks for it.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::message::{Id, Message};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::stanza::Stanza;

use super::message_ext::Incoming;
use super::{Ctx, FeatureCommand, IqResponse, muc, new_id};
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

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {}

/// A command from the public API.
pub(crate) enum Command {
    MarkRead { peer: BareJid, reply: Reply },
}

impl ClientHandle {
    /// Mark the chat or room `peer` as read up to its newest incoming message. This moves
    /// the read position, which resets the unread count, and sends a `<displayed/>`
    /// marker (XEP-0333) for that message. It sends no marker if that message was read
    /// already.
    pub async fn mark_read(&self, peer: BareJid) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Markers(Command::MarkRead { peer, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_connected(_ctx: &mut Ctx<'_>) {}

pub(crate) fn on_response(_ctx: &mut Ctx<'_>, pending: Pending, _response: IqResponse) {
    match pending {}
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::MarkRead { peer, reply } => {
            let _ = reply.send(mark_read(ctx, &peer));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
///
/// `mark_read` can work offline: `features::on_command_offline` should call
/// `offline_with_store` instead, which has the store.
pub(crate) fn offline(command: Command) {
    match command {
        Command::MarkRead { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// A command while no session is up, with the store. `mark_read` moves the read position
/// and sends no marker. The caller must mark the channel list as changed.
pub(crate) fn offline_with_store(store: &Store, account_id: i64, command: Command) {
    match command {
        Command::MarkRead { peer, reply } => {
            let result = newest_incoming(store, account_id, peer.as_str())
                .and_then(|newest| match newest {
                    Some(n) => set_last_read(store, account_id, peer.as_str(), n.rowid),
                    None => Ok(()),
                })
                .map_err(|e| ClientError::Invalid(format!("store: {e}")));
            let _ = reply.send(result);
        }
    }
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
    let peer = incoming.peer.as_str();
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
        Direction::In => set_status(ctx, peer, level, target.rowid, false),
        // A marker from our own account only counts when it says displayed.
        Direction::Out if level == Level::Displayed && !receipt => {
            set_last_read(ctx.store, ctx.account_id, peer, target.rowid).map(|()| true)
        }
        Direction::Out => Ok(false),
    };
    match result {
        Ok(true) => {
            ctx.changed(ViewKey::Timeline(incoming.peer.clone()));
            if incoming.direction == Direction::Out {
                changed_channel_list(ctx, incoming.peer, incoming.kind);
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

/// Move the read position of `peer` to `rowid`. Never moves it back.
fn set_last_read(store: &Store, account_id: i64, peer: &str, rowid: i64) -> rusqlite::Result<()> {
    store.conn().execute(
        "INSERT INTO read_state (account_id, peer, last_read) VALUES (?1, ?2, ?3)
         ON CONFLICT (account_id, peer) DO UPDATE SET last_read = MAX(last_read, excluded.last_read)",
        params![account_id, peer, rowid],
    )?;
    Ok(())
}

fn changed_channel_list(ctx: &mut Ctx<'_>, peer: &BareJid, kind: MessageKind) {
    match kind {
        // `mark_room` also marks the channel list of each space of the room.
        MessageKind::Groupchat => muc::mark_room(ctx, peer),
        MessageKind::Chat => ctx.changed(ViewKey::ChannelList(ChannelScope::Home)),
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
        .query_row(params![account_id, peer], |row| {
            let kind: String = row.get(1)?;
            Ok(Newest {
                rowid: row.get(0)?,
                kind: if kind == "groupchat" {
                    MessageKind::Groupchat
                } else {
                    MessageKind::Chat
                },
                message_id: row.get(2)?,
                origin_id: row.get(3)?,
                stanza_id: row.get(4)?,
            })
        })
        .optional()
}

fn mark_read(ctx: &mut Ctx<'_>, peer: &BareJid) -> Result<(), ClientError> {
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    let Some(newest) =
        newest_incoming(ctx.store, ctx.account_id, peer.as_str()).map_err(store_error)?
    else {
        return Ok(());
    };
    let last_read: Option<i64> = ctx
        .store
        .conn()
        .query_row(
            "SELECT last_read FROM read_state WHERE account_id = ?1 AND peer = ?2",
            params![ctx.account_id, peer.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(store_error)?;
    if last_read.is_some_and(|n| n >= newest.rowid) {
        return Ok(());
    }
    set_last_read(ctx.store, ctx.account_id, peer.as_str(), newest.rowid).map_err(store_error)?;
    changed_channel_list(ctx, peer, newest.kind);

    // In a room the marker names the stanza-id. In a chat it names the `id` attribute.
    let (reference, mut message) = match newest.kind {
        MessageKind::Groupchat => (
            newest.stanza_id.clone(),
            Message::groupchat(Jid::from(peer.clone())),
        ),
        MessageKind::Chat => (
            newest.message_id.clone().or(newest.origin_id.clone()),
            Message::chat(Jid::from(peer.clone())),
        ),
    };
    let Some(reference) = reference else {
        log::debug!("newest message of {peer} has no id for a marker");
        return Ok(());
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
    ctx.send(Stanza::Message(message));
    Ok(())
}

/// A new message is in the store. Nothing to do: we never send a marker on our own.
pub(crate) fn after_store(_ctx: &mut Ctx<'_>, _incoming: &Incoming<'_>, _stored: &StoredMessage) {}

/// Payloads for every outgoing message: `<markable/>`.
pub(crate) fn outgoing_payloads(_ctx: &mut Ctx<'_>) -> Vec<Element> {
    vec![Element::builder("markable", NS_MARKERS).build()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::store::queries::{KeyKind, MessageExtras, NewMessage, insert_message};

    const PEER: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    fn peer() -> BareJid {
        BareJid::new(PEER).unwrap()
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
        peer: &BareJid,
    ) -> bool {
        let sender = "x";
        let incoming = Incoming {
            message: m,
            kind,
            direction: dir,
            peer,
            sender,
            archived: false,
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
        assert!(h.take_dirty().contains(&ViewKey::Timeline(p.clone())));
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
    fn room_marker_from_another_occupant_marks_own_messages() {
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
            &room
        ));
        assert_eq!(statuses(&h), ["displayed"]);
        assert_eq!(last_read(&h, ROOM), None);
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
        h.with_ctx(|ctx| mark_read(ctx, &room)).unwrap();
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
}
