//! Client State Indication (XEP-0352): tell the server when nobody looks at the client.
//!
//! An inactive client lets the server hold back presence and chat states, and batch
//! other stanzas that do not need an answer now. That saves battery and bandwidth.
//!
//! - `<active/>` and `<inactive/>` are stream elements, not stanzas and not IQs.
//! - The server offers CSI as the `<csi xmlns='urn:xmpp:csi:0'/>` stream feature. We
//!   send nothing when it is missing.
//! - Section 5.2 of the XEP: the state is `active` on each new stream, and also on a
//!   resumed stream. So we send `<inactive/>` again after both, when the app is inactive.
//! - The wanted state lives in `State` and survives a new session. A command that comes
//!   while we are offline sets it too, and the next session sends it.

use futures_channel::oneshot;
use xmpp_parsers::ns;

use super::{Ctx, Effect, FeatureCommand};
use crate::actor::{ClientError, ClientHandle};

/// The stream feature namespace of XEP-0352.
pub const NS_CSI: &str = ns::CSI;

#[derive(Debug, Default)]
pub(crate) struct State {
    /// The state that the app wants. Kept over sessions.
    pub inactive: bool,
    /// The server offers CSI on this stream.
    offered: bool,
}

pub(crate) enum Command {
    /// The reply is `true` when the server got the state, and `false` when the server
    /// does not offer CSI or we are offline. The next session then sends it.
    Set {
        active: bool,
        reply: oneshot::Sender<bool>,
    },
}

impl ClientHandle {
    /// Tell the server that the app is active (a person looks at it) or inactive (the
    /// window is hidden or the app is in the background). Returns whether the server
    /// got it. Chord sends the state again after a reconnect or a resume. Without server
    /// support it does nothing, and the answer is `false`.
    pub async fn set_client_active(&self, active: bool) -> Result<bool, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Csi(Command::Set { active, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)
    }
}

/// The stream is up, new or resumed. The server starts each stream as active.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>, stream_features: &[String]) {
    ctx.state.csi.offered = stream_features.iter().any(|f| f == NS_CSI);
    if ctx.state.csi.offered && ctx.state.csi.inactive {
        ctx.effects.push(Effect::ClientState(false));
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    let Command::Set { active, reply } = command;
    ctx.state.csi.inactive = !active;
    let offered = ctx.state.csi.offered;
    if offered {
        ctx.effects.push(Effect::ClientState(active));
    }
    let _ = reply.send(offered);
}

/// No session is up. Keep the wanted state for the next one.
pub(crate) fn offline(state: &mut State, command: Command) {
    let Command::Set { active, reply } = command;
    state.inactive = !active;
    let _ = reply.send(false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;

    fn features(csi: bool) -> Vec<String> {
        if csi { vec![NS_CSI.to_owned()] } else { vec![] }
    }

    fn set(h: &mut Harness, active: bool) -> bool {
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Set { active, reply }));
        answer.try_recv().unwrap().unwrap()
    }

    fn states(h: &mut Harness) -> Vec<bool> {
        std::mem::take(&mut h.effects)
            .into_iter()
            .filter_map(|e| match e {
                Effect::ClientState(active) => Some(active),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn sends_the_state_when_the_server_offers_csi() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_connected(ctx, &features(true)));
        assert!(states(&mut h).is_empty(), "a new stream is active already");
        assert!(set(&mut h, false));
        assert!(set(&mut h, true));
        assert_eq!(states(&mut h), vec![false, true]);
    }

    #[test]
    fn sends_nothing_without_the_stream_feature() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_connected(ctx, &features(false)));
        assert!(!set(&mut h, false));
        assert!(states(&mut h).is_empty());
        // The wanted state stays, so a later stream that offers CSI gets it.
        h.with_ctx(|ctx| on_connected(ctx, &features(true)));
        assert_eq!(states(&mut h), vec![false]);
    }

    #[test]
    fn sends_inactive_again_after_a_new_or_resumed_stream() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_connected(ctx, &features(true)));
        set(&mut h, false);
        states(&mut h);
        // A resume and a reconnect call `on_connected` alike. XEP-0352, 5.2.
        h.with_ctx(|ctx| on_connected(ctx, &features(true)));
        assert_eq!(states(&mut h), vec![false]);
        // After `active`, the next stream sends nothing.
        set(&mut h, true);
        states(&mut h);
        h.with_ctx(|ctx| on_connected(ctx, &features(true)));
        assert!(states(&mut h).is_empty());
    }

    #[test]
    fn a_command_while_offline_sets_the_state_for_the_next_session() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        offline(
            &mut h.state.csi,
            Command::Set {
                active: false,
                reply,
            },
        );
        assert_eq!(answer.try_recv().unwrap(), Some(false));
        h.with_ctx(|ctx| on_connected(ctx, &features(true)));
        assert_eq!(states(&mut h), vec![false]);
    }
}
