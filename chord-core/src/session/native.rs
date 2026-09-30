//! `Session` implementation on `tokio-xmpp`. The only module that imports `tokio_xmpp`.
//!
//! Ownership (spec rev 5): `StanzaStream` owns stream management and resumption.
//! Chord owns the login and its retry loop. `connect` does the first login itself, so it
//! can return a typed `ConnectError`. It then gives the logged-in stream to
//! `StanzaStream::new` through a connector closure. On each reconnect, the connector runs
//! the same login in `retry_login`, which stops on a fatal SASL failure.
//!
//! Search for `TOKIO-XMPP-COPY` to find the code that copies `tokio-xmpp` internals.

use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll};
use core::time::Duration;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io;

use futures_core::Stream;
use jid::Jid;
use sasl::client::Mechanism;
use sasl::client::mechanisms::Scram;
use sasl::common::Credentials;
use sasl::common::scram::{Sha1, Sha256};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio_xmpp::connect::ServerConnector;
use tokio_xmpp::error::AuthError;
use tokio_xmpp::rustls;
use tokio_xmpp::stanzastream::{Connection, Event, StanzaStream, StreamEvent};
use tokio_xmpp::xmlstream::{
    FallibleStreamElement, RecvFeaturesError, StreamHeader, Timeouts, accept_stream,
    initiate_stream,
};
use xmpp_parsers::bind::BindFeature;
use xmpp_parsers::ns;
use xmpp_parsers::stanza::Stanza;
use xmpp_parsers::stream_features::StreamFeatures;

use super::binding::choose_binding;
use super::connector::{Connector, Mode};
use super::{
    AuthFailure, ConnectError, DisconnectReason, SaslRetry, ServerAddr, Session, SessionConfig,
    SessionError, SessionEvent, TICK, sasl_retry,
};

/// Size of the inbound and outbound queues.
const QUEUE_DEPTH: usize = 64;

/// Liveness limits of the stream (CORESESSION-07). After 60 s with no data from the server,
/// `StanzaStream` sends a XEP-0198 `<r/>`, or a XEP-0199 ping when the server has no stream
/// management. After 30 s more with no data, the stream is dead and `StanzaStream` reconnects
/// and resumes. The default values are 300 s and 300 s, so a dead link can stay up to 10
/// minutes.
const STREAM_TIMEOUTS: Timeouts = Timeouts {
    read_timeout: Duration::from_secs(60),
    response_timeout: Duration::from_secs(30),
};

/// Time limit for a clean close of the `StanzaStream`.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// First and largest delay between two login attempts. Same values as `new_c2s`.
const FIRST_RETRY_DELAY: Duration = Duration::from_secs(1);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

/// After a fatal auth failure on a reconnect, the connector waits this long for Chord to
/// close the session. Then it drops the slot anyway, so it never holds it forever.
const FATAL_SLOT_HOLD: Duration = Duration::from_secs(10);

enum Command {
    Send(Box<Stanza>),
    ClientState(bool),
    Close,
}

/// A session on `tokio-xmpp`. Call `connect` inside a tokio runtime.
pub struct NativeSession {
    commands: mpsc::Sender<Command>,
    events: Option<NativeEvents>,
    task: JoinHandle<()>,
}

/// The event stream of a `NativeSession`.
pub struct NativeEvents(mpsc::Receiver<SessionEvent>);

impl Stream for NativeEvents {
    type Item = SessionEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<SessionEvent>> {
        self.0.poll_recv(cx)
    }
}

impl Session for NativeSession {
    type Events = NativeEvents;

    async fn connect(config: SessionConfig) -> Result<Self, ConnectError> {
        let SessionConfig {
            jid,
            password,
            server,
            login_timeout,
        } = config;
        let jid = Jid::from(jid);
        match server {
            ServerAddr::Srv => start(Connector(Mode::Srv), jid, password, login_timeout).await,
            ServerAddr::StartTls { host, port } => {
                let server = Connector(Mode::StartTls { host, port });
                start(server, jid, password, login_timeout).await
            }
            ServerAddr::DirectTls { host, port } => {
                let server = Connector(Mode::DirectTls { host, port });
                start(server, jid, password, login_timeout).await
            }
            #[cfg(feature = "dev-insecure")]
            ServerAddr::InsecureTcp { host, port } => {
                let server = tokio_xmpp::connect::TcpServerConnector::from(
                    tokio_xmpp::connect::DnsConfig::no_srv(&host, port),
                );
                start(server, jid, password, login_timeout).await
            }
        }
    }

