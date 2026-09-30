//! Protocol features. No module here imports `tokio_xmpp` or `tokio`.
//!
//! A feature is a set of plain functions. The actor calls them with a `Ctx` for each
//! input: a new session, a stanza, the answer to an IQ, or a command. A feature reads and
//! writes the store directly, and asks for everything else through the `Ctx`:
//! - `ctx.send(stanza)` queues a stanza for the session.
//! - `ctx.request(iq, pending)` sends an IQ and routes its answer back to the feature.
//! - `ctx.changed(view)` marks a view for a new query and a diff.
//!
//! The actor runs the queued effects after the function returns. So feature code is
//! synchronous, has no `Send` bound, and a test can call it with no session.

pub mod adhoc;
pub mod avatars;
pub mod blocking;
pub mod bookmarks;
pub mod carbons;
pub mod chat;
pub mod chat_states;
pub mod corrections;
pub mod csi;
pub mod disco;
pub mod extdisco;
pub mod file_sharing;
pub mod jmi;
pub mod mam;
pub mod markers;
pub mod message_ext;
pub mod muc;
pub mod notify;
pub mod orphans;
pub mod presence;
pub mod pubsub;
pub mod push;
pub mod reactions;
pub mod replies;
pub mod retraction;
pub mod roster;
pub mod search;
pub mod spaces;
pub mod upload;

#[cfg(test)]
pub(crate) mod testing;

use std::collections::{HashMap, HashSet};

use jid::{BareJid, Jid};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::Message;
use xmpp_parsers::presence::Presence;
use xmpp_parsers::stanza::Stanza;
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use crate::actor::ClientEvent;
use crate::store::Store;
use crate::views::ViewKey;

/// Work that a feature asks the actor to do.
#[derive(Debug)]
pub(crate) enum Effect {
    /// Boxed, because a stanza is large.
    Send(Box<Stanza>),
    /// Send a XEP-0352 client state: `true` for `<active/>`, `false` for `<inactive/>`.
    ClientState(bool),
    Emit(ClientEvent),
    /// Run an HTTP PUT for XEP-0363 upload. The actor gives it to the runtime.
    Upload(upload::PutRequest),
    /// Run an HTTP GET for a space avatar at a URL.
    Download {
        request: spaces::DownloadRequest,
    },
}

/// A result from work outside the session, for example an HTTP upload.
#[derive(Debug)]
pub(crate) enum Internal {
    UploadDone(upload::PutDone),
    DownloadDone(spaces::DownloadDone),
}

/// What to do with the answer to an IQ that a feature sent. One variant per feature, so
/// each feature keeps its own continuation type.
#[derive(Debug)]
pub(crate) enum Pending {
    Carbons,
    Disco(disco::Pending),
    Roster(roster::Pending),
    Mam(mam::Pending),
    Muc(muc::Pending),
    Bookmarks(bookmarks::Pending),
    Spaces(spaces::Pending),
    Upload(upload::Pending),
    Avatars(avatars::Pending),
    Push(push::Pending),
    Blocking(blocking::Pending),
    Presence(presence::Pending),
    Extdisco(extdisco::Pending),
    Adhoc(adhoc::Pending),
}

/// An IQ that waits for its answer.
#[derive(Debug)]
pub(crate) struct PendingIq {
    /// The `to` of the request. The answer must come from it (RFC 6120, 8.1.2.1).
    pub to: Option<Jid>,
    pub then: Pending,
    /// The session ticks since the request went out. See `actor::IQ_TIMEOUT_TICKS`.
    pub ticks: u8,
}

impl PendingIq {
    /// Whether an answer from `from` can be the answer to this request. With no `to`,
    /// the request went to our own account, so the answer comes from our bare JID, our
    /// server, or has no `from`.
    pub fn accepts(&self, from: Option<&Jid>, account: &BareJid) -> bool {
        match (&self.to, from) {
            (Some(to), Some(from)) => to == from,
            (Some(to), None) => to.to_bare() == *account,
            (None, None) => true,
            (None, Some(from)) => {
                from.to_bare() == *account || from.as_str() == account.domain().as_str()
            }
        }
    }
}

