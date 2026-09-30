//! Jingle Message Initiation (XEP-0353, version 0.8.0): the message layer of a call.
//!
//! One side proposes a session in a chat message. The other side rings, proceeds, or
//! rejects. The proposer can retract. After `proceed` the two sides run Jingle (XEP-0166),
//! and `finish` ends the call. The core has no media stack yet, so this module stops at
//! the messages: it keeps one small state machine for each call, and it tells the UI with
//! `CallEvent`s. `docs/calls-plan.md` says how the media layer joins in.
//!
//! Chord does not advertise `urn:xmpp:jingle-message:0` in disco yet. A peer that saw it
//! would ring into nothing.
//!
//! Rules from the XEP that this module follows:
//! - Every message has `type="chat"`, an id, and a `<store/>` hint (XEP-0334), so carbons
//!   and the archive carry it to all devices.
//! - `propose` and `retract` go to the bare JID of the other side. `ringing`, `proceed`,
//!   `reject`, and `finish` go to the full JID of the device that answers or proposes.
//! - The session id is a UUIDv4. The `id` of `propose` is the `sid` of the later Jingle IQ.
//! - Carbons copy a `proceed` or a `reject` of another device of ours to us. The call
//!   then stops ringing here.
//! - A message from the archive or with a delay never starts or changes a call.
//! - Two proposals at once: the lower session id wins (`i;octet`, so byte order).
//! - A new proposal from a peer that has an accepted call with us replaces that call.
//! - Chord never sends the old `accept` message (XEP-0353, section 8, privacy).
//! - A call that nobody finishes ends after 24 hours.

use std::collections::HashMap;

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use xmpp_parsers::message::{Message, MessageType};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;

use super::{Ctx, FeatureCommand, new_id};
use crate::actor::{ClientError, ClientEvent, ClientHandle};

pub const NS_JMI: &str = "urn:xmpp:jingle-message:0";
const NS_RTP: &str = "urn:xmpp:jingle:apps:rtp:1";
const NS_JINGLE: &str = "urn:xmpp:jingle:1";
const NS_HINTS: &str = "urn:xmpp:hints";
const NS_DELAY: &str = "urn:xmpp:delay";

/// A session tick is 15 s. An outgoing call that nobody answers ends after one minute.
const OUTGOING_TIMEOUT_TICKS: u32 = 4;
/// A ringing call that we do not answer ends after 90 s.
const INCOMING_TIMEOUT_TICKS: u32 = 6;
/// An accepted call with no `finish` ends after 24 hours (XEP-0353, section 5).
const ACCEPTED_TIMEOUT_TICKS: u32 = 24 * 60 * 4;
/// An ended call stays in the table for ten minutes. It absorbs late and repeated messages.
const ENDED_KEEP_TICKS: u32 = 40;
/// The longest session id that we accept.
const MAX_SID: usize = 256;

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// Who started the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum CallDirection {
    Incoming,
    Outgoing,
}

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum CallEnd {
    /// The other side rejected the call, or we did.
    Rejected,
    /// The proposer retracted the call.
    Retracted,
    /// An accepted call finished.
    Finished,
    /// Another device of ours proceeded or rejected.
    HandledElsewhere,
    /// Nobody answered in time.
    TimedOut,
    /// A proposal with a lower session id won.
    TieBreak,
    /// A new call from the same peer replaced this one.
    Migrated,
}

/// The state of a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum CallState {
    /// Outgoing: sent, no answer. Incoming: received, we did not answer.
    Proposed,
    /// Outgoing: a device of the peer rings. Incoming: we told the peer that we ring.
    Ringing,
    /// Someone sent `proceed`. The Jingle session comes next.
    Accepted,
    Ended(CallEnd),
}

/// The reason in a `reject`, `retract`, or `finish`: a Jingle reason condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallReason {
    Success,
    Busy,
    Cancel,
    Decline,
    Expired,
}

impl CallReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Busy => "busy",
            Self::Cancel => "cancel",
            Self::Decline => "decline",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "success" => Self::Success,
            "busy" => Self::Busy,
            "cancel" => Self::Cancel,
            "decline" => Self::Decline,
            "expired" => Self::Expired,
            _ => return None,
        })
    }
}

/// A call, as the UI sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct CallSession {
    /// The session id. The Jingle session uses it too.
    pub sid: String,
    pub direction: CallDirection,
    /// The bare JID of the other side.
    pub peer: String,
    /// The full JID that we address messages to, when we know it.
    pub peer_resource: Option<String>,
    /// The media of the proposal, for example `audio` and `video`.
    pub media: Vec<String>,
    pub state: CallState,
}

/// An event about a call. Every event carries the `sid`.
#[derive(Debug, Clone, PartialEq, Eq)]
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
pub enum CallEvent {
    /// Someone proposes a call. Call `accept_call` or `reject_call`. Call `ring_call` when
    /// the UI shows the call.
    Incoming {
        sid: String,
        from: String,
        media: Vec<String>,
    },
    /// A device of the peer rings (an answer to our proposal).
    Ringing { sid: String, from: String },
    /// The peer sent `proceed`. Start the Jingle session.
    Proceeded { sid: String, from: String },
    /// The call ended. `reason` is the Jingle condition of the message, for example `busy`.
    Ended {
        sid: String,
        peer: String,
        end: CallEnd,
        reason: Option<String>,
    },
}

/// A message of the protocol, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Jmi {
    Propose {
        sid: String,
        media: Vec<String>,
    },
    Ringing {
        sid: String,
    },
    Proceed {
        sid: String,
    },
    Reject {
        sid: String,
        reason: Option<String>,
        tie_break: bool,
    },
    Retract {
        sid: String,
        reason: Option<String>,
        tie_break: bool,
    },
    Finish {
        sid: String,
        reason: Option<String>,
        migrated_to: Option<String>,
    },
}

impl Jmi {
    fn sid(&self) -> &str {
        match self {
            Self::Propose { sid, .. }
            | Self::Ringing { sid }
            | Self::Proceed { sid }
            | Self::Reject { sid, .. }
            | Self::Retract { sid, .. }
            | Self::Finish { sid, .. } => sid,
        }
    }
}