    async fn send(&self, stanza: Stanza) -> Result<(), SessionError> {
        self.commands
            .send(Command::Send(Box::new(stanza)))
            .await
            .map_err(|_| SessionError::Closed)
    }

    async fn send_client_state(&self, active: bool) -> Result<(), SessionError> {
        self.commands
            .send(Command::ClientState(active))
            .await
            .map_err(|_| SessionError::Closed)
    }

    fn events(&mut self) -> Option<NativeEvents> {
        self.events.take()
    }

    async fn disconnect(self) {
        // An error means that the task already stopped.
        let _ = self.commands.send(Command::Close).await;
        let _ = self.task.await;
    }
}

/// Do the first login, then hand the stream to a new `StanzaStream`.
async fn start<C: ServerConnector>(
    server: C,
    jid: Jid,
    password: String,
    login_timeout: Duration,
) -> Result<NativeSession, ConnectError> {
    let first = tokio::time::timeout(login_timeout, first_login(&server, &jid, &password))
        .await
        .map_err(|_| ConnectError::Timeout)??;

    let attempt = move || {
        let (server, jid, password) = (server.clone(), jid.clone(), password.clone());
        async move { login(server, &jid, &password).await }
    };
    Ok(NativeSession::spawn(Some(first), attempt, login_timeout))
}

/// The first login. Retries only a temporary SASL failure, until the caller's timeout.
async fn first_login<C: ServerConnector>(
    server: &C,
    jid: &Jid,
    password: &str,
) -> Result<Connection, ConnectError> {
    let mut delay = FIRST_RETRY_DELAY;
    loop {
        match login(server.clone(), jid, password).await {
            Ok(connection) => return Ok(connection),
            Err(LoginError::Fatal(failure)) => return Err(ConnectError::AuthFailed(failure)),
            Err(LoginError::Unreachable(msg)) => return Err(ConnectError::Unreachable(msg)),
            Err(LoginError::TlsInvalid(msg)) => return Err(ConnectError::TlsInvalid(msg)),
            Err(LoginError::Temporary(msg)) => {
                log::info!("temporary login failure: {msg}. Retrying in {delay:?}.");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(MAX_RETRY_DELAY);
            }
        }
    }
}

