//! Message reactions (XEP-0444).
//!
//! Each reactions message holds the full set of one sender for one message. A new set
//! replaces the old one, and an empty set removes it. The store keeps one row per sender:
//! the account bare JID for our own reactions, the bare JID of the peer in a chat, and the
//! occupant JID (room@service/nick) in a room.

use futures_channel::oneshot;
use jid::Jid;
use rusqlite::params;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::reactions::{Reaction, Reactions};

use super::message_ext::Incoming;
use super::{Ctx, FeatureCommand, IqResponse, new_id};
use crate::actor::{ClientError, ClientHandle};
use crate::store::json::{from_array, to_array};
use crate::store::queries::{self, Direction, MessageKind};
use crate::views::ViewKey;

/// XEP-0334 namespace, for the `<store/>` hint. xmpp-parsers 0.23 has no hints module.
const NS_HINTS: &str = "urn:xmpp:hints";
/// The longest emoji that we accept, in bytes.
const MAX_EMOJI_BYTES: usize = 64;
/// The most emojis that one sender can set on one message.
const MAX_EMOJIS: usize = 20;

type Reply = oneshot::Sender<Result<(), ClientError>>;

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {}

/// A command from the public API.
pub(crate) enum Command {
    React {
        item_id: String,
        emojis: Vec<String>,
        reply: Reply,
    },
    Toggle {
        item_id: String,
        emoji: String,
        reply: Reply,
    },
}

