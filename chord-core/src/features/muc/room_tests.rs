//! Tests for the room status codes, the assigned nick, invitations, the occupant-id, the
//! room password and the CAPTCHA of a join.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::params;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::{Id, Message, MessageType};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::presence::Presence;
use xmpp_parsers::stanza::Stanza;
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use super::*;
use crate::features::testing::{ACCOUNT, Harness};
use crate::secrets::room_password_key;
use crate::secrets::testing::MemorySecrets;

const ROOM: &str = "dev@rooms.chord.localhost";
const BOB: &str = "bob@chord.localhost";

fn room() -> BareJid {
    BareJid::new(ROOM).unwrap()
}

fn jid(s: &str) -> Jid {
    Jid::new(s).unwrap()
}

fn x_user(inner: &str) -> Element {
    format!("<x xmlns='{NS_MUC_USER}'>{inner}</x>")
        .parse()
        .unwrap()
}

/// The presence of an occupant, with raw XML inside the `muc#user` element.
fn presence(nick: &str, inner: &str, extra: Option<&str>) -> Presence {
    let mut p = Presence::available().with_from(jid(&format!("{ROOM}/{nick}")));
    p.payloads.push(x_user(inner));
    if let Some(extra) = extra {
        p.payloads.push(extra.parse().unwrap());
    }
    p
}

fn unavailable(nick: &str, inner: &str) -> Presence {
    let mut p = Presence::unavailable().with_from(jid(&format!("{ROOM}/{nick}")));
    p.payloads.push(x_user(inner));
    p
}

fn occupant_id_xml(id: &str) -> String {
    format!("<occupant-id xmlns='{NS_OCCUPANT_ID}' id='{id}'/>")
}

/// Join the room as `nick` and finish the join with our own presence.
fn joined(h: &mut Harness, nick: &str) {
    h.with_ctx(|ctx| join_room(ctx, &room(), Some(nick.into()), None, None));
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                nick,
                "<item affiliation='member' role='participant'/><status code='110'/>",
                None,
            ),
        )
    });
    h.take_sent();
    h.effects.clear();
}

fn notices(h: &mut Harness) -> Vec<String> {
    std::mem::take(&mut h.effects)
        .into_iter()
        .filter_map(|e| match e {
            crate::features::Effect::Emit(ClientEvent::Notice(n)) => Some(n),
            _ => None,
        })
        .collect()
}

fn events(h: &mut Harness) -> Vec<ClientEvent> {
    std::mem::take(&mut h.effects)
        .into_iter()
        .filter_map(|e| match e {
            crate::features::Effect::Emit(e) => Some(e),
            _ => None,
        })
        .collect()
}

fn room_message(inner: &str) -> Message {
    let mut m = Message::new(Some(jid(ACCOUNT)));
    m.type_ = MessageType::Groupchat;
    m.from = Some(jid(ROOM));
    m.payloads.push(x_user(inner));
    m
}

fn anonymity(h: &Harness) -> Option<String> {
    h.store
        .conn()
        .query_row(
            "SELECT anonymity FROM rooms WHERE account_id = ?1 AND jid = ?2",
            params![h.account_id, ROOM],
            |r| r.get(0),
        )
        .unwrap()
}

fn stored_nick(h: &Harness) -> Option<String> {
    h.store
        .conn()
        .query_row(
            "SELECT nick FROM rooms WHERE account_id = ?1 AND jid = ?2",
            params![h.account_id, ROOM],
            |r| r.get(0),
        )
        .unwrap()
}

// ---------------------------------------------------------------- status codes (#50)

#[test]
fn a_status_code_that_xmpp_parsers_does_not_know_keeps_the_others() {
    // 174 is in XEP-0045 but not in xmpp-parsers 0.23.
    let payloads = vec![x_user(
        "<item affiliation='member' role='participant'/><status code='110'/><status code='174'/>",
    )];
    let user = status::muc_user(&payloads).expect("a muc user");
    assert_eq!(
        user.status,
        vec![xmpp_parsers::muc::user::Status::SelfPresence]
    );
    assert_eq!(status::codes(&payloads), vec![110, 174]);
}

#[test]
fn a_join_with_a_code_that_xmpp_parsers_does_not_know_completes() {
    let mut h = Harness::new();
    let (reply, mut answer) = oneshot::channel();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, Some(reply)));
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "alice",
                "<item affiliation='member' role='participant'/><status code='110'/><status code='174'/>",
                None,
            ),
        )
    });
    assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    assert_eq!(anonymity(&h).as_deref(), Some("anonymous"));
}

