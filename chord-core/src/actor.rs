//! Command loop. One task owns the session, the store, the feature state, and the views.
//!
//! The public API is `ClientHandle`: it is `Send + Sync + Clone` and only holds the
//! sender of the command channel. The actor itself has no `Send` bound, so it also
//! runs on a single-threaded WASM runtime in phase 2. The caller spawns `Actor::run`.
//!
//! For each input (a command, a session event, or an internal result) the actor:
//! 1. calls the features with a `Ctx` (see `features`),
//! 2. runs the effects that they queued (stanzas, events, uploads),
//! 3. runs the queries of the changed views and sends the diffs.

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::{HashMap, HashSet};

use futures_channel::{mpsc, oneshot};
use futures_core::Stream;
use jid::{BareJid, Jid};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::ping::Ping;
use xmpp_parsers::stanza::Stanza;
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use crate::features::{
    self, Ctx, Effect, FeatureCommand, FeatureState, Internal, IqResponse, PendingIq, muc,
};
use crate::session::{
    AuthFailure, ConnectError, DisconnectReason, Session, SessionConfig, SessionError, SessionEvent,
};
use crate::store::queries::{self, StoredMessage};
use crate::store::{Store, StoreError};
use crate::views::{
    ChannelItem, ChannelScope, MemberItem, QueryCtx, Registry, SpaceItem, TimelineItem, ViewKey,
    ViewStream,
};

/// An IQ with no answer after this many session ticks (`session::TICK`, 15 s) fails.
/// A time limit of 60 s to 75 s.
const IQ_TIMEOUT_TICKS: u8 = 5;

/// A command to the actor. Each one carries the channel for its reply.
pub(crate) enum Command {
    Login {
        config: SessionConfig,
        reply: oneshot::Sender<Result<(), ConnectError>>,
    },
    Logout {
        reply: oneshot::Sender<()>,
    },
    SendChat {
        to: Jid,
        body: String,
        reply: oneshot::Sender<Result<String, ClientError>>,
    },
    SpaceList(oneshot::Sender<ViewStream<SpaceItem>>),
    ChannelList(ChannelScope, oneshot::Sender<ViewStream<ChannelItem>>),
    Timeline(
        BareJid,
        Option<String>,
        oneshot::Sender<(u64, ViewStream<TimelineItem>)>,
    ),
    MemberList(BareJid, oneshot::Sender<ViewStream<MemberItem>>),
    PaginateBack {
        timeline: u64,
        count: usize,
    },
    Feature(FeatureCommand),
}

/// The connection state, as a UI shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected {
        bound_jid: Jid,
        resumed: bool,
    },
    /// The connection is lost. The session reconnects by itself.
    Suspended,
    /// The first login failed.
    LoginFailed(ConnectError),
    /// The server rejected the credentials on a reconnect. Ask for new credentials.
    AuthFailed(AuthFailure),
    Disconnected,
}

/// An event from the actor. New variants can come in later versions.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ClientEvent {
    ConnectionState(ConnectionState),
    /// A new live chat message arrived and is in the database. Messages from the
    /// archive (MAM) produce no event: they reach the frontends through the views.
    MessageReceived(StoredMessage),
    /// A feature has a notice for the user, for example a failed room join.
    Notice(String),
    /// A contact asks to see our presence. Call `approve_subscription` or `deny_subscription`.
    SubscriptionRequest(BareJid),
}

/// An error from a `ClientHandle` call.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientError {
    /// No session. Call `login` first.
    NotConnected,
    Session(SessionError),
    /// The server answered with an error.
    Server(String),
    /// The request is not valid, for example a bad JID or a missing file.
    Invalid(String),
    /// The server does not offer the service, for example no upload service.
    Unsupported(String),
    /// The actor stopped.
    ActorGone,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("not logged in"),
            Self::Session(e) => write!(f, "{e}"),
            Self::Server(e) => write!(f, "server error: {e}"),
            Self::Invalid(e) => write!(f, "invalid request: {e}"),
            Self::Unsupported(e) => write!(f, "not supported: {e}"),
            Self::ActorGone => f.write_str("the client stopped"),
        }
    }
}

impl std::error::Error for ClientError {}

/// The public handle. Cheap to clone. Each call sends a command to the actor.
///
/// Feature modules add more methods in their own `impl ClientHandle` blocks.
#[derive(Clone)]
pub struct ClientHandle {
    commands: mpsc::UnboundedSender<Command>,
}

