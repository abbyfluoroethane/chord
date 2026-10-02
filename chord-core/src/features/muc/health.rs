//! Room health: the self-ping (XEP-0410) and the time limit of a join.
//!
//! A server can drop us from a room and tell nobody, for example after a lossy reconnect
//! or a restart of the room. Then the room looks joined to us, and our messages go nowhere.
//! XEP-0410 has a check: ping our own occupant JID `room/nick`.
//!
//! - A result means that we are in the room. So does `service-unavailable` or
//!   `feature-not-implemented`: the room does not do pings, but it knows the occupant.
//! - `not-acceptable` (we are not an occupant) or `item-not-found` mean that we are out.
//!   We join again.
//! - Any other error, or no answer, says nothing. A timeout can have many causes. We do
//!   nothing and ask again later.
//!
//! When we ping: after a resumed stream, at once for every room. Then, for each room that
//! has been quiet for `SELF_PING_QUIET_TICKS` ticks. Any stanza from the room, and any
//! answer to a ping, ends the quiet time. A busy room proves itself.
//!
//! A join that gets no answer for `JOIN_TIMEOUT_TICKS` ticks fails. So does a subject
//! change that gets no echo and no error for `SUBJECT_TIMEOUT_TICKS` ticks.

use std::collections::{HashMap, HashSet};

use jid::{BareJid, Jid};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::ping::Ping;
use xmpp_parsers::presence::Presence;
use xmpp_parsers::stanza_error::{DefinedCondition, StanzaError};

use super::{Pending, clear_occupants, drop_outbox, ensure_room, join_room, mark_room, set_joined};
use crate::actor::{ClientError, ClientEvent};
use crate::features::{self, Ctx, IqResponse};

/// Ticks (about 15 s each) of silence from a room before we ping it. 20 ticks are 5 min.
pub(super) const SELF_PING_QUIET_TICKS: u32 = 20;

/// Ticks that a join waits for the self-presence. 4 ticks are 1 min.
pub(super) const JOIN_TIMEOUT_TICKS: u8 = 4;

/// Ticks that a subject change waits for the echo of the room. 4 ticks are 1 min.
pub(super) const SUBJECT_TIMEOUT_TICKS: u8 = 4;

#[derive(Debug, Default)]
pub(crate) struct PingState {
    /// Ticks since we last heard from a room, for each joined room.
    quiet: HashMap<BareJid, u32>,
    /// The rooms that have a ping on its way.
    pinging: HashSet<BareJid>,
}

/// We heard from the room: a message or a presence. It is no use to ping it now.
pub(super) fn heard_from(ctx: &mut Ctx<'_>, room: &BareJid) {
    ctx.state.muc.ping.quiet.remove(room);
}

/// Ping our occupant JID in `room`, unless a ping is on its way or we are not in the room.
fn self_ping(ctx: &mut Ctx<'_>, room: &BareJid) {
    let Some(nick) = ctx.state.muc.nicks.get(room).cloned() else {
        return;
    };
    // A join or a nick change on its way has its own answer.
    if ctx.state.muc.joins.contains_key(room) || ctx.state.muc.ping.pinging.contains(room) {
        return;
    }
    let Ok(target) = room.with_resource_str(&nick) else {
        return;
    };
    log::debug!("self-ping {room}/{nick}");
    ctx.state.muc.ping.quiet.remove(room);
    ctx.state.muc.ping.pinging.insert(room.clone());
    let iq = Iq::from_get(String::new(), Ping).with_to(Jid::from(target));
    ctx.request(iq, features::Pending::Muc(Pending::SelfPing(room.clone())));
}

/// The stream resumed. The server kept the session, but the room may not have: ping them all.
pub(crate) fn on_resumed(ctx: &mut Ctx<'_>) {
    let rooms: Vec<BareJid> = ctx.state.muc.nicks.keys().cloned().collect();
    for room in rooms {
        self_ping(ctx, &room);
    }
}

