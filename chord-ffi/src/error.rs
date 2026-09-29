//! `ChordError`: one error type for every call that crosses the FFI boundary.

use core::fmt;

use chord_core::actor::{ClientError, LoginError};
use chord_core::jid::{BareJid, Jid};
use chord_core::session::ConnectError;
use chord_core::store::StoreError;

/// An error from a `ChordClient` call. The `detail` text is for the user or the log.
#[derive(Clone, Debug, PartialEq, uniffi::Error)]
pub enum ChordError {
    /// No session. Call `login` first.
    NotConnected,
    /// The session is closed.
    Session { detail: String },
    /// The server answered with an error.
    Server { detail: String },
    /// The request is not valid, for example a missing file.
    Invalid { detail: String },
    /// The server does not offer the service, for example no upload service.
    Unsupported { detail: String },
    /// The client stopped.
    ActorGone,
    /// A JID argument does not parse.
    InvalidJid { detail: String },
    /// The server string of `login` does not parse.
    InvalidServer { detail: String },
    /// The server rejected the credentials.
    AuthFailed { detail: String },
    /// The server is not reachable.
    Unreachable { detail: String },
    /// The server certificate is not valid.
    TlsInvalid { detail: String },
    /// The login took too long.
    Timeout,
    /// The database cannot open or is too new.
    Store { detail: String },
    /// A bug or a panic inside the client.
    Internal { detail: String },
}

impl fmt::Display for ChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("not logged in"),
            Self::Session { detail } => write!(f, "session error: {detail}"),
            Self::Server { detail } => write!(f, "server error: {detail}"),
            Self::Invalid { detail } => write!(f, "invalid request: {detail}"),
            Self::Unsupported { detail } => write!(f, "not supported: {detail}"),
            Self::ActorGone => f.write_str("the client stopped"),
            Self::InvalidJid { detail } => write!(f, "invalid JID: {detail}"),
            Self::InvalidServer { detail } => write!(f, "invalid server: {detail}"),
            Self::AuthFailed { detail } => write!(f, "{detail}"),
            Self::Unreachable { detail } => write!(f, "server unreachable: {detail}"),
            Self::TlsInvalid { detail } => write!(f, "server certificate invalid: {detail}"),
            Self::Timeout => f.write_str("login timed out"),
            Self::Store { detail } => write!(f, "{detail}"),
            Self::Internal { detail } => write!(f, "internal error: {detail}"),
        }
    }
}

impl std::error::Error for ChordError {}

impl From<ClientError> for ChordError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::NotConnected => Self::NotConnected,
            ClientError::Session(e) => Self::Session {
                detail: e.to_string(),
            },
            ClientError::Server(detail) => Self::Server { detail },
            ClientError::Invalid(detail) => Self::Invalid { detail },
            ClientError::Unsupported(detail) => Self::Unsupported { detail },
            ClientError::ActorGone => Self::ActorGone,
        }
    }
}

impl From<ConnectError> for ChordError {
    fn from(e: ConnectError) -> Self {
        match e {
            ConnectError::AuthFailed(f) => Self::AuthFailed {
                detail: f.to_string(),
            },
            ConnectError::Unreachable(detail) => Self::Unreachable { detail },
            ConnectError::TlsInvalid(detail) => Self::TlsInvalid { detail },
            ConnectError::Timeout => Self::Timeout,
        }
    }
}

impl From<LoginError> for ChordError {
    fn from(e: LoginError) -> Self {
        match e {
            LoginError::Connect(e) => e.into(),
            LoginError::ActorGone => Self::ActorGone,
        }
    }
}

impl From<StoreError> for ChordError {
    fn from(e: StoreError) -> Self {
        Self::Store {
            detail: e.to_string(),
        }
    }
}

/// Parse a full or bare JID.
pub(crate) fn parse_jid(s: &str) -> Result<Jid, ChordError> {
    s.parse().map_err(|e| ChordError::InvalidJid {
        detail: format!("{s}: {e}"),
    })
}

/// Parse a bare JID. A JID with a resource is an error.
pub(crate) fn parse_bare(s: &str) -> Result<BareJid, ChordError> {
    s.parse().map_err(|e| ChordError::InvalidJid {
        detail: format!("{s}: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use chord_core::session::{AuthFailure, SessionError};

    use super::*;

    #[test]
    fn client_errors_map() {
        assert_eq!(
            ChordError::from(ClientError::NotConnected),
            ChordError::NotConnected
        );
        assert_eq!(
            ChordError::from(ClientError::Session(SessionError::Closed)),
            ChordError::Session {
                detail: "session is closed".into()
            }
        );
        assert_eq!(
            ChordError::from(ClientError::Server("x".into())),
            ChordError::Server { detail: "x".into() }
        );
        assert_eq!(
            ChordError::from(ClientError::Invalid("y".into())),
            ChordError::Invalid { detail: "y".into() }
        );
        assert_eq!(
            ChordError::from(ClientError::Unsupported("z".into())),
            ChordError::Unsupported { detail: "z".into() }
        );
        assert_eq!(
            ChordError::from(ClientError::ActorGone),
            ChordError::ActorGone
        );
    }

    #[test]
    fn connect_and_login_errors_map() {
        assert_eq!(
            ChordError::from(LoginError::Connect(ConnectError::Timeout)),
            ChordError::Timeout
        );
        assert_eq!(
            ChordError::from(LoginError::ActorGone),
            ChordError::ActorGone
        );
        assert_eq!(
            ChordError::from(ConnectError::AuthFailed(AuthFailure::NoMechanism)),
            ChordError::AuthFailed {
                detail: "no common SASL mechanism".into()
            }
        );
        assert!(matches!(
            ChordError::from(ConnectError::Unreachable("u".into())),
            ChordError::Unreachable { .. }
        ));
        assert!(matches!(
            ChordError::from(ConnectError::TlsInvalid("t".into())),
            ChordError::TlsInvalid { .. }
        ));
    }

    #[test]
    fn jid_parsing_errors_are_typed() {
        assert!(parse_jid("a@b/c").is_ok());
        assert!(matches!(
            parse_jid("@@"),
            Err(ChordError::InvalidJid { .. })
        ));
        assert!(parse_bare("a@b").is_ok());
        assert!(matches!(
            parse_bare("a@b/res"),
            Err(ChordError::InvalidJid { .. })
        ));
    }
}
