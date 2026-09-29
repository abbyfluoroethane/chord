//! 1:1 chat messages (RFC 6121): store, report, and send them.

use jid::{BareJid, Jid};
use xmpp_parsers::delay::Delay;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::oob::Oob;
use xmpp_parsers::stanza_id::{OriginId, StanzaId};

use super::message_ext::{self, Incoming, Outgoing};
use super::{Ctx, new_id};
use crate::actor::ClientEvent;
use crate::store::queries::{self, Direction, KeyKind, MessageKind, NewMessage};
use crate::views::{ChannelScope, ViewKey};

/// Store a chat message and report it, once per key. A message from our own account
/// (a copy of a message that another client sent) is outgoing.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, message: &Message) {
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
        peer: &peer,
        sender: &sender,
        archived: !live,
        timestamp,
    };
    // Corrections, retractions, reactions, and markers change earlier messages.
    if message_ext::intercept(ctx, &incoming) {
        return;
    }
    let Some((_, body)) = message.get_best_body(vec![]) else {
        return;
    };
    let peer_str = peer.to_string();

    // A message that is stored under its origin-id gets its stanza-id now.
    if let (Some(stanza_id), Some(origin_id)) = (&ids.stanza_id, &ids.origin_id) {
        match queries::upgrade_to_stanza_id(
            ctx.store.conn(),
            ctx.account_id,
            origin_id,
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
    let Some((key_kind, key)) = ids.key() else {
        log::debug!("chat message from {from} has no stanza-id or origin-id. Not stored.");
        return;
    };
    let new = NewMessage {
        kind: MessageKind::Chat,
        key_kind,
        key: &key,
        direction,
        peer: &peer_str,
        sender: &sender,
        body,
        timestamp,
        extras,
    };
    match queries::insert_message(ctx.store.conn(), ctx.account_id, &new) {
        Ok(Some(stored)) => {
            message_ext::after_store(ctx, &incoming, &stored);
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
