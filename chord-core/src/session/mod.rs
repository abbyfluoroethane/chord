//! The `Session` trait and the `SessionEvent` type. The only boundary to the stream library.
//!
//! Feature code sees stanzas as `xmpp-parsers` types. It never sees `tokio-xmpp` types.

use core::fmt;

pub use futures_core::Stream;
use jid::{BareJid, Jid};
use xmpp_parsers::stanza::Stanza;

#[cfg(feature = "native-session")]
pub mod native;

/// Where to connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerAddr {
    /// Find the server with DNS SRV records for the JID domain. Use STARTTLS.
    Srv,
    /// Connect to this host and port. Use STARTTLS.
    StartTls { host: String, port: u16 },
    /// Connect to this host and port with no TLS. Only for the local test server.
    InsecureTcp { host: String, port: u16 },
}

/// The account and server for one session.
#[derive(Clone)]
pub struct SessionConfig {
    pub jid: BareJid,
    pub password: String,
    pub server: ServerAddr,
}

impl fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionConfig")
            .field("jid", &self.jid)
            .field("password", &"<hidden>")
            .field("server", &self.server)
            .finish()
    }
}

/// Why the session is not connected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisconnectReason {
    /// The connection is lost. The session tries to reconnect and to resume the stream.
    Suspended,
    /// The session is closed. No more events follow.
    Closed,
}

/// A change in the session, or a stanza from the server.
#[derive(Debug)]
pub enum SessionEvent {
    /// A stanza from the server. Boxed, because a stanza is large (about 400 bytes).
    Stanza(Box<Stanza>),
    /// The stream is up.
    ///
    /// If `resumed` is false, the server lost the session state.
    /// Features must then sync again (MAM catch-up, rejoin MUCs).
    Connected { bound_jid: Jid, resumed: bool },
    /// The stream is down.
    Disconnected(DisconnectReason),
}

/// An error from a session call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// The session is closed. It sends no more stanzas.
    Closed,
    /// The session could not start.
    Connect(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("session is closed"),
            Self::Connect(msg) => write!(f, "cannot start session: {msg}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A connection to one XMPP account.
///
/// Implementations reconnect by themselves. They report each change with
/// `SessionEvent::Connected` and `SessionEvent::Disconnected`.
// The futures have no `Send` bound on purpose: the phase 2 WASM session is not `Send`.
#[allow(async_fn_in_trait)]
pub trait Session: Sized {
    /// The stream of events.
    type Events: Stream<Item = SessionEvent> + Unpin;

    /// Start the session. `Connected` arrives on the event stream when the stream is up.
    async fn connect(config: SessionConfig) -> Result<Self, SessionError>;

    /// Queue a stanza. The session sends it when the stream is up.
    async fn send(&self, stanza: Stanza) -> Result<(), SessionError>;

    /// Take the event stream. The first call returns it. Each later call returns `None`.
    fn events(&mut self) -> Option<Self::Events>;

    /// Close the stream. Stop reconnect attempts.
    async fn disconnect(self);
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::message::Message;

    use super::*;
    use crate::test_support::{FakeSession, block_on, next};

    fn jid(s: &str) -> Jid {
        Jid::new(s).unwrap()
    }

    #[test]
    fn fake_session_returns_scripted_events_and_records_sends() {
        let bound = jid("alice@chord.localhost/chord");
        let incoming =
            Message::chat(jid("alice@chord.localhost")).with_body("en".into(), "hi".into());
        let mut session = FakeSession::scripted(vec![
            SessionEvent::Connected {
                bound_jid: bound.clone(),
                resumed: false,
            },
            SessionEvent::Stanza(Box::new(incoming.into())),
            SessionEvent::Disconnected(DisconnectReason::Suspended),
            SessionEvent::Connected {
                bound_jid: bound.clone(),
                resumed: true,
            },
        ]);
        let mut events = session.events().expect("first call returns the stream");
        assert!(session.events().is_none(), "second call returns None");

        block_on(async {
            assert!(matches!(
                next(&mut events).await,
                Some(SessionEvent::Connected { ref bound_jid, resumed: false }) if *bound_jid == bound
            ));
            match next(&mut events).await.map(|e| match e {
                SessionEvent::Stanza(stanza) => Ok(*stanza),
                other => Err(other),
            }) {
                Some(Ok(Stanza::Message(m))) => {
                    assert_eq!(m.bodies.get("en").map(String::as_str), Some("hi"));
                }
                other => panic!("expected a message stanza, got {other:?}"),
            }
            assert!(matches!(
                next(&mut events).await,
                Some(SessionEvent::Disconnected(DisconnectReason::Suspended))
            ));
            assert!(matches!(
                next(&mut events).await,
                Some(SessionEvent::Connected { resumed: true, .. })
            ));
            assert!(
                next(&mut events).await.is_none(),
                "stream ends after the script"
            );

            let out =
                Message::chat(jid("bob@chord.localhost")).with_body("en".into(), "hello".into());
            session.send(out.into()).await.unwrap();
            let sent = session.sent();
            assert_eq!(sent.borrow().len(), 1);

            session.close();
            let again = Message::chat(jid("bob@chord.localhost"));
            assert_eq!(session.send(again.into()).await, Err(SessionError::Closed));
            session.disconnect().await;
            assert_eq!(sent.borrow().len(), 1);
        });
    }
}