/// One tick: ping the quiet rooms, and end the joins that wait too long.
pub(crate) fn on_tick(ctx: &mut Ctx<'_>) {
    // A room that we left keeps no quiet time.
    let muc = &mut ctx.state.muc;
    muc.ping
        .quiet
        .retain(|room, _| muc.nicks.contains_key(room));
    expire_subjects(ctx);
    let rooms: Vec<BareJid> = ctx.state.muc.nicks.keys().cloned().collect();
    for room in rooms {
        let quiet = ctx.state.muc.ping.quiet.entry(room.clone()).or_insert(0);
        *quiet += 1;
        if *quiet >= SELF_PING_QUIET_TICKS {
            self_ping(ctx, &room);
        }
    }
    let mut expired = Vec::new();
    for (room, join) in &mut ctx.state.muc.joins {
        join.ticks = join.ticks.saturating_add(1);
        if join.ticks >= JOIN_TIMEOUT_TICKS {
            expired.push(room.clone());
        }
    }
    for room in expired {
        join_timed_out(ctx, &room);
    }
    super::rejoin::on_tick(ctx);
}

/// Fail the subject changes that got no echo and no error for too long. Without this, the
/// entry and its reply stay in memory when the room never answers.
fn expire_subjects(ctx: &mut Ctx<'_>) {
    let mut expired = Vec::new();
    for (id, (_, _, ticks)) in &mut ctx.state.muc.subjects {
        *ticks = ticks.saturating_add(1);
        if *ticks >= SUBJECT_TIMEOUT_TICKS {
            expired.push(id.clone());
        }
    }
    for id in expired {
        if let Some((room, reply, _)) = ctx.state.muc.subjects.remove(&id) {
            log::warn!("the subject change in {room} got no answer in time");
            let _ = reply.send(Err(ClientError::Server(
                "the room did not answer in time".into(),
            )));
        }
    }
}

/// The room did not answer a join in time. Fail the join, as a join error does. If the
/// presence went through, and only the answer is lost, we leave the room again so that the
/// server does not keep an occupant that we do not know.
fn join_timed_out(ctx: &mut Ctx<'_>, room: &BareJid) {
    let Some(join) = ctx.state.muc.joins.remove(room) else {
        return;
    };
    log::warn!("the join of {room} got no answer in time");
    if join.changing_nick {
        // Keep the nick that we have.
        if let Some(old) = ctx.state.muc.nicks.get(room).cloned() {
            ensure_room(ctx, room, Some(&old), None);
        }
    } else {
        if let Ok(full) = room.with_resource_str(&join.nick) {
            ctx.send(Presence::unavailable().with_to(Jid::from(full)));
        }
        set_joined(ctx, room, false);
    }
    for reply in join.replies {
        let _ = reply.send(Err(ClientError::Server(
            "the room did not answer in time".into(),
        )));
    }
    ctx.emit(ClientEvent::Notice(format!(
        "Cannot join {room}: no answer in time"
    )));
    drop_outbox(ctx, room);
    mark_room(ctx, room);
}

/// The answer to a self-ping.
pub(super) fn on_response(ctx: &mut Ctx<'_>, room: BareJid, response: IqResponse) {
    ctx.state.muc.ping.pinging.remove(&room);
    match response {
        IqResponse::Result(_) => log::debug!("self-ping {room}: in the room"),
        IqResponse::Error(error) if left_the_room(&error) => {
            log::info!(
                "self-ping {room}: {:?}. We are not in the room. Joining again.",
                error.defined_condition
            );
            rejoin(ctx, &room);
        }
        IqResponse::Error(error) => {
            // `service-unavailable` and `feature-not-implemented` mean "in the room, but no
            // ping". A timeout or any other error says nothing (XEP-0410, 4).
            log::debug!(
                "self-ping {room}: {:?}. Nothing to do.",
                error.defined_condition
            );
        }
        IqResponse::Lost => {}
    }
}