impl NativeSession {
    /// Start the `StanzaStream` and the task that maps its events.
    ///
    /// `first` goes to the first connector call. Each later call runs `retry_login`
    /// with `attempt`. With `first` set to `None`, the first call also runs `retry_login`.
    fn spawn<F, Fut>(first: Option<Connection>, attempt: F, login_timeout: Duration) -> Self
    where
        F: FnMut() -> Fut + Clone + Send + 'static,
        Fut: Future<Output = Result<Connection, LoginError>> + Send + 'static,
    {
        let (auth_tx, auth_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut first = first;
        let connector = Box::new(
            move |_: Option<String>, slot: oneshot::Sender<Connection>| {
                if let Some(connection) = first.take() {
                    // The receiver lives in the worker, which called us. It cannot be gone yet.
                    let _ = slot.send(connection);
                    return;
                }
                tokio::spawn(retry_login(
                    attempt.clone(),
                    slot,
                    release_slot,
                    auth_tx.clone(),
                    shutdown_rx.clone(),
                    login_timeout,
                ));
            },
        );
        let stream = StanzaStream::new(connector, QUEUE_DEPTH);

        let (commands, command_rx) = mpsc::channel(QUEUE_DEPTH);
        let (event_tx, event_rx) = mpsc::channel(QUEUE_DEPTH);
        let task = tokio::spawn(run(stream, command_rx, event_tx, auth_rx, shutdown_tx));
        Self {
            commands,
            events: Some(NativeEvents(event_rx)),
            task,
        }
    }
}

/// The reconnect login loop.
///
/// TOKIO-XMPP-COPY: this is the retry loop of `StanzaStream::new_c2s` in tokio-xmpp 6.0.0
/// (src/stanzastream/mod.rs:128-181), with these changes:
/// - A fatal SASL failure stops the loop and goes to `auth_failed`. `new_c2s` retries it
///   forever (mod.rs:170-179).
/// - The loop stops when `shutdown` turns true. `new_c2s` has no stop.
/// - Each attempt has a time limit.
///
/// When the loop stops without a connection, `release` gets the slot. It must fill the
/// slot: if the slot drops, tokio-xmpp 6.0.0 panics in the worker
/// (stanzastream/worker.rs:180-183 and 548-549). The loop never holds the slot forever.
/// It releases the slot on shutdown, or `FATAL_SLOT_HOLD` after a fatal failure.
async fn retry_login<T, F, Fut, R>(
    attempt: F,
    slot: oneshot::Sender<T>,
    release: R,
    auth_failed: mpsc::UnboundedSender<AuthFailure>,
    shutdown: watch::Receiver<bool>,
    login_timeout: Duration,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LoginError>>,
    R: FnOnce(oneshot::Sender<T>),
{
    match login_until_stop(attempt, auth_failed, shutdown, login_timeout).await {
        Some(connection) => {
            // An error means that the worker is gone. Nothing waits for the connection.
            let _ = slot.send(connection);
        }
        None => release(slot),
    }
}

/// Try to log in until an attempt succeeds, a fatal failure occurs, or `shutdown` turns
/// true. Returns `None` in the last two cases.
async fn login_until_stop<T, F, Fut>(
    mut attempt: F,
    auth_failed: mpsc::UnboundedSender<AuthFailure>,
    mut shutdown: watch::Receiver<bool>,
    login_timeout: Duration,
) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LoginError>>,
{
    let mut delay = FIRST_RETRY_DELAY;
    loop {
        let result = tokio::select! {
            result = tokio::time::timeout(login_timeout, attempt()) => result,
            _ = shutdown.wait_for(|stop| *stop) => return None,
        };
        match result {
            Ok(Ok(connection)) => return Some(connection),
            Ok(Err(LoginError::Fatal(failure))) => {
                log::warn!("login failed on reconnect: {failure}. Stopping.");
                let _ = auth_failed.send(failure);
                let _ =
                    tokio::time::timeout(FATAL_SLOT_HOLD, shutdown.wait_for(|stop| *stop)).await;
                return None;
            }
            Ok(Err(error)) => log::info!("reconnect failed: {error}. Retrying in {delay:?}."),
            Err(_) => log::info!("reconnect timed out. Retrying in {delay:?}."),
        }
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = shutdown.wait_for(|stop| *stop) => return None,
        }
        delay = (delay * 2).min(MAX_RETRY_DELAY);
    }
}

/// Fill the slot with a connection whose server end is closed already.
///
/// A worker that waits for a connection can end in two ways only. If the slot drops, it
/// panics (stanzastream/worker.rs:548-549). If it gets a connection, it negotiates. With
/// this dead connection, the negotiation fails at once, and the worker takes its normal
/// exit because the frontend is gone (worker.rs:501-514). `run` makes sure that the
/// frontend is gone first: it starts `StanzaStream::close` before it sets `shutdown`.
///
/// TOKIO-XMPP-COPY: the in-memory stream pair is `custom_stream_pair` from the tokio-xmpp
/// 6.0.0 tests (src/stanzastream/tests.rs:60-110), without the bind exchange.
fn release_slot(slot: oneshot::Sender<Connection>) {
    tokio::spawn(async move {
        match dead_connection().await {
            // An error means that the worker ended already.
            Ok(connection) => drop(slot.send(connection)),
            Err(e) => log::error!("cannot build the connection that ends the worker: {e}"),
        }
    });
}

async fn dead_connection() -> io::Result<Connection> {
    const JID: &str = "closed@closed.invalid";
    const DOMAIN: &str = "closed.invalid";
    let header = |from: &'static str, to: &'static str| StreamHeader {
        from: Some(Cow::Borrowed(from)),
        to: Some(Cow::Borrowed(to)),
        id: Some(Cow::Borrowed("closed")),
    };
    let (client, server) = tokio::io::duplex(1024);
    let client = async move {
        let io = tokio::io::BufReader::new(client);
        let pending = initiate_stream(
            io,
            ns::JABBER_CLIENT,
            header(JID, DOMAIN),
            Timeouts::default(),
        )
        .await?;
        pending
            .recv_features::<FallibleStreamElement>()
            .await
            .map_err(|e| match e {
                RecvFeaturesError::Io(e) => e,
                RecvFeaturesError::StreamError(e) => io::Error::other(e),
            })
    };
    let server = async move {
        let io = tokio::io::BufReader::new(server);
        let accepted = accept_stream(io, ns::JABBER_CLIENT, Timeouts::default()).await?;
        let pending = accepted.send_header(header(DOMAIN, JID)).await?;
        // The worker requires a bind feature to start negotiation (worker.rs:165-171).
        let features = StreamFeatures {
            bind: Some(BindFeature { required: false }),
            ..Default::default()
        };
        // The server end drops after the features, so the client end reads EOF next.
        pending
            .send_features::<FallibleStreamElement>(&features)
            .await
            .map(drop)
    };
    let ((features, stream), ()) = tokio::try_join!(client, server)?;
    let identity = Jid::new(JID).map_err(io::Error::other)?;
    Ok(Connection {
        stream: stream.box_stream(),
        features,
        identity,
    })
}

/// Own the stream. Forward commands to it, and map its events to `SessionEvent`.
async fn run(
    mut stream: StanzaStream,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<SessionEvent>,
    mut auth_failed: mpsc::UnboundedReceiver<AuthFailure>,
    shutdown: watch::Sender<bool>,
) {
    // `StreamEvent::Resumed` does not carry the JID, so keep the one from the last `Reset`.
    let mut bound_jid: Option<Jid> = None;
    let mut features: Vec<String> = Vec::new();
    let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + TICK, TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                // A full queue means that the actor is busy. It gets the next tick.
                let _ = events.try_send(SessionEvent::Tick);
            }
            command = commands.recv() => match command {
                Some(Command::Send(stanza)) => {
                    // The token reports delivery progress. Nothing uses it yet.
                    let _token = stream.send(stanza).await;
                }
                Some(Command::ClientState(active)) => {
                    let _token = stream.send_client_state(active).await;
                }
                Some(Command::Close) | None => break,
            },
            Some(failure) = auth_failed.recv() => {
                let reason = DisconnectReason::AuthFailed(failure);
                let _ = events.send(SessionEvent::Disconnected(reason)).await;
                break;
            }
            event = next_event(&mut stream) => {
                let mapped = match event {
                    Some(Event::Stanza(stanza)) => SessionEvent::Stanza(Box::new(stanza)),
                    Some(Event::Stream(StreamEvent::Reset { bound_jid: jid, features: stream_features })) => {
                        bound_jid = Some(jid.clone());
                        // xmpp-parsers stream_features.rs:62: the elements that it does not parse.
                        features = stream_features.others.iter().map(|e| e.ns()).collect();
                        SessionEvent::Connected { bound_jid: jid, resumed: false, features: features.clone() }
                    }
                    Some(Event::Stream(StreamEvent::Resumed)) => match &bound_jid {
                        Some(jid) => SessionEvent::Connected {
                            bound_jid: jid.clone(),
                            resumed: true,
                            features: features.clone(),
                        },
                        // tokio-xmpp only resumes a stream that it bound before.
                        None => continue,
                    },
                    Some(Event::Stream(StreamEvent::Suspended)) => {
                        SessionEvent::Disconnected(DisconnectReason::Suspended)
                    }
                    None => break,
                };
                // An error means that nobody reads the events. Keep the session up for sends.
                let _ = events.send(mapped).await;
            }
        }
    }
    // `StanzaStream::close` does not return while the worker waits for a connection
    // (stanzastream/worker.rs:150-159), so it gets a time limit.
    let mut close = pin!(tokio::time::timeout(CLOSE_TIMEOUT, stream.close()));
    // The first poll closes the transmit queue. Only then may a pending reconnect
    // release its slot, so the worker sees that the frontend is gone (see `release_slot`).
    let first = core::future::poll_fn(|cx| Poll::Ready(close.as_mut().poll(cx))).await;
    let _ = shutdown.send(true);
    if first.is_pending() {
        let _ = close.await;
    }
    let _ = events
        .send(SessionEvent::Disconnected(DisconnectReason::Closed))
        .await;
}