#[test]
fn status_104_in_a_room_message_reads_the_disco_info_again() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| on_message(ctx, &room_message("<status code='104'/>")));
    let iqs = h.sent_iqs();
    assert_eq!(iqs.len(), 1);
    let Iq::Get { to, payload, .. } = &iqs[0] else {
        panic!("not a get")
    };
    assert_eq!(to.as_ref().map(|t| t.to_string()).as_deref(), Some(ROOM));
    assert!(payload.is("query", NS_DISCO_INFO));
    // A second 104 while the answer is on its way sends nothing.
    h.with_ctx(|ctx| on_message(ctx, &room_message("<status code='104'/>")));
    assert!(h.sent_iqs().is_empty());

    let answer: Element = format!(
        "<query xmlns='{NS_DISCO_INFO}'><identity category='conference' type='text' name='Dev'/>\
         <feature var='muc_semianonymous'/></query>"
    )
    .parse()
    .unwrap();
    h.answer(
        |p| matches!(p, crate::features::Pending::Muc(Pending::Refresh(_))),
        Some(answer),
    );
    assert_eq!(anonymity(&h).as_deref(), Some("semi-anonymous"));
    // The next 104 asks again.
    h.with_ctx(|ctx| on_message(ctx, &room_message("<status code='104'/>")));
    assert_eq!(h.sent_iqs().len(), 1);
}

#[test]
fn status_104_for_a_room_that_we_are_not_in_asks_nothing() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), None));
    h.with_ctx(|ctx| on_message(ctx, &room_message("<status code='104'/>")));
    assert!(h.take_sent().is_empty());
}

#[test]
fn status_172_173_174_set_the_anonymity_and_tell_the_user() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    assert_eq!(anonymity(&h), None);
    for (code, expected, text) in [
        ("172", "non-anonymous", "non-anonymous"),
        ("173", "semi-anonymous", "semi-anonymous"),
        ("174", "anonymous", "now anonymous"),
    ] {
        h.with_ctx(|ctx| on_message(ctx, &room_message(&format!("<status code='{code}'/>"))));
        assert_eq!(anonymity(&h).as_deref(), Some(expected));
        let n = notices(&mut h);
        assert_eq!(n.len(), 1, "{code}: {n:?}");
        assert!(n[0].contains(text), "{}", n[0]);
    }
    // The same state again says nothing.
    h.with_ctx(|ctx| on_message(ctx, &room_message("<status code='174'/>")));
    assert!(notices(&mut h).is_empty());
}

#[test]
fn status_100_in_our_presence_marks_the_room_non_anonymous_without_a_notice() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "alice",
                "<item affiliation='member' role='participant'/><status code='100'/><status code='110'/>",
                None,
            ),
        )
    });
    assert_eq!(anonymity(&h).as_deref(), Some("non-anonymous"));
    assert!(notices(&mut h).is_empty());
}

#[test]
fn the_room_card_reads_the_anonymity_from_the_features() {
    let query = |feature: &str| -> Element {
        format!(
            "<query xmlns='{NS_DISCO_INFO}'><identity category='conference' type='text'/>\
             <feature var='{feature}'/></query>"
        )
        .parse()
        .unwrap()
    };
    let card = |f: &str| room_card(&room(), query(f)).unwrap().anonymity;
    assert_eq!(card("muc_nonanonymous").as_deref(), Some("non-anonymous"));
    assert_eq!(card("muc_semianonymous").as_deref(), Some("semi-anonymous"));
    assert_eq!(card("muc_anonymous").as_deref(), Some("anonymous"));
    assert_eq!(card("muc_open"), None);
}

// ---------------------------------------------------------------- removal (#51)

#[test]
fn each_removal_code_has_one_plain_sentence() {
    let r = room();
    let text = |codes: &[u16]| status::removal_text(&r, codes, None);
    assert_eq!(text(&[301]), format!("You were banned from {ROOM}"));
    assert_eq!(text(&[307]), format!("You were kicked from {ROOM}"));
    assert_eq!(
        text(&[321]),
        format!("You were removed from {ROOM} because your affiliation changed")
    );
    assert_eq!(
        text(&[322]),
        format!("You were removed from {ROOM} because it is now for members only")
    );
    assert_eq!(
        text(&[332]),
        format!("You were removed from {ROOM} because the chat service is shutting down")
    );
    assert_eq!(
        text(&[333]),
        format!("You were removed from {ROOM} because of a technical error in the chat service")
    );
    assert_eq!(text(&[]), format!("You were removed from {ROOM}"));
    assert_eq!(
        status::removal_text(&r, &[307], Some("spam")),
        format!("You were kicked from {ROOM}: spam")
    );
}

#[test]
fn a_removal_for_a_change_of_the_room_says_why() {
    for (code, part) in [("321", "affiliation changed"), ("322", "members only")] {
        let mut h = Harness::new();
        joined(&mut h, "alice");
        h.with_ctx(|ctx| {
            on_presence(
                ctx,
                &unavailable(
                    "alice",
                    &format!("<item affiliation='none' role='none'/><status code='{code}'/><status code='110'/>"),
                ),
            )
        });
        let n = notices(&mut h);
        assert_eq!(n.len(), 1);
        assert!(n[0].contains(part), "{}", n[0]);
        assert!(!h.state.muc.rejoin.contains(&room()), "{code} is final");
    }
}

