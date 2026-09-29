//! Command loop. One task owns the session and the database.
//!
//! The public API is `ClientHandle`: it is `Send + Sync + Clone` and only holds the
//! sender of the command channel. The actor itself has no `Send` bound, so it also
//! runs on a single-threaded WASM runtime in phase 2. The caller spawns `Actor::run`.

use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};

use futures_channel::{mpsc, oneshot};
use futures_core::Stream;
use jid::{BareJid, Jid};
use xmpp_parsers::delay::Delay;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::ping::Ping;
use xmpp_parsers::stanza::Stanza;
use xmpp_parsers::stanza_id::{OriginId, StanzaId};

use crate::session::{
    AuthFailure, ConnectError, DisconnectReason, Session, SessionConfig, SessionError, SessionEvent,
};
use crate::store::Store;
use crate::store::queries::{self, Direction, KeyKind, NewMessage, StoredMessage};

/// A command to the actor. Each one carries the channel for its reply.
enum Command {
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

/// An event from the actor.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientEvent {
    ConnectionState(ConnectionState),
    /// A new chat message arrived and is in the database.
    MessageReceived(StoredMessage),
}

/// An error from a `ClientHandle` call.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientError {
    /// No session. Call `login` first.
    NotConnected,
    Session(SessionError),
    /// The actor stopped.
    ActorGone,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("not logged in"),
            Self::Session(e) => write!(f, "{e}"),
            Self::ActorGone => f.write_str("the client stopped"),
        }
    }
}

impl std::error::Error for ClientError {}

/// The public handle. Cheap to clone. Each call sends a command to the actor.
#[derive(Clone)]
pub struct ClientHandle {
    commands: mpsc::UnboundedSender<Command>,
}

impl ClientHandle {
    /// Log in. Returns after the first login succeeds or fails.
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

    fn send(&self, command: Command) -> Result<(), ()> {
        self.commands.unbounded_send(command).map_err(|_| ())
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
    account: BareJid,
    /// Row id in `accounts`, or `None` if the database failed.
    account_id: Option<i64>,
    /// The id of the ping that `logout` waits for, and the logout reply.
    logout: Option<(String, oneshot::Sender<()>)>,
}

/// The actor. It owns the session and the database. Run it with `run`.
pub struct Actor<S: Session> {
    commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<ClientEvent>,
    store: Store,
    online: Option<Online<S>>,
}

/// Create an actor on `store`. Spawn `Actor::run`, then use the handle and the events.
pub fn new<S: Session>(store: Store) -> (ClientHandle, ClientEvents, Actor<S>) {
    let (command_tx, command_rx) = mpsc::unbounded();
    let (event_tx, event_rx) = mpsc::unbounded();
    let actor = Actor {
        commands: command_rx,
        events: event_tx,
        store,
        online: None,
    };
    (
        ClientHandle {
            commands: command_tx,
        },
        ClientEvents(event_rx),
        actor,
    )
}

enum Next<E> {
    Command(Option<Command>),
    Session(Option<E>),
}

impl<S: Session> Actor<S> {
    /// Run until every `ClientHandle` is dropped. Then close the session.
    pub async fn run(mut self) {
        loop {
            match self.next().await {
                Next::Command(Some(command)) => self.handle_command(command).await,
                Next::Command(None) => break,
                Next::Session(Some(event)) => self.handle_session_event(event).await,
                Next::Session(None) => self.went_offline(),
            }
        }
        if let Some(online) = self.online.take() {
            online.session.disconnect().await;
        }
    }

