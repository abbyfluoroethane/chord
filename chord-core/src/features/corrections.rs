//! Last message correction (XEP-0308).
//!
//! An incoming message with `<replace id=.../>` sets `edited_body` and `edited_at` of the
//! original message. The `id` is the `id` attribute of the original, in a chat and in a
//! room. `find_message` matches it against the three ids that we store.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::params;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::message_correct::Replace;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::stanza_id::OriginId;

use super::message_ext::{self, Incoming};
use super::{Ctx, new_id};
use crate::actor::{ClientError, ClientHandle};
use crate::store::queries::{self, Direction, MessageKind, MessageRow};
use crate::views::ViewKey;

type Reply = oneshot::Sender<Result<(), ClientError>>;

/// Current Unix time in ms, from SQLite (see `queries::NOW_MS`).
pub(super) const NOW_MS: &str = "CAST(unixepoch('subsec') * 1000 AS INTEGER)";

/// A command from the public API.
pub(crate) enum Command {
    Edit {
        item_id: String,
        body: String,
        reply: Reply,
    },
}

impl ClientHandle {
    /// Correct the text of one of our own messages (XEP-0308). `item_id` is the
    /// `TimelineItem::id` of the message. The row changes at once, and a correction
    /// goes to the peer or the room.
    ///
    /// Fails with `Invalid` for a message from another sender, a retracted message, and a
    /// room that we have not joined.
    pub async fn edit_message(&self, item_id: String, body: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Corrections(Command::Edit {
            item_id,
            body,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Edit {
            item_id,
            body,
            reply,
        } => {
            let _ = reply.send(edit(ctx, &item_id, body));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Edit { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// The id of the message for XEP-0308 and XEP-0424 references in a chat: the `id`
/// attribute, or the origin-id.
pub(super) fn original_id(row: &MessageRow) -> Option<&str> {
    row.message_id.as_deref().or(row.origin_id.as_deref())
}

/// True if `sender` may change the message in `row`: the same bare JID in a chat, the same
/// occupant JID in a room.
pub(super) fn same_sender(row: &MessageRow, sender: &str) -> bool {
    match row.kind {
        MessageKind::Groupchat => row.sender == sender,
        MessageKind::Chat => match (Jid::new(&row.sender), Jid::new(sender)) {
            (Ok(a), Ok(b)) => a.to_bare() == b.to_bare(),
            _ => false,
        },
    }
}

/// The own message with this timeline id, and its peer. Fails when the message is not
/// ours, or when it is in a room that we have not joined.
pub(super) fn own_message(
    ctx: &Ctx<'_>,
    item_id: &str,
) -> Result<(MessageRow, BareJid), ClientError> {
    let row = queries::find_by_timeline_id(ctx.store.conn(), ctx.account_id, item_id)
        .map_err(|e| ClientError::Invalid(format!("store: {e}")))?
        .ok_or_else(|| ClientError::Invalid(format!("no message {item_id}")))?;
    if row.direction != Direction::Out {
        return Err(ClientError::Invalid(
            "only our own messages can change".into(),
        ));
    }
    let peer = BareJid::new(&row.peer)
        .map_err(|e| ClientError::Invalid(format!("bad peer {}: {e}", row.peer)))?;
    Ok((row, peer))
}

/// A new message to the peer of `row`, with the type of the row and a fresh id that is
/// also its origin-id. It has no body and no row in the store.
pub(super) fn new_message(row: &MessageRow, peer: &BareJid) -> Message {
    let to = Jid::from(peer.clone());
    let id = new_id();
    let mut message = match row.kind {
        MessageKind::Chat => Message::chat(to),
        MessageKind::Groupchat => Message::groupchat(to),
    };
    message.type_ = match row.kind {
        MessageKind::Chat => MessageType::Chat,
        MessageKind::Groupchat => MessageType::Groupchat,
    };
    message
        .payloads
        .push(Element::from(OriginId { id: id.clone() }));
    message.id = Some(Id(id));
    message
}

/// Send a correction of an own message and change its row.
fn edit(ctx: &mut Ctx<'_>, item_id: &str, body: String) -> Result<(), ClientError> {
    let (row, peer) = own_message(ctx, item_id)?;
    if row.retracted {
        return Err(ClientError::Invalid("the message is retracted".into()));
    }
    if body.is_empty() {
        return Err(ClientError::Invalid("the new text is empty".into()));
    }
    let original = original_id(&row)
        .ok_or_else(|| ClientError::Invalid("the message has no id".into()))?
        .to_owned();
    let mut message = new_message(&row, &peer).with_body("".into(), body.clone());
    // xmpp-parsers message_correct.rs:16: `Replace` holds the `id` of the original.
    message
        .payloads
        .push(Element::from(Replace { id: Id(original) }));
    message_ext::send_to_peer(ctx, row.kind, &peer, message)?;
    apply(ctx, &row, &body, None);
    Ok(())
}

/// Set the edited text of a message, unless a newer edit is there already. Marks the
/// timeline as changed.
fn apply(ctx: &mut Ctx<'_>, row: &MessageRow, body: &str, timestamp: Option<i64>) {
    let result = ctx.store.conn().execute(
        &format!(
            "UPDATE messages SET edited_body = ?3, edited_at = COALESCE(?4, {NOW_MS})
             WHERE account_id = ?1 AND id = ?2
               AND COALESCE(edited_at, 0) <= COALESCE(?4, {NOW_MS})"
        ),
        params![ctx.account_id, row.rowid, body, timestamp],
    );
    if let Err(e) = result {
        ctx.store_error("store a correction", e);
        return;
    }
    if let Ok(peer) = BareJid::new(&row.peer) {
        ctx.changed(ViewKey::Timeline(peer));
    }
}

/// A message that corrects an earlier message. Returns true if this module handled it, so
/// that no new timeline row appears. A correction with no known target is a new message
/// (XEP-0308, section 3.1), so it returns false.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    let Some(replace) = incoming
        .message
        .payloads
        .iter()
        .find_map(|p| Replace::try_from(p.clone()).ok())
    else {
        return false;
    };
    let Some((_, body)) = incoming.message.get_best_body(vec![]) else {
        return false;
    };
    let target = queries::find_message(
        ctx.store.conn(),
        ctx.account_id,
        incoming.peer.as_str(),
        &replace.id.0,
    );
    let row = match target {
        Ok(Some(row)) => row,
        Ok(None) => return false,
        Err(e) => {
            ctx.store_error("find the message to correct", e);
            return false;
        }
    };
    if !same_sender(&row, incoming.sender) {
        log::debug!(
            "correction from {} for another sender ignored",
            incoming.sender
        );
        return true;
    }
    if row.retracted {
        return true;
    }
    apply(ctx, &row, body, incoming.timestamp);
    true
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::store::queries::{KeyKind, MessageExtras, NewMessage};
    use xmpp_parsers::stanza::Stanza;

    const BOB: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    /// Store a message with body "old". Returns its timeline id.
    pub(crate) fn store_row(
        h: &Harness,
        kind: MessageKind,
        direction: Direction,
        peer: &str,
        sender: &str,
        id: &str,
        stanza_id: Option<&str>,
    ) -> String {
        let (key_kind, key) = match stanza_id {
            Some(s) => (KeyKind::StanzaId, s),
            None => (KeyKind::OriginId, id),
        };
        queries::insert_message(
            h.store.conn(),
            h.account_id,
            &NewMessage {
                kind,
                key_kind,
                key,
                direction,
                peer,
                sender,
                body: "old",
                timestamp: None,
                extras: MessageExtras {
                    message_id: Some(id.into()),
                    origin_id: Some(id.into()),
                    stanza_id: stanza_id.map(Into::into),
                    ..MessageExtras::default()
                },
            },
        )
        .unwrap()
        .unwrap();
        let prefix = if stanza_id.is_some() {
            "stanza-id"
        } else {
            "origin-id"
        };
        format!("{prefix}:{key}")
    }

    fn correction(target: &str, body: &str) -> Message {
        let mut m = Message::chat(None).with_body("".into(), body.into());
        m.payloads.push(Element::from(Replace {
            id: Id(target.into()),
        }));
        m
    }

    fn deliver(h: &mut Harness, m: &Message, sender: &str) -> bool {
        let peer = BareJid::new(BOB).unwrap();
        let incoming = Incoming {
            message: m,
            kind: MessageKind::Chat,
            direction: Direction::In,
            peer: &peer,
            sender,
            timestamp: Some(5000),
        };
        h.with_ctx(|ctx| on_message(ctx, &incoming))
    }

    fn row(h: &Harness, item: &str) -> MessageRow {
        queries::find_by_timeline_id(h.store.conn(), h.account_id, item)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn incoming_edit_applies() {
        let mut h = Harness::new();
        let item = store_row(
            &h,
            MessageKind::Chat,
            Direction::In,
            BOB,
            "bob@chord.localhost/p",
            "m1",
            None,
        );
        let m = correction("m1", "new");
        assert!(deliver(&mut h, &m, "bob@chord.localhost/laptop"));
        assert_eq!(row(&h, &item).body, "new");
        let at: i64 = h
            .store
            .conn()
            .query_row("SELECT edited_at FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(at, 5000);
        assert!(
            h.take_dirty()
                .contains(&ViewKey::Timeline(BareJid::new(BOB).unwrap()))
        );
    }

    #[test]
    fn edit_from_another_sender_is_ignored() {
        let mut h = Harness::new();
        let item = store_row(&h, MessageKind::Chat, Direction::In, BOB, BOB, "m1", None);
        let m = correction("m1", "evil");
        assert!(deliver(&mut h, &m, "eve@chord.localhost/x"));
        assert_eq!(row(&h, &item).body, "old");
    }

    #[test]
    fn edit_before_target_falls_through() {
        let mut h = Harness::new();
        let m = correction("missing", "new");
        assert!(!deliver(&mut h, &m, BOB));
    }

    #[test]
    fn outgoing_edit_in_a_chat_sends_and_changes_the_row() {
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
        h.with_ctx(|ctx| edit(ctx, &item, "fixed".into())).unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.type_, MessageType::Chat);
        assert_eq!(m.to, Some(Jid::new(BOB).unwrap()));
        assert_eq!(m.get_best_body(vec![]).unwrap().1, "fixed");
        let replace = m
            .payloads
            .iter()
            .find_map(|p| Replace::try_from(p.clone()).ok())
            .unwrap();
        assert_eq!(replace.id.0, "m1");
        assert!(
            m.payloads
                .iter()
                .any(|p| OriginId::try_from(p.clone()).is_ok())
        );
        assert_ne!(m.id.as_ref().unwrap().0, "m1");
        assert_eq!(row(&h, &item).body, "fixed");
        // The correction has no row of its own.
        let all = queries::messages_with(h.store.conn(), h.account_id, BOB).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn outgoing_edit_in_a_room_needs_a_join_and_uses_the_message_id() {
        let mut h = Harness::new();
        let sender = format!("{ROOM}/alice");
        let item = store_row(
            &h,
            MessageKind::Groupchat,
            Direction::Out,
            ROOM,
            &sender,
            "m1",
            Some("s1"),
        );
        assert!(h.with_ctx(|ctx| edit(ctx, &item, "x".into())).is_err());
        let room = BareJid::new(ROOM).unwrap();
        h.state.muc.nicks.insert(room, "alice".into());
        h.with_ctx(|ctx| edit(ctx, &item, "fixed".into())).unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert_eq!(m.type_, MessageType::Groupchat);
        let replace = m
            .payloads
            .iter()
            .find_map(|p| Replace::try_from(p.clone()).ok())
            .unwrap();
        assert_eq!(replace.id.0, "m1");
        assert_eq!(row(&h, &item).body, "fixed");
    }

    #[test]
    fn cannot_edit_a_message_from_someone_else() {
        let mut h = Harness::new();
        let item = store_row(&h, MessageKind::Chat, Direction::In, BOB, BOB, "m1", None);
        assert!(h.with_ctx(|ctx| edit(ctx, &item, "x".into())).is_err());
        assert!(h.take_sent().is_empty());
    }
}
