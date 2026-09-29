//! Message retraction (XEP-0424).
//!
//! A retraction keeps the row. It sets `retracted_at`, clears the text, and removes the
//! reactions, so the timeline can show "retracted". The `id` is the `id` attribute of the
//! message in a chat and the stanza-id in a room. A retraction by a room moderator
//! (XEP-0425) is not supported.

use futures_channel::oneshot;
use rusqlite::params;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;

use super::Ctx;
use super::corrections::{NOW_MS, new_message, original_id, own_message, same_sender};
use super::message_ext::{self, Incoming};
use crate::actor::{ClientError, ClientHandle};
use crate::store::queries::{self, MessageKind, MessageRow};
use crate::views::{ChannelScope, ViewKey};

type Reply = oneshot::Sender<Result<(), ClientError>>;

const NS_RETRACT: &str = "urn:xmpp:message-retract:1";
/// The older form: `<apply-to xmlns="urn:xmpp:fasten:0"><retract xmlns=...:0/></apply-to>`.
const NS_RETRACT_OLD: &str = "urn:xmpp:message-retract:0";
const NS_FASTEN: &str = "urn:xmpp:fasten:0";
const NS_FALLBACK: &str = "urn:xmpp:fallback:0";
const NS_HINTS: &str = "urn:xmpp:hints";
const FALLBACK_BODY: &str =
    "This person attempted to retract a previous message, but it's unsupported by your client.";

/// A command from the public API.
pub(crate) enum Command {
    Retract { item_id: String, reply: Reply },
}

impl ClientHandle {
    /// Retract one of our own messages (XEP-0424). `item_id` is the `TimelineItem::id` of
    /// the message. The row shows as retracted at once.
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
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Retract { item_id, reply } => {
            let _ = reply.send(retract(ctx, &item_id));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Retract { reply, .. } => {
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
    let reference = match row.kind {
        MessageKind::Groupchat => row.stanza_id.clone(),
        MessageKind::Chat => original_id(&row).map(str::to_owned),
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

/// A message that retracts an earlier message. Returns true if this module handled it, so
/// that no new timeline row appears.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    let Some(id) = retracted_id(incoming.message) else {
        return false;
    };
    let row = match queries::find_message(ctx.store.conn(), ctx.account_id, incoming.peer, &id) {
        Ok(Some(row)) => row,
        Ok(None) => {
            log::debug!("retraction of {id} dropped: no such message");
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
}