/// The in-memory state of all features for one session.
#[derive(Default)]
pub(crate) struct FeatureState {
    pub disco: disco::State,
    pub roster: roster::State,
    pub mam: mam::State,
    pub muc: muc::State,
    pub spaces: spaces::State,
    pub upload: upload::State,
    pub avatars: avatars::State,
    pub chat_states: chat_states::State,
    pub csi: csi::State,
    pub extdisco: extdisco::State,
    pub jmi: jmi::State,
    /// Commands that need a server service (pubsub, upload) and arrived before service
    /// discovery finished. They run when it finishes.
    pub deferred: Vec<FeatureCommand>,
}

/// A command for one feature. The public API in each feature module sends it.
pub(crate) enum FeatureCommand {
    Roster(roster::Command),
    Mam(mam::Command),
    Muc(muc::Command),
    Spaces(spaces::Command),
    Upload(upload::Command),
    Avatars(avatars::Command),
    Corrections(corrections::Command),
    Retraction(retraction::Command),
    Reactions(reactions::Command),
    Replies(replies::Command),
    Markers(markers::Command),
    Push(push::Command),
    Notify(notify::Command),
    ChatStates(chat_states::Command),
    Blocking(blocking::Command),
    Presence(presence::Command),
    Search(search::Command),
    Csi(csi::Command),
    Extdisco(extdisco::Command),
    Jmi(jmi::Command),
    Adhoc(adhoc::Command),
}

/// Everything a feature function can use.
pub(crate) struct Ctx<'a> {
    pub store: &'a Store,
    /// The account, as a bare JID.
    pub account: &'a BareJid,
    /// Row id in `accounts`.
    pub account_id: i64,
    pub state: &'a mut FeatureState,
    pub(crate) effects: &'a mut Vec<Effect>,
    pub(crate) pending: &'a mut HashMap<String, PendingIq>,
    pub(crate) dirty: &'a mut HashSet<ViewKey>,
}

impl Ctx<'_> {
    /// Queue a stanza for the session.
    pub fn send(&mut self, stanza: impl Into<Stanza>) {
        self.effects.push(Effect::Send(Box::new(stanza.into())));
    }

    /// Send an IQ get or set. Its result or error goes to the feature named in `then`.
    /// Returns the IQ id.
    pub fn request(&mut self, mut iq: Iq, then: Pending) -> String {
        let id = new_id();
        *iq.id_mut() = id.clone();
        self.pending.insert(
            id.clone(),
            PendingIq {
                to: iq.to().cloned(),
                then,
                ticks: 0,
            },
        );
        self.send(iq);
        id
    }

    /// Report an event to the frontends.
    pub fn emit(&mut self, event: ClientEvent) {
        self.effects.push(Effect::Emit(event));
    }

    /// Mark a view as changed. The actor runs its query again and sends the diff.
    pub fn changed(&mut self, view: ViewKey) {
        self.dirty.insert(view);
    }

    pub(crate) fn upload(&mut self, request: upload::PutRequest) {
        self.effects.push(Effect::Upload(request));
    }

    pub(crate) fn download(&mut self, request: spaces::DownloadRequest) {
        self.effects.push(Effect::Download { request });
    }

    /// Log a store error. A store error must not stop the actor.
    pub fn store_error(&self, what: &str, error: impl core::fmt::Display) {
        log::error!("store: {what}: {error}");
    }
}

/// A new random id for a stanza.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The answer to an IQ, as a feature sees it.
#[derive(Debug)]
pub(crate) enum IqResponse {
    Result(Option<xmpp_parsers::minidom::Element>),
    Error(StanzaError),
    /// The session closed before the answer arrived.
    Lost,
}