async fn next_event(stream: &mut StanzaStream) -> Option<Event> {
    core::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

/// Why one login attempt failed.
#[derive(Debug)]
enum LoginError {
    /// Stop. New credentials or a server change are necessary.
    Fatal(AuthFailure),
    /// A temporary SASL failure. Retry with backoff.
    Temporary(String),
    Unreachable(String),
    TlsInvalid(String),
}

impl core::fmt::Display for LoginError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Fatal(failure) => write!(f, "{failure}"),
            Self::Temporary(msg) | Self::Unreachable(msg) | Self::TlsInvalid(msg) => {
                f.write_str(msg)
            }
        }
    }
}

/// Connect, do SASL, and restart the stream. Returns a stream that is ready to bind.
///
/// TOKIO-XMPP-COPY: this is `client_auth` from tokio-xmpp 6.0.0
/// (src/client/login.rs:113-138). That function is not public (client/mod.rs:33).
/// Changes:
/// - The errors map to `LoginError`, so the callers can stop on a fatal SASL failure.
/// - The channel binding from the connector goes to SASL, as in the original, except when
///   `choose_binding` says so (session/binding.rs). With binding data, the sasl crate names
///   SCRAM only as `-PLUS` (sasl-0.5.2 src/client/mechanisms/scram.rs:103-108), so
///   `client_login` skips SCRAM and falls back to PLAIN (client/login.rs:36-45).
/// - It logs the SASL mechanism.
async fn login<C: ServerConnector>(
    server: C,
    jid: &Jid,
    password: &str,
) -> Result<Connection, LoginError> {
    let username = jid
        .node()
        .ok_or_else(|| LoginError::Fatal(AuthFailure::Local("the JID has no local part".into())))?
        .as_str();

    let (stream, channel_binding) = server
        .connect(jid, ns::JABBER_CLIENT, STREAM_TIMEOUTS)
        .await
        .map_err(map_error)?;
    let (features, stream) = stream
        .recv_features()
        .await
        .map_err(|e| map_error(e.into()))?;

    let channel_binding = choose_binding(
        channel_binding,
        &features.sasl_mechanisms,
        features.sasl_cb.as_ref(),
    );
    let creds = Credentials::default()
        .with_username(username)
        .with_password(password)
        .with_channel_binding(channel_binding);
    let mechanism = chosen_mechanism(&creds, &features.sasl_mechanisms);

    let stream = tokio_xmpp::client_login(stream, features.sasl_mechanisms, creds)
        .await
        .map_err(map_error)?;
    log::info!(
        "SASL mechanism accepted: {}",
        mechanism.as_deref().unwrap_or("unknown")
    );

    let stream = stream
        .send_header(StreamHeader {
            to: Some(Cow::Borrowed(jid.domain().as_str())),
            from: None,
            id: None,
        })
        .await
        .map_err(|e| map_error(e.into()))?;
    let (features, stream) = stream
        .recv_features()
        .await
        .map_err(|e| map_error(e.into()))?;
    Ok(Connection {
        stream: stream.box_stream(),
        features,
        identity: jid.clone(),
    })
}