#[test]
fn a_service_shutdown_says_why_and_joins_again_later() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &unavailable(
                "alice",
                "<item affiliation='member' role='none'/><status code='332'/><status code='110'/>",
            ),
        )
    });
    let n = notices(&mut h);
    assert!(n[0].contains("shutting down"), "{}", n[0]);
    assert!(h.state.muc.rejoin.contains(&room()));
    // Nothing goes out at the first tick. The join starts at the second.
    h.with_ctx(health::on_tick);
    assert!(h.take_sent().is_empty());
    h.with_ctx(health::on_tick);
    let sent = h.take_sent();
    assert!(
        sent.iter().any(|s| matches!(s, Stanza::Presence(p)
            if p.to.as_ref().map(|t| t.to_string()).as_deref() == Some(&format!("{ROOM}/alice")[..]))),
        "{sent:?}"
    );
    // The join completes: the waiting stops.
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "alice",
                "<item affiliation='member' role='participant'/><status code='110'/>",
                None,
            ),
        )
    });
    assert!(!h.state.muc.rejoin.contains(&room()));
}

#[test]
fn the_rejoin_waits_longer_each_time_and_gives_up() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &unavailable("alice", "<status code='333'/><status code='110'/>"),
        )
    });
    notices(&mut h);
    let mut tries = 0;
    let mut gave_up = false;
    for _ in 0..400 {
        h.with_ctx(health::on_tick);
        // The join that we sent got an error: nothing runs now.
        if h.take_sent()
            .iter()
            .any(|s| matches!(s, Stanza::Presence(_)))
        {
            tries += 1;
            h.with_ctx(|ctx| {
                ctx.state.muc.joins.clear();
            });
        }
        if notices(&mut h).iter().any(|n| n.contains("could not join")) {
            gave_up = true;
            break;
        }
    }
    assert_eq!(tries, usize::from(rejoin::MAX_TRIES));
    assert!(gave_up);
    assert!(!h.state.muc.rejoin.contains(&room()));
}

#[test]
fn leaving_the_room_cancels_the_rejoin() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &unavailable("alice", "<status code='332'/><status code='110'/>"),
        )
    });
    assert!(h.state.muc.rejoin.contains(&room()));
    h.with_ctx(|ctx| leave_room_quietly(ctx, &room()));
    assert!(!h.state.muc.rejoin.contains(&room()));
}

// ---------------------------------------------------------------- assigned nick (#52)

#[test]
fn a_nick_that_the_service_assigns_is_stored() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    assert_eq!(stored_nick(&h).as_deref(), Some("alice"));
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "alice-2",
                "<item affiliation='member' role='participant'/><status code='110'/><status code='210'/>",
                None,
            ),
        )
    });
    assert_eq!(stored_nick(&h).as_deref(), Some("alice-2"));
    assert_eq!(
        h.state.muc.nicks.get(&room()).map(String::as_str),
        Some("alice-2")
    );
    let n = notices(&mut h);
    assert_eq!(n, vec![format!("{ROOM} gave you the nick alice-2")]);
}

#[test]
fn the_nick_that_we_asked_for_is_no_notice() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "alice",
                "<item affiliation='member' role='participant'/><status code='110'/>",
                None,
            ),
        )
    });
    assert_eq!(stored_nick(&h).as_deref(), Some("alice"));
    assert!(notices(&mut h).is_empty());
}

// ---------------------------------------------------------------- decline notice (#56)

fn decline_message(from_room: &str, who: &str, reason: Option<&str>) -> Message {
    let reason = reason.map_or_else(String::new, |r| format!("<reason>{r}</reason>"));
    let mut m = Message::new(Some(jid(ACCOUNT)));
    m.from = Some(jid(from_room));
    m.payloads
        .push(x_user(&format!("<decline from='{who}'>{reason}</decline>")));
    m
}

#[test]
fn a_decline_from_the_room_says_who_declined() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), None));
    let handled = h.with_ctx(|ctx| {
        on_message(
            ctx,
            &decline_message(ROOM, "bob@chord.localhost/phone", Some("not now")),
        )
    });
    assert!(handled);
    assert_eq!(
        notices(&mut h),
        vec![format!("{BOB} declined your invitation to {ROOM}: not now")]
    );
    h.with_ctx(|ctx| on_message(ctx, &decline_message(ROOM, BOB, None)));
    assert_eq!(
        notices(&mut h),
        vec![format!("{BOB} declined your invitation to {ROOM}")]
    );
}