impl ClientHandle {
    /// Set the full set of our own reactions to a message (XEP-0444). An empty list
    /// removes them. `item_id` is the timeline id of the message.
    ///
    /// Each emoji must have 1 to 64 bytes. At most 20 emojis are allowed. Duplicates are
    /// removed. Fails for a room message that has no stanza-id yet.
    pub async fn react(&self, item_id: String, emojis: Vec<String>) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Reactions(Command::React {
            item_id,
            emojis,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Add `emoji` to our reactions to a message, or remove it if it is there already.
    pub async fn toggle_reaction(&self, item_id: String, emoji: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Reactions(Command::Toggle {
            item_id,
            emoji,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_connected(_ctx: &mut Ctx<'_>) {}

pub(crate) fn on_response(_ctx: &mut Ctx<'_>, pending: Pending, _response: IqResponse) {
    match pending {}
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::React {
            item_id,
            emojis,
            reply,
        } => {
            let _ = reply.send(react(ctx, &item_id, emojis));
        }
        Command::Toggle {
            item_id,
            emoji,
            reply,
        } => {
            let _ = reply.send(toggle(ctx, &item_id, emoji));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::React { reply, .. } | Command::Toggle { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

fn store_error(e: rusqlite::Error) -> ClientError {
    ClientError::Invalid(format!("store: {e}"))
}

/// Remove empty and too long emojis and duplicates. Keep the order.
fn clean(emojis: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in emojis {
        if !e.is_empty() && e.len() <= MAX_EMOJI_BYTES && !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// Check the emojis of an outgoing set, then deduplicate them.
fn validate(emojis: Vec<String>) -> Result<Vec<String>, ClientError> {
    if let Some(e) = emojis
        .iter()
        .find(|e| e.is_empty() || e.len() > MAX_EMOJI_BYTES)
    {
        return Err(ClientError::Invalid(format!(
            "an emoji must have 1 to {MAX_EMOJI_BYTES} bytes, got {}",
            e.len()
        )));
    }
    let emojis = clean(emojis);
    if emojis.len() > MAX_EMOJIS {
        return Err(ClientError::Invalid(format!(
            "at most {MAX_EMOJIS} reactions are allowed"
        )));
    }
    Ok(emojis)
}

/// Replace the set of `sender` for a message. An empty set deletes the row.
fn save(ctx: &Ctx<'_>, message: i64, sender: &str, emojis: &[String]) -> rusqlite::Result<()> {
    let conn = ctx.store.conn();
    if emojis.is_empty() {
        conn.execute(
            "DELETE FROM reactions WHERE account_id = ?1 AND message = ?2 AND sender = ?3",
            params![ctx.account_id, message, sender],
        )?;
    } else {
        conn.execute(
            "INSERT INTO reactions (account_id, message, sender, emojis) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account_id, message, sender) DO UPDATE SET emojis = excluded.emojis",
            params![ctx.account_id, message, sender, to_array(emojis)],
        )?;
    }
    Ok(())
}

/// The set that `sender` has now on a message.
fn load(ctx: &Ctx<'_>, message: i64, sender: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = ctx.store.conn().prepare(
        "SELECT emojis FROM reactions WHERE account_id = ?1 AND message = ?2 AND sender = ?3",
    )?;
    let mut rows = stmt.query(params![ctx.account_id, message, sender])?;
    Ok(match rows.next()? {
        Some(row) => from_array(&row.get::<_, String>(0)?),
        None => Vec::new(),
    })
}

fn find(ctx: &Ctx<'_>, item_id: &str) -> Result<queries::MessageRow, ClientError> {
    queries::find_by_timeline_id(ctx.store.conn(), ctx.account_id, item_id)
        .map_err(store_error)?
        .ok_or_else(|| ClientError::Invalid(format!("no message with id {item_id}")))
}

fn react(ctx: &mut Ctx<'_>, item_id: &str, emojis: Vec<String>) -> Result<(), ClientError> {
    let emojis = validate(emojis)?;
    let row = find(ctx, item_id)?;
    let target = queries::reference_id(&row)
        .ok_or_else(|| ClientError::Invalid("the message has no id to react to yet".into()))?
        .to_owned();
    let to: Jid = row
        .peer
        .parse()
        .map_err(|e| ClientError::Invalid(format!("bad peer {}: {e}", row.peer)))?;
    let type_ = match row.kind {
        MessageKind::Chat => MessageType::Chat,
        MessageKind::Groupchat => MessageType::Groupchat,
    };
    let reactions = Reactions {
        id: target,
        reactions: emojis
            .iter()
            .map(|e| Reaction { emoji: e.clone() })
            .collect(),
    };
    let mut message = Message::new_with_type(type_, Some(to.clone())).with_payload(reactions);
    message.id = Some(Id(new_id()));
    message
        .payloads
        .push(Element::builder("store", NS_HINTS).build());
    save(ctx, row.rowid, &ctx.account.to_string(), &emojis).map_err(store_error)?;
    ctx.send(message);
    ctx.changed(ViewKey::Timeline(to.to_bare()));
    Ok(())
}

fn toggle(ctx: &mut Ctx<'_>, item_id: &str, emoji: String) -> Result<(), ClientError> {
    let row = find(ctx, item_id)?;
    let mut set = load(ctx, row.rowid, &ctx.account.to_string()).map_err(store_error)?;
    match set.iter().position(|e| *e == emoji) {
        Some(i) => {
            set.remove(i);
        }
        None => set.push(emoji),
    }
    react(ctx, item_id, set)
}

/// A message that carries reactions to an earlier message. Returns true if this module applied it, so that no new
/// timeline row appears.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    let Some(reactions) = incoming
        .message
        .payloads
        .iter()
        .find_map(|p| Reactions::try_from(p.clone()).ok())
    else {
        return false;
    };
    let peer = incoming.peer.to_string();
    let row = match queries::find_message(ctx.store.conn(), ctx.account_id, &peer, &reactions.id) {
        Ok(Some(row)) => row,
        Ok(None) => {
            log::debug!(
                "reactions to unknown message {} in {peer}: dropped",
                reactions.id
            );
            return true;
        }
        Err(e) => {
            ctx.store_error("find the reaction target", e);
            return true;
        }
    };
    let sender = match (incoming.direction, incoming.kind) {
        (Direction::Out, _) => ctx.account.to_string(),
        (_, MessageKind::Groupchat) => incoming.sender.to_owned(),
        (_, MessageKind::Chat) => incoming
            .sender
            .split('/')
            .next()
            .unwrap_or(incoming.sender)
            .to_owned(),
    };
    let mut emojis = clean(reactions.reactions.into_iter().map(|r| r.emoji).collect());
    emojis.truncate(MAX_EMOJIS);
    match save(ctx, row.rowid, &sender, &emojis) {
        Ok(()) => ctx.changed(ViewKey::Timeline(incoming.peer.clone())),
        Err(e) => ctx.store_error("save reactions", e),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::{ACCOUNT, Harness};
    use crate::store::queries::{KeyKind, MessageExtras, NewMessage};
    use jid::BareJid;
    use xmpp_parsers::stanza::Stanza;

    const BOB: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    /// Store a message and return its rowid and timeline id.
    fn add(
        h: &Harness,
        kind: MessageKind,
        peer: &str,
        message_id: &str,
        stanza_id: &str,
    ) -> (i64, String) {
        let extras = MessageExtras {
            message_id: Some(message_id.into()),
            stanza_id: Some(stanza_id.into()),
            ..Default::default()
        };
        let new = NewMessage {
            kind,
            key_kind: KeyKind::StanzaId,
            key: stanza_id,
            direction: Direction::In,
            peer,
            sender: peer,
            body: "hello",
            timestamp: None,
            extras,
        };
        let stored = queries::insert_message(h.store.conn(), h.account_id, &new)
            .unwrap()
            .unwrap();
        (stored.rowid, format!("stanza-id:{stanza_id}"))
    }

    fn reactions_message(id: &str, emojis: &[&str]) -> Message {
        let mut m = Message::new(None);
        m.payloads.push(
            Reactions {
                id: id.into(),
                reactions: emojis
                    .iter()
                    .map(|e| Reaction { emoji: (*e).into() })
                    .collect(),
            }
            .into(),
        );
        m
    }

    fn incoming(
        h: &mut Harness,
        message: &Message,
        kind: MessageKind,
        dir: Direction,
        peer: &str,
        sender: &str,
    ) -> bool {
        let peer = BareJid::new(peer).unwrap();
        let inc = Incoming {
            message,
            kind,
            direction: dir,
            peer: &peer,
            sender,
            archived: false,
            timestamp: None,
        };
        h.with_ctx(|ctx| on_message(ctx, &inc))
    }

    fn senders(h: &Harness, rowid: i64) -> Vec<(String, Vec<String>)> {
        let mut stmt = h
            .store
            .conn()
            .prepare("SELECT sender, emojis FROM reactions WHERE message = ?1 ORDER BY sender")
            .unwrap();
        stmt.query_map([rowid], |r| {
            Ok((r.get::<_, String>(0)?, from_array(&r.get::<_, String>(1)?)))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).into()).collect()
    }

    fn timeline(peer: &str) -> ViewKey {
        ViewKey::Timeline(BareJid::new(peer).unwrap())
    }

    #[test]
    fn incoming_set_replace_remove() {
        let mut h = Harness::new();
        let (row, _) = add(&h, MessageKind::Chat, BOB, "m1", "s1");
        let from = format!("{BOB}/phone");
        let m = reactions_message("m1", &["👍", "🎉"]);
        assert!(incoming(
            &mut h,
            &m,
            MessageKind::Chat,
            Direction::In,
            BOB,
            &from
        ));
        assert_eq!(senders(&h, row), vec![(BOB.into(), strs(&["👍", "🎉"]))]);
        assert!(h.take_dirty().contains(&timeline(BOB)));

        let m = reactions_message("m1", &["❤️"]);
        assert!(incoming(
            &mut h,
            &m,
            MessageKind::Chat,
            Direction::In,
            BOB,
            &from
        ));
        assert_eq!(senders(&h, row), vec![(BOB.into(), strs(&["❤️"]))]);

        let m = reactions_message("m1", &[]);
        assert!(incoming(
            &mut h,
            &m,
            MessageKind::Chat,
            Direction::In,
            BOB,
            &from
        ));
        assert!(senders(&h, row).is_empty());
    }

    #[test]
    fn room_reaction_uses_stanza_id_and_occupant() {
        let mut h = Harness::new();
        let (row, _) = add(&h, MessageKind::Groupchat, ROOM, "m1", "s1");
        let m = reactions_message("s1", &["👍"]);
        let from = format!("{ROOM}/carol");
        assert!(incoming(
            &mut h,
            &m,
            MessageKind::Groupchat,
            Direction::In,
            ROOM,
            &from
        ));
        assert_eq!(senders(&h, row), vec![(from, strs(&["👍"]))]);
    }

    #[test]
    fn own_reaction_from_carbon() {
        let mut h = Harness::new();
        let (row, _) = add(&h, MessageKind::Chat, BOB, "m1", "s1");
        let m = reactions_message("m1", &["👍"]);
        let from = format!("{ACCOUNT}/laptop");
        assert!(incoming(
            &mut h,
            &m,
            MessageKind::Chat,
            Direction::Out,
            BOB,
            &from
        ));
        assert_eq!(senders(&h, row), vec![(ACCOUNT.into(), strs(&["👍"]))]);
    }

    #[test]
    fn unknown_target_is_dropped() {
        let mut h = Harness::new();
        let m = reactions_message("nope", &["👍"]);
        assert!(incoming(
            &mut h,
            &m,
            MessageKind::Chat,
            Direction::In,
            BOB,
            BOB
        ));
        let n: i64 = h
            .store
            .conn()
            .query_row("SELECT COUNT(*) FROM reactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        // A message without reactions is not ours.
        assert!(!incoming(
            &mut h,
            &Message::new(None),
            MessageKind::Chat,
            Direction::In,
            BOB,
            BOB
        ));
    }

    #[test]
    fn outgoing_stanza_shape() {
        let mut h = Harness::new();
        let (row, id) = add(&h, MessageKind::Chat, BOB, "m1", "s1");
        h.with_ctx(|ctx| react(ctx, &id, strs(&["👍", "🎉", "👍"])))
            .unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = &sent[..] else {
            panic!("expected one message");
        };
        assert_eq!(m.type_, MessageType::Chat);
        assert_eq!(m.to.as_ref().unwrap().to_string(), BOB);
        let r = m
            .payloads
            .iter()
            .find_map(|p| Reactions::try_from(p.clone()).ok())
            .unwrap();
        assert_eq!(r.id, "m1");
        let got: Vec<_> = r.reactions.iter().map(|r| r.emoji.as_str()).collect();
        assert_eq!(got, ["👍", "🎉"]);
        assert!(m.payloads.iter().any(|p| p.is("store", NS_HINTS)));
        assert_eq!(
            senders(&h, row),
            vec![(ACCOUNT.into(), strs(&["👍", "🎉"]))]
        );
        assert!(h.take_dirty().contains(&timeline(BOB)));

        h.with_ctx(|ctx| react(ctx, &id, vec![])).unwrap();
        assert!(senders(&h, row).is_empty());
    }

    #[test]
    fn outgoing_room_uses_stanza_id_and_validates() {
        let mut h = Harness::new();
        let (_, id) = add(&h, MessageKind::Groupchat, ROOM, "m1", "s1");
        h.with_ctx(|ctx| react(ctx, &id, strs(&["👍"]))).unwrap();
        let sent = h.take_sent();
        let [Stanza::Message(m)] = &sent[..] else {
            panic!("expected one message");
        };
        assert_eq!(m.type_, MessageType::Groupchat);
        let r = m
            .payloads
            .iter()
            .find_map(|p| Reactions::try_from(p.clone()).ok())
            .unwrap();
        assert_eq!(r.id, "s1");

        assert!(h.with_ctx(|ctx| react(ctx, &id, strs(&[""]))).is_err());
        assert!(
            h.with_ctx(|ctx| react(ctx, &id, vec!["x".repeat(65)]))
                .is_err()
        );
        let many: Vec<String> = (0..21).map(|i| format!("e{i}")).collect();
        assert!(h.with_ctx(|ctx| react(ctx, &id, many)).is_err());
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn room_message_without_stanza_id_fails() {
        let mut h = Harness::new();
        let new = NewMessage {
            kind: MessageKind::Groupchat,
            key_kind: KeyKind::OriginId,
            key: "o1",
            direction: Direction::Out,
            peer: ROOM,
            sender: ROOM,
            body: "hi",
            timestamp: None,
            extras: MessageExtras::default(),
        };
        queries::insert_message(h.store.conn(), h.account_id, &new).unwrap();
        assert!(
            h.with_ctx(|ctx| react(ctx, "origin-id:o1", strs(&["👍"])))
                .is_err()
        );
    }

    #[test]
    fn toggle_adds_and_removes() {
        let mut h = Harness::new();
        let (row, id) = add(&h, MessageKind::Chat, BOB, "m1", "s1");
        h.with_ctx(|ctx| toggle(ctx, &id, "👍".into())).unwrap();
        h.with_ctx(|ctx| toggle(ctx, &id, "🎉".into())).unwrap();
        assert_eq!(
            senders(&h, row),
            vec![(ACCOUNT.into(), strs(&["👍", "🎉"]))]
        );
        h.with_ctx(|ctx| toggle(ctx, &id, "👍".into())).unwrap();
        assert_eq!(senders(&h, row), vec![(ACCOUNT.into(), strs(&["🎉"]))]);
        assert_eq!(h.take_sent().len(), 3);
    }
}
