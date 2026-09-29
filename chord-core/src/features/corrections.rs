//! Last message correction (XEP-0308).

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

/// A message that corrects an earlier message. Returns true if this module applied it, so that no new
/// timeline row appears.
pub(crate) fn on_message(_ctx: &mut Ctx<'_>, _incoming: &super::message_ext::Incoming<'_>) -> bool {
    false
}