/// Read the JMI element of a message. `None` if the message has none, or if it is not valid.
pub(crate) fn parse(message: &Message) -> Option<Jmi> {
    let el = message.payloads.iter().find(|p| p.ns() == NS_JMI)?;
    let sid = el.attr("id")?.to_owned();
    if sid.is_empty() || sid.len() > MAX_SID {
        return None;
    }
    let reason = || {
        el.get_child("reason", NS_JINGLE)
            .and_then(|r| r.children().find(|c| c.name() != "text"))
            .map(|c| c.name().to_owned())
    };
    let tie_break = el.has_child("tie-break", NS_JMI);
    Some(match el.name() {
        "propose" => {
            // The XEP wants one description for each media type.
            let media: Vec<String> = el
                .children()
                .filter(|c| c.is("description", NS_RTP))
                .filter_map(|c| c.attr("media").map(str::to_owned))
                .collect();
            if media.is_empty() {
                return None;
            }
            Jmi::Propose { sid, media }
        }
        "ringing" => Jmi::Ringing { sid },
        "proceed" => Jmi::Proceed { sid },
        "reject" => Jmi::Reject {
            sid,
            reason: reason(),
            tie_break,
        },
        "retract" => Jmi::Retract {
            sid,
            reason: reason(),
            tie_break,
        },
        "finish" => Jmi::Finish {
            sid,
            reason: reason(),
            migrated_to: el
                .get_child("migrated", NS_JMI)
                .and_then(|m| m.attr("to"))
                .map(str::to_owned),
        },
        _ => return None,
    })
}

/// What can happen to a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    RemoteRinging,
    RemoteProceed,
    RemoteReject,
    RemoteRetract,
    RemoteFinish,
    LocalRing,
    LocalAccept,
    LocalReject,
    LocalRetract,
    LocalFinish,
    /// Another device of ours sent `proceed` or `reject`.
    ElsewhereAnswered,
    Timeout,
}

/// The state machine. Returns the next state, or `None` when `input` is not valid in
/// `state` (the caller ignores it or reports an error).
pub(crate) fn next_state(
    direction: CallDirection,
    state: CallState,
    input: Input,
) -> Option<CallState> {
    use CallDirection::{Incoming, Outgoing};
    use CallState::{Accepted, Ended, Proposed, Ringing};
    let open = matches!(state, Proposed | Ringing);
    Some(match (direction, state, input) {
        (_, Accepted, Input::LocalFinish | Input::RemoteFinish) => Ended(CallEnd::Finished),
        (_, Accepted, Input::Timeout) => Ended(CallEnd::TimedOut),
        (Outgoing, _, Input::RemoteRinging) if open => Ringing,
        (Outgoing, _, Input::RemoteProceed) if open => Accepted,
        (Outgoing, _, Input::RemoteReject) if open => Ended(CallEnd::Rejected),
        (Outgoing, _, Input::LocalRetract) if open => Ended(CallEnd::Retracted),
        (Incoming, _, Input::LocalRing) if open => Ringing,
        (Incoming, _, Input::LocalAccept) if open => Accepted,
        (Incoming, _, Input::LocalReject) if open => Ended(CallEnd::Rejected),
        (Incoming, _, Input::RemoteRetract) if open => Ended(CallEnd::Retracted),
        (Incoming, _, Input::ElsewhereAnswered) if open => Ended(CallEnd::HandledElsewhere),
        (_, _, Input::Timeout) if open => Ended(CallEnd::TimedOut),
        _ => return None,
    })
}

/// One call in the table.
#[derive(Debug)]
struct Call {
    direction: CallDirection,
    peer: BareJid,
    peer_full: Option<Jid>,
    media: Vec<String>,
    state: CallState,
    /// Session ticks since the last change of state.
    ticks: u32,
}

impl Call {
    fn snapshot(&self, sid: &str) -> CallSession {
        CallSession {
            sid: sid.to_owned(),
            direction: self.direction,
            peer: self.peer.to_string(),
            peer_resource: self.peer_full.as_ref().map(Jid::to_string),
            media: self.media.clone(),
            state: self.state,
        }
    }

    fn set(&mut self, state: CallState) {
        self.state = state;
        self.ticks = 0;
    }

    fn is_open(&self) -> bool {
        matches!(self.state, CallState::Proposed | CallState::Ringing)
    }
}

/// The calls of this session. They live in memory only: a call cannot survive a new stream.
#[derive(Debug, Default)]
pub(crate) struct State {
    calls: HashMap<String, Call>,
}

/// A command from the public API.
pub(crate) enum Command {
    Propose {
        to: BareJid,
        media: Vec<String>,
        reply: Reply<String>,
    },
    Ring {
        sid: String,
        reply: Reply<()>,
    },
    Accept {
        sid: String,
        reply: Reply<()>,
    },
    Reject {
        sid: String,
        reason: Option<CallReason>,
        reply: Reply<()>,
    },
    Retract {
        sid: String,
        reply: Reply<()>,
    },
    Finish {
        sid: String,
        reason: Option<CallReason>,
        reply: Reply<()>,
    },
    List {
        reply: Reply<Vec<CallSession>>,
    },
}

impl ClientHandle {
    /// Propose a call (XEP-0353). `media` holds `audio`, `video`, or both. Returns the
    /// session id. The call ends by itself when nobody answers in one minute.
    ///
    /// This is the message layer only. Chord has no media stack, and it does not advertise
    /// the protocol in disco.
    pub async fn propose_call(
        &self,
        to: BareJid,
        media: Vec<String>,
    ) -> Result<String, ClientError> {
        self.call_command(|reply| Command::Propose { to, media, reply })
            .await
    }

    /// Tell the initiator that this device rings. Send it when the UI shows the call.
    pub async fn ring_call(&self, sid: String) -> Result<(), ClientError> {
        self.call_command(|reply| Command::Ring { sid, reply })
            .await
    }

    /// Accept an incoming call: send `proceed`. The Jingle session follows.
    pub async fn accept_call(&self, sid: String) -> Result<(), ClientError> {
        self.call_command(|reply| Command::Accept { sid, reply })
            .await
    }