/// A new session is up. `resumed` is true when stream management restored it.
/// `stream_features` lists the namespaces of the other stream features.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>, resumed: bool, stream_features: &[String]) {
    if resumed {
        // The server kept the presence, the carbons state, and the room joins. It did not
        // keep the client state (XEP-0352, 5.2).
        csi::on_connected(ctx, stream_features);
        return;
    }
    chat_states::on_new_session(ctx);
    // The MUC state keeps the rooms to join again, and fails the joins that wait.
    let muc_state = muc::next_session(ctx);
    // Commands that wait for service discovery stay: the new session runs discovery again.
    let deferred = std::mem::take(&mut ctx.state.deferred);
    let inactive = ctx.state.csi.inactive;
    *ctx.state = FeatureState::default();
    ctx.state.muc = muc_state;
    ctx.state.csi.inactive = inactive;
    ctx.state.deferred = deferred;
    if let Err(e) = crate::store::queries::clear_volatile(ctx.store, ctx.account_id) {
        ctx.store_error("clear presence and occupants", e);
    }
    presence::on_connected(ctx);
    carbons::on_connected(ctx);
    disco::on_connected(ctx);
    roster::on_connected(ctx, stream_features);
    bookmarks::on_connected(ctx);
    mam::on_connected(ctx);
    muc::on_connected(ctx);
    markers::on_connected(ctx);
    spaces::on_connected(ctx);
    avatars::on_connected(ctx);
    csi::on_connected(ctx, stream_features);
}

/// A stanza from the server that is not the answer to one of our IQs.
pub(crate) fn on_stanza(ctx: &mut Ctx<'_>, stanza: Stanza) {
    match stanza {
        Stanza::Message(message) => on_message(ctx, message),
        Stanza::Presence(presence) => on_presence(ctx, presence),
        Stanza::Iq(iq) => on_iq_request(ctx, iq),
    }
}

fn on_message(ctx: &mut Ctx<'_>, message: Message) {
    // A carbon holds a copy of a message that another client of the account sent or got.
    let message = match carbons::unwrap(ctx, message) {
        Some(message) => message,
        None => return,
    };
    if pubsub::on_event(ctx, &message) {
        return;
    }
    if mam::on_result(ctx, &message) {
        return;
    }
    if jmi::on_message(ctx, &message) {
        return;
    }
    chat_states::on_message(ctx, &message);
    if muc::on_message(ctx, &message) {
        return;
    }
    chat::on_message(ctx, &message);
}

fn on_presence(ctx: &mut Ctx<'_>, presence: Presence) {
    if muc::on_presence(ctx, &presence) {
        return;
    }
    if avatars::on_presence(ctx, &presence) {
        return;
    }
    roster::on_presence(ctx, &presence);
}

/// An IQ get or set to us. Every one gets an answer (RFC 6120, 8.2.3).
fn on_iq_request(ctx: &mut Ctx<'_>, iq: Iq) {
    let handled = match &iq {
        Iq::Get { .. } | Iq::Set { .. } => {
            disco::on_iq(ctx, &iq)
                || roster::on_iq(ctx, &iq)
                || blocking::on_iq(ctx, &iq)
                || extdisco::on_iq(ctx, &iq)
                || ping_reply(ctx, &iq)
        }
        // Results and errors without a pending entry: late answers. Ignore them.
        Iq::Result { .. } | Iq::Error { .. } => true,
    };
    if !handled {
        let error = StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::ServiceUnavailable,
            "en",
            "Chord does not handle this request",
        );
        ctx.send(error_reply(&iq, error));
    }
}

fn ping_reply(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let Iq::Get { payload, .. } = iq else {
        return false;
    };
    if !payload.is("ping", xmpp_parsers::ns::PING) {
        return false;
    }
    ctx.send(result_reply(iq, None));
    true
}

/// The `result` answer to an IQ get or set.
pub(crate) fn result_reply(iq: &Iq, payload: Option<xmpp_parsers::minidom::Element>) -> Iq {
    Iq::Result {
        from: None,
        to: iq.from().cloned(),
        id: iq.id().to_owned(),
        payload,
    }
}

