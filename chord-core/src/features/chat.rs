//! 1:1 chat messages (RFC 6121): store, report, and send them.

use jid::{BareJid, Jid};
use xmpp_parsers::delay::Delay;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::oob::Oob;
use xmpp_parsers::stanza_error::StanzaError;
use xmpp_parsers::stanza_id::{OriginId, StanzaId};

use super::message_ext::{self, Incoming, Outgoing};
use super::{Ctx, new_id};
use crate::actor::ClientEvent;
use crate::store::queries::{self, Direction, KeyKind, LOCAL_KEY_PREFIX, MessageKind, NewMessage};
use crate::views::{ChannelScope, ViewKey};

/// Store a chat message and report it, once per key. A message from our own account
/// (a copy of a message that another client sent) is outgoing.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, message: &Message) {
    if message.type_ == MessageType::Error {
        on_error(ctx, message);
        return;
    }
    let ids = MessageIds::of(message, ctx.account);
    store(ctx, message, ids, delay_ms(message), true);
}

/// Store a chat message from our account archive (MAM). `archive_id` is the MAM result
/// id, which is the stanza-id of our server. The MAM module calls it.
pub(crate) fn store_archived(
    ctx: &mut Ctx<'_>,
    message: &Message,
    archive_id: &str,
    timestamp: Option<i64>,
) {
    let mut ids = MessageIds::of(message, ctx.account);
    ids.stanza_id = Some(archive_id.to_owned());
    // History is not news: it goes to the store and the views, but not to the events.
    store(
        ctx,
        message,
        ids,
        timestamp.or_else(|| delay_ms(message)),
        false,
    );
}

/// Store a message. With `live`, also report it as `ClientEvent::MessageReceived`.
fn store(
    ctx: &mut Ctx<'_>,
    message: &Message,
    ids: MessageIds,
    timestamp: Option<i64>,
    live: bool,
) {
    if !matches!(message.type_, MessageType::Chat | MessageType::Normal) {
        return;
    }
    let Some(from) = &message.from else {
        return;
    };
    // A private message of a room occupant (a carbon or an archive result).
    if super::muc::store_private(ctx, message, &ids, timestamp, live) {
        return;
    }
    let (direction, peer) = if from.to_bare() == *ctx.account {
        match &message.to {
            Some(to) => (Direction::Out, to.to_bare()),
            None => return,
        }
    } else {
        (Direction::In, from.to_bare())
    };
    let sender = from.to_string();
    let incoming = Incoming {
        message,
        kind: MessageKind::Chat,
        direction,
        peer: peer.as_str(),
        sender: &sender,
        timestamp,
    };
    // Corrections, retractions, reactions, and markers change earlier messages.
    if message_ext::intercept(ctx, &incoming) {
        return;
    }
    let Some(body) = message_ext::body(message) else {
        return;
    };
    let peer_str = peer.to_string();

    // A message that is stored under its origin-id, or under a local key from its `id`
    // attribute, gets its stanza-id now.
    let local = local_key(message, from);
    if let (Some(stanza_id), Some(old_key)) = (
        &ids.stanza_id,
        ids.origin_id.clone().or_else(|| local.clone()),
    ) {
        match queries::upgrade_to_stanza_id(
            ctx.store.conn(),
            ctx.account_id,
            &old_key,
            direction,
            &peer_str,
            stanza_id,
        ) {
            Ok(true) => {
                ctx.changed(ViewKey::Timeline(peer.clone()));
                return;
            }
            Ok(false) => {}
            Err(e) => ctx.store_error("update a message key", e),
        }
    }
    let extras = message_ext::extras(message, &ids);
    // A message without both ids gets a local key. The `id` attribute of the sender keeps
    // a repeat of the same stanza out. Without an `id`, the key is random and a repeat
    // cannot be found.
    let (key_kind, key) = ids
        .key()
        .unwrap_or_else(|| (KeyKind::OriginId, local.unwrap_or_else(random_local_key)));
    let new = NewMessage {
        kind: MessageKind::Chat,
        key_kind,
        key: &key,
        direction,
        peer: &peer_str,
        sender: &sender,
        body: &body,
        timestamp,
        extras,
    };
    match queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        Ok(Some(stored)) => {
            message_ext::after_store(ctx, &incoming, &stored, live);
            if live {
                ctx.emit(ClientEvent::MessageReceived(stored));
            }
            ctx.changed(ViewKey::Timeline(peer.clone()));
            ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
        }
        Ok(None) => log::debug!("message {key} is stored already"),
        Err(e) => ctx.store_error("store a message", e),
    }
}