#[test]
fn a_decline_from_an_unknown_room_or_from_an_occupant_is_ignored() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| on_message(ctx, &decline_message(ROOM, BOB, None)));
    assert!(notices(&mut h).is_empty());
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), None));
    h.with_ctx(|ctx| on_message(ctx, &decline_message(&format!("{ROOM}/mallory"), BOB, None)));
    assert!(notices(&mut h).is_empty());
}

#[test]
fn a_decline_is_an_invitation_message_for_the_archive() {
    assert!(is_invitation(&decline_message(ROOM, BOB, None)));
}

// ---------------------------------------------------------------- direct invitation (#55)

fn sent_messages(h: &mut Harness) -> Vec<Message> {
    h.take_sent()
        .into_iter()
        .filter_map(|s| match s {
            Stanza::Message(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn direct_invite(
    h: &mut Harness,
    reason: Option<&str>,
) -> oneshot::Receiver<Result<(), ClientError>> {
    let (reply, answer) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::InviteDirect {
                room: room(),
                jid: BareJid::new(BOB).unwrap(),
                reason: reason.map(str::to_owned),
                reply,
            },
        )
    });
    answer
}

#[test]
fn a_direct_invitation_goes_to_the_person_with_the_room_in_it() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), None));
    let mut answer = direct_invite(&mut h, Some("come in"));
    assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    let messages = sent_messages(&mut h);
    assert_eq!(messages.len(), 1);
    let m = &messages[0];
    assert_eq!(m.to.as_ref().map(|t| t.to_string()).as_deref(), Some(BOB));
    let x = m
        .payloads
        .iter()
        .find(|p| p.is("x", NS_CONFERENCE))
        .expect("a conference element");
    assert_eq!(x.attr("jid"), Some(ROOM));
    assert_eq!(x.attr("reason"), Some("come in"));
    assert_eq!(x.attr("password"), None);
}

#[test]
fn a_direct_invitation_carries_the_room_password() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), Some("hunter2")));
    drop(direct_invite(&mut h, None));
    let messages = sent_messages(&mut h);
    let x = messages[0]
        .payloads
        .iter()
        .find(|p| p.is("x", NS_CONFERENCE))
        .unwrap();
    assert_eq!(x.attr("password"), Some("hunter2"));
    assert_eq!(x.attr("reason"), None);
}

#[test]
fn an_owner_grants_membership_before_a_direct_invitation() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    // We are owner.
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "alice",
                "<item affiliation='owner' role='moderator'/><status code='110'/>",
                None,
            ),
        )
    });
    h.take_sent();
    let mut answer = direct_invite(&mut h, None);
    let iqs = h.sent_iqs();
    assert_eq!(iqs.len(), 1, "the grant goes first");
    assert_eq!(answer.try_recv().unwrap(), None);
    h.answer(
        |p| {
            matches!(
                p,
                crate::features::Pending::Muc(Pending::InviteGrant { .. })
            )
        },
        None,
    );
    assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    let messages = sent_messages(&mut h);
    assert!(
        messages[0]
            .payloads
            .iter()
            .any(|p| p.is("x", NS_CONFERENCE))
    );
}

#[test]
fn a_direct_invitation_offline_fails() {
    let (reply, mut answer) = oneshot::channel();
    offline(Command::InviteDirect {
        room: room(),
        jid: BareJid::new(BOB).unwrap(),
        reason: None,
        reply,
    });
    assert_eq!(
        answer.try_recv().unwrap(),
        Some(Err(ClientError::NotConnected))
    );
}

fn refusal(id: &str, condition: DefinedCondition) -> Message {
    let mut m = Message::new(Some(jid(ACCOUNT)));
    m.type_ = MessageType::Error;
    m.from = Some(jid(ROOM));
    m.id = Some(Id(id.into()));
    m.payloads.push(x_user(&format!("<invite to='{BOB}'/>")));
    m.payloads
        .push(StanzaError::new(ErrorType::Auth, condition, "en", "no").into());
    m
}

fn mediated_invite(h: &mut Harness) -> String {
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), None));
    let (reply, _answer) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::Invite {
                room: room(),
                jid: BareJid::new(BOB).unwrap(),
                reason: Some("hi".into()),
                reply,
            },
        )
    });
    let messages = sent_messages(h);
    assert_eq!(messages.len(), 1);
    messages[0].id.as_ref().unwrap().0.clone()
}

