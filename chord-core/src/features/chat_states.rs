//! Chat state notifications (XEP-0085): typing indicators.
//!
//! Incoming: `on_message` reads the chat state of each live message. A `composing` state
//! puts the sender in the typing set of the conversation. Any other state, and any message
//! with a body, takes the sender out. A typer expires after `EXPIRE` without a new
//! `composing` (`on_tick`). The set changes send `ClientEvent::Typing`. Messages from the
//! archive do not come here, because they are not live. A message that carries only a
//! state has no body, so `chat` and `muc` never store it.
//!
//! Outgoing: `ClientHandle::set_typing` sends `composing` and `paused`. Every message with
//! a body carries `<active/>` (XEP-0085, section 5.4). We send states to a chat peer only
//! after that peer sent us a state in this session, as XEP-0085 section 5.1 asks. We send
//! them to a joined room at all times (section 5.5). We never send one state twice in a row.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use jid::{BareJid, Jid};
use xmpp_parsers::chatstates::ChatState;
use xmpp_parsers::message::{Message, MessageType};
use xmpp_parsers::minidom::Element;

use super::{Ctx, FeatureCommand, chat, muc};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::session::TICK;

/// A typer that sends no new `composing` for this long is no typer any more.
const EXPIRE: Duration = Duration::from_secs(30);

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// Peer (as `messages.peer` stores it) to typers, each with the ticks since the last
    /// `composing`. A typer is a bare JID in a chat, and a nick in a room.
    typing: HashMap<String, HashMap<String, u32>>,
    /// The peers that sent us a chat state in this session.
    supported: HashSet<String>,
    /// The last state that we sent to each peer.
    last_sent: HashMap<String, ChatState>,
}

/// A command from the public API.
pub(crate) enum Command {
    SetTyping { peer: String, typing: bool },
}

impl ClientHandle {
    /// Tell `peer` that we start or stop to type (XEP-0085). `peer` is a bare JID of a
    /// chat or a room, or room@service/nick for a private message in a room.
    ///
    /// The UI calls `set_typing(peer, true)` on input, and `set_typing(peer, false)` after
    /// 5 seconds without input, or when the user empties the input. Repeat calls with the
    /// same value send nothing. `true` sends `<composing/>`. `false` sends `<paused/>`
    /// only after a `<composing/>`.
    ///
    /// A chat or a private message gets the state only after the peer sent us a state in
    /// this session. A room gets it when we are in the room. Otherwise the call does
    /// nothing and returns no error.
    pub fn set_typing(&self, peer: String, typing: bool) -> Result<(), ClientError> {
        Jid::new(&peer).map_err(|e| ClientError::Invalid(format!("bad JID {peer}: {e}")))?;
        self.feature(FeatureCommand::ChatStates(Command::SetTyping {
            peer,
            typing,
        }))
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::SetTyping { peer, typing } => set_typing(ctx, &peer, typing),
    }
}

/// A command while no session is up: nothing to send.
pub(crate) fn offline(command: Command) {
    let Command::SetTyping { .. } = command;
}

/// The chat state of a message.
fn state_of(message: &Message) -> Option<ChatState> {
    // xmpp-parsers 0.23 chatstates.rs:12-32: one enum with the five elements.
    message
        .payloads
        .iter()
        .find_map(|p| ChatState::try_from(p.clone()).ok())
}

/// Payloads for every outgoing message with a body: `<active/>`.
pub(crate) fn outgoing_payloads(_ctx: &mut Ctx<'_>) -> Vec<Element> {
    vec![ChatState::Active.into()]
}

/// A message with a body went out to `peer`. Its `<active/>` is the last state now.
pub(crate) fn on_sent(ctx: &mut Ctx<'_>, peer: &str) {
    ctx.state
        .chat_states
        .last_sent
        .insert(peer.to_owned(), ChatState::Active);
}

