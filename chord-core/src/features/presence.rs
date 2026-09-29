//! Presence (RFC 6121): our own presence. Contact presence is in `roster`, room presence
//! in `muc`.

use xmpp_parsers::presence::Presence;

use super::{Ctx, disco};

/// Send initial presence (RFC 6121, 4.2) with our entity capabilities (XEP-0115). The
/// server routes chat messages to a resource only after it is available. PEP sends
/// notifications (`+notify`) only to clients whose caps ask for them.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    ctx.send(initial());
}

/// Available presence with our caps.
pub fn initial() -> Presence {
    Presence::available().with_payloads(vec![disco::caps().into()])
}