#[test]
fn a_room_that_refuses_a_mediated_invitation_gets_a_direct_one() {
    for condition in [DefinedCondition::Forbidden, DefinedCondition::NotAllowed] {
        let mut h = Harness::new();
        let id = mediated_invite(&mut h);
        h.with_ctx(|ctx| on_message(ctx, &refusal(&id, condition.clone())));
        let messages = sent_messages(&mut h);
        assert_eq!(messages.len(), 1, "{condition:?}");
        let x = messages[0]
            .payloads
            .iter()
            .find(|p| p.is("x", NS_CONFERENCE))
            .expect("direct");
        assert_eq!(x.attr("jid"), Some(ROOM));
        assert_eq!(x.attr("reason"), Some("hi"));
        assert_eq!(
            messages[0].to.as_ref().map(|t| t.to_string()).as_deref(),
            Some(BOB)
        );
        let n = notices(&mut h);
        assert!(n[0].contains("direct invitation"), "{}", n[0]);
        // The same error again does not send a second one.
        h.with_ctx(|ctx| on_message(ctx, &refusal(&id, condition)));
        assert!(sent_messages(&mut h).is_empty());
    }
}

#[test]
fn another_refusal_is_a_notice_and_no_direct_invitation() {
    let mut h = Harness::new();
    let id = mediated_invite(&mut h);
    h.with_ctx(|ctx| on_message(ctx, &refusal(&id, DefinedCondition::ItemNotFound)));
    assert!(sent_messages(&mut h).is_empty());
    let n = notices(&mut h);
    assert!(n[0].contains("refused the invitation"), "{}", n[0]);
}

// ---------------------------------------------------------------- occupant-id (#60)

#[test]
fn the_occupant_id_of_an_occupant_is_stored_and_a_presence_without_one_keeps_it() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    let read = |h: &Harness, nick: &str| -> Option<String> {
        h.store
            .conn()
            .query_row(
                "SELECT occupant_id FROM occupants WHERE room = ?1 AND nick = ?2",
                params![ROOM, nick],
                |r| r.get(0),
            )
            .unwrap()
    };
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "bob",
                "<item affiliation='member' role='participant'/>",
                Some(&occupant_id_xml("OCC-BOB")),
            ),
        )
    });
    assert_eq!(read(&h, "bob").as_deref(), Some("OCC-BOB"));
    // A presence with no id (a status change) keeps the id.
    h.with_ctx(|ctx| {
        on_presence(
            ctx,
            &presence(
                "bob",
                "<item affiliation='member' role='participant'/>",
                None,
            ),
        )
    });
    assert_eq!(read(&h, "bob").as_deref(), Some("OCC-BOB"));
}

fn stored_occupant_ids(h: &Harness) -> Vec<(String, Option<String>)> {
    let mut stmt = h
        .store
        .conn()
        .prepare("SELECT sender, occupant_id FROM messages WHERE peer = ?1 ORDER BY id")
        .unwrap();
    stmt.query_map(params![ROOM], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn chat_from(nick: &str, body: &str, stanza_id: &str, occupant: Option<&str>) -> Message {
    let mut m = Message::groupchat(jid(ACCOUNT)).with_body("".into(), body.into());
    m.from = Some(jid(&format!("{ROOM}/{nick}")));
    m.payloads.push(
        format!("<stanza-id xmlns='urn:xmpp:sid:0' id='{stanza_id}' by='{ROOM}'/>")
            .parse()
            .unwrap(),
    );
    if let Some(occupant) = occupant {
        m.payloads.push(occupant_id_xml(occupant).parse().unwrap());
    }
    m
}

#[test]
fn a_room_message_stores_the_occupant_id() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| on_message(ctx, &chat_from("bob", "one", "s1", Some("OCC-BOB"))));
    h.with_ctx(|ctx| on_message(ctx, &chat_from("carol", "two", "s2", None)));
    assert_eq!(
        stored_occupant_ids(&h),
        vec![
            (format!("{ROOM}/bob"), Some("OCC-BOB".to_owned())),
            (format!("{ROOM}/carol"), None),
        ]
    );
}

fn timeline(h: &Harness) -> Vec<crate::views::timeline::TimelineItem> {
    let q = crate::views::QueryCtx {
        store: &h.store,
        account_id: h.account_id,
        account: &h.account,
    };
    crate::views::timeline::query(&q, ROOM, 50).unwrap()
}

#[test]
fn a_nick_that_another_person_takes_does_not_join_the_group_of_the_first() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| on_message(ctx, &chat_from("bob", "one", "s1", Some("OCC-1"))));
    h.with_ctx(|ctx| on_message(ctx, &chat_from("bob", "two", "s2", Some("OCC-1"))));
    h.with_ctx(|ctx| on_message(ctx, &chat_from("bob", "three", "s3", Some("OCC-2"))));
    let grouped: Vec<bool> = timeline(&h)
        .iter()
        .map(|i| i.same_sender_as_previous)
        .collect();
    assert_eq!(grouped, vec![false, true, false]);
}

#[test]
fn messages_without_an_occupant_id_group_by_the_sender_as_before() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| on_message(ctx, &chat_from("bob", "one", "s1", None)));
    h.with_ctx(|ctx| on_message(ctx, &chat_from("bob", "two", "s2", None)));
    h.with_ctx(|ctx| on_message(ctx, &chat_from("bob", "three", "s3", Some("OCC-2"))));
    let grouped: Vec<bool> = timeline(&h)
        .iter()
        .map(|i| i.same_sender_as_previous)
        .collect();
    assert_eq!(grouped, vec![false, true, true]);
}