impl ClientHandle {
    /// Log in. Returns after the first login succeeds or fails. The JID must be the
    /// account of the actor.
    pub async fn login(&self, config: SessionConfig) -> Result<(), LoginError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Login { config, reply })
            .map_err(|_| LoginError::ActorGone)?;
        answer
            .await
            .map_err(|_| LoginError::ActorGone)?
            .map_err(LoginError::Connect)
    }

    /// Log out. The server gets every queued stanza before the stream closes.
    pub async fn logout(&self) {
        let (reply, answer) = oneshot::channel();
        if self.send(Command::Logout { reply }).is_ok() {
            let _ = answer.await;
        }
    }

    /// Send a chat message and store it. Returns its origin-id.
    pub async fn send_chat(&self, to: Jid, body: String) -> Result<String, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::SendChat { to, body, reply })
            .map_err(|_| ClientError::ActorGone)?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The spaces for the space rail. Works offline, from the store.
    pub async fn space_list(&self) -> Result<ViewStream<SpaceItem>, ClientError> {
        self.ask(Command::SpaceList).await
    }

    /// The channels of a space, or of Home. Works offline, from the store.
    pub async fn channel_list(
        &self,
        scope: ChannelScope,
    ) -> Result<ViewStream<ChannelItem>, ClientError> {
        self.ask(|reply| Command::ChannelList(scope, reply)).await
    }

    /// The messages of a room or a 1:1 chat. Works offline, from the store.
    pub async fn timeline(&self, room: BareJid) -> Result<Timeline, ClientError> {
        self.open_timeline(room, None).await
    }

    /// The private messages with one occupant of a room (XEP-0045, section 7.5). Works
    /// offline, from the store.
    pub async fn private_timeline(
        &self,
        room: BareJid,
        nick: String,
    ) -> Result<Timeline, ClientError> {
        self.open_timeline(room, Some(nick)).await
    }

    async fn open_timeline(
        &self,
        room: BareJid,
        nick: Option<String>,
    ) -> Result<Timeline, ClientError> {
        let (id, stream) = self
            .ask(|reply| Command::Timeline(room, nick, reply))
            .await?;
        Ok(Timeline {
            id,
            stream,
            handle: self.clone(),
        })
    }

    /// The members of a room, or both people of a 1:1 chat.
    pub async fn member_list(&self, room: BareJid) -> Result<ViewStream<MemberItem>, ClientError> {
        self.ask(|reply| Command::MemberList(room, reply)).await
    }

    async fn ask<T>(
        &self,
        command: impl FnOnce(oneshot::Sender<T>) -> Command,
    ) -> Result<T, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.send(command(reply))
            .map_err(|_| ClientError::ActorGone)?;
        answer.await.map_err(|_| ClientError::ActorGone)
    }

    /// Send a feature command. For the `impl ClientHandle` blocks of the features.
    pub(crate) fn feature(&self, command: FeatureCommand) -> Result<(), ClientError> {
        self.send(Command::Feature(command))
            .map_err(|_| ClientError::ActorGone)
    }

    fn send(&self, command: Command) -> Result<(), ()> {
        self.commands.unbounded_send(command).map_err(|_| ())
    }
}

/// A timeline subscription: the diff stream, and `paginate_back`. It holds a
/// `ClientHandle`, so the actor keeps running while a timeline exists.
pub struct Timeline {
    id: u64,
    pub stream: ViewStream<TimelineItem>,
    handle: ClientHandle,
}

impl Timeline {
    /// Show `count` older messages. If the store has fewer, MAM fetches them.
    pub fn paginate_back(&self, count: usize) -> Result<(), ClientError> {
        self.handle
            .send(Command::PaginateBack {
                timeline: self.id,
                count,
            })
            .map_err(|_| ClientError::ActorGone)
    }
}

/// An error from `ClientHandle::login`.
#[derive(Clone, Debug, PartialEq)]
pub enum LoginError {
    Connect(ConnectError),
    ActorGone,
}

impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(e) => write!(f, "{e}"),
            Self::ActorGone => f.write_str("the client stopped"),
        }
    }
}

impl std::error::Error for LoginError {}

/// The event stream of the actor.
pub struct ClientEvents(mpsc::UnboundedReceiver<ClientEvent>);

impl Stream for ClientEvents {
    type Item = ClientEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<ClientEvent>> {
        Pin::new(&mut self.0).poll_next(cx)
    }
}

/// The session of the logged-in account.
struct Online<S: Session> {
    session: S,
    events: S::Events,
    bound_jid: Option<Jid>,
    /// The id of the ping that `logout` waits for, and the logout reply.
    logout: Option<(String, oneshot::Sender<()>)>,
    /// A logout that waits until the features send their queued stanzas.
    logout_waiting: Option<oneshot::Sender<()>>,
}

