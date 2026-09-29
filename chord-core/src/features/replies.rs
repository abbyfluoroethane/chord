//! Message replies (XEP-0461), with the XEP-0428 fallback text.
//!
//! An outgoing reply carries `<reply to=SENDER id=REF/>`. It also carries a quoted
//! fallback in the body for clients that do not know XEP-0461. The store keeps only the
//! text of the user, so the timeline shows the reply reference and not the quote.
//!
//! An incoming reply may carry the same fallback. `strip_fallback` gives the body without
//! it. The code that stores an incoming body must call it.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::params;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;

use super::message_ext::Outgoing;
use super::{Ctx, FeatureCommand, IqResponse, chat, muc};
use crate::actor::{ClientError, ClientHandle};
use crate::store::queries::{self, MessageKind};

/// XEP-0461 namespace.
pub const NS_REPLY: &str = "urn:xmpp:reply:0";
/// XEP-0428 namespace.
const NS_FALLBACK: &str = "urn:xmpp:fallback:0";
/// The most quoted lines in a fallback.
const QUOTE_LINES: usize = 3;

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
    Reply {
        item_id: String,
        body: String,
        reply: Reply,
    },
}

impl ClientHandle {
    /// Send a reply to the message with the timeline id `item_id` (XEP-0461). The reply
    /// goes to the chat or room of that message. The stored text is `body` alone.
    ///
    /// Fails with `Invalid` if the message is unknown or has no id that others can use.
    pub async fn reply(&self, item_id: String, body: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Replies(Command::Reply {
            item_id,
            body,
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
        Command::Reply {
            item_id,
            body,
            reply,
        } => {
            let _ = reply.send(send_reply(ctx, &item_id, body));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Reply { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// The reply reference of a message: the id it replies to, and the JID of the sender of
/// that message.
pub(crate) fn reference(message: &Message) -> Option<(String, Option<String>)> {
    let reply = message.payloads.iter().find(|p| p.is("reply", NS_REPLY))?;
    let id = reply.attr("id")?;
    if id.is_empty() {
        return None;
    }
    Some((id.to_owned(), reply.attr("to").map(str::to_owned)))
}

/// The body of a message without its XEP-0428 fallback for a reply. Returns `None` if the
/// message has no such fallback, or if the range is not valid.
///
/// The range counts Unicode code points (XEP-0426).
pub(crate) fn strip_fallback(message: &Message) -> Option<String> {
    let (_, body) = message.get_best_body(vec![])?;
    let fallback = message
        .payloads
        .iter()
        .find(|p| p.is("fallback", NS_FALLBACK) && p.attr("for") == Some(NS_REPLY))?;
    let range = fallback.get_child("body", NS_FALLBACK)?;
    let chars: Vec<char> = body.chars().collect();
    let (start, end) = match (range.attr("start"), range.attr("end")) {
        (Some(s), Some(e)) => (s.parse::<usize>().ok()?, e.parse::<usize>().ok()?),
        // No range means the fallback is the whole body.
        (None, None) => (0, chars.len()),
        _ => return None,
    };
    if start > end || end > chars.len() {
        return None;
    }
    Some(chars[..start].iter().chain(&chars[end..]).collect())
}

/// The quoted fallback text for `target`: `> ` lines with the start of its body.
fn quote(target: &str) -> String {
    target
        .lines()
        .take(QUOTE_LINES)
        .map(|line| format!("> {line}\n"))
        .collect()
}

fn send_reply(ctx: &mut Ctx<'_>, item_id: &str, body: String) -> Result<(), ClientError> {
    let invalid = |what: &str| ClientError::Invalid(what.to_owned());
    let row = queries::find_by_timeline_id(ctx.store.conn(), ctx.account_id, item_id)
        .map_err(|e| ClientError::Invalid(format!("store: {e}")))?
        .ok_or_else(|| invalid("unknown message"))?;
    let reference = queries::reference_id(&row)
        .ok_or_else(|| invalid("the message has no id to reply to"))?
        .to_owned();
    // In a chat the sender is the bare JID. In a room it is room@service/nick.
    let sender = match row.kind {
        MessageKind::Chat => row
            .sender
            .parse::<Jid>()
            .map(|j| j.to_bare().to_string())
            .map_err(|_| invalid("bad sender"))?,
        MessageKind::Groupchat => row.sender.clone(),
    };
    let peer: BareJid = row.peer.parse().map_err(|_| invalid("bad peer"))?;

    let mut out = Outgoing::default();
    out.payloads.push(
        Element::builder("reply", NS_REPLY)
            .attr(nc("to"), sender.as_str())
            .attr(nc("id"), reference.as_str())
            .build(),
    );
    out.extras.reply_to = Some(reference);
    out.extras.reply_to_sender = Some(sender);

    let quoted = if row.retracted {
        String::new()
    } else {
        quote(&row.body)
    };
    let wire_body = if quoted.is_empty() {
        body.clone()
    } else {
        // XEP-0426: the range counts code points.
        let end = quoted.chars().count();
        out.payloads.push(
            Element::builder("fallback", NS_FALLBACK)
                .attr(nc("for"), NS_REPLY)
                .append(
                    Element::builder("body", NS_FALLBACK)
                        .attr(nc("start"), "0")
                        .attr(nc("end"), end.to_string())
                        .build(),
                )
                .build(),
        );
        format!("{quoted}{body}")
    };

    let origin_id = match row.kind {
        MessageKind::Chat => chat::send_message(ctx, Jid::from(peer), wire_body, out),
        MessageKind::Groupchat => muc::send_message(ctx, &peer, wire_body, out)?,
    };
    // The sender stores the body that it sends. Store the text of the user instead.
    if !quoted.is_empty()
        && let Err(e) = ctx.store.conn().execute(
            "UPDATE messages SET body = ?1
             WHERE account_id = ?2 AND key_kind = 'origin-id' AND key = ?3",
            params![body, ctx.account_id, origin_id],
        )
    {
        ctx.store_error("store the body of a reply", e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::store::queries::{Direction, KeyKind, MessageExtras, NewMessage};
    use xmpp_parsers::stanza::Stanza;

    const PEER: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    fn insert(h: &Harness, kind: MessageKind, peer: &str, sender: &str, body: &str) -> String {
        let extras = MessageExtras {
            message_id: Some("m1".into()),
            stanza_id: Some("s1".into()),
            ..Default::default()
        };
        let new = NewMessage {
            kind,
            key_kind: KeyKind::StanzaId,
            key: "s1",
            direction: Direction::In,
            peer,
            sender,
            body,
            timestamp: None,
            extras,
        };
        queries::insert_message(h.store.conn(), h.account_id, &new).unwrap();
        "stanza-id:s1".into()
    }

    fn sent_message(h: &mut Harness) -> Message {
        match h.take_sent().pop().unwrap() {
            Stanza::Message(m) => m,
            other => panic!("{other:?}"),
        }
    }

    fn stored_body(h: &Harness) -> (String, Option<String>) {
        h.store
            .conn()
            .query_row(
                "SELECT body, reply_to FROM messages WHERE direction = 'out'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }

    fn parse(xml: String) -> Message {
        Message::try_from(xml.parse::<Element>().unwrap()).unwrap()
    }

    #[test]
    fn parses_a_reference() {
        let m = parse(format!(
            "<message xmlns='jabber:client' type='chat'><body>x</body>\
             <reply xmlns='{NS_REPLY}' id='abc' to='bob@chord.localhost'/></message>"
        ));
        assert_eq!(
            reference(&m),
            Some(("abc".into(), Some("bob@chord.localhost".into())))
        );
        let plain = Message::chat(None).with_body("".into(), "hi".into());
        assert_eq!(reference(&plain), None);
    }

    #[test]
    fn strips_a_fallback_by_code_points() {
        let end = "> héllo 🎉\n".chars().count();
        let m = parse(format!(
            "<message xmlns='jabber:client' type='chat'><body>&gt; héllo 🎉\nanswer</body>\
             <fallback xmlns='{NS_FALLBACK}' for='{NS_REPLY}'><body start='0' end='{end}'/></fallback></message>"
        ));
        assert_eq!(strip_fallback(&m).as_deref(), Some("answer"));
        let plain = Message::chat(None).with_body("".into(), "hi".into());
        assert_eq!(strip_fallback(&plain), None);
    }

    #[test]
    fn ignores_a_bad_range() {
        let m = parse(format!(
            "<message xmlns='jabber:client' type='chat'><body>abc</body>\
             <fallback xmlns='{NS_FALLBACK}' for='{NS_REPLY}'><body start='0' end='99'/></fallback></message>"
        ));
        assert_eq!(strip_fallback(&m), None);
    }

    #[test]
    fn replies_in_a_chat() {
        let mut h = Harness::new();
        let id = insert(
            &h,
            MessageKind::Chat,
            PEER,
            "bob@chord.localhost/phone",
            "one\ntwo\nthree\nfour",
        );
        h.with_ctx(|ctx| send_reply(ctx, &id, "yes".into()))
            .unwrap();
        let m = sent_message(&mut h);
        let (ref_id, to) = reference(&m).unwrap();
        assert_eq!(ref_id, "m1");
        assert_eq!(to.as_deref(), Some(PEER));
        let (_, wire) = m.get_best_body(vec![]).unwrap();
        assert_eq!(wire, "> one\n> two\n> three\nyes");
        assert_eq!(strip_fallback(&m).as_deref(), Some("yes"));
        assert!(
            m.payloads
                .iter()
                .any(|p| p.is("markable", "urn:xmpp:chat-markers:0"))
        );
        assert_eq!(stored_body(&h), ("yes".into(), Some("m1".into())));
    }

    #[test]
    fn replies_in_a_room() {
        let mut h = Harness::new();
        let sender = format!("{ROOM}/carol");
        let id = insert(&h, MessageKind::Groupchat, ROOM, &sender, "hello");
        h.state
            .muc
            .nicks
            .insert(BareJid::new(ROOM).unwrap(), "alice".into());
        h.with_ctx(|ctx| send_reply(ctx, &id, "hi".into())).unwrap();
        let m = sent_message(&mut h);
        assert_eq!(m.type_, xmpp_parsers::message::MessageType::Groupchat);
        let (ref_id, to) = reference(&m).unwrap();
        assert_eq!(ref_id, "s1");
        assert_eq!(to.as_deref(), Some(sender.as_str()));
        assert_eq!(stored_body(&h), ("hi".into(), Some("s1".into())));
    }

    #[test]
    fn unknown_target_fails() {
        let mut h = Harness::new();
        let r = h.with_ctx(|ctx| send_reply(ctx, "stanza-id:nope", "x".into()));
        assert!(matches!(r, Err(ClientError::Invalid(_))));
    }

    #[test]
    fn offline_answers_an_error() {
        let (reply, mut answer) = oneshot::channel();
        offline(Command::Reply {
            item_id: "x".into(),
            body: "y".into(),
            reply,
        });
        assert!(matches!(
            answer.try_recv(),
            Ok(Some(Err(ClientError::NotConnected)))
        ));
    }
}