/// Where a live message comes from: the peer as `messages.peer` stores it, and the typer.
fn source(ctx: &Ctx<'_>, message: &Message) -> Option<(String, String)> {
    let from = message.from.as_ref()?;
    match message.type_ {
        MessageType::Groupchat => {
            let room = from.to_bare();
            let nick = from.resource()?.as_str();
            if !muc::is_room(ctx, &room) || muc::is_ours(ctx, &room, nick, message) {
                return None;
            }
            Some((room.to_string(), nick.to_owned()))
        }
        MessageType::Chat | MessageType::Normal => {
            let bare = from.to_bare();
            // A carbon of our own typing.
            if bare == *ctx.account {
                return None;
            }
            match from.resource() {
                Some(nick) if muc::is_room(ctx, &bare) => {
                    Some((format!("{bare}/{nick}"), nick.as_str().to_owned()))
                }
                _ => Some((bare.to_string(), bare.to_string())),
            }
        }
        _ => None,
    }
}

/// A live message. Read its chat state, and take a sender with a body out of the typers.
/// The message goes on to `chat` and `muc`.
pub(crate) fn on_message(ctx: &mut Ctx<'_>, message: &Message) {
    let state = state_of(message);
    let has_body = message.bodies.values().any(|b| !b.is_empty());
    // A delayed message is history, not news.
    if (state.is_none() && !has_body) || chat::delay_ms(message).is_some() {
        return;
    }
    let Some((peer, typer)) = source(ctx, message) else {
        return;
    };
    if state.is_some() {
        ctx.state.chat_states.supported.insert(peer.clone());
    }
    if has_body || !matches!(state, Some(ChatState::Composing)) {
        remove(ctx, &peer, &typer);
        return;
    }
    let typers = ctx
        .state
        .chat_states
        .typing
        .entry(peer.clone())
        .or_default();
    let is_new = typers.insert(typer, 0).is_none();
    if is_new {
        report(ctx, &peer);
    }
}

fn remove(ctx: &mut Ctx<'_>, peer: &str, typer: &str) {
    let Some(typers) = ctx.state.chat_states.typing.get_mut(peer) else {
        return;
    };
    if typers.remove(typer).is_none() {
        return;
    }
    if typers.is_empty() {
        ctx.state.chat_states.typing.remove(peer);
    }
    report(ctx, peer);
}

/// Send the typers of a conversation to the frontends.
fn report(ctx: &mut Ctx<'_>, peer: &str) {
    let mut typers: Vec<String> = ctx
        .state
        .chat_states
        .typing
        .get(peer)
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default();
    typers.sort();
    ctx.emit(ClientEvent::Typing {
        peer: peer.to_owned(),
        typers,
    });
}

/// A session tick. Expire the typers that sent no `composing` for `EXPIRE`.
pub(crate) fn on_tick(ctx: &mut Ctx<'_>) {
    if ctx.state.chat_states.typing.is_empty() {
        return;
    }
    let limit = (EXPIRE.as_secs() / TICK.as_secs()).max(1) as u32;
    let mut changed = Vec::new();
    ctx.state.chat_states.typing.retain(|peer, typers| {
        let before = typers.len();
        typers.retain(|_, age| {
            *age += 1;
            *age < limit
        });
        if typers.len() != before {
            changed.push(peer.clone());
        }
        !typers.is_empty()
    });
    changed.sort();
    for peer in changed {
        report(ctx, &peer);
    }
}

/// A new session starts and drops the state. Tell the frontends that nobody types.
pub(crate) fn on_new_session(ctx: &mut Ctx<'_>) {
    let peers: Vec<String> = ctx.state.chat_states.typing.keys().cloned().collect();
    ctx.state.chat_states.typing.clear();
    for peer in peers {
        report(ctx, &peer);
    }
}