/// The `error` answer to an IQ get or set.
pub(crate) fn error_reply(iq: &Iq, error: StanzaError) -> Iq {
    Iq::Error {
        from: None,
        to: iq.from().cloned(),
        id: iq.id().to_owned(),
        error,
        payload: None,
    }
}

/// The answer to one of our IQs.
pub(crate) fn on_iq_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Carbons => carbons::on_enabled(ctx, response),
        Pending::Disco(p) => disco::on_response(ctx, p, response),
        Pending::Roster(p) => roster::on_response(ctx, p, response),
        Pending::Mam(p) => mam::on_response(ctx, p, response),
        Pending::Muc(p) => muc::on_response(ctx, p, response),
        Pending::Bookmarks(p) => bookmarks::on_response(ctx, p, response),
        Pending::Spaces(p) => spaces::on_response(ctx, p, response),
        Pending::Upload(p) => upload::on_response(ctx, p, response),
        Pending::Avatars(p) => avatars::on_response(ctx, p, response),
        Pending::Push(p) => push::on_response(ctx, p, response),
        Pending::Blocking(p) => blocking::on_response(ctx, p, response),
        Pending::Presence(p) => presence::on_response(ctx, p, response),
        Pending::Extdisco(p) => extdisco::on_response(ctx, p, response),
        Pending::Adhoc(p) => adhoc::on_response(ctx, p, response),
    }
}

/// A command for one feature. A command that needs a server service waits until
/// service discovery finishes (see `on_services_ready`).
pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: FeatureCommand) {
    let needs_services = matches!(
        command,
        FeatureCommand::Spaces(_)
            | FeatureCommand::Upload(_)
            | FeatureCommand::Push(push::Command::Enable { .. })
            | FeatureCommand::Extdisco(_)
            | FeatureCommand::Muc(muc::Command::RoomService { .. })
            | FeatureCommand::Blocking(
                blocking::Command::Block { .. }
                    | blocking::Command::Unblock { .. }
                    | blocking::Command::UnblockAll { .. }
            )
    );
    if needs_services && !ctx.state.disco.complete {
        ctx.state.deferred.push(command);
        return;
    }
    dispatch(ctx, command);
}

/// Service discovery finished. Run the commands that waited for it.
pub(crate) fn on_services_ready(ctx: &mut Ctx<'_>) {
    blocking::on_services_ready(ctx);
    extdisco::on_services_ready(ctx);
    for command in std::mem::take(&mut ctx.state.deferred) {
        dispatch(ctx, command);
    }
}

fn dispatch(ctx: &mut Ctx<'_>, command: FeatureCommand) {
    match command {
        FeatureCommand::Roster(c) => roster::on_command(ctx, c),
        FeatureCommand::Mam(c) => mam::on_command(ctx, c),
        FeatureCommand::Muc(c) => muc::on_command(ctx, c),
        FeatureCommand::Spaces(c) => spaces::on_command(ctx, c),
        FeatureCommand::Upload(c) => upload::on_command(ctx, c),
        FeatureCommand::Avatars(c) => avatars::on_command(ctx, c),
        FeatureCommand::Corrections(c) => corrections::on_command(ctx, c),
        FeatureCommand::Retraction(c) => retraction::on_command(ctx, c),
        FeatureCommand::Reactions(c) => reactions::on_command(ctx, c),
        FeatureCommand::Replies(c) => replies::on_command(ctx, c),
        FeatureCommand::Markers(c) => markers::on_command(ctx, c),
        FeatureCommand::Push(c) => push::on_command(ctx, c),
        FeatureCommand::Notify(c) => notify::on_command(ctx, c),
        FeatureCommand::ChatStates(c) => chat_states::on_command(ctx, c),
        FeatureCommand::Blocking(c) => blocking::on_command(ctx, c),
        FeatureCommand::Presence(c) => presence::on_command(ctx, c),
        FeatureCommand::Search(c) => search::on_command(ctx, c),
        FeatureCommand::Csi(c) => csi::on_command(ctx, c),
        FeatureCommand::Extdisco(c) => extdisco::on_command(ctx, c),
        FeatureCommand::Jmi(c) => jmi::on_command(ctx, c),
        FeatureCommand::Adhoc(c) => adhoc::on_command(ctx, c),
    }
}