/// The mechanism that `client_login` picks: the first local one that the server offers.
/// TOKIO-XMPP-COPY: same order as tokio-xmpp 6.0.0 src/client/login.rs:36-41.
fn chosen_mechanism(creds: &Credentials, offered: &BTreeSet<String>) -> Option<String> {
    let scram256 = Scram::<Sha256>::from_credentials(creds.clone()).ok()?;
    let scram1 = Scram::<Sha1>::from_credentials(creds.clone()).ok()?;
    [scram256.name(), scram1.name(), "PLAIN", "ANONYMOUS"]
        .into_iter()
        .find(|name| offered.contains(*name))
        .map(str::to_owned)
}

fn map_error(error: tokio_xmpp::Error) -> LoginError {
    match error {
        tokio_xmpp::Error::Auth(AuthError::Fail(condition)) => match sasl_retry(&condition) {
            SaslRetry::Retry => LoginError::Temporary(format!("SASL {condition:?}")),
            SaslRetry::Stop => LoginError::Fatal(AuthFailure::Sasl(condition)),
        },
        tokio_xmpp::Error::Auth(AuthError::NoMechanism) => {
            LoginError::Fatal(AuthFailure::NoMechanism)
        }
        tokio_xmpp::Error::Auth(other) => LoginError::Fatal(AuthFailure::Local(other.to_string())),
        // rustls handshake errors arrive as `Error::Io` (connect/tls_common.rs:154-157).
        tokio_xmpp::Error::Io(ref e) if is_certificate_error(e) => {
            LoginError::TlsInvalid(error.to_string())
        }
        other => LoginError::Unreachable(other.to_string()),
    }
}

