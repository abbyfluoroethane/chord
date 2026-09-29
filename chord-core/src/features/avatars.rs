//! User avatars (XEP-0084), and the avatars of rooms and spaces.

use super::{Ctx, IqResponse};

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {}

/// A command from the public API.
pub(crate) enum Command {}

pub(crate) fn on_connected(_ctx: &mut Ctx<'_>) {}

pub(crate) fn on_response(_ctx: &mut Ctx<'_>, pending: Pending, _response: IqResponse) {
    match pending {}
}

pub(crate) fn on_command(_ctx: &mut Ctx<'_>, command: Command) {
    match command {}
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {}
}

/// A presence that can carry an avatar hint. Returns true if this module handled it
/// fully, so that no other module needs it.
pub(crate) fn on_presence(
    _ctx: &mut Ctx<'_>,
    _presence: &xmpp_parsers::presence::Presence,
) -> bool {
    false
}

/// A PEP event on the avatar metadata node of `owner`.
pub(crate) fn on_metadata_event(
    _ctx: &mut Ctx<'_>,
    _owner: &jid::BareJid,
    _payload: xmpp_parsers::pubsub::event::Payload,
) {
}