/// A command while no session is up. Reads from the store work offline, and `mark_read`
/// moves the read position. Every other command answers `ClientError::NotConnected`.
/// Returns true if the command changed the store, so that the views need a new query.
pub(crate) fn on_command_offline(store: &Store, account_id: i64, command: FeatureCommand) -> bool {
    use crate::actor::ClientError;
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    match command {
        FeatureCommand::Roster(roster::Command::Contacts { reply }) => {
            let _ =
                reply.send(roster::list_contacts(store.conn(), account_id).map_err(store_error));
        }
        FeatureCommand::Avatars(avatars::Command::Get { owner, reply }) => {
            let _ = reply.send(avatars::load(store, account_id, &owner).map_err(store_error));
        }
        FeatureCommand::Spaces(spaces::Command::PendingJoins { reply }) => {
            let _ = reply.send(spaces::pending_joins(store, account_id));
        }
        FeatureCommand::Push(push::Command::List { reply }) => {
            let _ = reply.send(push::list(store, account_id));
        }
        FeatureCommand::Blocking(blocking::Command::List { reply }) => {
            let _ = reply.send(blocking::list(store, account_id));
        }
        FeatureCommand::Notify(c) => notify::run(store, account_id, c),
        FeatureCommand::Search(c) => search::run(store, account_id, c),
        FeatureCommand::Markers(c) => {
            markers::offline_with_store(store, account_id, c);
            return true;
        }
        FeatureCommand::Roster(c) => roster::offline(c),
        FeatureCommand::Mam(c) => mam::offline(c),
        FeatureCommand::Muc(c) => muc::offline(c),
        FeatureCommand::Spaces(c) => spaces::offline(c),
        FeatureCommand::Upload(c) => upload::offline(c),
        FeatureCommand::Avatars(c) => avatars::offline(c),
        FeatureCommand::Corrections(c) => corrections::offline(c),
        FeatureCommand::Retraction(c) => retraction::offline(c),
        FeatureCommand::Reactions(c) => reactions::offline(c),
        FeatureCommand::Replies(c) => replies::offline(c),
        FeatureCommand::Push(c) => push::offline(c),
        FeatureCommand::Extdisco(c) => extdisco::offline(c),
        FeatureCommand::Jmi(c) => jmi::offline(c),
        FeatureCommand::Adhoc(c) => adhoc::offline(c),
        FeatureCommand::ChatStates(c) => chat_states::offline(c),
        FeatureCommand::Blocking(c) => blocking::offline(c),
        // The actor keeps the wanted state (`csi::offline`), so it never gets here.
        FeatureCommand::Csi(_) => {}
        FeatureCommand::Presence(c) => {
            presence::offline(store, account_id, c);
            return true;
        }
    }
    false
}

/// A session tick. Features use it for time limits.
pub(crate) fn on_tick(ctx: &mut Ctx<'_>) {
    chat_states::on_tick(ctx);
    extdisco::on_tick(ctx);
    jmi::on_tick(ctx);
}

/// A result from work outside the session.
pub(crate) fn on_internal(ctx: &mut Ctx<'_>, internal: Internal) {
    match internal {
        Internal::UploadDone(done) => upload::on_put_done(ctx, done),
        Internal::DownloadDone(done) => spaces::on_download_done(ctx, done),
    }
}

/// True while features hold stanzas that must go out before a logout, for example room
/// messages that wait for a join.
pub(crate) fn has_queued_stanzas(state: &FeatureState) -> bool {
    muc::has_outbox(&state.muc)
}

/// A timeline wants more history than the store has. MAM fetches older messages.
pub(crate) fn need_older(ctx: &mut Ctx<'_>, room: &BareJid) {
    mam::need_older(ctx, room);
}
