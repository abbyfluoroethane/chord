//! HTTP file upload (XEP-0363): slot request and upload.

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

/// An HTTP PUT for the runtime to run.
#[derive(Debug)]
pub(crate) struct PutRequest {}

/// The result of a `PutRequest`.
#[derive(Debug)]
pub(crate) struct PutDone {}

/// Run a PUT. Send the result to `done` as `Internal::UploadDone`.
pub(crate) fn start(
    request: PutRequest,
    _done: futures_channel::mpsc::UnboundedSender<super::Internal>,
) {
    log::warn!("HTTP upload is not implemented yet: {request:?}");
}

/// A PUT finished.
pub(crate) fn on_put_done(_ctx: &mut Ctx<'_>, _done: PutDone) {}