    /// Reject an incoming call. The default reason is `busy`.
    pub async fn reject_call(
        &self,
        sid: String,
        reason: Option<CallReason>,
    ) -> Result<(), ClientError> {
        self.call_command(|reply| Command::Reject { sid, reason, reply })
            .await
    }

    /// Withdraw an outgoing call that nobody accepted yet.
    pub async fn retract_call(&self, sid: String) -> Result<(), ClientError> {
        self.call_command(|reply| Command::Retract { sid, reply })
            .await
    }

    /// End an accepted call. The default reason is `success`.
    pub async fn finish_call(
        &self,
        sid: String,
        reason: Option<CallReason>,
    ) -> Result<(), ClientError> {
        self.call_command(|reply| Command::Finish { sid, reason, reply })
            .await
    }

    /// The calls of this session, open and recently ended.
    pub async fn calls(&self) -> Result<Vec<CallSession>, ClientError> {
        self.call_command(|reply| Command::List { reply }).await
    }

    async fn call_command<T>(
        &self,
        command: impl FnOnce(Reply<T>) -> Command,
    ) -> Result<T, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Jmi(command(reply)))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// A command while no session is up.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Propose { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::List { reply } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::Ring { reply, .. }
        | Command::Accept { reply, .. }
        | Command::Reject { reply, .. }
        | Command::Retract { reply, .. }
        | Command::Finish { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

// ---- Sending ----

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

/// A JMI message: `type="chat"`, an id, the element, and the `<store/>` hint.
fn message(to: Jid, element: Element) -> Message {
    let mut m = Message::chat(to);
    m.id = Some(xmpp_parsers::message::Id(new_id()));
    m.payloads.push(element);
    m.payloads.push(Element::builder("store", NS_HINTS).build());
    m
}

fn reason_element(reason: CallReason, tie_break: bool) -> Vec<Element> {
    let mut out = Vec::new();
    if tie_break {
        out.push(Element::builder("tie-break", NS_JMI).build());
    }
    out.push(
        Element::builder("reason", NS_JINGLE)
            .append(Element::builder(reason.as_str(), NS_JINGLE).build())
            .build(),
    );
    out
}

fn simple(name: &'static str, sid: &str, children: Vec<Element>) -> Element {
    let mut el = Element::builder(name, NS_JMI).attr(nc("id"), sid);
    for child in children {
        el = el.append(child);
    }
    el.build()
}

fn propose_element(sid: &str, media: &[String]) -> Element {
    let descriptions = media
        .iter()
        .map(|m| {
            Element::builder("description", NS_RTP)
                .attr(nc("media"), m.as_str())
                .build()
        })
        .collect();
    simple("propose", sid, descriptions)
}

fn emit(ctx: &mut Ctx<'_>, event: CallEvent) {
    ctx.emit(ClientEvent::Call(event));
}

fn ended(ctx: &mut Ctx<'_>, sid: &str, peer: String, end: CallEnd, reason: Option<String>) {
    emit(
        ctx,
        CallEvent::Ended {
            sid: sid.to_owned(),
            peer,
            end,
            reason,
        },
    );
}

// ---- Commands ----

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Propose { to, media, reply } => {
            let _ = reply.send(propose(ctx, to, media));
        }
        Command::Ring { sid, reply } => {
            let _ = reply.send(answer(ctx, &sid, Input::LocalRing, None));
        }
        Command::Accept { sid, reply } => {
            let _ = reply.send(answer(ctx, &sid, Input::LocalAccept, None));
        }
        Command::Reject { sid, reason, reply } => {
            let reason = reason.unwrap_or(CallReason::Busy);
            let _ = reply.send(answer(ctx, &sid, Input::LocalReject, Some(reason)));
        }
        Command::Retract { sid, reply } => {
            let _ = reply.send(answer(
                ctx,
                &sid,
                Input::LocalRetract,
                Some(CallReason::Cancel),
            ));
        }
        Command::Finish { sid, reason, reply } => {
            let reason = reason.unwrap_or(CallReason::Success);
            let _ = reply.send(answer(ctx, &sid, Input::LocalFinish, Some(reason)));
        }
        Command::List { reply } => {
            let mut calls: Vec<_> = ctx
                .state
                .jmi
                .calls
                .iter()
                .map(|(sid, call)| call.snapshot(sid))
                .collect();
            calls.sort_by(|a, b| a.sid.cmp(&b.sid));
            let _ = reply.send(Ok(calls));
        }
    }
}

fn propose(ctx: &mut Ctx<'_>, to: BareJid, media: Vec<String>) -> Result<String, ClientError> {
    if to == *ctx.account {
        return Err(ClientError::Invalid("cannot call our own account".into()));
    }
    if media.is_empty()
        || media
            .iter()
            .any(|m| !matches!(m.as_str(), "audio" | "video"))
    {
        return Err(ClientError::Invalid(
            "the media must be audio, video, or both".into(),
        ));
    }
    let sid = new_id();
    ctx.send(message(
        Jid::from(to.clone()),
        propose_element(&sid, &media),
    ));
    ctx.state.jmi.calls.insert(
        sid.clone(),
        Call {
            direction: CallDirection::Outgoing,
            peer: to,
            peer_full: None,
            media,
            state: CallState::Proposed,
            ticks: 0,
        },
    );
    Ok(sid)
}

