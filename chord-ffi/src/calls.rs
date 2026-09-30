//! Calls: the message layer of Jingle Message Initiation (XEP-0353) and the STUN and TURN
//! servers of XEP-0215. The methods live here, apart from `client.rs`, to keep that file small.
//!
//! There is no media stack yet. The methods send and read the call messages only.

use chord_core::features::extdisco::{IceKind as CoreIceKind, IceServer as CoreIceServer};
use chord_core::features::jmi::{
    CallDirection as CoreDirection, CallEnd as CoreCallEnd, CallEvent as CoreCallEvent, CallReason,
    CallSession as CoreCall, CallState as CoreCallState,
};

use crate::client::ChordClient;
use crate::error::{ChordError, parse_bare, parse_jid};
use crate::types::FormField;

/// The kind of an external service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum IceKind {
    Stun,
    Turn,
    Other,
}

/// A STUN or TURN server (XEP-0215), ready for a WebRTC `RTCIceServer`.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct IceServer {
    pub kind: IceKind,
    /// A URI for `RTCIceServer.urls`, for example `turn:host:3478?transport=udp`.
    pub uri: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub transport: Option<String>,
    pub restricted: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    /// When the credentials expire, in Unix ms.
    pub expires_ms: Option<i64>,
}

// The password is a secret, so it stays out of debug output.
impl core::fmt::Debug for IceServer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IceServer")
            .field("kind", &self.kind)
            .field("uri", &self.uri)
            .field("restricted", &self.restricted)
            .field("expires_ms", &self.expires_ms)
            .finish_non_exhaustive()
    }
}

impl From<CoreIceServer> for IceServer {
    fn from(s: CoreIceServer) -> Self {
        Self {
            kind: match s.kind {
                CoreIceKind::Stun => IceKind::Stun,
                CoreIceKind::Turn => IceKind::Turn,
                CoreIceKind::Other => IceKind::Other,
            },
            uri: s.uri(),
            host: s.host,
            port: s.port,
            transport: s.transport,
            restricted: s.restricted,
            username: s.username,
            password: s.password,
            expires_ms: s.expires_ms,
        }
    }
}

/// Who started the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CallDirection {
    Incoming,
    Outgoing,
}

/// How a call ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CallEnd {
    Rejected,
    Retracted,
    Finished,
    HandledElsewhere,
    TimedOut,
    TieBreak,
    Migrated,
}

impl From<CoreCallEnd> for CallEnd {
    fn from(e: CoreCallEnd) -> Self {
        match e {
            CoreCallEnd::Rejected => Self::Rejected,
            CoreCallEnd::Retracted => Self::Retracted,
            CoreCallEnd::Finished => Self::Finished,
            CoreCallEnd::HandledElsewhere => Self::HandledElsewhere,
            CoreCallEnd::TimedOut => Self::TimedOut,
            CoreCallEnd::TieBreak => Self::TieBreak,
            CoreCallEnd::Migrated => Self::Migrated,
        }
    }
}

/// The state of a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CallState {
    Proposed,
    Ringing,
    Accepted,
    Ended { end: CallEnd },
}

impl From<CoreCallState> for CallState {
    fn from(s: CoreCallState) -> Self {
        match s {
            CoreCallState::Proposed => Self::Proposed,
            CoreCallState::Ringing => Self::Ringing,
            CoreCallState::Accepted => Self::Accepted,
            CoreCallState::Ended(end) => Self::Ended { end: end.into() },
        }
    }
}

/// A call of this session.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CallSession {
    pub sid: String,
    pub direction: CallDirection,
    pub peer: String,
    pub peer_resource: Option<String>,
    pub media: Vec<String>,
    pub state: CallState,
}

impl From<CoreCall> for CallSession {
    fn from(c: CoreCall) -> Self {
        Self {
            sid: c.sid,
            direction: match c.direction {
                CoreDirection::Incoming => CallDirection::Incoming,
                CoreDirection::Outgoing => CallDirection::Outgoing,
            },
            peer: c.peer,
            peer_resource: c.peer_resource,
            media: c.media,
            state: c.state.into(),
        }
    }
}

/// An event about a call.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CallEvent {
    /// Someone proposes a call. Call `accept_call` or `reject_call`.
    Incoming {
        sid: String,
        from: String,
        media: Vec<String>,
    },
    /// A device of the peer rings.
    Ringing { sid: String, from: String },
    /// The peer sent `proceed`. Start the Jingle session.
    Proceeded { sid: String, from: String },
    /// The call ended. `reason` is the Jingle condition, for example `busy`.
    Ended {
        sid: String,
        peer: String,
        end: CallEnd,
        reason: Option<String>,
    },
}

impl From<CoreCallEvent> for CallEvent {
    fn from(e: CoreCallEvent) -> Self {
        match e {
            CoreCallEvent::Incoming { sid, from, media } => Self::Incoming { sid, from, media },
            CoreCallEvent::Ringing { sid, from } => Self::Ringing { sid, from },
            CoreCallEvent::Proceeded { sid, from } => Self::Proceeded { sid, from },
            CoreCallEvent::Ended {
                sid,
                peer,
                end,
                reason,
            } => Self::Ended {
                sid,
                peer,
                end: end.into(),
                reason,
            },
        }
    }
}

