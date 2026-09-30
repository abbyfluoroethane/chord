//! The `Session` trait and the `SessionEvent` type. The only boundary to the stream library.
//!
//! Feature code sees stanzas as `xmpp-parsers` types. It never sees `tokio-xmpp` types.

use core::fmt;
use core::time::Duration;

pub use futures_core::Stream;
use jid::{BareJid, Jid};
pub use xmpp_parsers::sasl::DefinedCondition as SaslCondition;
use xmpp_parsers::stanza::Stanza;

#[cfg(feature = "native-session")]
mod binding;
#[cfg(feature = "native-session")]
mod connector;
#[cfg(feature = "native-session")]
pub mod native;

pub mod cert_pin;
pub use cert_pin::CertPin;

/// Default time limit for the first login.
pub const DEFAULT_LOGIN_TIMEOUT: Duration = Duration::from_secs(15);

/// Where to connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerAddr {
    /// Find the server with DNS SRV records for the JID domain. Look up `_xmpps-client._tcp`
    /// (direct TLS, XEP-0368) and `_xmpp-client._tcp` (STARTTLS), and merge them by
    /// priority. Direct TLS wins on the same priority. A failed target falls back to the
    /// next one.
    Srv,
    /// Connect to this host and port. Use STARTTLS.
    StartTls { host: String, port: u16 },
    /// Connect to this host and port. Use TLS from the first byte (XEP-0368).
    DirectTls { host: String, port: u16 },
    /// Connect to this host and port with no TLS. Only for a dev server.
    #[cfg(feature = "dev-insecure")]
    InsecureTcp { host: String, port: u16 },
}

/// The account and server for one session.
#[derive(Clone)]
pub struct SessionConfig {
    pub jid: BareJid,
    pub password: String,
    pub server: ServerAddr,
    /// Time limit for each login attempt. `connect` returns `ConnectError::Timeout` after it.
    pub login_timeout: Duration,
    /// An opt-in pin of the server certificate. See `cert_pin`.
    pub pin: Option<CertPin>,
}

impl SessionConfig {
    /// A config with the default login timeout.
    pub fn new(jid: BareJid, password: String, server: ServerAddr) -> Self {
        Self {
            jid,
            password,
            server,
            login_timeout: DEFAULT_LOGIN_TIMEOUT,
            pin: None,
        }
    }

    /// The same config with a certificate pin.
    pub fn with_pin(mut self, pin: CertPin) -> Self {
        self.pin = Some(pin);
        self
    }
}

impl fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionConfig")
            .field("jid", &self.jid)
            .field("password", &"<hidden>")
            .field("server", &self.server)
            .field("login_timeout", &self.login_timeout)
            .field("pin", &self.pin)
            .finish()
    }
}

/// Why the server or the client rejected the login.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(
        tag = "type",
        content = "data",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum AuthFailure {
    /// The server sent a SASL `<failure/>` with this condition (RFC 6120, 6.5).
    Sasl(
        #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_condition"))] SaslCondition,
    ),
    /// The server offers no SASL mechanism that the client supports.
    NoMechanism,
    /// The client stopped the login, for example because the server signature was wrong.
    Local(String),
}

/// Serialize a SASL condition as its name, for example `NotAuthorized`. The xmpp-parsers
/// type has no serde support.
#[cfg(feature = "serde")]
fn serialize_condition<S: serde::Serializer>(
    condition: &SaslCondition,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&format_args!("{condition:?}"))
}

impl fmt::Display for AuthFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sasl(SaslCondition::NotAuthorized) => f.write_str("wrong username or password"),
            Self::Sasl(SaslCondition::AccountDisabled) => f.write_str("account disabled"),
            Self::Sasl(SaslCondition::CredentialsExpired) => f.write_str("password expired"),
            Self::Sasl(other) => write!(f, "server rejected the login: {other:?}"),
            Self::NoMechanism => f.write_str("no common SASL mechanism"),
            Self::Local(msg) => write!(f, "login stopped: {msg}"),
        }
    }
}

/// What to do after a SASL failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaslRetry {
    /// Try again with the normal backoff.
    Retry,
    /// Stop. New credentials or a server change are necessary.
    Stop,
}

/// Map a SASL failure condition (RFC 6120, 6.5) to a retry decision.
pub fn sasl_retry(condition: &SaslCondition) -> SaslRetry {
    match condition {
        SaslCondition::TemporaryAuthFailure | SaslCondition::Aborted => SaslRetry::Retry,
        SaslCondition::NotAuthorized
        | SaslCondition::CredentialsExpired
        | SaslCondition::AccountDisabled
        | SaslCondition::EncryptionRequired
        | SaslCondition::MechanismTooWeak
        | SaslCondition::InvalidMechanism
        | SaslCondition::MalformedRequest
        // Not in the Chord spec list. A retry cannot fix them, so stop.
        | SaslCondition::IncorrectEncoding
        | SaslCondition::InvalidAuthzid => SaslRetry::Stop,
    }
}

