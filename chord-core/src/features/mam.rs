//! Message archive management (XEP-0313): catch-up after a new session, and older history for a timeline.

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

/// A message that carries a MAM result. Returns true if it is one.
pub(crate) fn on_result(_ctx: &mut Ctx<'_>, _message: &xmpp_parsers::message::Message) -> bool {
    false
}

/// A timeline wants messages older than the oldest stored one.
pub(crate) fn need_older(_ctx: &mut Ctx<'_>, _room: &jid::BareJid) {}