/// The actor. It owns the session, the store, and the views. Run it with `run`.
pub struct Actor<S: Session> {
    commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<ClientEvent>,
    internal_tx: mpsc::UnboundedSender<Internal>,
    internal_rx: mpsc::UnboundedReceiver<Internal>,
    store: Store,
    account: BareJid,
    account_id: i64,
    online: Option<Online<S>>,
    state: FeatureState,
    pending: HashMap<String, PendingIq>,
    dirty: HashSet<ViewKey>,
    effects: Vec<Effect>,
    views: Registry,
}

/// Create an actor for `account` on its `store`. Spawn `Actor::run`, then use the
/// handle and the events. The views work before login, from the store.
pub fn new<S: Session>(
    store: Store,
    account: BareJid,
) -> Result<(ClientHandle, ClientEvents, Actor<S>), StoreError> {
    let account_id = queries::ensure_account(store.conn(), account.as_str())?;
    let (command_tx, command_rx) = mpsc::unbounded();
    let (event_tx, event_rx) = mpsc::unbounded();
    let (internal_tx, internal_rx) = mpsc::unbounded();
    let actor = Actor {
        commands: command_rx,
        events: event_tx,
        internal_tx,
        internal_rx,
        store,
        account,
        account_id,
        online: None,
        state: FeatureState::default(),
        pending: HashMap::new(),
        dirty: HashSet::new(),
        effects: Vec::new(),
        views: Registry::default(),
    };
    Ok((
        ClientHandle {
            commands: command_tx,
        },
        ClientEvents(event_rx),
        actor,
    ))
}

enum Next<E> {
    Command(Option<Command>),
    Internal(Internal),
    Session(Option<E>),
}

impl<S: Session> Actor<S> {
    /// Run until every `ClientHandle` is dropped. Then close the session.
    pub async fn run(mut self) {
        loop {
            match self.next().await {
                Next::Command(Some(command)) => self.handle_command(command).await,
                Next::Command(None) => break,
                Next::Internal(internal) => {
                    self.with_ctx(|ctx| features::on_internal(ctx, internal));
                }
                Next::Session(Some(event)) => self.handle_session_event(event).await,
                Next::Session(None) => self.went_offline(),
            }
            self.flush().await;
        }
        if let Some(online) = self.online.take() {
            online.session.disconnect().await;
        }
    }

    /// Wait for the next input. Commands come first.
    async fn next(&mut self) -> Next<SessionEvent> {
        core::future::poll_fn(|cx| {
            if let Poll::Ready(command) = Pin::new(&mut self.commands).poll_next(cx) {
                return Poll::Ready(Next::Command(command));
            }
            if let Poll::Ready(Some(internal)) = Pin::new(&mut self.internal_rx).poll_next(cx) {
                return Poll::Ready(Next::Internal(internal));
            }
            if let Some(online) = &mut self.online
                && let Poll::Ready(event) = Pin::new(&mut online.events).poll_next(cx)
            {
                return Poll::Ready(Next::Session(event));
            }
            Poll::Pending
        })
        .await
    }

