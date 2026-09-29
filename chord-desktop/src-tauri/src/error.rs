//! The error type of every command. The UI gets `{ "code": "...", "message": "..." }`.
//!
//! The code is stable and the UI can branch on it. The message is for the user.
//! The TypeScript type is `ChordError` in src/lib/chord/types.ts.

use chord_core::actor::{ClientError, LoginError};
use chord_core::session::ConnectError;
use chord_core::store::StoreError;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChordError {
    pub code: &'static str,
    pub message: String,
}

pub type Res<T> = Result<T, ChordError>;

impl ChordError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The UI sent a value that does not parse, for example a bad JID.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }

    /// A command needs the client, and `open` did not run yet.
    pub fn not_open() -> Self {
        Self::new("notOpen", "no account is open: call open first")
    }

    pub fn io(what: &str, error: impl std::fmt::Display) -> Self {
        Self::new("io", format!("{what}: {error}"))
    }
}

impl From<ClientError> for ChordError {
    fn from(error: ClientError) -> Self {
        let code = match &error {
            ClientError::NotConnected => "notConnected",
            ClientError::Session(_) => "session",
            ClientError::Server(_) => "server",
            ClientError::Invalid(_) => "invalid",
            ClientError::Unsupported(_) => "unsupported",
            ClientError::ActorGone => "actorGone",
        };
        Self::new(code, error.to_string())
    }
}

impl From<LoginError> for ChordError {
    fn from(error: LoginError) -> Self {
        let code = match &error {
            LoginError::Connect(ConnectError::AuthFailed(_)) => "authFailed",
            LoginError::Connect(ConnectError::Unreachable(_)) => "unreachable",
            LoginError::Connect(ConnectError::TlsInvalid(_)) => "tlsInvalid",
            LoginError::Connect(ConnectError::Timeout) => "timeout",
            LoginError::ActorGone => "actorGone",
        };
        Self::new(code, error.to_string())
    }
}

impl From<StoreError> for ChordError {
    fn from(error: StoreError) -> Self {
        Self::new("store", error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use chord_core::session::{AuthFailure, SessionError};

    use super::*;

    #[test]
    fn client_errors_keep_a_stable_code() {
        let cases = [
            (ClientError::NotConnected, "notConnected"),
            (ClientError::Session(SessionError::Closed), "session"),
            (ClientError::Server("x".into()), "server"),
            (ClientError::Invalid("x".into()), "invalid"),
            (ClientError::Unsupported("x".into()), "unsupported"),
            (ClientError::ActorGone, "actorGone"),
        ];
        for (error, code) in cases {
            assert_eq!(ChordError::from(error).code, code);
        }
    }

    #[test]
    fn login_errors_map_to_codes_and_texts() {
        let wrong = LoginError::Connect(ConnectError::AuthFailed(AuthFailure::NoMechanism));
        let error = ChordError::from(wrong);
        assert_eq!(error.code, "authFailed");
        assert!(error.message.contains("no common SASL mechanism"));
        assert_eq!(
            ChordError::from(LoginError::Connect(ConnectError::Timeout)).code,
            "timeout"
        );
        assert_eq!(
            ChordError::from(LoginError::Connect(ConnectError::TlsInvalid("x".into()))).code,
            "tlsInvalid"
        );
        assert_eq!(
            ChordError::from(LoginError::Connect(ConnectError::Unreachable("x".into()))).code,
            "unreachable"
        );
    }

    #[test]
    fn the_json_has_a_code_and_a_message() {
        let json = serde_json::to_value(ChordError::not_open()).unwrap();
        assert_eq!(json["code"], "notOpen");
        assert!(json["message"].is_string());
        assert_eq!(json.as_object().map(|o| o.len()), Some(2));
    }
}