fn is_certificate_error(error: &io::Error) -> bool {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|e| {
            matches!(
                e,
                rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SaslCondition;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    type Scripted<T> = std::vec::IntoIter<Result<T, LoginError>>;

    /// A fake login that returns scripted results and counts its calls.
    fn scripted<T>(
        results: Vec<Result<T, LoginError>>,
    ) -> (
        Arc<AtomicUsize>,
        impl FnMut() -> core::future::Ready<Result<T, LoginError>>,
    ) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let mut results: Scripted<T> = results.into_iter();
        let attempt = move || {
            counter.fetch_add(1, Ordering::SeqCst);
            core::future::ready(results.next().expect("no more scripted results"))
        };
        (calls, attempt)
    }

    fn not_authorized() -> LoginError {
        LoginError::Fatal(AuthFailure::Sasl(SaslCondition::NotAuthorized))
    }

    const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);

    #[tokio::test(start_paused = true)]
    async fn fatal_failure_reports_once_and_drops_slot_on_shutdown() {
        let (calls, attempt) = scripted::<u32>(vec![Err(not_authorized())]);
        let (slot, slot_rx) = oneshot::channel();
        let (auth_tx, mut auth_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(retry_login(
            attempt,
            slot,
            drop,
            auth_tx,
            shutdown_rx,
            LOGIN_TIMEOUT,
        ));

        let failure = auth_rx.recv().await.unwrap();
        assert_eq!(failure, AuthFailure::Sasl(SaslCondition::NotAuthorized));
        // The loop waits for the shutdown and does not try again.
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!task.is_finished());

        shutdown_tx.send(true).unwrap();
        task.await.unwrap();
        assert!(slot_rx.await.is_err(), "the slot is dropped, not filled");
    }

    #[tokio::test(start_paused = true)]
    async fn fatal_failure_without_shutdown_releases_slot_after_hold() {
        let (_, attempt) = scripted::<u32>(vec![Err(not_authorized())]);
        let (slot, slot_rx) = oneshot::channel();
        let (auth_tx, _auth_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let start = tokio::time::Instant::now();
        tokio::spawn(retry_login(
            attempt,
            slot,
            drop,
            auth_tx,
            shutdown_rx,
            LOGIN_TIMEOUT,
        ));
        assert!(slot_rx.await.is_err());
        assert_eq!(start.elapsed(), FATAL_SLOT_HOLD);
    }

    #[tokio::test(start_paused = true)]
    async fn temporary_failures_retry_with_backoff_then_fill_slot() {
        let temporary = || LoginError::Temporary("SASL TemporaryAuthFailure".into());
        let unreachable = || LoginError::Unreachable("connection refused".into());
        let (calls, attempt) = scripted(vec![Err(temporary()), Err(unreachable()), Ok(7_u32)]);
        let (slot, slot_rx) = oneshot::channel();
        let (auth_tx, mut auth_rx) = mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let start = tokio::time::Instant::now();
        tokio::spawn(retry_login(
            attempt,
            slot,
            drop,
            auth_tx,
            shutdown_rx,
            LOGIN_TIMEOUT,
        ));

        assert_eq!(slot_rx.await.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        // Delays of 1 s and 2 s.
        assert_eq!(start.elapsed(), Duration::from_secs(3));
        assert!(
            auth_rx.try_recv().is_err(),
            "no auth failure for a temporary error"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_during_backoff_stops_the_loop() {
        let (calls, attempt) =
            scripted::<u32>(vec![Err(LoginError::Unreachable("refused".into()))]);
        let (slot, slot_rx) = oneshot::channel();
        let (auth_tx, _auth_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(retry_login(
            attempt,
            slot,
            drop,
            auth_tx,
            shutdown_rx,
            LOGIN_TIMEOUT,
        ));
        tokio::task::yield_now().await;
        shutdown_tx.send(true).unwrap();
        task.await.unwrap();
        assert!(slot_rx.await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// Count the panics from the tokio-xmpp worker when a connector drops its slot
    /// (tokio-xmpp 6.0.0 src/stanzastream/worker.rs:549). The hook is global, so it keeps
    /// the previous hook and only counts this one message.
    fn worker_panics() -> &'static AtomicUsize {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        static INSTALL: std::sync::Once = std::sync::Once::new();
        INSTALL.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let payload = info.payload();
                let message = payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
                if message == Some("Backend was unable to handle reconnect request.") {
                    COUNT.fetch_add(1, Ordering::SeqCst);
                }
                previous(info);
            }));
        });
        &COUNT
    }

    /// The whole path, with the real `StanzaStream` worker and a fake login: a reconnect
    /// fails with `not-authorized`. The session reports `AuthFailed`, then `Closed`, and
    /// the session task ends. The worker gets a dead connection instead of a dropped slot,
    /// so it ends without its panic, and the process keeps running.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn auth_failure_on_reconnect_closes_session_without_zombie() {
        let panics_before = worker_panics().load(Ordering::SeqCst);
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let attempt = move || {
            counter.fetch_add(1, Ordering::SeqCst);
            async { Err::<Connection, _>(not_authorized()) }
        };
        let start = Instant::now();
        // With no first connection, the first connector call already runs `retry_login`,
        // the same as a reconnect.
        let mut session = NativeSession::spawn(None, attempt, LOGIN_TIMEOUT);
        let mut events = session.events().unwrap();

        async fn next(events: &mut NativeEvents) -> Option<SessionEvent> {
            let fut = core::future::poll_fn(|cx| Pin::new(&mut *events).poll_next(cx));
            tokio::time::timeout(Duration::from_secs(6), fut)
                .await
                .expect("event within 6 s")
        }
        match next(&mut events).await {
            Some(SessionEvent::Disconnected(DisconnectReason::AuthFailed(failure))) => {
                assert_eq!(failure, AuthFailure::Sasl(SaslCondition::NotAuthorized));
            }
            other => panic!("expected AuthFailed, got {other:?}"),
        }
        assert!(matches!(
            next(&mut events).await,
            Some(SessionEvent::Disconnected(DisconnectReason::Closed))
        ));
        assert!(next(&mut events).await.is_none(), "no event after Closed");
        tokio::time::timeout(Duration::from_secs(1), session.task)
            .await
            .unwrap()
            .unwrap();
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "took {:?}",
            start.elapsed()
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "no retry after a fatal failure"
        );
        // No worker panic. `Closed` comes only after `close` returns, and `close` returns
        // early only after the worker ends, so the fast close shows a clean worker exit.
        assert_eq!(worker_panics().load(Ordering::SeqCst), panics_before);
        // The runtime and the process keep running after the panic.
        assert_eq!(tokio::spawn(async { 40 + 2 }).await.unwrap(), 42);
    }

    /// A silent server gives a soft timeout after 60 s (where `StanzaStream` sends its
    /// `<r/>` or ping) and a dead stream 30 s later (where it reconnects).
    #[tokio::test(start_paused = true)]
    async fn a_silent_stream_times_out_soft_then_hard() {
        use tokio_xmpp::xmlstream::ReadError;
        let (lhs, rhs) = tokio::io::duplex(65536);
        let server = tokio::spawn(async move {
            let stream = accept_stream(
                tokio::io::BufReader::new(rhs),
                ns::JABBER_CLIENT,
                STREAM_TIMEOUTS,
            )
            .await?;
            let header = StreamHeader::default();
            let stream = stream.send_header(header).await?;
            let stream = stream
                .send_features::<FallibleStreamElement>(&StreamFeatures::default())
                .await?;
            // Stay silent, and keep the stream open.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            drop(stream);
            Ok::<_, io::Error>(())
        });
        let pending = initiate_stream(
            tokio::io::BufReader::new(lhs),
            ns::JABBER_CLIENT,
            StreamHeader::default(),
            STREAM_TIMEOUTS,
        )
        .await
        .unwrap();
        let (_, mut stream) = pending
            .recv_features::<FallibleStreamElement>()
            .await
            .unwrap();
        let start = tokio::time::Instant::now();
        match core::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            Some(Err(ReadError::SoftTimeout)) => {}
            other => panic!("expected a soft timeout, got {other:?}"),
        }
        assert_eq!(start.elapsed(), Duration::from_secs(60));
        match core::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            Some(Err(ReadError::HardError(e))) if e.kind() == io::ErrorKind::TimedOut => {}
            other => panic!("expected a hard timeout, got {other:?}"),
        }
        assert_eq!(start.elapsed(), Duration::from_secs(90));
        server.abort();
    }
}