/// Does this error say that we are not in the room?
fn left_the_room(error: &StanzaError) -> bool {
    matches!(
        error.defined_condition,
        DefinedCondition::NotAcceptable | DefinedCondition::ItemNotFound
    )
}

/// We are not in the room, but we think so. Forget the occupants, and join again with the
/// nick and the password that we know.
fn rejoin(ctx: &mut Ctx<'_>, room: &BareJid) {
    // Another path may have changed the state since the ping went out.
    if ctx.state.muc.joins.contains_key(room) || !ctx.state.muc.nicks.contains_key(room) {
        return;
    }
    ctx.state.muc.nicks.remove(room);
    clear_occupants(ctx, room);
    set_joined(ctx, room, false);
    join_room(ctx, room, None, None, None);
}

#[cfg(test)]
mod tests {
    use futures_channel::oneshot;
    use xmpp_parsers::muc::user::{Affiliation, Item, MucUser, Role, Status};
    use xmpp_parsers::stanza::Stanza;
    use xmpp_parsers::stanza_error::ErrorType;

    use super::*;
    use crate::features::muc::{Command, on_command, on_presence};
    use crate::features::testing::Harness;

    const ROOM: &str = "dev@rooms.chord.localhost";

    fn room() -> BareJid {
        BareJid::new(ROOM).unwrap()
    }

    fn self_presence(nick: &str) -> Presence {
        Presence::available()
            .with_from(Jid::new(&format!("{ROOM}/{nick}")).unwrap())
            .with_payload(
                MucUser::new()
                    .with_statuses(vec![Status::SelfPresence])
                    .with_items(vec![Item::new(Affiliation::Member, Role::Participant)]),
            )
    }