/// Run a local input on a call and send the message that goes with it.
fn answer(
    ctx: &mut Ctx<'_>,
    sid: &str,
    input: Input,
    reason: Option<CallReason>,
) -> Result<(), ClientError> {
    let call = ctx
        .state
        .jmi
        .calls
        .get_mut(sid)
        .ok_or_else(|| ClientError::Invalid(format!("no call {sid}")))?;
    let next = next_state(call.direction, call.state, input).ok_or_else(|| {
        ClientError::Invalid(format!("cannot do that in the state {:?}", call.state))
    })?;
    let full = call
        .peer_full
        .clone()
        .unwrap_or_else(|| Jid::from(call.peer.clone()));
    let bare = Jid::from(call.peer.clone());
    call.set(next);
    let (to, element, end) = match input {
        Input::LocalRing => (full, simple("ringing", sid, vec![]), None),
        Input::LocalAccept => (full, simple("proceed", sid, vec![]), None),
        Input::LocalReject => (
            full,
            simple(
                "reject",
                sid,
                reason_element(reason.unwrap_or(CallReason::Busy), false),
            ),
            Some(CallEnd::Rejected),
        ),
        Input::LocalRetract => (
            bare,
            simple(
                "retract",
                sid,
                reason_element(reason.unwrap_or(CallReason::Cancel), false),
            ),
            Some(CallEnd::Retracted),
        ),
        Input::LocalFinish => (
            full,
            simple(
                "finish",
                sid,
                reason_element(reason.unwrap_or(CallReason::Success), false),
            ),
            Some(CallEnd::Finished),
        ),
        _ => unreachable!("only local inputs come here"),
    };
    ctx.send(message(to, element));
    if let Some(end) = end {
        let call = &ctx.state.jmi.calls[sid];
        let event = CallEvent::Ended {
            sid: sid.to_owned(),
            peer: call.peer.to_string(),
            end,
            reason: reason.map(|r| r.as_str().to_owned()),
        };
        emit(ctx, event);
    }
    Ok(())
}

// ---- Incoming messages ----

/// A message that may belong to a call. Returns true if it is one, so that no other
/// feature handles it. `message` comes after the carbon unwrap and after the archive.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    let Some(jmi) = parse(message) else {
        // A JMI element that is not valid still belongs to us. Keep it out of the chats.
        return message.payloads.iter().any(|p| p.ns() == NS_JMI);
    };
    if matches!(message.type_, MessageType::Groupchat | MessageType::Error) {
        return true;
    }
    // A delayed message comes from offline storage or the archive: the call is over or
    // the other side gave up, so it must not ring or change a call.
    if message.payloads.iter().any(|p| p.is("delay", NS_DELAY)) {
        log::debug!("ignored a delayed call message for {}", jmi.sid());
        return true;
    }
    let Some(from) = message.from.clone() else {
        return true;
    };
    if from.to_bare() == *ctx.account {
        on_own_device(ctx, &jmi);
    } else {
        on_peer(ctx, from, jmi);
    }
    true
}

/// A message that another device of our account sent. It reached us as a carbon.
fn on_own_device(ctx: &mut Ctx<'_>, jmi: &Jmi) {
    let (Jmi::Proceed { sid } | Jmi::Reject { sid, .. }) = jmi else {
        return;
    };
    let Some(call) = ctx.state.jmi.calls.get_mut(sid) else {
        return;
    };
    if let Some(next) = next_state(call.direction, call.state, Input::ElsewhereAnswered) {
        call.set(next);
        let reason = match jmi {
            Jmi::Reject { reason, .. } => reason.clone(),
            _ => None,
        };
        let call = &ctx.state.jmi.calls[sid];
        let sid = sid.clone();
        let call_peer = call.peer.to_string();
        emit(
            ctx,
            CallEvent::Ended {
                sid,
                peer: call_peer,
                end: CallEnd::HandledElsewhere,
                reason,
            },
        );
    }
}

fn on_peer(ctx: &mut Ctx<'_>, from: Jid, jmi: Jmi) {
    if let Jmi::Propose { sid, media } = jmi {
        on_propose(ctx, from, sid, media);
        return;
    }
    let sid = jmi.sid().to_owned();
    let Some(call) = ctx.state.jmi.calls.get_mut(&sid) else {
        return;
    };
    // Only the peer of the call can change it. A third party knows no session id, but a
    // forged message must still not touch a call.
    if call.peer != from.to_bare() {
        log::warn!("ignored a call message for {sid} from {from}: not the peer");
        return;
    }
    let (input, reason) = match &jmi {
        Jmi::Ringing { .. } => (Input::RemoteRinging, None),
        Jmi::Proceed { .. } => (Input::RemoteProceed, None),
        Jmi::Reject { reason, .. } => (Input::RemoteReject, reason.clone()),
        Jmi::Retract { reason, .. } => (Input::RemoteRetract, reason.clone()),
        Jmi::Finish { reason, .. } => (Input::RemoteFinish, reason.clone()),
        Jmi::Propose { .. } => unreachable!("handled above"),
    };
    let Some(next) = next_state(call.direction, call.state, input) else {
        log::debug!("ignored {input:?} for {sid} in the state {:?}", call.state);
        return;
    };
    // The full JID of the device that answers is the one for the rest of the call.
    if matches!(input, Input::RemoteRinging | Input::RemoteProceed) {
        call.peer_full = Some(from.clone());
    }
    call.set(next);
    let peer = call.peer.to_string();
    let event = match (input, next) {
        (Input::RemoteRinging, _) => CallEvent::Ringing {
            sid,
            from: from.to_string(),
        },
        (Input::RemoteProceed, _) => CallEvent::Proceeded {
            sid,
            from: from.to_string(),
        },
        (_, CallState::Ended(end)) => CallEvent::Ended {
            sid,
            peer,
            end,
            reason,
        },
        _ => return,
    };
    emit(ctx, event);
}

