//! Pubsub client (XEP-0060): requests, and the routing of event notifications.
//!
//! Routing of `<event/>` messages:
//! - node `urn:xmpp:bookmarks:1` from our own account: `bookmarks::on_event`,
//! - node `urn:chord:pins:0` from our own account: `pins::on_event`,
//! - node `urn:xmpp:avatar:metadata` (PEP of any contact): `avatars::on_metadata_event`,
//! - a data form `subscribe_authorization` from the pubsub service: `spaces::on_authorization`,
//! - anything else (a pubsub service, for example a space node): `spaces::on_event`.

use jid::Jid;
use xmpp_parsers::message::Message;
use xmpp_parsers::pubsub::event::{Event, Payload};

use super::{Ctx, avatars, bookmarks, pins, spaces};

pub const NODE_BOOKMARKS: &str = "urn:xmpp:bookmarks:1";
pub const NODE_AVATAR_METADATA: &str = "urn:xmpp:avatar:metadata";

/// A message that carries a pubsub event (also PEP). Returns true if it is one.
pub(crate) fn on_event(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    // A join request for a space that we own is a data form, not an event.
    if spaces::on_authorization(ctx, message) {
        return true;
    }
    let Some(event) = message
        .payloads
        .iter()
        .find_map(|p| Event::try_from(p.clone()).ok())
    else {
        return false;
    };
    // PEP events come from the bare JID of the account that owns the node. A missing
    // `from` means our own account.
    let from = message
        .from
        .clone()
        .unwrap_or_else(|| Jid::from(ctx.account.clone()));
    match node_of(&event.payload) {
        NODE_BOOKMARKS => {
            if from.to_bare() == *ctx.account {
                bookmarks::on_event(ctx, event.payload);
            } else {
                log::warn!("dropped a bookmarks event from {from}: not our account");
            }
        }
        pins::NODE_PINS => {
            if from.to_bare() == *ctx.account {
                pins::on_event(ctx, event.payload);
            } else {
                log::warn!("dropped a pins event from {from}: not our account");
            }
        }
        NODE_AVATAR_METADATA => avatars::on_metadata_event(ctx, &from.to_bare(), event.payload),
        _ => spaces::on_event(ctx, &from, event.payload),
    }
    true
}

/// The node name of an event payload.
pub fn node_of(payload: &Payload) -> &str {
    match payload {
        Payload::Configuration { node, .. }
        | Payload::Delete { node, .. }
        | Payload::Items { node, .. }
        | Payload::Purge { node }
        | Payload::Subscription { node, .. } => &node.0,
    }
}