fn reaction(nick: &str, target: &str, emoji: &str, occupant: Option<&str>) -> Message {
    let mut m = Message::groupchat(jid(ACCOUNT));
    m.from = Some(jid(&format!("{ROOM}/{nick}")));
    m.id = Some(Id(format!("r-{nick}-{emoji}")));
    m.payloads.push(
        format!(
            "<reactions xmlns='urn:xmpp:reactions:0' id='{target}'><reaction>{emoji}</reaction></reactions>"
        )
        .parse()
        .unwrap(),
    );
    m.payloads.push(
        format!("<stanza-id xmlns='urn:xmpp:sid:0' id='rs-{nick}-{emoji}' by='{ROOM}'/>")
            .parse()
            .unwrap(),
    );
    if let Some(occupant) = occupant {
        m.payloads.push(occupant_id_xml(occupant).parse().unwrap());
    }
    m
}

#[test]
fn reactions_are_keyed_by_the_occupant_id_so_a_new_nick_replaces_the_old_set() {
    let mut h = Harness::new();
    joined(&mut h, "alice");
    h.with_ctx(|ctx| on_message(ctx, &chat_from("carol", "hello", "s1", Some("OCC-C"))));
    h.with_ctx(|ctx| on_message(ctx, &reaction("bob", "s1", "\u{1F44D}", Some("OCC-B"))));
    // Bob changes his nick and sets another reaction: the set of Bob is one set.
    h.with_ctx(|ctx| on_message(ctx, &reaction("bobby", "s1", "\u{1F389}", Some("OCC-B"))));
    let items = timeline(&h);
    let reactions = &items[0].reactions;
    assert_eq!(reactions.len(), 1, "{reactions:?}");
    assert_eq!(reactions[0].emoji, "\u{1F389}");
    assert_eq!(reactions[0].count, 1);
    // A reaction with no occupant-id is keyed by the occupant JID, as before.
    h.with_ctx(|ctx| on_message(ctx, &reaction("dave", "s1", "\u{1F44D}", None)));
    let total: u32 = timeline(&h)[0].reactions.iter().map(|r| r.count).sum();
    assert_eq!(total, 2);
}

// ---------------------------------------------------------------- room password (#58)

fn secrets_for(h: &mut Harness) -> Arc<MemorySecrets> {
    let secrets = Arc::new(MemorySecrets::default());
    h.state.muc.set_secret_store(secrets.clone());
    secrets
}

fn column_password(h: &Harness) -> Option<String> {
    h.store
        .conn()
        .query_row(
            "SELECT password FROM rooms WHERE account_id = ?1 AND jid = ?2",
            params![h.account_id, ROOM],
            |r| r.get(0),
        )
        .unwrap()
}

fn key() -> String {
    room_password_key(ACCOUNT, ROOM)
}

fn join_password(sent: &[Stanza]) -> Option<String> {
    let Some(Stanza::Presence(p)) = sent.first() else {
        panic!("no presence")
    };
    let ns = "http://jabber.org/protocol/muc";
    let muc = p.payloads.iter().find(|x| x.is("x", ns))?;
    muc.get_child("password", ns).map(Element::text)
}

#[test]
fn a_join_password_goes_to_the_keychain_and_not_to_the_database() {
    let mut h = Harness::new();
    let secrets = secrets_for(&mut h);
    h.with_ctx(|ctx| {
        join_room(
            ctx,
            &room(),
            Some("alice".into()),
            Some("hunter2".into()),
            None,
        )
    });
    assert_eq!(column_password(&h), None);
    assert_eq!(
        secrets.map.lock().unwrap().get(&key()).map(String::as_str),
        Some("hunter2")
    );
    // The join itself sent the password.
    assert_eq!(join_password(&h.take_sent()).as_deref(), Some("hunter2"));
    // A later join with no password uses the keychain.
    h.with_ctx(|ctx| leave_room_quietly(ctx, &room()));
    h.take_sent();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    assert_eq!(join_password(&h.take_sent()).as_deref(), Some("hunter2"));
    assert_eq!(column_password(&h), None);
}

#[test]
fn a_password_in_an_old_row_moves_to_the_keychain_when_it_is_used() {
    let mut h = Harness::new();
    // An older version stored the password in the column.
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), Some("old-secret")));
    assert_eq!(column_password(&h).as_deref(), Some("old-secret"));
    let secrets = secrets_for(&mut h);
    let found = h.with_ctx(|ctx| stored_password(ctx, &room()));
    assert_eq!(found.as_deref(), Some("old-secret"));
    assert_eq!(
        column_password(&h),
        None,
        "the column clears after the move"
    );
    assert_eq!(
        secrets.map.lock().unwrap().get(&key()).map(String::as_str),
        Some("old-secret")
    );
}