fn on_propose(ctx: &mut Ctx<'_>, from: Jid, sid: String, media: Vec<String>) {
    if ctx.state.jmi.calls.contains_key(&sid) {
        return;
    }
    let peer = from.to_bare();

    // Two proposals at once (section 4.1): the lower session id wins.
    let clash = ctx
        .state
        .jmi
        .calls
        .iter()
        .find(|(_, c)| c.direction == CallDirection::Outgoing && c.peer == peer && c.is_open())
        .map(|(own_sid, _)| own_sid.clone());
    if let Some(own_sid) = clash {
        if sid.as_bytes() > own_sid.as_bytes() {
            // Our proposal wins. Reject theirs.
            let reject = simple("reject", &sid, reason_element(CallReason::Expired, true));
            ctx.send(message(from, reject));
            return;
        }
        // Their proposal wins. Retract ours and take theirs.
        let retract = simple(
            "retract",
            &own_sid,
            reason_element(CallReason::Expired, true),
        );
        ctx.send(message(Jid::from(peer.clone()), retract));
        let call = ctx
            .state
            .jmi
            .calls
            .get_mut(&own_sid)
            .expect("the call exists");
        call.set(CallState::Ended(CallEnd::TieBreak));
        let event = CallEvent::Ended {
            sid: own_sid.clone(),
            peer: peer.to_string(),
            end: CallEnd::TieBreak,
            reason: Some("expired".into()),
        };
        emit(ctx, event);
    }

    // A new call from a peer with an accepted call replaces that call (section 4.2).
    let old = ctx
        .state
        .jmi
        .calls
        .iter()
        .find(|(_, c)| c.peer == peer && c.state == CallState::Accepted)
        .map(|(old_sid, _)| old_sid.clone());
    if let Some(old_sid) = old {
        let call = ctx
            .state
            .jmi
            .calls
            .get_mut(&old_sid)
            .expect("the call exists");
        let to = call
            .peer_full
            .clone()
            .unwrap_or_else(|| Jid::from(call.peer.clone()));
        call.set(CallState::Ended(CallEnd::Migrated));
        let mut children = reason_element(CallReason::Success, false);
        children.push(
            Element::builder("migrated", NS_JMI)
                .attr(nc("to"), sid.as_str())
                .build(),
        );
        ctx.send(message(to, simple("finish", &old_sid, children)));
        emit(
            ctx,
            CallEvent::Ended {
                sid: old_sid,
                peer: peer.to_string(),
                end: CallEnd::Migrated,
                reason: None,
            },
        );
    }

    ctx.state.jmi.calls.insert(
        sid.clone(),
        Call {
            direction: CallDirection::Incoming,
            peer,
            peer_full: Some(from.clone()),
            media: media.clone(),
            state: CallState::Proposed,
            ticks: 0,
        },
    );
    emit(
        ctx,
        CallEvent::Incoming {
            sid,
            from: from.to_string(),
            media,
        },
    );
}

// ---- Time ----