/// The local key of a message that has no stanza-id and no origin-id: its `id` attribute
/// with the bare JID of the sender. The bare JID stops one sender from taking the key of
/// another. A later copy from the archive has a stanza-id, and `store` then changes the
/// key to it.
fn local_key(message: &Message, from: &Jid) -> Option<String> {
    let id = message.id.as_ref().filter(|id| !id.0.is_empty())?;
    Some(format!("{LOCAL_KEY_PREFIX}{}:{}", from.to_bare(), id.0))
}

fn random_local_key() -> String {
    format!("{LOCAL_KEY_PREFIX}{}", new_id())
}

/// A `type="error"` message answers a message of ours that the server or the peer could
/// not deliver (RFC 6121, 8.5). Mark the message that has the same `id`. An error
/// without a match goes to the log only.
fn on_error(ctx: &mut Ctx<'_>, message: &Message) {
    let (Some(from), Some(id)) = (&message.from, &message.id) else {
        return;
    };
    let peer = from.to_bare();
    match queries::mark_failed(ctx.store.conn(), ctx.account_id, peer.as_str(), &id.0) {
        Ok(true) => {
            let condition = message
                .payloads
                .iter()
                .find_map(|p| StanzaError::try_from(p.clone()).ok())
                .map(|e| e.defined_condition);
            log::info!("message {} to {peer} failed: {condition:?}", id.0);
            ctx.changed(ViewKey::Timeline(peer));
            ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
        }
        Ok(false) => log::debug!("error from {from} for {}: no such message", id.0),
        Err(e) => ctx.store_error("mark a message as failed", e),
    }
}

/// Send a chat message and store it. Returns its origin-id.
pub(crate) fn send(ctx: &mut Ctx<'_>, to: Jid, body: String) -> String {
    send_message(ctx, to, body, Outgoing::default())
}

/// Send a chat message with an optional XEP-0066 out-of-band URL and store it. Returns
/// its origin-id.
pub(crate) fn send_with_oob(ctx: &mut Ctx<'_>, to: Jid, body: String, oob: Option<Oob>) -> String {
    let mut out = Outgoing::default();
    if let Some(oob) = oob {
        out.extras.oob_url = Some(oob.url.clone());
        out.payloads.push(oob.into());
    }
    send_message(ctx, to, body, out)
}

/// Send a chat message with extra payloads and references, and store it. Returns its
/// origin-id, which is also its `id` attribute.
pub(crate) fn send_message(ctx: &mut Ctx<'_>, to: Jid, body: String, out: Outgoing) -> String {
    let origin_id = new_id();
    let mut message = Message::chat(to.clone())
        .with_body("".into(), body.clone())
        .with_payload(OriginId {
            id: origin_id.clone(),
        });
    message.payloads.extend(message_ext::outgoing_payloads(ctx));
    // XEP-0184: ask for a receipt. Only a chat message has one, not a room message.
    message.payloads.push(super::markers::request_payload());
    message.payloads.extend(out.payloads);
    message.id = Some(Id(origin_id.clone()));
    ctx.send(message);

    let peer = to.to_bare();
    let peer_str = peer.to_string();
    let sender = ctx.account.to_string();
    let mut extras = out.extras;
    extras.message_id = Some(origin_id.clone());
    extras.origin_id = Some(origin_id.clone());
    let new = NewMessage {
        kind: MessageKind::Chat,
        key_kind: KeyKind::OriginId,
        key: &origin_id,
        direction: Direction::Out,
        peer: &peer_str,
        sender: &sender,
        body: &body,
        timestamp: None,
        extras,
    };
    if let Err(e) = queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        ctx.store_error("store a sent message", e);
    }
    super::chat_states::on_sent(ctx, &peer_str);
    ctx.changed(ViewKey::Timeline(peer));
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
    origin_id
}

/// The XEP-0203 delay of a message, in Unix ms.
pub(crate) fn delay_ms(message: &Message) -> Option<i64> {
    message
        .payloads
        .iter()
        .find_map(|p| Delay::try_from(p.clone()).ok())
        .map(|delay| delay.stamp.0.timestamp_millis())
}

/// The XEP-0359 ids of a message.
pub(crate) struct MessageIds {
    /// The stanza-id that `by` added. A stanza-id from any other entity can be forged,
    /// so it does not count (XEP-0359, section 7).
    pub stanza_id: Option<String>,
    pub origin_id: Option<String>,
}

impl MessageIds {
    /// The ids of a message, with the stanza-id of `by` (the account, or a room).
    pub fn of(message: &Message, by: &BareJid) -> Self {
        let by = Jid::from(by.clone());
        let stanza_id = message
            .payloads
            .iter()
            .filter_map(|p| StanzaId::try_from(p.clone()).ok())
            .find(|id| id.by == by)
            .map(|id| id.id);
        let origin_id = message
            .payloads
            .iter()
            .find_map(|p| OriginId::try_from(p.clone()).ok())
            .map(|origin| origin.id);
        Self {
            stanza_id,
            origin_id,
        }
    }