#[test]
fn a_keychain_that_refuses_loses_nothing() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), Some("old-secret")));
    let secrets = secrets_for(&mut h);
    secrets.fail.store(true, Ordering::SeqCst);
    let found = h.with_ctx(|ctx| stored_password(ctx, &room()));
    assert_eq!(found.as_deref(), Some("old-secret"));
    assert_eq!(
        column_password(&h).as_deref(),
        Some("old-secret"),
        "the column stays"
    );
    // A new password goes to the column when the keychain refuses it.
    h.with_ctx(|ctx| ensure_room(ctx, &room(), None, Some("new-secret")));
    assert_eq!(column_password(&h).as_deref(), Some("new-secret"));
}

#[test]
fn with_no_keychain_the_column_holds_the_password_as_before() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| {
        join_room(
            ctx,
            &room(),
            Some("alice".into()),
            Some("hunter2".into()),
            None,
        )
    });
    assert_eq!(column_password(&h).as_deref(), Some("hunter2"));
    assert_eq!(
        h.with_ctx(|ctx| stored_password(ctx, &room())).as_deref(),
        Some("hunter2")
    );
}

#[test]
fn the_keychain_is_the_truth_when_the_column_has_a_leftover() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), Some("stale")));
    let secrets = secrets_for(&mut h);
    secrets.map.lock().unwrap().insert(key(), "fresh".into());
    assert_eq!(
        h.with_ctx(|ctx| stored_password(ctx, &room())).as_deref(),
        Some("fresh")
    );
    assert_eq!(column_password(&h), None);
}

#[test]
fn the_keychain_survives_a_new_session() {
    let mut h = Harness::new();
    let secrets = secrets_for(&mut h);
    let next = h.with_ctx(next_session);
    h.state.muc = next;
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), Some("pw")));
    assert_eq!(
        secrets.map.lock().unwrap().get(&key()).map(String::as_str),
        Some("pw")
    );
}

// ---------------------------------------------------------------- CAPTCHA (#130)

const CAPTCHA_XML: &str = "<captcha xmlns='urn:xmpp:captcha'><x xmlns='jabber:x:data' type='form'>\
    <field var='FORM_TYPE' type='hidden'><value>urn:xmpp:captcha</value></field>\
    <field var='challenge' type='hidden'><value>c-1</value></field>\
    <field var='sid' type='hidden'><value>s-1</value></field>\
    <field var='ocr' type='text-single' label='Type the letters'><required/>\
    <media xmlns='urn:xmpp:media-element'><uri type='image/png'>cid:sha1+a@bob.xmpp.org</uri></media>\
    </field></x></captcha>";

fn challenge_message() -> Message {
    let mut m = Message::new(Some(jid(ACCOUNT)));
    m.from = Some(jid(ROOM));
    m.id = Some(Id("c-1".into()));
    m.payloads.push(CAPTCHA_XML.parse().unwrap());
    m.payloads.push(
        "<data xmlns='urn:xmpp:bob' cid='sha1+a@bob.xmpp.org' type='image/png'>AAAA</data>"
            .parse()
            .unwrap(),
    );
    m
}

fn captcha_events(h: &mut Harness) -> Vec<(BareJid, crate::forms::Form)> {
    events(h)
        .into_iter()
        .filter_map(|e| match e {
            ClientEvent::RoomCaptcha { room, form } => Some((room, form)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_captcha_from_a_room_that_holds_our_join_becomes_an_event_with_the_image() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    h.take_sent();
    // The join waited for a while.
    h.with_ctx(|ctx| ctx.state.muc.joins.get_mut(&room()).unwrap().ticks = 3);
    assert!(h.with_ctx(|ctx| on_message(ctx, &challenge_message())));
    let found = captcha_events(&mut h);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, room());
    let form = &found[0].1;
    assert_eq!(
        form.get("ocr").unwrap().label.as_deref(),
        Some("Type the letters")
    );
    assert!(
        form.get("ocr").unwrap().media[0]
            .uri
            .starts_with("data:image/png;base64,AAAA"),
        "{:?}",
        form.get("ocr").unwrap().media
    );
    // The join stays and its timeout starts again.
    assert_eq!(h.state.muc.joins.get(&room()).unwrap().ticks, 0);
}

#[test]
fn a_captcha_in_the_join_error_keeps_the_join() {
    let mut h = Harness::new();
    let (reply, mut answer) = oneshot::channel();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, Some(reply)));
    let mut error =
        Presence::new(xmpp_parsers::presence::Type::Error).with_from(jid(&format!("{ROOM}/alice")));
    error.payloads.push(CAPTCHA_XML.parse().unwrap());
    error.payloads.push(
        StanzaError::new(
            ErrorType::Auth,
            DefinedCondition::NotAuthorized,
            "en",
            "captcha",
        )
        .into(),
    );
    h.with_ctx(|ctx| on_presence(ctx, &error));
    assert_eq!(captcha_events(&mut h).len(), 1);
    assert_eq!(answer.try_recv().unwrap(), None, "the join still waits");
    assert!(h.state.muc.joins.contains_key(&room()));
}

