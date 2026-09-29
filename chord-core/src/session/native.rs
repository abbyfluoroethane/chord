//! `Session` implementation on `tokio-xmpp`. The only module that imports `tokio_xmpp`.
//!
//! A background task owns the `StanzaStream`. `StanzaStream` does the reconnects and
//! XEP-0198 stream management. It reports `StreamEvent::Resumed` when stream management
//! restores the session, and `StreamEvent::Reset` when the server lost the session state.

use core::pin::Pin;
use core::task::{Context, Poll};

use futures_core::Stream;
use jid::Jid;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_xmpp::connect::{DnsConfig, StartTlsServerConnector, TcpServerConnector};
use tokio_xmpp::stanzastream::{Event, StanzaStream, StreamEvent};
use tokio_xmpp::xmlstream::Timeouts;
use xmpp_parsers::stanza::Stanza;

use super::{DisconnectReason, ServerAddr, Session, SessionConfig, SessionError, SessionEvent};

/// Size of the inbound and outbound queues.
const QUEUE_DEPTH: usize = 64;

/// Time limit for a clean close. See the comment in `run`.
const CLOSE_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(5);

enum Command {
    Send(Box<Stanza>),
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

    async fn connect(config: SessionConfig) -> Result<Self, SessionError> {
        let SessionConfig {
            jid,
            password,
            server,
        } = config;
        let domain = jid.domain().as_str().to_owned();
        let jid = Jid::from(jid);
        let timeouts = Timeouts::default();
        // `new_c2s` does not fail. It retries in the background until the login succeeds.
        let stream = match server {
            ServerAddr::Srv => StanzaStream::new_c2s(
                StartTlsServerConnector::from(DnsConfig::srv_default_client(&domain)),
                jid,
                password,
                timeouts,
                QUEUE_DEPTH,
            ),
            ServerAddr::StartTls { host, port } => StanzaStream::new_c2s(
                StartTlsServerConnector::from(DnsConfig::no_srv(&host, port)),
                jid,
                password,
                timeouts,
                QUEUE_DEPTH,
            ),
            ServerAddr::InsecureTcp { host, port } => StanzaStream::new_c2s(
                TcpServerConnector::from(DnsConfig::no_srv(&host, port)),
                jid,
                password,
                timeouts,
                QUEUE_DEPTH,
            ),
        };

        let (commands, command_rx) = mpsc::channel(QUEUE_DEPTH);
        let (event_tx, event_rx) = mpsc::channel(QUEUE_DEPTH);
        let task = tokio::spawn(run(stream, command_rx, event_tx));
        Ok(Self {
            commands,
            events: Some(NativeEvents(event_rx)),
            task,
        })
    }

    async fn send(&self, stanza: Stanza) -> Result<(), SessionError> {
        self.commands
            .send(Command::Send(Box::new(stanza)))
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

/// Own the stream. Forward commands to it, and map its events to `SessionEvent`.
async fn run(
    mut stream: StanzaStream,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<SessionEvent>,
) {
    // `StreamEvent::Resumed` does not carry the JID, so keep the one from the last `Reset`.
    let mut bound_jid: Option<Jid> = None;
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Send(stanza)) => {
                    // The token reports delivery progress. Nothing uses it yet.
                    let _token = stream.send(stanza).await;
                }
                Some(Command::Close) | None => {
                    // TODO(tokio-xmpp 6.0.0): `StanzaStream::close` never returns while the stream
                    // waits for a login (for example, a wrong password). The worker polls only the
                    // reconnect slot in that state (stanzastream/worker.rs:150-159). A time limit
                    // stops the wait. The library reconnect task keeps its retry loop until exit.
                    let _ = tokio::time::timeout(CLOSE_TIMEOUT, stream.close()).await;
                    break;
                }
            },
            event = next_event(&mut stream) => {
                let mapped = match event {
                    Some(Event::Stanza(stanza)) => SessionEvent::Stanza(Box::new(stanza)),
                    Some(Event::Stream(StreamEvent::Reset { bound_jid: jid, .. })) => {
                        bound_jid = Some(jid.clone());
                        SessionEvent::Connected { bound_jid: jid, resumed: false }
                    }
                    Some(Event::Stream(StreamEvent::Resumed)) => match &bound_jid {
                        Some(jid) => SessionEvent::Connected { bound_jid: jid.clone(), resumed: true },
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
    let _ = events
        .send(SessionEvent::Disconnected(DisconnectReason::Closed))
        .await;
}

async fn next_event(stream: &mut StanzaStream) -> Option<Event> {
    core::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}