    /// Call a feature function with a `Ctx`.
    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut Ctx<'_>) -> R) -> R {
        let mut ctx = Ctx {
            store: &self.store,
            account: &self.account,
            account_id: self.account_id,
            state: &mut self.state,
            effects: &mut self.effects,
            pending: &mut self.pending,
            dirty: &mut self.dirty,
        };
        f(&mut ctx)
    }

    /// Run the queued effects, then refresh the changed views.
    async fn flush(&mut self) {
        for effect in std::mem::take(&mut self.effects) {
            match effect {
                Effect::Send(stanza) => {
                    if let Some(online) = &self.online
                        && let Err(e) = online.session.send(*stanza).await
                    {
                        log::warn!("cannot send a stanza: {e}");
                    }
                }
                Effect::Emit(event) => self.emit(event),
                Effect::Upload(request) => {
                    features::upload::start(request, self.internal_tx.clone());
                }
            }
        }
        // A logout that waited for queued stanzas can go on now.
        if !features::has_queued_stanzas(&self.state)
            && let Some(reply) = self.online.as_mut().and_then(|o| o.logout_waiting.take())
        {
            self.logout(reply).await;
        }
        let dirty = std::mem::take(&mut self.dirty);
        let q = QueryCtx {
            store: &self.store,
            account_id: self.account_id,
            account: &self.account,
        };
        self.views.refresh(&q, &dirty);
    }

    fn emit(&self, event: ClientEvent) {
        // An error means that nobody reads the events. The actor keeps working.
        let _ = self.events.unbounded_send(event);
    }

    fn emit_state(&self, state: ConnectionState) {
        self.emit(ClientEvent::ConnectionState(state));
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::Login { config, reply } => {
                let result = self.login(config).await;
                let _ = reply.send(result);
            }
            Command::Logout { reply } => self.logout(reply).await,
            Command::SendChat { to, body, reply } => {
                let result = if self.online.is_some() {
                    self.with_ctx(|ctx| muc::send_chat(ctx, to, body))
                } else {
                    Err(ClientError::NotConnected)
                };
                let _ = reply.send(result);
            }
            Command::SpaceList(reply) => {
                let q = QueryCtx {
                    store: &self.store,
                    account_id: self.account_id,
                    account: &self.account,
                };
                let _ = reply.send(self.views.subscribe_space_list(&q));
            }
            Command::ChannelList(scope, reply) => {
                let q = QueryCtx {
                    store: &self.store,
                    account_id: self.account_id,
                    account: &self.account,
                };
                let _ = reply.send(self.views.subscribe_channel_list(&q, scope));
            }
            Command::Timeline(room, nick, reply) => {
                let q = QueryCtx {
                    store: &self.store,
                    account_id: self.account_id,
                    account: &self.account,
                };
                let _ = reply.send(self.views.subscribe_timeline(&q, room, nick));
            }
            Command::MemberList(room, reply) => {
                let q = QueryCtx {
                    store: &self.store,
                    account_id: self.account_id,
                    account: &self.account,
                };
                let _ = reply.send(self.views.subscribe_member_list(&q, room));
            }
            Command::PaginateBack { timeline, count } => {
                let q = QueryCtx {
                    store: &self.store,
                    account_id: self.account_id,
                    account: &self.account,
                };
                if let Some(room) = self.views.paginate_back(&q, timeline, count)
                    && self.online.is_some()
                {
                    self.with_ctx(|ctx| features::need_older(ctx, &room));
                }
            }
            Command::Feature(command) => {
                if self.online.is_some() {
                    self.with_ctx(|ctx| features::on_command(ctx, command));
                } else {
                    if features::on_command_offline(&self.store, self.account_id, command) {
                        self.dirty.insert(ViewKey::All);
                    }
                }
            }
        }
    }

    async fn login(&mut self, config: SessionConfig) -> Result<(), ConnectError> {
        if config.jid != self.account {
            let msg = format!("this client is for {}, not {}", self.account, config.jid);
            let error = ConnectError::AuthFailed(AuthFailure::Local(msg));
            self.emit_state(ConnectionState::LoginFailed(error.clone()));
            return Err(error);
        }
        if let Some(online) = self.online.take() {
            online.session.disconnect().await;
            self.fail_pending();
            self.emit_state(ConnectionState::Disconnected);
        }
        self.emit_state(ConnectionState::Connecting);
        let mut session = match S::connect(config).await {
            Ok(session) => session,
            Err(error) => {
                self.emit_state(ConnectionState::LoginFailed(error.clone()));
                return Err(error);
            }
        };
        let events = session
            .events()
            .expect("a new session has its event stream");
        self.online = Some(Online {
            session,
            events,
            bound_jid: None,
            logout: None,
            logout_waiting: None,
        });
        Ok(())
    }

    /// Send a ping to the server and close the session when the answer arrives. The
    /// server handles stanzas in order, so it then has every stanza sent before.
    async fn logout(&mut self, reply: oneshot::Sender<()>) {
        let Some(online) = &mut self.online else {
            let _ = reply.send(());
            return;
        };
        if features::has_queued_stanzas(&self.state) {
            // For example a room message that waits for its join. `flush` calls this
            // again when the queue is empty.
            online.logout_waiting = Some(reply);
            return;
        }
        let id = format!("logout-{}", features::new_id());
        let server = features::disco::domain_of(&self.account);
        let ping = Iq::from_get(id.clone(), Ping).with_to(server);
        if online.session.send(ping.into()).await.is_err() {
            // The session is closed already.
            self.close_session().await;
            let _ = reply.send(());
            return;
        }
        online.logout = Some((id, reply));
    }

    async fn close_session(&mut self) {
        if let Some(online) = self.online.take() {
            online.session.disconnect().await;
            if let Some((_, reply)) = online.logout {
                let _ = reply.send(());
            }
            self.fail_pending();
            self.emit_state(ConnectionState::Disconnected);
        }
    }

    /// The session ended by itself.
    fn went_offline(&mut self) {
        if let Some(online) = self.online.take() {
            if let Some((_, reply)) = online.logout {
                let _ = reply.send(());
            }
            self.fail_pending();
            self.emit_state(ConnectionState::Disconnected);
        }
    }

    /// Count a session tick for each pending IQ. An IQ with no answer after
    /// `IQ_TIMEOUT_TICKS` ticks fails with `remote-server-timeout`, so its command does
    /// not wait forever. A late answer is then dropped.
    fn expire_pending(&mut self) {
        let mut expired = Vec::new();
        for (id, pending) in &mut self.pending {
            pending.ticks = pending.ticks.saturating_add(1);
            if pending.ticks >= IQ_TIMEOUT_TICKS {
                expired.push(id.clone());
            }
        }
        for id in expired {
            let Some(pending) = self.pending.remove(&id) else {
                continue;
            };
            log::warn!("no answer to IQ {id} from {:?}. It failed.", pending.to);
            let error = StanzaError::new(
                ErrorType::Wait,
                DefinedCondition::RemoteServerTimeout,
                "en",
                "no answer in time",
            );
            self.with_ctx(|ctx| {
                features::on_iq_response(ctx, pending.then, IqResponse::Error(error))
            });
        }
    }

    /// Tell each feature that its IQs will get no answer.
    fn fail_pending(&mut self) {
        for (_, pending) in std::mem::take(&mut self.pending) {
            self.with_ctx(|ctx| features::on_iq_response(ctx, pending.then, IqResponse::Lost));
        }
    }

    async fn handle_session_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected {
                bound_jid,
                resumed,
                features,
            } => {
                if let Some(online) = &mut self.online {
                    online.bound_jid = Some(bound_jid.clone());
                }
                self.emit_state(ConnectionState::Connected { bound_jid, resumed });
                if !resumed {
                    // IQs from a lost stream get no answer.
                    self.fail_pending();
                    self.dirty.insert(ViewKey::All);
                }
                self.with_ctx(|ctx| features::on_connected(ctx, resumed, &features));
            }
            SessionEvent::Disconnected(DisconnectReason::Suspended) => {
                self.emit_state(ConnectionState::Suspended);
            }
            SessionEvent::Disconnected(DisconnectReason::AuthFailed(failure)) => {
                self.emit_state(ConnectionState::AuthFailed(failure));
            }
            SessionEvent::Disconnected(DisconnectReason::Closed) => self.went_offline(),
            SessionEvent::Stanza(stanza) => self.handle_stanza(*stanza).await,
            SessionEvent::Tick => self.expire_pending(),
        }
    }

    async fn handle_stanza(&mut self, stanza: Stanza) {
        if let Stanza::Iq(iq @ (Iq::Result { .. } | Iq::Error { .. })) = stanza {
            self.handle_iq_answer(iq).await;
            return;
        }
        self.with_ctx(|ctx| features::on_stanza(ctx, stanza));
    }

    async fn handle_iq_answer(&mut self, iq: Iq) {
        let is_logout = self
            .online
            .as_ref()
            .and_then(|o| o.logout.as_ref())
            .is_some_and(|(id, _)| id == iq.id());
        if is_logout {
            self.close_session().await;
            return;
        }
        let Some(pending) = self.pending.remove(iq.id()) else {
            // A late answer, or an answer to an IQ that the session library sent.
            return;
        };
        if !pending.accepts(iq.from(), &self.account) {
            log::warn!(
                "dropped an IQ answer from {:?}: the request went to {:?}",
                iq.from(),
                pending.to
            );
            self.pending.insert(iq.id().to_owned(), pending);
            return;
        }
        let response = match iq {
            Iq::Result { payload, .. } => IqResponse::Result(payload),
            Iq::Error { error, .. } => IqResponse::Error(error),
            Iq::Get { .. } | Iq::Set { .. } => unreachable!("only answers come here"),
        };
        self.with_ctx(|ctx| features::on_iq_response(ctx, pending.then, response));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use xmpp_parsers::date::DateTime;
    use xmpp_parsers::delay::Delay;
    use xmpp_parsers::message::{Id, Message};
    use xmpp_parsers::stanza_id::{OriginId, StanzaId};

    use super::*;
    use crate::session::{SaslCondition, ServerAddr};
    use crate::store::queries::{Direction, KeyKind};
    use crate::test_support::{FakeSession, next, run_with};

    const ALICE: &str = "alice@chord.localhost";

    fn jid(s: &str) -> Jid {
        Jid::new(s).unwrap()
    }

    fn config() -> SessionConfig {
        let server = ServerAddr::StartTls {
            host: "localhost".into(),
            port: 5222,
        };
        SessionConfig::new(BareJid::new(ALICE).unwrap(), "secret".into(), server)
    }

    fn connected() -> SessionEvent {
        SessionEvent::Connected {
            bound_jid: jid("alice@chord.localhost/chord"),
            resumed: false,
            features: Vec::new(),
        }
    }

    /// An incoming chat message from bob with the given XEP-0359 ids.
    fn incoming(stanza_id: Option<(&str, &str)>, origin_id: Option<&str>) -> SessionEvent {
        let mut message = Message::chat(jid(ALICE)).with_body("".into(), "hi alice".into());
        message.from = Some(jid("bob@chord.localhost/phone"));
        if let Some((id, by)) = stanza_id {
            message = message.with_payload(StanzaId {
                id: id.into(),
                by: jid(by),
            });
        }
        if let Some(id) = origin_id {
            message = message.with_payload(OriginId { id: id.into() });
        }
        SessionEvent::Stanza(Box::new(message.into()))
    }

    /// A database file that the test can open again after the actor stops.
    fn temp_db() -> std::path::PathBuf {
        let name = format!("chord-actor-test-{}.sqlite3", uuid::Uuid::new_v4());
        std::env::temp_dir().join(name)
    }

    fn stored_with_bob(path: &std::path::Path) -> Vec<StoredMessage> {
        let store = Store::open(path).unwrap();
        let account = queries::ensure_account(store.conn(), ALICE).unwrap();
        queries::messages_with(store.conn(), account, "bob@chord.localhost").unwrap()
    }

    /// Collect the events that are ready now.
    fn drain(events: &mut ClientEvents) -> Vec<ClientEvent> {
        let mut out = Vec::new();
        while let Ok(event) = events.0.try_recv() {
            out.push(event);
        }
        out
    }

    #[test]
    fn handle_is_send_sync_clone() {
        fn check<T: Send + Sync + Clone>() {}
        check::<ClientHandle>();
    }

    #[test]
    fn incoming_message_is_stored_and_reported_once() {
        let path = temp_db();
        FakeSession::prepare_connect(vec![
            connected(),
            incoming(Some(("s-1", ALICE)), Some("o-1")),
            // The same message again, for example from MAM: stored once, reported once.
            incoming(Some(("s-1", ALICE)), Some("o-1")),
        ]);
        let (handle, mut events, actor) =
            new::<FakeSession>(Store::open(&path).unwrap(), BareJid::new(ALICE).unwrap()).unwrap();

        let seen = run_with(actor.run(), async {
            handle.login(config()).await.unwrap();
            let mut seen = Vec::new();
            for _ in 0..3 {
                seen.push(next(&mut events).await.unwrap());
            }
            handle.logout().await;
            seen.push(next(&mut events).await.unwrap());
            drop(handle);
            seen.extend(drain(&mut events));
            seen
        });

        assert_eq!(
            seen[0],
            ClientEvent::ConnectionState(ConnectionState::Connecting)
        );
        assert!(matches!(
            &seen[1],
            ClientEvent::ConnectionState(ConnectionState::Connected { resumed: false, .. })
        ));
        match &seen[2] {
            ClientEvent::MessageReceived(message) => {
                assert_eq!(message.key_kind, KeyKind::StanzaId);
                assert_eq!(message.key, "s-1");
                assert_eq!(message.direction, Direction::In);
                assert_eq!(message.peer, "bob@chord.localhost");
                assert_eq!(message.sender, "bob@chord.localhost/phone");
                assert_eq!(message.body, "hi alice");
            }
            other => panic!("expected MessageReceived, got {other:?}"),
        }
        assert_eq!(
            seen[3],
            ClientEvent::ConnectionState(ConnectionState::Disconnected)
        );
        assert_eq!(seen.len(), 4, "no second MessageReceived: {seen:?}");

        let stored = stored_with_bob(&path);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].key, "s-1");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stanza_id_from_another_entity_is_ignored() {
        let path = temp_db();
        let mut delayed = match incoming(Some(("forged", "mallory@evil.example")), Some("o-2")) {
            SessionEvent::Stanza(stanza) => match *stanza {
                Stanza::Message(m) => m,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        let stamp: DateTime = "2026-09-28T10:00:00Z".parse().unwrap();
        delayed = delayed.with_payload(Delay {
            from: None,
            stamp,
            data: None,
        });
        FakeSession::prepare_connect(vec![
            connected(),
            SessionEvent::Stanza(Box::new(delayed.into())),
        ]);
        let (handle, mut events, actor) =
            new::<FakeSession>(Store::open(&path).unwrap(), BareJid::new(ALICE).unwrap()).unwrap();

        let received = run_with(actor.run(), async {
            handle.login(config()).await.unwrap();
            loop {
                if let ClientEvent::MessageReceived(m) = next(&mut events).await.unwrap() {
                    return m;
                }
            }
        });
        assert_eq!(received.key_kind, KeyKind::OriginId);
        assert_eq!(received.key, "o-2");
        assert_eq!(
            received.timestamp, 1_790_589_600_000,
            "the XEP-0203 delay stamp"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn send_chat_sends_origin_id_and_stores_the_message() {
        let path = temp_db();
        let sent: Rc<RefCell<Vec<Stanza>>> = FakeSession::prepare_connect(vec![connected()]);
        let (handle, _events, actor) =
            new::<FakeSession>(Store::open(&path).unwrap(), BareJid::new(ALICE).unwrap()).unwrap();

        let origin_id = run_with(actor.run(), async {
            assert_eq!(
                handle
                    .send_chat(jid("bob@chord.localhost"), "before login".into())
                    .await,
                Err(ClientError::NotConnected)
            );
            handle.login(config()).await.unwrap();
            let id = handle
                .send_chat(jid("bob@chord.localhost"), "hello bob".into())
                .await
                .unwrap();
            handle.logout().await;
            id
        });

        let sent = sent.borrow();
        assert!(
            matches!(&sent[0], Stanza::Presence(_)),
            "initial presence comes first after Connected: {sent:?}"
        );
        let messages: Vec<&Message> = sent
            .iter()
            .filter_map(|s| match s {
                Stanza::Message(m) => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(messages.len(), 1, "{sent:?}");
        let message = messages[0];
        assert_eq!(message.id, Some(Id(origin_id.clone())));
        let origin = message
            .payloads
            .iter()
            .find_map(|p| OriginId::try_from(p.clone()).ok());
        assert_eq!(
            origin,
            Some(OriginId {
                id: origin_id.clone()
            })
        );
        assert!(
            matches!(sent.last(), Some(Stanza::Iq(Iq::Get { payload, .. })) if payload.is("ping", xmpp_parsers::ns::PING)),
            "logout sends a ping last: {sent:?}"
        );

        let stored = stored_with_bob(&path);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].key_kind, KeyKind::OriginId);
        assert_eq!(stored[0].key, origin_id);
        assert_eq!(stored[0].direction, Direction::Out);
        assert_eq!(stored[0].body, "hello bob");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_iq_with_no_answer_fails_after_the_time_limit() {
        let path = temp_db();
        let _sent = FakeSession::prepare_connect(vec![connected()]);
        let (handle, _events, actor) =
            new::<FakeSession>(Store::open(&path).unwrap(), BareJid::new(ALICE).unwrap()).unwrap();

        let result = run_with(actor.run(), async {
            handle.login(config()).await.unwrap();
            // The server never answers. The ticks arrive after the roster set goes out,
            // because the actor takes commands first.
            for _ in 0..IQ_TIMEOUT_TICKS {
                FakeSession::push_event(SessionEvent::Tick);
            }
            let result = handle
                .add_contact(BareJid::new("bob@chord.localhost").unwrap(), None)
                .await;
            drop(handle);
            result
        });

        match result {
            Err(ClientError::Server(text)) => {
                assert!(text.contains("RemoteServerTimeout"), "{text}");
            }
            other => panic!("expected a timeout, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn echo_of_a_sent_message_gets_its_stanza_id() {
        let path = temp_db();
        FakeSession::prepare_connect(vec![connected()]);
        let (handle, mut events, actor) =
            new::<FakeSession>(Store::open(&path).unwrap(), BareJid::new(ALICE).unwrap()).unwrap();

        let seen = run_with(actor.run(), async {
            handle.login(config()).await.unwrap();
            let origin_id = handle
                .send_chat(jid("bob@chord.localhost"), "hello bob".into())
                .await
                .unwrap();
            // The server sends our own message back (as a carbon or a MAM result) with
            // the origin-id and the stanza-id that it gave the message.
            let mut echo = Message::chat(jid("bob@chord.localhost"))
                .with_body("".into(), "hello bob".into())
                .with_payload(OriginId { id: origin_id })
                .with_payload(StanzaId {
                    id: "s-9".into(),
                    by: jid(ALICE),
                });
            echo.from = Some(jid("alice@chord.localhost/other-device"));
            FakeSession::push_event(SessionEvent::Stanza(Box::new(echo.into())));
            handle.logout().await;
            drop(handle);
            drain(&mut events)
        });

        assert!(
            !seen
                .iter()
                .any(|e| matches!(e, ClientEvent::MessageReceived(_))),
            "the echo is not a new message: {seen:?}"
        );
        let stored = stored_with_bob(&path);
        assert_eq!(stored.len(), 1);
        assert_eq!(
            (stored[0].key_kind, stored[0].key.as_str()),
            (KeyKind::StanzaId, "s-9")
        );
        assert_eq!(stored[0].direction, Direction::Out);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn message_from_another_own_client_is_outgoing() {
        let path = temp_db();
        let mut carbon = Message::chat(jid("bob@chord.localhost"))
            .with_body("".into(), "sent from my phone".into())
            .with_payload(StanzaId {
                id: "s-10".into(),
                by: jid(ALICE),
            });
        carbon.from = Some(jid("alice@chord.localhost/phone"));
        FakeSession::prepare_connect(vec![
            connected(),
            SessionEvent::Stanza(Box::new(carbon.into())),
        ]);
        let (handle, mut events, actor) =
            new::<FakeSession>(Store::open(&path).unwrap(), BareJid::new(ALICE).unwrap()).unwrap();

        let received = run_with(actor.run(), async {
            handle.login(config()).await.unwrap();
            loop {
                if let ClientEvent::MessageReceived(m) = next(&mut events).await.unwrap() {
                    return m;
                }
            }
        });
        assert_eq!(received.direction, Direction::Out);
        assert_eq!(received.peer, "bob@chord.localhost");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn timeline_and_channel_list_get_diffs_for_a_new_message() {
        use crate::views::{ChannelKind, ChannelScope, ListDiff};
        FakeSession::prepare_connect(vec![connected()]);
        let (handle, _events, actor) = new::<FakeSession>(
            Store::open_in_memory().unwrap(),
            BareJid::new(ALICE).unwrap(),
        )
        .unwrap();
        let bob = BareJid::new("bob@chord.localhost").unwrap();

        let (timeline_diffs, channel_diffs) = run_with(actor.run(), async {
            // Views work before login, from the store.
            let mut timeline = handle.timeline(bob.clone()).await.unwrap();
            let mut channels = handle.channel_list(ChannelScope::Home).await.unwrap();
            assert_eq!(
                next(&mut timeline.stream).await,
                Some(ListDiff::Reset(vec![]))
            );
            assert_eq!(next(&mut channels).await, Some(ListDiff::Reset(vec![])));

            handle.login(config()).await.unwrap();
            FakeSession::push_event(incoming(Some(("s-1", ALICE)), None));
            (next(&mut timeline.stream).await, next(&mut channels).await)
        });

        match timeline_diffs {
            Some(ListDiff::Insert { index: 0, item }) => {
                assert_eq!(item.id, "stanza-id:s-1");
                assert_eq!(item.sender_name, "bob");
                assert_eq!(item.body, "hi alice");
                assert!(!item.outgoing);
                assert!(!item.same_sender_as_previous);
            }
            other => panic!("expected an insert, got {other:?}"),
        }
        match channel_diffs {
            Some(ListDiff::Insert { index: 0, item }) => {
                assert_eq!(item.jid, "bob@chord.localhost");
                assert_eq!(item.kind, ChannelKind::Direct);
            }
            other => panic!("expected an insert, got {other:?}"),
        }
    }

    #[test]
    fn login_failure_and_auth_failure_after_reconnect_are_reported() {
        FakeSession::prepare_connect_error(ConnectError::AuthFailed(AuthFailure::Sasl(
            SaslCondition::NotAuthorized,
        )));
        let (handle, mut events, actor) = new::<FakeSession>(
            Store::open_in_memory().unwrap(),
            BareJid::new(ALICE).unwrap(),
        )
        .unwrap();
        let failure = AuthFailure::Sasl(SaslCondition::NotAuthorized);

        let seen = run_with(actor.run(), async {
            let error = handle.login(config()).await.unwrap_err();
            assert_eq!(
                error,
                LoginError::Connect(ConnectError::AuthFailed(failure.clone()))
            );

            // Second login works, then a reconnect fails: the session reports AuthFailed.
            FakeSession::prepare_connect(vec![
                connected(),
                SessionEvent::Disconnected(DisconnectReason::Suspended),
                SessionEvent::Disconnected(DisconnectReason::AuthFailed(failure.clone())),
                SessionEvent::Disconnected(DisconnectReason::Closed),
            ]);
            handle.login(config()).await.unwrap();
            let mut seen = Vec::new();
            while seen.last() != Some(&ClientEvent::ConnectionState(ConnectionState::Disconnected))
            {
                seen.push(next(&mut events).await.unwrap());
            }
            seen
        });

        let states: Vec<_> = seen
            .into_iter()
            .map(|e| match e {
                ClientEvent::ConnectionState(state) => state,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(states[0], ConnectionState::Connecting);
        assert!(matches!(
            states[1],
            ConnectionState::LoginFailed(ConnectError::AuthFailed(_))
        ));
        assert_eq!(states[2], ConnectionState::Connecting);
        assert!(matches!(states[3], ConnectionState::Connected { .. }));
        assert_eq!(states[4], ConnectionState::Suspended);
        assert_eq!(states[5], ConnectionState::AuthFailed(failure));
        assert_eq!(states[6], ConnectionState::Disconnected);
    }
}