/// Why `connect` failed.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(
        tag = "type",
        content = "data",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum ConnectError {
    /// The server rejected the credentials. Never retry. Ask for new credentials.
    AuthFailed(AuthFailure),
    /// DNS, TCP, or TLS failed. The caller decides about a retry.
    Unreachable(String),
    /// The server certificate did not pass verification. Never retry automatically.
    TlsInvalid(String),
    /// No result within the login timeout. The caller decides about a retry.
    Timeout,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthFailed(failure) => write!(f, "login failed: {failure}"),
            Self::Unreachable(msg) => write!(f, "server unreachable: {msg}"),
            Self::TlsInvalid(msg) => write!(f, "server certificate invalid: {msg}"),
            Self::Timeout => f.write_str("login timed out"),
        }
    }
}

impl std::error::Error for ConnectError {}

/// Why the session is not connected.
#[derive(Clone, Debug, PartialEq)]
pub enum DisconnectReason {
    /// The connection is lost. The session tries to reconnect and to resume the stream.
    Suspended,
    /// The server rejected the credentials on a reconnect. The session stops.
    /// `Closed` follows.
    AuthFailed(AuthFailure),
    /// The session is closed. This is always the last event.
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
    ///
    /// `features` lists the namespaces of the stream features that tokio-xmpp does not
    /// parse itself, for example `urn:xmpp:features:pre-approval`. A resumed stream keeps
    /// the features of its first connection.
    Connected {
        bound_jid: Jid,
        resumed: bool,
        features: Vec<String>,
    },
    /// The stream is down.
    Disconnected(DisconnectReason),
    /// A clock tick, about every `TICK`, while the session runs. The actor uses it for
    /// time limits, so that the features need no timers.
    Tick,
}

/// The time between two `SessionEvent::Tick` events.
pub const TICK: core::time::Duration = core::time::Duration::from_secs(15);

/// An error from `send`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(
        tag = "type",
        content = "data",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum SessionError {
    /// The session is closed. It sends no more stanzas.
    Closed,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("session is closed"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A connection to one XMPP account.
///
/// After the first login, implementations reconnect by themselves. They report each
/// change with `SessionEvent::Connected` and `SessionEvent::Disconnected`.
// The futures have no `Send` bound on purpose: the phase 2 WASM session is not `Send`.
#[allow(async_fn_in_trait)]
pub trait Session: Sized {
    /// The stream of events.
    type Events: Stream<Item = SessionEvent> + Unpin;

    /// Log in. Returns after the first login succeeds or fails. Never retries on
    /// `AuthFailed`. `Connected` is the first event on the event stream.
    async fn connect(config: SessionConfig) -> Result<Self, ConnectError>;

    /// Queue a stanza. The session sends it when the stream is up.
    async fn send(&self, stanza: Stanza) -> Result<(), SessionError>;

    /// Send a XEP-0352 client state as a stream element: `<active/>` for `true`,
    /// `<inactive/>` for `false`. Queued like a stanza, and in order with the stanzas.
    async fn send_client_state(&self, active: bool) -> Result<(), SessionError>;

    /// Use a new password for the next login. The account changed its password on the
    /// server. A session that never logs in again has nothing to do.
    fn set_password(&self, _password: &str) {}

    /// Take the event stream. The first call returns it. Each later call returns `None`.
    fn events(&mut self) -> Option<Self::Events>;

    /// Close the stream and stop reconnect attempts. Returns within 5 s.
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
    fn sasl_conditions_map_to_retry_or_stop() {
        use SaslCondition::*;
        for condition in [TemporaryAuthFailure, Aborted] {
            assert_eq!(sasl_retry(&condition), SaslRetry::Retry, "{condition:?}");
        }
        for condition in [
            NotAuthorized,
            CredentialsExpired,
            AccountDisabled,
            EncryptionRequired,
            MechanismTooWeak,
            InvalidMechanism,
            MalformedRequest,
            IncorrectEncoding,
            InvalidAuthzid,
        ] {
            assert_eq!(sasl_retry(&condition), SaslRetry::Stop, "{condition:?}");
        }
    }

    #[test]
    fn auth_failure_names_the_condition() {
        let wrong = AuthFailure::Sasl(SaslCondition::NotAuthorized);
        let disabled = AuthFailure::Sasl(SaslCondition::AccountDisabled);
        assert_eq!(wrong.to_string(), "wrong username or password");
        assert_eq!(disabled.to_string(), "account disabled");
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
                features: Vec::new(),
            },
            SessionEvent::Stanza(Box::new(incoming.into())),
            SessionEvent::Disconnected(DisconnectReason::Suspended),
            SessionEvent::Connected {
                bound_jid: bound.clone(),
                resumed: true,
                features: Vec::new(),
            },
        ]);
        let mut events = session.events().expect("first call returns the stream");
        assert!(session.events().is_none(), "second call returns None");

        block_on(async {
            assert!(matches!(
                next(&mut events).await,
                Some(SessionEvent::Connected { ref bound_jid, resumed: false, .. }) if *bound_jid == bound
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