    fn start_join(h: &mut Harness, nick: &str) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Join {
                    room: room(),
                    nick: nick.into(),
                    password: None,
                    reply,
                },
            )
        });
        answer
    }

    /// Join the room as `nick` and complete the join.
    fn joined(h: &mut Harness, nick: &str) {
        let mut answer = start_join(h, nick);
        h.with_ctx(|ctx| on_presence(ctx, &self_presence(nick)));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        h.take_sent();
        h.take_dirty();
    }

    fn is_self_ping(pending: &features::Pending) -> bool {
        matches!(pending, features::Pending::Muc(Pending::SelfPing(_)))
    }

    fn error(condition: DefinedCondition) -> IqResponse {
        IqResponse::Error(StanzaError::new(ErrorType::Cancel, condition, "en", ""))
    }

    /// The sent stanzas, as kind and target text.
    fn sent_to(h: &mut Harness) -> Vec<String> {
        h.take_sent()
            .iter()
            .map(|s| match s {
                Stanza::Iq(iq) => format!("iq {}", iq.to().unwrap()),
                Stanza::Presence(p) => format!("presence {}", p.to.as_ref().unwrap()),
                Stanza::Message(_) => "message".to_owned(),
            })
            .collect()
    }

    fn tick(h: &mut Harness, n: u32) {
        for _ in 0..n {
            h.with_ctx(on_tick);
        }
    }

    #[test]
    fn a_room_that_we_left_keeps_no_quiet_time() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        tick(&mut h, 2);
        assert_eq!(h.state.muc.ping.quiet.len(), 1);
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Leave {
                    room: room(),
                    reply,
                },
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        tick(&mut h, 1);
        assert!(h.state.muc.ping.quiet.is_empty());
    }

    #[test]
    fn a_resumed_stream_pings_our_occupant_jid_once() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        h.with_ctx(on_resumed);
        let iqs = h.sent_iqs();
        assert_eq!(iqs.len(), 1);
        let Iq::Get { to, payload, .. } = &iqs[0] else {
            panic!("expected an IQ get");
        };
        assert_eq!(to.as_ref().unwrap().to_string(), format!("{ROOM}/alice"));
        assert!(payload.is("ping", "urn:xmpp:ping"));
        // The ping is on its way. A second trigger sends nothing.
        h.with_ctx(on_resumed);
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn a_result_or_an_unhelpful_error_changes_nothing() {
        for response in [
            IqResponse::Result(None),
            error(DefinedCondition::ServiceUnavailable),
            error(DefinedCondition::FeatureNotImplemented),
            error(DefinedCondition::RemoteServerTimeout),
            error(DefinedCondition::RemoteServerNotFound),
            IqResponse::Lost,
        ] {
            let mut h = Harness::new();
            joined(&mut h, "alice");
            h.with_ctx(on_resumed);
            h.take_sent();
            h.respond(is_self_ping, response);
            assert!(h.take_sent().is_empty(), "no rejoin");
            assert_eq!(
                h.state.muc.nicks.get(&room()).map(String::as_str),
                Some("alice")
            );
            // The ping is over. A new trigger pings again.
            h.with_ctx(on_resumed);
            assert_eq!(h.sent_iqs().len(), 1);
        }
    }

    #[test]
    fn not_acceptable_and_item_not_found_join_the_room_again() {
        for condition in [
            DefinedCondition::NotAcceptable,
            DefinedCondition::ItemNotFound,
        ] {
            let mut h = Harness::new();
            joined(&mut h, "alice");
            h.with_ctx(on_resumed);
            h.take_sent();
            h.respond(is_self_ping, error(condition));
            // A join presence to our nick, with the muc payload.
            let sent = h.take_sent();
            assert_eq!(sent.len(), 1);
            let Stanza::Presence(p) = &sent[0] else {
                panic!("expected a presence");
            };
            assert_eq!(p.to.as_ref().unwrap().to_string(), format!("{ROOM}/alice"));
            assert!(
                p.payloads
                    .iter()
                    .any(|e| e.is("x", "http://jabber.org/protocol/muc"))
            );
            assert!(h.state.muc.joins.contains_key(&room()));
            assert!(!h.state.muc.nicks.contains_key(&room()));
            // The room answers, and the join completes.
            h.with_ctx(|ctx| on_presence(ctx, &self_presence("alice")));
            assert!(h.state.muc.nicks.contains_key(&room()));
        }
    }

    #[test]
    fn a_quiet_room_gets_a_ping_and_a_busy_room_does_not() {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        tick(&mut h, SELF_PING_QUIET_TICKS - 1);
        assert!(h.take_sent().is_empty());
        // A message from the room ends the quiet time.
        h.with_ctx(|ctx| heard_from(ctx, &room()));
        tick(&mut h, SELF_PING_QUIET_TICKS - 1);
        assert!(h.take_sent().is_empty());
        tick(&mut h, 1);
        assert_eq!(sent_to(&mut h), [format!("iq {ROOM}/alice")]);
        // No second ping while the first has no answer.
        tick(&mut h, SELF_PING_QUIET_TICKS);
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn a_room_that_we_are_not_in_gets_no_ping() {
        let mut h = Harness::new();
        h.with_ctx(on_resumed);
        tick(&mut h, SELF_PING_QUIET_TICKS);
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn a_join_without_an_answer_fails_and_leaves_the_room() {
        let mut h = Harness::new();
        let mut answer = start_join(&mut h, "alice");
        h.take_sent();
        tick(&mut h, u32::from(JOIN_TIMEOUT_TICKS) - 1);
        assert!(answer.try_recv().unwrap().is_none(), "still waiting");
        tick(&mut h, 1);
        let Some(Err(ClientError::Server(text))) = answer.try_recv().unwrap() else {
            panic!("expected a server error");
        };
        assert!(text.contains("in time"));
        // We tell the room that we leave, in case the join went through.
        assert_eq!(sent_to(&mut h), [format!("presence {ROOM}/alice")]);
        assert!(!h.state.muc.joins.contains_key(&room()));
    }
}
