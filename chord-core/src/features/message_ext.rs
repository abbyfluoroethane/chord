//! Message extensions: the hooks where each XEP that changes or annotates a message
//! gets it.
//!
//! `chat` and `muc` call these hooks for every chat or groupchat message, live or from the
//! archive, before they store it:
//! - `intercept`: a message that changes an earlier one (XEP-0308 correction, XEP-0424
//!   retraction, XEP-0444 reaction) or only carries a chat marker (XEP-0333). The feature
//!   applies it, and no new timeline row appears.
//! - `extras`: the ids and references of a new message (XEP-0461 reply, XEP-0066 URL).
//! - `after_store`: a new message is in the store.
//!
//! Outgoing messages get `outgoing_payloads` (for example XEP-0333 `<markable/>`).

use jid::BareJid;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::oob::Oob;

use super::chat::MessageIds;
use super::{Ctx, corrections, markers, muc, reactions, replies, retraction};
use crate::actor::ClientError;
use crate::store::queries::{Direction, MessageExtras, MessageKind, StoredMessage};

/// A chat or groupchat message, as the extension features see it.
pub(crate) struct Incoming<'a> {
    pub message: &'a Message,
    pub kind: MessageKind,
    pub direction: Direction,
    /// Bare JID of the chat peer, or of the room.
    pub peer: &'a BareJid,
    /// JID of the sender: a full or bare JID in a 1:1 chat, room@service/nick in a room.
    pub sender: &'a str,
    /// Unix time in ms, from the archive or the XEP-0203 delay.
    pub timestamp: Option<i64>,
}

/// Apply a message that changes an earlier message, or that only carries a marker.
/// Returns true if the message is fully handled, so that no new timeline row appears.
pub(crate) fn intercept(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>) -> bool {
    corrections::on_message(ctx, incoming)
        || retraction::on_message(ctx, incoming)
        || reactions::on_message(ctx, incoming)
        || markers::on_message(ctx, incoming)
}

/// The ids and references of a new message.
pub(crate) fn extras(message: &Message, ids: &MessageIds) -> MessageExtras {
    let (reply_to, reply_to_sender) = match replies::reference(message) {
        Some((id, to)) => (Some(id), to),
        None => (None, None),
    };
    MessageExtras {
        message_id: message.id.as_ref().map(|id| id.0.clone()),
        origin_id: ids.origin_id.clone(),
        stanza_id: ids.stanza_id.clone(),
        reply_to,
        reply_to_sender,
        oob_url: message
            .payloads
            .iter()
            .find_map(|p| Oob::try_from(p.clone()).ok())
            .map(|oob| oob.url),
    }
}

/// The body to store for a new message: without the XEP-0428 fallback of a reply.
pub(crate) fn body(message: &Message) -> Option<String> {
    replies::strip_fallback(message)
        .or_else(|| message.get_best_body(vec![]).map(|(_, body)| body.clone()))
}

/// A new message is in the store.
pub(crate) fn after_store(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>, stored: &StoredMessage) {
    markers::after_store(ctx, incoming, stored);
}

/// Payloads for every outgoing chat or groupchat message.
pub(crate) fn outgoing_payloads(ctx: &mut Ctx<'_>) -> Vec<Element> {
    markers::outgoing_payloads(ctx)
}

/// Send a message that changes or annotates an earlier message (a correction, a
/// retraction, a reaction, or a marker) to the peer of that message. A message to a room
/// waits in the room outbox until the join completes.
pub(crate) fn send_to_peer(
    ctx: &mut Ctx<'_>,
    kind: MessageKind,
    peer: &BareJid,
    message: Message,
) -> Result<(), ClientError> {
    match kind {
        MessageKind::Chat => {
            ctx.send(message);
            Ok(())
        }
        MessageKind::Groupchat => muc::send_to_room(ctx, peer, message),
    }
}

/// What a sender adds to a new outgoing message: payloads and the references to store.
#[derive(Debug, Default)]
pub(crate) struct Outgoing {
    pub payloads: Vec<Element>,
    /// References to store with the message (`reply_to`, `oob_url`). The ids are set
    /// by the sender.
    pub extras: MessageExtras,
}