fn set_typing(ctx: &mut Ctx<'_>, peer: &str, typing: bool) {
    let Ok(jid) = Jid::new(peer) else {
        return;
    };
    let bare: BareJid = jid.to_bare();
    let state = if typing {
        ChatState::Composing
    } else {
        ChatState::Paused
    };
    let is_room = muc::is_room(ctx, &bare);
    let message = if is_room && jid.resource().is_none() {
        // A room relays the state to all occupants (XEP-0085, section 5.5).
        if !ctx.state.muc.nicks.contains_key(&bare) {
            return;
        }
        Message::groupchat(Jid::from(bare))
    } else if is_room {
        if !ctx.state.muc.nicks.contains_key(&bare)
            || !ctx.state.chat_states.supported.contains(peer)
        {
            return;
        }
        let mut m = Message::chat(jid.clone());
        m.payloads
            .push(Element::builder("x", muc::NS_MUC_USER).build());
        m
    } else {
        if !ctx.state.chat_states.supported.contains(peer) {
            return;
        }
        Message::chat(Jid::from(bare))
    };
    let last = ctx.state.chat_states.last_sent.get(peer);
    if last == Some(&state) {
        return;
    }
    // A stop makes sense only after a start.
    if !typing && last != Some(&ChatState::Composing) {
        return;
    }
    ctx.state
        .chat_states
        .last_sent
        .insert(peer.to_owned(), state.clone());
    ctx.send(message.with_payload(state));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::features::{Effect, on_message as dispatch_message};
    use rusqlite::params;
    use xmpp_parsers::stanza::Stanza;

    const BOB: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    fn from_bob(state: Option<ChatState>, body: Option<&str>) -> Message {
        incoming(MessageType::Chat, &format!("{BOB}/phone"), state, body)
    }

    fn incoming(
        kind: MessageType,
        from: &str,
        state: Option<ChatState>,
        body: Option<&str>,
    ) -> Message {
        let mut m = Message::new(Some(Jid::new("alice@chord.localhost/chord").unwrap()));
        m.type_ = kind;
        m.from = Some(Jid::new(from).unwrap());
        if let Some(state) = state {
            m.payloads.push(state.into());
        }
        if let Some(body) = body {
            m.bodies.insert(Default::default(), body.into());
        }
        m.id = Some(xmpp_parsers::message::Id("m1".into()));
        m
    }

    fn deliver(h: &mut Harness, m: Message) {
        h.with_ctx(|ctx| dispatch_message(ctx, m));
    }

    fn typing_events(h: &mut Harness) -> Vec<(String, Vec<String>)> {
        let effects = std::mem::take(&mut h.effects);
        let mut out = Vec::new();
        for e in effects {
            match e {
                Effect::Emit(ClientEvent::Typing { peer, typers }) => out.push((peer, typers)),
                other => h.effects.push(other),
            }
        }
        out
    }

    fn add_room(h: &mut Harness, joined: bool) {
        h.store
            .conn()
            .execute(
                "INSERT INTO rooms (account_id, jid, nick) VALUES (?1, ?2, 'alice')",
                params![h.account_id, ROOM],
            )
            .unwrap();
        if joined {
            h.state
                .muc
                .nicks
                .insert(BareJid::new(ROOM).unwrap(), "alice".into());
        }
    }

    fn message_count(h: &Harness) -> i64 {
        h.store
            .conn()
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn composing_then_paused() {
        let mut h = Harness::new();
        deliver(&mut h, from_bob(Some(ChatState::Composing), None));
        assert_eq!(
            typing_events(&mut h),
            [(BOB.to_owned(), vec![BOB.to_owned()])]
        );
        // A second composing changes nothing.
        deliver(&mut h, from_bob(Some(ChatState::Composing), None));
        assert!(typing_events(&mut h).is_empty());
        deliver(&mut h, from_bob(Some(ChatState::Paused), None));
        assert_eq!(typing_events(&mut h), [(BOB.to_owned(), vec![])]);
        // Nothing to clear now.
        deliver(&mut h, from_bob(Some(ChatState::Gone), None));
        assert!(typing_events(&mut h).is_empty());
    }

    #[test]
    fn a_body_clears_the_typer() {
        let mut h = Harness::new();
        deliver(&mut h, from_bob(Some(ChatState::Composing), None));
        typing_events(&mut h);
        let mut m = from_bob(None, Some("hi"));
        m.payloads.push(
            "<origin-id xmlns='urn:xmpp:sid:0' id='o1'/>"
                .parse::<Element>()
                .unwrap(),
        );
        deliver(&mut h, m);
        assert_eq!(typing_events(&mut h), [(BOB.to_owned(), vec![])]);
        assert_eq!(message_count(&h), 1);
    }

    #[test]
    fn a_typer_expires_after_30_seconds() {
        let mut h = Harness::new();
        deliver(&mut h, from_bob(Some(ChatState::Composing), None));
        typing_events(&mut h);
        let ticks = (EXPIRE.as_secs() / TICK.as_secs()) as usize;
        for _ in 0..ticks - 1 {
            h.with_ctx(on_tick);
        }
        assert!(typing_events(&mut h).is_empty());
        h.with_ctx(on_tick);
        assert_eq!(typing_events(&mut h), [(BOB.to_owned(), vec![])]);
    }

    #[test]
    fn a_new_composing_restarts_the_expiry() {
        let mut h = Harness::new();
        deliver(&mut h, from_bob(Some(ChatState::Composing), None));
        h.with_ctx(on_tick);
        deliver(&mut h, from_bob(Some(ChatState::Composing), None));
        h.with_ctx(on_tick);
        typing_events(&mut h);
        assert!(h.state.chat_states.typing.contains_key(BOB));
    }

    #[test]
    fn our_own_carbon_is_ignored() {
        let mut h = Harness::new();
        let m = incoming(
            MessageType::Chat,
            "alice@chord.localhost/phone",
            Some(ChatState::Composing),
            None,
        );
        deliver(&mut h, m);
        assert!(typing_events(&mut h).is_empty());
    }

    #[test]
    fn a_delayed_state_is_ignored() {
        let mut h = Harness::new();
        let mut m = from_bob(Some(ChatState::Composing), None);
        m.payloads.push(
            "<delay xmlns='urn:xmpp:delay' from='chord.localhost' stamp='2024-01-01T00:00:00Z'/>"
                .parse::<Element>()
                .unwrap(),
        );
        deliver(&mut h, m);
        assert!(typing_events(&mut h).is_empty());
    }

    #[test]
    fn a_state_from_the_archive_is_ignored() {
        // Archive results reach `store_archived`, never `on_message` here. A state-only
        // result stores no row and emits nothing.
        let mut h = Harness::new();
        let m = from_bob(Some(ChatState::Composing), None);
        h.with_ctx(|ctx| chat::store_archived(ctx, &m, "arch1", Some(1)));
        assert!(typing_events(&mut h).is_empty());
        assert_eq!(message_count(&h), 0);
    }

    #[test]
    fn room_composing_by_nick() {
        let mut h = Harness::new();
        add_room(&mut h, true);
        let m = incoming(
            MessageType::Groupchat,
            &format!("{ROOM}/bob"),
            Some(ChatState::Composing),
            None,
        );
        deliver(&mut h, m);
        assert_eq!(
            typing_events(&mut h),
            [(ROOM.to_owned(), vec!["bob".to_owned()])]
        );
        // Our own echo is ignored.
        let m = incoming(
            MessageType::Groupchat,
            &format!("{ROOM}/alice"),
            Some(ChatState::Composing),
            None,
        );
        deliver(&mut h, m);
        assert!(typing_events(&mut h).is_empty());
        assert_eq!(message_count(&h), 0);
    }

    #[test]
    fn room_private_message_uses_the_occupant_peer() {
        let mut h = Harness::new();
        add_room(&mut h, true);
        let from = format!("{ROOM}/bob");
        deliver(
            &mut h,
            incoming(MessageType::Chat, &from, Some(ChatState::Composing), None),
        );
        assert_eq!(typing_events(&mut h), [(from, vec!["bob".to_owned()])]);
    }

    #[test]
    fn a_state_message_is_not_stored() {
        let mut h = Harness::new();
        add_room(&mut h, true);
        deliver(&mut h, from_bob(Some(ChatState::Composing), None));
        deliver(&mut h, from_bob(Some(ChatState::Active), None));
        deliver(
            &mut h,
            incoming(
                MessageType::Groupchat,
                &format!("{ROOM}/bob"),
                Some(ChatState::Paused),
                None,
            ),
        );
        assert_eq!(message_count(&h), 0);
    }

    fn sent_states(h: &mut Harness) -> Vec<(MessageType, ChatState)> {
        h.take_sent()
            .into_iter()
            .filter_map(|s| match s {
                Stanza::Message(m) => state_of(&m).map(|s| (m.type_, s)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn composing_goes_out_only_after_the_peer_sent_a_state() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| set_typing(ctx, BOB, true));
        assert!(sent_states(&mut h).is_empty());
        deliver(&mut h, from_bob(Some(ChatState::Active), None));
        h.with_ctx(|ctx| set_typing(ctx, BOB, true));
        assert_eq!(
            sent_states(&mut h),
            [(MessageType::Chat, ChatState::Composing)]
        );
        h.with_ctx(|ctx| set_typing(ctx, BOB, false));
        assert_eq!(
            sent_states(&mut h),
            [(MessageType::Chat, ChatState::Paused)]
        );
    }

    #[test]
    fn the_same_state_goes_out_once() {
        let mut h = Harness::new();
        deliver(&mut h, from_bob(Some(ChatState::Active), None));
        h.with_ctx(|ctx| set_typing(ctx, BOB, true));
        h.with_ctx(|ctx| set_typing(ctx, BOB, true));
        assert_eq!(sent_states(&mut h).len(), 1);
        h.with_ctx(|ctx| set_typing(ctx, BOB, false));
        h.with_ctx(|ctx| set_typing(ctx, BOB, false));
        assert_eq!(sent_states(&mut h).len(), 1);
    }

    #[test]
    fn a_room_gets_states_only_when_joined() {
        let mut h = Harness::new();
        add_room(&mut h, false);
        h.with_ctx(|ctx| set_typing(ctx, ROOM, true));
        assert!(sent_states(&mut h).is_empty());
        h.state
            .muc
            .nicks
            .insert(BareJid::new(ROOM).unwrap(), "alice".into());
        h.with_ctx(|ctx| set_typing(ctx, ROOM, true));
        assert_eq!(
            sent_states(&mut h),
            [(MessageType::Groupchat, ChatState::Composing)]
        );
    }

    #[test]
    fn a_private_message_state_has_the_muc_user_element() {
        let mut h = Harness::new();
        add_room(&mut h, true);
        let occupant = format!("{ROOM}/bob");
        deliver(
            &mut h,
            incoming(MessageType::Chat, &occupant, Some(ChatState::Active), None),
        );
        h.with_ctx(|ctx| set_typing(ctx, &occupant, true));
        let sent = h.take_sent();
        let Stanza::Message(m) = &sent[0] else {
            panic!("no message");
        };
        assert_eq!(m.to.as_ref().unwrap().to_string(), occupant);
        assert!(m.payloads.iter().any(|p| p.is("x", muc::NS_MUC_USER)));
    }

    #[test]
    fn outgoing_body_carries_active() {
        let mut h = Harness::new();
        let to = Jid::new(BOB).unwrap();
        h.with_ctx(|ctx| chat::send(ctx, to, "hello".into()));
        let sent = h.take_sent();
        let Stanza::Message(m) = &sent[0] else {
            panic!("no message");
        };
        assert_eq!(state_of(m), Some(ChatState::Active));
        // The sent message is the last state: a stop sends nothing.
        deliver(&mut h, from_bob(Some(ChatState::Active), None));
        h.with_ctx(|ctx| set_typing(ctx, BOB, false));
        assert!(sent_states(&mut h).is_empty());
    }
}