/// A session tick: end the calls that nobody answered, and drop the old ones.
pub(crate) fn on_tick(ctx: &mut Ctx<'_>) {
    let mut expired = Vec::new();
    for (sid, call) in &mut ctx.state.jmi.calls {
        call.ticks += 1;
        let limit = match (call.direction, call.state) {
            (CallDirection::Outgoing, CallState::Proposed | CallState::Ringing) => {
                OUTGOING_TIMEOUT_TICKS
            }
            (CallDirection::Incoming, CallState::Proposed | CallState::Ringing) => {
                INCOMING_TIMEOUT_TICKS
            }
            (_, CallState::Accepted) => ACCEPTED_TIMEOUT_TICKS,
            (_, CallState::Ended(_)) => continue,
        };
        if call.ticks >= limit
            && let Some(next) = next_state(call.direction, call.state, Input::Timeout)
        {
            call.set(next);
            expired.push(sid.clone());
        }
    }
    for sid in expired {
        let call = &ctx.state.jmi.calls[&sid];
        // An unanswered proposal of ours goes away for the peer too.
        if call.direction == CallDirection::Outgoing {
            let retract = simple("retract", &sid, reason_element(CallReason::Cancel, false));
            let to = Jid::from(call.peer.clone());
            ctx.send(message(to, retract));
        }
        let call = &ctx.state.jmi.calls[&sid];
        let peer = call.peer.to_string();
        ended(ctx, &sid, peer, CallEnd::TimedOut, None);
    }
    ctx.state
        .jmi
        .calls
        .retain(|_, c| !(matches!(c.state, CallState::Ended(_)) && c.ticks >= ENDED_KEEP_TICKS));
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::stanza::Stanza;

    use super::*;
    use crate::features::testing::Harness;
    use crate::features::{Effect, on_command, on_message as features_on_message};

    const SID: &str = "ca3cf894-5325-482f-a412-a6e9f832298d";

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn incoming(from: &str, inner: &str) -> Message {
        let mut m = Message::chat(Jid::new("alice@chord.localhost/chord").unwrap());
        m.from = Some(Jid::new(from).unwrap());
        m.payloads.push(el(inner));
        m
    }

    fn propose_xml(sid: &str) -> String {
        format!(
            "<propose xmlns='urn:xmpp:jingle-message:0' id='{sid}'>\
             <description xmlns='urn:xmpp:jingle:apps:rtp:1' media='audio'/></propose>"
        )
    }

    fn simple_xml(name: &str, sid: &str) -> String {
        format!("<{name} xmlns='urn:xmpp:jingle-message:0' id='{sid}'/>")
    }

    /// The events that the features emitted, in order. The sent stanzas stay.
    fn events(h: &mut Harness) -> Vec<CallEvent> {
        let mut out = Vec::new();
        let mut keep = Vec::new();
        for effect in std::mem::take(&mut h.effects) {
            match effect {
                Effect::Emit(ClientEvent::Call(e)) => out.push(e),
                other => keep.push(other),
            }
        }
        h.effects = keep;
        out
    }

    fn sent(h: &mut Harness) -> Vec<Message> {
        h.take_sent()
            .into_iter()
            .filter_map(|s| match s {
                Stanza::Message(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    fn recv(h: &mut Harness, message: Message) {
        h.with_ctx(|ctx| assert!(on_message(ctx, &message)));
    }

    fn command<T>(
        h: &mut Harness,
        make: impl FnOnce(Reply<T>) -> Command,
    ) -> Result<T, ClientError> {
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, FeatureCommand::Jmi(make(reply))));
        answer.try_recv().unwrap().unwrap()
    }

    fn propose_to(h: &mut Harness, to: &str) -> String {
        command(h, |reply| Command::Propose {
            to: BareJid::new(to).unwrap(),
            media: vec!["audio".into()],
            reply,
        })
        .unwrap()
    }

    /// The name and the `id` of the JMI element of a message.
    fn jmi_of(m: &Message) -> (String, String) {
        let e = m.payloads.iter().find(|p| p.ns() == NS_JMI).unwrap();
        (e.name().to_owned(), e.attr("id").unwrap().to_owned())
    }

    #[test]
    fn parses_the_propose_of_the_xep() {
        let m = incoming("juliet@capulet.example/phone", &propose_xml(SID));
        assert_eq!(
            parse(&m),
            Some(Jmi::Propose {
                sid: SID.into(),
                media: vec!["audio".into()]
            })
        );
    }

    #[test]
    fn parses_the_other_elements() {
        let m = incoming("a@b/c", &simple_xml("ringing", SID));
        assert_eq!(parse(&m), Some(Jmi::Ringing { sid: SID.into() }));
        let m = incoming("a@b/c", &simple_xml("proceed", SID));
        assert_eq!(parse(&m), Some(Jmi::Proceed { sid: SID.into() }));
        let finish = format!(
            "<finish xmlns='urn:xmpp:jingle-message:0' id='{SID}'>\
             <reason xmlns='urn:xmpp:jingle:1'><success/><text>Success</text></reason>\
             <migrated to='new'/></finish>"
        );
        assert_eq!(
            parse(&incoming("a@b/c", &finish)),
            Some(Jmi::Finish {
                sid: SID.into(),
                reason: Some("success".into()),
                migrated_to: Some("new".into())
            })
        );
        let reject = format!(
            "<reject xmlns='urn:xmpp:jingle-message:0' id='{SID}'><tie-break/>\
             <reason xmlns='urn:xmpp:jingle:1'><expired/></reason></reject>"
        );
        assert_eq!(
            parse(&incoming("a@b/c", &reject)),
            Some(Jmi::Reject {
                sid: SID.into(),
                reason: Some("expired".into()),
                tie_break: true
            })
        );
    }

    #[test]
    fn a_propose_without_media_or_id_is_not_valid() {
        let no_media = "<propose xmlns='urn:xmpp:jingle-message:0' id='x'/>";
        assert_eq!(parse(&incoming("a@b/c", no_media)), None);
        let no_id = "<proceed xmlns='urn:xmpp:jingle-message:0'/>";
        assert_eq!(parse(&incoming("a@b/c", no_id)), None);
    }

    #[test]
    fn state_machine_outgoing() {
        use CallDirection::Outgoing as O;
        use CallState::*;
        assert_eq!(next_state(O, Proposed, Input::RemoteRinging), Some(Ringing));
        assert_eq!(next_state(O, Ringing, Input::RemoteProceed), Some(Accepted));
        assert_eq!(
            next_state(O, Proposed, Input::RemoteProceed),
            Some(Accepted)
        );
        assert_eq!(
            next_state(O, Ringing, Input::RemoteReject),
            Some(Ended(CallEnd::Rejected))
        );
        assert_eq!(
            next_state(O, Proposed, Input::LocalRetract),
            Some(Ended(CallEnd::Retracted))
        );
        assert_eq!(
            next_state(O, Accepted, Input::RemoteFinish),
            Some(Ended(CallEnd::Finished))
        );
        // A retract after the peer proceeded is not valid. Neither is a ring after the end.
        assert_eq!(next_state(O, Accepted, Input::LocalRetract), None);
        assert_eq!(
            next_state(O, Ended(CallEnd::Rejected), Input::RemoteRinging),
            None
        );
        // The initiator cannot accept its own call.
        assert_eq!(next_state(O, Proposed, Input::LocalAccept), None);
    }

    #[test]
    fn state_machine_incoming() {
        use CallDirection::Incoming as I;
        use CallState::*;
        assert_eq!(next_state(I, Proposed, Input::LocalRing), Some(Ringing));
        assert_eq!(next_state(I, Ringing, Input::LocalAccept), Some(Accepted));
        assert_eq!(
            next_state(I, Ringing, Input::LocalReject),
            Some(Ended(CallEnd::Rejected))
        );
        assert_eq!(
            next_state(I, Ringing, Input::RemoteRetract),
            Some(Ended(CallEnd::Retracted))
        );
        assert_eq!(
            next_state(I, Proposed, Input::ElsewhereAnswered),
            Some(Ended(CallEnd::HandledElsewhere))
        );
        assert_eq!(
            next_state(I, Proposed, Input::Timeout),
            Some(Ended(CallEnd::TimedOut))
        );
        assert_eq!(next_state(I, Accepted, Input::RemoteRetract), None);
        assert_eq!(next_state(I, Accepted, Input::LocalReject), None);
        assert_eq!(
            next_state(I, Accepted, Input::LocalFinish),
            Some(Ended(CallEnd::Finished))
        );
    }

    #[test]
    fn propose_sends_the_message_of_the_xep() {
        let mut h = Harness::new();
        let sid = propose_to(&mut h, "juliet@capulet.example");
        let sent = sent(&mut h);
        assert_eq!(sent.len(), 1);
        let m = &sent[0];
        assert_eq!(m.type_, MessageType::Chat);
        // A bare JID, so that the server tells all devices.
        assert_eq!(m.to.as_ref().unwrap().to_string(), "juliet@capulet.example");
        assert!(m.id.is_some());
        assert!(m.payloads.iter().any(|p| p.is("store", NS_HINTS)));
        let propose = m.payloads.iter().find(|p| p.is("propose", NS_JMI)).unwrap();
        assert_eq!(propose.attr("id"), Some(sid.as_str()));
        let d = propose.get_child("description", NS_RTP).unwrap();
        assert_eq!(d.attr("media"), Some("audio"));
        // The id is a UUID.
        assert!(uuid::Uuid::parse_str(&sid).is_ok());
    }

    #[test]
    fn propose_refuses_bad_input() {
        let mut h = Harness::new();
        let own = command(&mut h, |reply| Command::Propose {
            to: BareJid::new("alice@chord.localhost").unwrap(),
            media: vec!["audio".into()],
            reply,
        });
        assert!(matches!(own, Err(ClientError::Invalid(_))));
        let media = command(&mut h, |reply| Command::Propose {
            to: BareJid::new("bob@chord.localhost").unwrap(),
            media: vec!["hologram".into()],
            reply,
        });
        assert!(matches!(media, Err(ClientError::Invalid(_))));
        assert!(sent(&mut h).is_empty());
    }

    #[test]
    fn outgoing_call_rings_and_proceeds() {
        let mut h = Harness::new();
        let sid = propose_to(&mut h, "bob@chord.localhost");
        sent(&mut h);
        recv(
            &mut h,
            incoming("bob@chord.localhost/phone", &simple_xml("ringing", &sid)),
        );
        recv(
            &mut h,
            incoming("bob@chord.localhost/phone", &simple_xml("proceed", &sid)),
        );
        assert_eq!(
            events(&mut h),
            vec![
                CallEvent::Ringing {
                    sid: sid.clone(),
                    from: "bob@chord.localhost/phone".into()
                },
                CallEvent::Proceeded {
                    sid: sid.clone(),
                    from: "bob@chord.localhost/phone".into()
                },
            ]
        );
        // Finish goes to the full JID of the device that proceeded.
        command(&mut h, |reply| Command::Finish {
            sid: sid.clone(),
            reason: None,
            reply,
        })
        .unwrap();
        let out = sent(&mut h);
        assert_eq!(
            out[0].to.as_ref().unwrap().to_string(),
            "bob@chord.localhost/phone"
        );
        assert_eq!(jmi_of(&out[0]).0, "finish");
    }

    #[test]
    fn outgoing_call_is_rejected() {
        let mut h = Harness::new();
        let sid = propose_to(&mut h, "bob@chord.localhost");
        let reject = format!(
            "<reject xmlns='urn:xmpp:jingle-message:0' id='{sid}'>\
             <reason xmlns='urn:xmpp:jingle:1'><busy/></reason></reject>"
        );
        recv(&mut h, incoming("bob@chord.localhost/phone", &reject));
        assert_eq!(
            events(&mut h),
            vec![CallEvent::Ended {
                sid: sid.clone(),
                peer: "bob@chord.localhost".into(),
                end: CallEnd::Rejected,
                reason: Some("busy".into())
            }]
        );
        // A late proceed changes nothing.
        recv(
            &mut h,
            incoming("bob@chord.localhost/phone", &simple_xml("proceed", &sid)),
        );
        assert!(events(&mut h).is_empty());
    }

    #[test]
    fn retract_goes_to_the_bare_jid() {
        let mut h = Harness::new();
        let sid = propose_to(&mut h, "bob@chord.localhost");
        sent(&mut h);
        command(&mut h, |reply| Command::Retract {
            sid: sid.clone(),
            reply,
        })
        .unwrap();
        let out = sent(&mut h);
        assert_eq!(
            out[0].to.as_ref().unwrap().to_string(),
            "bob@chord.localhost"
        );
        assert_eq!(jmi_of(&out[0]).0, "retract");
        assert!(out[0].payloads.iter().any(|p| p.is("store", NS_HINTS)));
        let e = out[0].payloads.iter().find(|p| p.ns() == NS_JMI).unwrap();
        assert!(
            e.get_child("reason", NS_JINGLE)
                .unwrap()
                .has_child("cancel", NS_JINGLE)
        );
        // A second retract is not valid.
        let again = command(&mut h, |reply| Command::Retract { sid, reply });
        assert!(matches!(again, Err(ClientError::Invalid(_))));
    }

    #[test]
    fn incoming_call_rings_and_proceeds_to_the_full_jid() {
        let mut h = Harness::new();
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(SID)),
        );
        assert_eq!(
            events(&mut h),
            vec![CallEvent::Incoming {
                sid: SID.into(),
                from: "bob@chord.localhost/laptop".into(),
                media: vec!["audio".into()]
            }]
        );
        command(&mut h, |reply| Command::Ring {
            sid: SID.into(),
            reply,
        })
        .unwrap();
        command(&mut h, |reply| Command::Accept {
            sid: SID.into(),
            reply,
        })
        .unwrap();
        let out = sent(&mut h);
        assert_eq!(out.len(), 2);
        for m in &out {
            assert_eq!(
                m.to.as_ref().unwrap().to_string(),
                "bob@chord.localhost/laptop"
            );
            assert_eq!(m.type_, MessageType::Chat);
            assert!(m.payloads.iter().any(|p| p.is("store", NS_HINTS)));
        }
        assert_eq!(jmi_of(&out[0]).0, "ringing");
        assert_eq!(jmi_of(&out[1]).0, "proceed");
        // Chord never sends the old accept message.
        assert!(out.iter().all(|m| jmi_of(m).0 != "accept"));
    }

    #[test]
    fn incoming_call_is_rejected_with_busy_by_default() {
        let mut h = Harness::new();
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(SID)),
        );
        events(&mut h);
        command(&mut h, |reply| Command::Reject {
            sid: SID.into(),
            reason: None,
            reply,
        })
        .unwrap();
        let out = sent(&mut h);
        assert_eq!(jmi_of(&out[0]).0, "reject");
        let e = out[0].payloads.iter().find(|p| p.ns() == NS_JMI).unwrap();
        assert!(
            e.get_child("reason", NS_JINGLE)
                .unwrap()
                .has_child("busy", NS_JINGLE)
        );
        // The call cannot be accepted after that.
        let late = command(&mut h, |reply| Command::Accept {
            sid: SID.into(),
            reply,
        });
        assert!(matches!(late, Err(ClientError::Invalid(_))));
    }

    #[test]
    fn retract_of_the_peer_ends_an_incoming_call() {
        let mut h = Harness::new();
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(SID)),
        );
        events(&mut h);
        recv(
            &mut h,
            incoming("bob@chord.localhost", &simple_xml("retract", SID)),
        );
        assert_eq!(
            events(&mut h),
            vec![CallEvent::Ended {
                sid: SID.into(),
                peer: "bob@chord.localhost".into(),
                end: CallEnd::Retracted,
                reason: None
            }]
        );
    }

    #[test]
    fn a_third_party_cannot_end_a_call() {
        let mut h = Harness::new();
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(SID)),
        );
        events(&mut h);
        recv(
            &mut h,
            incoming("mallory@evil.example/x", &simple_xml("retract", SID)),
        );
        assert!(events(&mut h).is_empty());
        let calls = command(&mut h, |reply| Command::List { reply }).unwrap();
        assert_eq!(calls[0].state, CallState::Proposed);
    }

    #[test]
    fn another_device_of_ours_stops_the_ringing() {
        let mut h = Harness::new();
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(SID)),
        );
        events(&mut h);
        // The carbon of the proceed that our phone sent. Carbons already unwrapped it.
        recv(
            &mut h,
            incoming("alice@chord.localhost/phone", &simple_xml("proceed", SID)),
        );
        assert_eq!(
            events(&mut h),
            vec![CallEvent::Ended {
                sid: SID.into(),
                peer: "bob@chord.localhost".into(),
                end: CallEnd::HandledElsewhere,
                reason: None
            }]
        );
    }

    #[test]
    fn an_own_propose_from_another_device_does_not_ring() {
        let mut h = Harness::new();
        recv(
            &mut h,
            incoming("alice@chord.localhost/phone", &propose_xml(SID)),
        );
        assert!(events(&mut h).is_empty());
    }

    #[test]
    fn a_delayed_propose_does_not_ring() {
        let mut h = Harness::new();
        let mut m = incoming("bob@chord.localhost/laptop", &propose_xml(SID));
        m.payloads.push(el(
            "<delay xmlns='urn:xmpp:delay' from='chord.localhost' stamp='2020-01-01T00:00:00Z'/>",
        ));
        recv(&mut h, m);
        assert!(events(&mut h).is_empty());
        assert!(
            command(&mut h, |reply| Command::List { reply })
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_repeated_propose_rings_once() {
        let mut h = Harness::new();
        for _ in 0..2 {
            recv(
                &mut h,
                incoming("bob@chord.localhost/laptop", &propose_xml(SID)),
            );
        }
        assert_eq!(events(&mut h).len(), 1);
    }

    #[test]
    fn tie_break_the_lower_session_id_wins() {
        // Their id is lower: we retract ours with a tie-break and take theirs.
        let mut h = Harness::new();
        let ours = propose_to(&mut h, "bob@chord.localhost");
        sent(&mut h);
        let theirs = "00000000-0000-4000-8000-000000000000";
        assert!(theirs < ours.as_str());
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(theirs)),
        );
        let out = sent(&mut h);
        assert_eq!(jmi_of(&out[0]), ("retract".into(), ours.clone()));
        let e = out[0].payloads.iter().find(|p| p.ns() == NS_JMI).unwrap();
        assert!(e.has_child("tie-break", NS_JMI));
        assert!(
            e.get_child("reason", NS_JINGLE)
                .unwrap()
                .has_child("expired", NS_JINGLE)
        );
        let evs = events(&mut h);
        assert!(matches!(
            &evs[0],
            CallEvent::Ended {
                end: CallEnd::TieBreak,
                ..
            }
        ));
        assert!(matches!(&evs[1], CallEvent::Incoming { sid, .. } if sid == theirs));

        // Their id is higher: we reject it with a tie-break and keep ours.
        let mut h = Harness::new();
        let ours = propose_to(&mut h, "bob@chord.localhost");
        sent(&mut h);
        let theirs = "ffffffff-ffff-4fff-8fff-ffffffffffff";
        assert!(theirs > ours.as_str());
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(theirs)),
        );
        let out = sent(&mut h);
        assert_eq!(jmi_of(&out[0]), ("reject".into(), theirs.into()));
        assert_eq!(
            out[0].to.as_ref().unwrap().to_string(),
            "bob@chord.localhost/laptop"
        );
        let e = out[0].payloads.iter().find(|p| p.ns() == NS_JMI).unwrap();
        assert!(e.has_child("tie-break", NS_JMI));
        assert!(events(&mut h).is_empty());
    }

    #[test]
    fn a_new_call_replaces_an_accepted_call_of_the_same_peer() {
        let mut h = Harness::new();
        recv(
            &mut h,
            incoming("bob@chord.localhost/laptop", &propose_xml(SID)),
        );
        command(&mut h, |reply| Command::Accept {
            sid: SID.into(),
            reply,
        })
        .unwrap();
        events(&mut h);
        sent(&mut h);
        let new = "11111111-1111-4111-8111-111111111111";
        recv(
            &mut h,
            incoming("bob@chord.localhost/phone", &propose_xml(new)),
        );
        let out = sent(&mut h);
        assert_eq!(jmi_of(&out[0]), ("finish".into(), SID.into()));
        let e = out[0].payloads.iter().find(|p| p.ns() == NS_JMI).unwrap();
        assert_eq!(
            e.get_child("migrated", NS_JMI).unwrap().attr("to"),
            Some(new)
        );
        let evs = events(&mut h);
        assert!(matches!(
            &evs[0],
            CallEvent::Ended {
                end: CallEnd::Migrated,
                ..
            }
        ));
        assert!(matches!(&evs[1], CallEvent::Incoming { .. }));
    }

    #[test]
    fn unanswered_calls_time_out() {
        let mut h = Harness::new();
        let sid = propose_to(&mut h, "bob@chord.localhost");
        recv(
            &mut h,
            incoming("carol@chord.localhost/x", &propose_xml(SID)),
        );
        sent(&mut h);
        events(&mut h);
        for _ in 0..OUTGOING_TIMEOUT_TICKS {
            h.with_ctx(on_tick);
        }
        // Our proposal times out and goes away for the peer too.
        let out = sent(&mut h);
        assert_eq!(jmi_of(&out[0]), ("retract".into(), sid.clone()));
        let evs = events(&mut h);
        assert_eq!(evs.len(), 1);
        assert!(matches!(
            &evs[0],
            CallEvent::Ended {
                end: CallEnd::TimedOut,
                ..
            }
        ));
        for _ in OUTGOING_TIMEOUT_TICKS..INCOMING_TIMEOUT_TICKS {
            h.with_ctx(on_tick);
        }
        let evs = events(&mut h);
        assert!(
            matches!(&evs[0], CallEvent::Ended { sid: s, end: CallEnd::TimedOut, .. } if s == SID)
        );
        // Ended calls leave the table later.
        for _ in 0..ENDED_KEEP_TICKS {
            h.with_ctx(on_tick);
        }
        assert!(
            command(&mut h, |reply| Command::List { reply })
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn the_hook_in_the_message_path_keeps_call_messages_out_of_the_chat() {
        let mut h = Harness::new();
        let m = incoming("bob@chord.localhost/laptop", &propose_xml(SID));
        h.with_ctx(|ctx| features_on_message(ctx, m));
        assert_eq!(events(&mut h).len(), 1);
        // No chat message reached the store or the views.
        assert!(h.take_dirty().is_empty());
    }
}