#[test]
fn a_captcha_from_a_room_that_we_do_not_join_is_not_shown() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| ensure_room(ctx, &room(), Some("alice"), None));
    h.with_ctx(|ctx| on_message(ctx, &challenge_message()));
    assert!(captcha_events(&mut h).is_empty());
}

fn answer_captcha(
    h: &mut Harness,
    answer: Option<crate::forms::Form>,
) -> oneshot::Receiver<Result<(), ClientError>> {
    let (reply, rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::Captcha {
                room: room(),
                answer,
                reply,
            },
        )
    });
    rx
}

#[test]
fn the_answer_to_a_captcha_is_an_iq_with_the_submitted_form() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    h.take_sent();
    h.with_ctx(|ctx| on_message(ctx, &challenge_message()));
    let (_, mut form) = captcha_events(&mut h).remove(0);
    assert!(form.set("ocr", vec!["xk3p".into()]));
    let mut result = answer_captcha(&mut h, Some(form));
    let iqs = h.sent_iqs();
    let Iq::Set { to, payload, .. } = &iqs[0] else {
        panic!("not a set")
    };
    assert_eq!(to.as_ref().map(|t| t.to_string()).as_deref(), Some(ROOM));
    assert!(payload.is("captcha", "urn:xmpp:captcha"));
    let x = payload.get_child("x", "jabber:x:data").expect("a form");
    assert_eq!(x.attr("type"), Some("submit"));
    let values: Vec<(String, String)> = x
        .children()
        .map(|f| {
            (
                f.attr("var").unwrap().to_owned(),
                f.get_child("value", "jabber:x:data").unwrap().text(),
            )
        })
        .collect();
    assert!(
        values.contains(&("ocr".into(), "xk3p".into())),
        "{values:?}"
    );
    // The hidden fields of the challenge go back (XEP-0158).
    assert!(
        values.contains(&("challenge".into(), "c-1".into())),
        "{values:?}"
    );
    assert!(
        values.contains(&("FORM_TYPE".into(), "urn:xmpp:captcha".into())),
        "{values:?}"
    );
    assert_eq!(result.try_recv().unwrap(), None);
    h.answer(
        |p| matches!(p, crate::features::Pending::Muc(Pending::Captcha(_))),
        None,
    );
    assert_eq!(result.try_recv().unwrap(), Some(Ok(())));
    // The join is still the join: the room lets us in.
    assert!(h.state.muc.joins.contains_key(&room()));
}

#[test]
fn a_wrong_captcha_answer_fails_with_the_error_of_the_room() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    h.take_sent();
    h.with_ctx(|ctx| on_message(ctx, &challenge_message()));
    let (_, form) = captcha_events(&mut h).remove(0);
    let mut result = answer_captcha(&mut h, Some(form));
    let error = StanzaError::new(
        ErrorType::Modify,
        DefinedCondition::NotAcceptable,
        "en",
        "wrong",
    );
    h.respond(
        |p| matches!(p, crate::features::Pending::Muc(Pending::Captcha(_))),
        IqResponse::Error(error),
    );
    let Some(Err(ClientError::Server(text))) = result.try_recv().unwrap() else {
        panic!("no server error")
    };
    assert!(text.contains("not-acceptable"), "{text}");
}

#[test]
fn a_captcha_answer_without_a_waiting_join_fails() {
    let mut h = Harness::new();
    let mut result = answer_captcha(&mut h, Some(crate::forms::Form::default()));
    assert!(matches!(
        result.try_recv().unwrap(),
        Some(Err(ClientError::Invalid(_)))
    ));
}

#[test]
fn giving_up_a_captcha_sends_a_cancel_and_leaves() {
    let mut h = Harness::new();
    h.with_ctx(|ctx| join_room(ctx, &room(), Some("alice".into()), None, None));
    h.take_sent();
    h.with_ctx(|ctx| on_message(ctx, &challenge_message()));
    captcha_events(&mut h);
    let mut result = answer_captcha(&mut h, None);
    assert_eq!(result.try_recv().unwrap(), Some(Ok(())));
    let sent = h.take_sent();
    let cancel = sent.iter().find_map(|s| match s {
        Stanza::Iq(Iq::Set { payload, .. }) => payload.get_child("x", "jabber:x:data").cloned(),
        _ => None,
    });
    assert_eq!(cancel.expect("a cancel form").attr("type"), Some("cancel"));
    assert!(!h.state.muc.joins.contains_key(&room()));
}
