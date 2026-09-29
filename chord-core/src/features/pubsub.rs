//! Pubsub client (XEP-0060): requests, and the routing of event notifications.

use super::{Ctx, IqResponse};

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {}

/// A message that carries a pubsub event (also PEP). Returns true if it is one.
pub(crate) fn on_event(_ctx: &mut Ctx<'_>, _message: &xmpp_parsers::message::Message) -> bool {
    false
}

pub(crate) fn on_response(_ctx: &mut Ctx<'_>, pending: Pending, _response: IqResponse) {
    match pending {}
}