    /// Wait for the next command or session event. Commands come first.
    async fn next(&mut self) -> Next<SessionEvent> {
        core::future::poll_fn(|cx| {
            if let Poll::Ready(command) = Pin::new(&mut self.commands).poll_next(cx) {
                return Poll::Ready(Next::Command(command));
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
                let _ = reply.send(self.send_chat(to, body).await);
            }
        }
    }

    async fn login(&mut self, config: SessionConfig) -> Result<(), ConnectError> {
        if let Some(online) = self.online.take() {
            online.session.disconnect().await;
            self.emit_state(ConnectionState::Disconnected);
        }
        self.emit_state(ConnectionState::Connecting);
        let account = config.jid.clone();
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
        let account_id = queries::ensure_account(self.store.conn(), account.as_str())
            .map_err(|e| log::error!("cannot store the account: {e}"))
            .ok();
        self.online = Some(Online {
            session,
            events,
            account,
            account_id,
            logout: None,
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
        let id = format!("logout-{}", uuid::Uuid::new_v4());
        let server = Jid::from(BareJid::from_parts(None, online.account.domain()));
        let ping = Iq::from_get(id.clone(), Ping).with_to(server);
        if online.session.send(ping.into()).await.is_err() {
            // The session is closed already.
            self.close_session(Some(reply)).await;
            return;
        }
        online.logout = Some((id, reply));
    }

    async fn close_session(&mut self, reply: Option<oneshot::Sender<()>>) {
        if let Some(online) = self.online.take() {
            online.session.disconnect().await;
            self.emit_state(ConnectionState::Disconnected);
            if let Some((_, reply)) = online.logout {
                let _ = reply.send(());
            }
        }
        if let Some(reply) = reply {
            let _ = reply.send(());
        }
    }

    /// The session ended by itself.
    fn went_offline(&mut self) {
        if let Some(online) = self.online.take() {
            self.emit_state(ConnectionState::Disconnected);
            if let Some((_, reply)) = online.logout {
                let _ = reply.send(());
            }
        }
    }

    async fn send_chat(&mut self, to: Jid, body: String) -> Result<String, ClientError> {
        let Some(online) = &self.online else {
            return Err(ClientError::NotConnected);
        };
        let origin_id = uuid::Uuid::new_v4().to_string();
        let mut message = Message::chat(to.clone())
            .with_body("".into(), body.clone())
            .with_payload(OriginId {
                id: origin_id.clone(),
            });
        message.id = Some(Id(origin_id.clone()));
        online
            .session
            .send(message.into())
            .await
            .map_err(ClientError::Session)?;

        if let Some(account_id) = online.account_id {
            let sender = online.account.to_string();
            let peer = to.to_bare().to_string();
            let new = NewMessage {
                key_kind: KeyKind::OriginId,
                key: &origin_id,
                direction: Direction::Out,
                peer: &peer,
                sender: &sender,
                body: &body,
                timestamp: None,
            };
            if let Err(e) = queries::insert_message(self.store.conn(), account_id, &new) {
                log::error!("cannot store the sent message {origin_id}: {e}");
            }
        }
        Ok(origin_id)
    }

    async fn handle_session_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { bound_jid, resumed } => {
                self.emit_state(ConnectionState::Connected { bound_jid, resumed });
            }
            SessionEvent::Disconnected(DisconnectReason::Suspended) => {
                self.emit_state(ConnectionState::Suspended);
            }
            SessionEvent::Disconnected(DisconnectReason::AuthFailed(failure)) => {
                self.emit_state(ConnectionState::AuthFailed(failure));
            }
            SessionEvent::Disconnected(DisconnectReason::Closed) => self.went_offline(),
            SessionEvent::Stanza(stanza) => match *stanza {
                Stanza::Message(message) => self.handle_message(message),
                Stanza::Iq(iq) => self.handle_iq(iq).await,
                Stanza::Presence(_) => {}
            },
        }
    }

    async fn handle_iq(&mut self, iq: Iq) {
        let is_logout_answer = matches!(
            (&self.online, &iq),
            (Some(Online { logout: Some((id, _)), .. }), Iq::Result { .. } | Iq::Error { .. })
                if id == iq.id()
        );
        if is_logout_answer {
            self.close_session(None).await;
        }
    }

    /// Store an incoming chat message and report it, once per key.
    fn handle_message(&mut self, message: Message) {
        let Some(online) = &self.online else { return };
        let Some(account_id) = online.account_id else {
            return;
        };
        if !matches!(message.type_, MessageType::Chat | MessageType::Normal) {
            return;
        }
        let (Some(from), Some((_, body))) = (&message.from, message.get_best_body(vec![])) else {
            return;
        };
        let Some((key_kind, key)) = message_key(&message, &online.account) else {
            log::debug!("chat message from {from} has no stanza-id or origin-id. Not stored.");
            return;
        };
        let timestamp = message
            .payloads
            .iter()
            .find_map(|p| Delay::try_from(p.clone()).ok())
            .map(|delay| delay.stamp.0.timestamp_millis());
        let peer = from.to_bare().to_string();
        let sender = from.to_string();
        let new = NewMessage {
            key_kind,
            key: &key,
            direction: Direction::In,
            peer: &peer,
            sender: &sender,
            body,
            timestamp,
        };
        match queries::insert_message(self.store.conn(), account_id, &new) {
            Ok(Some(stored)) => self.emit(ClientEvent::MessageReceived(stored)),
            Ok(None) => log::debug!("message {key} is stored already"),
            Err(e) => log::error!("cannot store the message {key}: {e}"),
        }
    }
}

/// The key of a message: the stanza-id from our own server, otherwise the origin-id.
/// A stanza-id from any other entity can be forged, so it does not count (XEP-0359, 7).
fn message_key(message: &Message, account: &BareJid) -> Option<(KeyKind, String)> {
    let own = Jid::from(account.clone());
    let stanza_id = message
        .payloads
        .iter()
        .filter_map(|p| StanzaId::try_from(p.clone()).ok())
        .find(|id| id.by == own);
    if let Some(stanza_id) = stanza_id {
        return Some((KeyKind::StanzaId, stanza_id.id));
    }
    message
        .payloads
        .iter()
        .find_map(|p| OriginId::try_from(p.clone()).ok())
        .map(|origin| (KeyKind::OriginId, origin.id))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use xmpp_parsers::date::DateTime;

    use super::*;
    use crate::session::{SaslCondition, ServerAddr};
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
        let (handle, mut events, actor) = new::<FakeSession>(Store::open(&path).unwrap());

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
        let (handle, mut events, actor) = new::<FakeSession>(Store::open(&path).unwrap());

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
        let (handle, _events, actor) = new::<FakeSession>(Store::open(&path).unwrap());

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
        let Stanza::Message(message) = &sent[0] else {
            panic!("expected a message: {sent:?}")
        };
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
            matches!(&sent[1], Stanza::Iq(Iq::Get { .. })),
            "logout sends a ping"
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
    fn login_failure_and_auth_failure_after_reconnect_are_reported() {
        FakeSession::prepare_connect_error(ConnectError::AuthFailed(AuthFailure::Sasl(
            SaslCondition::NotAuthorized,
        )));
        let (handle, mut events, actor) = new::<FakeSession>(Store::open_in_memory().unwrap());
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