    /// The key: the stanza-id, otherwise the origin-id.
    pub fn key(self) -> Option<(KeyKind, String)> {
        match (self.stanza_id, self.origin_id) {
            (Some(id), _) => Some((KeyKind::StanzaId, id)),
            (None, Some(id)) => Some((KeyKind::OriginId, id)),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::views::QueryCtx;
    use crate::views::timeline::{self, DeliveryStatus};
    use xmpp_parsers::minidom::Element;

    const PEER: &str = "bob@chord.localhost";

    fn incoming(id: Option<&str>, stanza_id: Option<&str>) -> Message {
        let mut m = Message::chat(Jid::new("alice@chord.localhost").unwrap())
            .with_body("".into(), "hi".into());
        m.from = Some(Jid::new("bob@chord.localhost/phone").unwrap());
        m.id = id.map(|id| Id(id.into()));
        if let Some(stanza_id) = stanza_id {
            m.payloads.push(
                StanzaId {
                    id: stanza_id.into(),
                    by: Jid::new("alice@chord.localhost").unwrap(),
                }
                .into(),
            );
        }
        m
    }

    type Row = (String, String, Option<String>, Option<String>);

    fn rows(h: &Harness) -> Vec<Row> {
        let mut stmt = h
            .store
            .conn()
            .prepare("SELECT key_kind, key, origin_id, stanza_id FROM messages ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn a_live_request_from_a_contact_gets_a_receipt_and_the_archive_copy_does_not() {
        use xmpp_parsers::stanza::Stanza;
        let mut h = Harness::new();
        h.store
            .conn()
            .execute(
                "INSERT INTO contacts (account_id, jid, subscription) VALUES (?1, ?2, 'both')",
                rusqlite::params![h.account_id, PEER],
            )
            .unwrap();
        let mut m = incoming(Some("m-1"), None);
        m.payloads.push(super::super::markers::request_payload());
        h.with_ctx(|ctx| on_message(ctx, &m));
        let sent = h.take_sent();
        let [Stanza::Message(receipt)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert!(
            receipt
                .payloads
                .iter()
                .any(|p| p.is("received", "urn:xmpp:receipts") && p.attr("id") == Some("m-1"))
        );
        let mut old = incoming(Some("m-2"), None);
        old.payloads.push(super::super::markers::request_payload());
        h.with_ctx(|ctx| store_archived(ctx, &old, "s-2", Some(1000)));
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn an_outgoing_chat_message_asks_for_a_receipt() {
        use xmpp_parsers::stanza::Stanza;
        let mut h = Harness::new();
        h.with_ctx(|ctx| send(ctx, Jid::new(PEER).unwrap(), "hello".into()));
        let sent = h.take_sent();
        let [Stanza::Message(m)] = sent.as_slice() else {
            panic!("{sent:?}")
        };
        assert!(
            m.payloads
                .iter()
                .any(|p| p.is("request", "urn:xmpp:receipts"))
        );
    }

    #[test]
    fn a_message_without_ids_is_stored_under_its_id_attribute_and_sender() {
        let mut h = Harness::new();
        let m = incoming(Some("m-1"), None);
        h.with_ctx(|ctx| on_message(ctx, &m));
        // The same stanza again is a repeat.
        h.with_ctx(|ctx| on_message(ctx, &m));
        let stored = rows(&h);
        assert_eq!(stored.len(), 1);
        // A local key is no origin-id.
        assert_eq!(
            stored[0],
            (
                "origin-id".into(),
                "local:bob@chord.localhost:m-1".into(),
                None,
                None
            )
        );
        // The same id from another sender is another message.
        let mut other = incoming(Some("m-1"), None);
        other.from = Some(Jid::new("carol@chord.localhost").unwrap());
        h.with_ctx(|ctx| on_message(ctx, &other));
        assert_eq!(rows(&h).len(), 2);
    }

    #[test]
    fn a_message_without_ids_and_without_an_id_attribute_gets_a_random_key() {
        let mut h = Harness::new();
        let m = incoming(None, None);
        h.with_ctx(|ctx| on_message(ctx, &m));
        h.with_ctx(|ctx| on_message(ctx, &m));
        let stored = rows(&h);
        // The limit: nothing identifies a repeat of a message with no id at all.
        assert_eq!(stored.len(), 2);
        assert!(stored.iter().all(|r| r.1.starts_with(LOCAL_KEY_PREFIX)));
        assert_ne!(stored[0].1, stored[1].1);
    }

    #[test]
    fn the_archive_copy_replaces_the_local_key_with_the_stanza_id() {
        let mut h = Harness::new();
        let m = incoming(Some("m-1"), None);
        h.with_ctx(|ctx| on_message(ctx, &m));
        let archived = incoming(Some("m-1"), Some("srv-9"));
        h.with_ctx(|ctx| store_archived(ctx, &archived, "srv-9", Some(1_000)));
        assert_eq!(
            rows(&h),
            [(
                "stanza-id".into(),
                "srv-9".into(),
                None,
                Some("srv-9".into())
            )]
        );
    }

    #[test]
    fn a_message_with_a_stanza_id_keeps_deduplicating_by_it() {
        let mut h = Harness::new();
        let m = incoming(Some("m-1"), Some("srv-1"));
        h.with_ctx(|ctx| on_message(ctx, &m));
        h.with_ctx(|ctx| on_message(ctx, &m));
        assert_eq!(rows(&h).len(), 1);
        assert_eq!(rows(&h)[0].1, "srv-1");
    }

    fn error_for(id: &str, from: &str) -> Message {
        let mut m = Message::new(Jid::new("alice@chord.localhost").unwrap());
        m.type_ = MessageType::Error;
        m.from = Some(Jid::new(from).unwrap());
        m.id = Some(Id(id.into()));
        let error: Element = "<error xmlns='jabber:client' type='cancel'><service-unavailable xmlns='urn:ietf:params:xml:ns:xmpp-stanzas'/></error>"
            .parse()
            .unwrap();
        m.payloads.push(error);
        m
    }

    fn statuses(h: &Harness, peer: &str) -> Vec<DeliveryStatus> {
        let account = h.account.clone();
        let q = QueryCtx {
            store: &h.store,
            account_id: h.account_id,
            account: &account,
        };
        timeline::query(&q, peer, 50)
            .unwrap()
            .into_iter()
            .map(|i| i.status)
            .collect()
    }

    #[test]
    fn an_error_message_marks_the_sent_message_as_failed() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| send(ctx, Jid::new(PEER).unwrap(), "one".into()));
        let second = h.with_ctx(|ctx| send(ctx, Jid::new(PEER).unwrap(), "two".into()));
        h.take_dirty();
        let error = error_for(&second, "bob@chord.localhost/phone");
        h.with_ctx(|ctx| on_message(ctx, &error));
        assert_eq!(
            statuses(&h, PEER),
            [DeliveryStatus::Sent, DeliveryStatus::Failed]
        );
        assert!(
            h.take_dirty()
                .contains(&ViewKey::Timeline(BareJid::new(PEER).unwrap()))
        );
        // An error is no new message.
        assert_eq!(rows(&h).len(), 2);
    }

    #[test]
    fn an_error_for_an_unknown_id_or_another_peer_changes_nothing() {
        let mut h = Harness::new();
        let id = h.with_ctx(|ctx| send(ctx, Jid::new(PEER).unwrap(), "one".into()));
        h.take_dirty();
        let unknown = error_for("nope", PEER);
        h.with_ctx(|ctx| on_message(ctx, &unknown));
        let forged = error_for(&id, "mallory@evil.example");
        h.with_ctx(|ctx| on_message(ctx, &forged));
        assert_eq!(statuses(&h, PEER), [DeliveryStatus::Sent]);
        assert!(h.take_dirty().is_empty());
    }

    #[test]
    fn retract_of_a_failed_message_hides_it_and_sends_nothing() {
        use crate::features::retraction::{Command, on_command};
        let mut h = Harness::new();
        let id = h.with_ctx(|ctx| send(ctx, Jid::new(PEER).unwrap(), "one".into()));
        let error = error_for(&id, PEER);
        h.with_ctx(|ctx| on_message(ctx, &error));
        h.take_sent();
        let command = Command::Retract {
            item_id: format!("origin-id:{id}"),
            reply: futures_channel::oneshot::channel().0,
        };
        h.with_ctx(|ctx| on_command(ctx, command));
        assert!(statuses(&h, PEER).is_empty());
        assert!(h.take_sent().is_empty());
        // The row stays, so an archive copy of the message does not add it again.
        assert_eq!(rows(&h).len(), 1);
        let mut archived = incoming(Some(&id), Some("srv-3"));
        archived.from = Some(Jid::new("alice@chord.localhost/x").unwrap());
        archived.to = Some(Jid::new(PEER).unwrap());
        archived.payloads.push(OriginId { id: id.clone() }.into());
        h.with_ctx(|ctx| store_archived(ctx, &archived, "srv-3", Some(5)));
        assert_eq!(rows(&h).len(), 1);
        assert!(statuses(&h, PEER).is_empty());
    }
}