/// The reason of a `reject_call` or a `finish_call`: a Jingle condition.
fn parse_reason(reason: Option<String>) -> Result<Option<CallReason>, ChordError> {
    reason
        .map(|r| {
            CallReason::parse(&r).ok_or_else(|| ChordError::Invalid {
                detail: format!("unknown call reason {r}"),
            })
        })
        .transpose()
}

#[uniffi::export(async_runtime = "tokio")]
impl ChordClient {
    // ---- Ad-hoc commands (XEP-0050) ----

    /// Run an ad-hoc command that has one step. A push app server uses it to register a
    /// device (see `docs/android-push.md`). `fields` go in the submitted form. Returns the
    /// fields of the result form.
    pub async fn execute_command(
        &self,
        service: String,
        node: String,
        fields: Vec<FormField>,
    ) -> Result<Vec<FormField>, ChordError> {
        let service = parse_jid(&service)?;
        let fields = fields.into_iter().map(|f| (f.name, f.value)).collect();
        let result = self
            .call(move |h| async move { h.execute_command(service, node, fields).await })
            .await?;
        Ok(result
            .into_iter()
            .map(|(name, value)| FormField { name, value })
            .collect())
    }

    // ---- STUN and TURN (XEP-0215) ----

    /// The STUN and TURN servers of the server, with valid credentials. Fails with
    /// `Unsupported` when the server has no XEP-0215.
    pub async fn ice_servers(&self) -> Result<Vec<IceServer>, ChordError> {
        let list = self.call(|h| async move { h.ice_servers().await }).await?;
        Ok(list.into_iter().map(Into::into).collect())
    }

    // ---- Call messages (XEP-0353) ----

    /// Propose a call. `media` holds "audio", "video", or both. Returns the session id.
    pub async fn propose_call(&self, to: String, media: Vec<String>) -> Result<String, ChordError> {
        let to = parse_bare(&to)?;
        self.call(move |h| async move { h.propose_call(to, media).await })
            .await
    }

    /// Tell the caller that this device rings.
    pub async fn ring_call(&self, sid: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.ring_call(sid).await })
            .await
    }

    /// Accept an incoming call: send `proceed`.
    pub async fn accept_call(&self, sid: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.accept_call(sid).await })
            .await
    }

    /// Reject an incoming call. `reason` is a Jingle condition: "busy" (the default),
    /// "decline", "expired", "cancel", or "success".
    pub async fn reject_call(&self, sid: String, reason: Option<String>) -> Result<(), ChordError> {
        let reason = parse_reason(reason)?;
        self.call(move |h| async move { h.reject_call(sid, reason).await })
            .await
    }

    /// Withdraw an outgoing call that nobody accepted.
    pub async fn retract_call(&self, sid: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.retract_call(sid).await })
            .await
    }

    /// End an accepted call.
    pub async fn finish_call(&self, sid: String, reason: Option<String>) -> Result<(), ChordError> {
        let reason = parse_reason(reason)?;
        self.call(move |h| async move { h.finish_call(sid, reason).await })
            .await
    }

    /// The calls of this session, open and recently ended.
    pub async fn calls(&self) -> Result<Vec<CallSession>, ChordError> {
        let list = self.call(|h| async move { h.calls().await }).await?;
        Ok(list.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn client() -> Arc<ChordClient> {
        ChordClient::new(":memory:".into(), "alice@example.org".into()).unwrap()
    }

    #[tokio::test]
    async fn call_methods_check_input_and_need_a_session() {
        let client = client();
        assert!(matches!(
            client.propose_call("@@".into(), vec!["audio".into()]).await,
            Err(ChordError::InvalidJid { .. })
        ));
        assert!(matches!(
            client.reject_call("sid".into(), Some("nope".into())).await,
            Err(ChordError::Invalid { .. })
        ));
        assert!(matches!(
            client
                .propose_call("bob@example.org".into(), vec!["audio".into()])
                .await,
            Err(ChordError::NotConnected)
        ));
        assert!(matches!(
            client.ice_servers().await,
            Err(ChordError::NotConnected)
        ));
    }

    #[test]
    fn ice_server_debug_hides_the_credentials() {
        let s = IceServer {
            kind: IceKind::Turn,
            uri: Some("turn:h:1".into()),
            host: "h".into(),
            port: Some(1),
            transport: None,
            restricted: true,
            username: Some("the-user".into()),
            password: Some("the-secret".into()),
            expires_ms: None,
        };
        let text = format!("{s:?}");
        assert!(!text.contains("the-secret") && !text.contains("the-user"));
    }

    // The push methods (XEP-0357) are in `client.rs`. The Android glue in
    // `docs/android-push.md` calls them, so they get a test here.
    #[tokio::test]
    async fn push_methods_check_input_and_need_a_session() {
        let client = client();
        assert!(matches!(
            client.enable_push("@@".into(), "node".into(), None).await,
            Err(ChordError::InvalidJid { .. })
        ));
        assert!(matches!(
            client.disable_push("@@".into(), None).await,
            Err(ChordError::InvalidJid { .. })
        ));
        let form = vec![crate::FormField {
            name: "secret".into(),
            value: "s".into(),
        }];
        assert!(matches!(
            client
                .enable_push("push.example.org".into(), "node".into(), Some(form))
                .await,
            Err(ChordError::NotConnected)
        ));
        // The list reads the store, so it works offline.
        assert!(client.push_registrations().await.unwrap().is_empty());
    }
}
