//! Tests of the OMEMO feature. Two accounts in memory exchange stanzas through two
//! harnesses. The crypto is the real one (libomemo-c), so a message that decrypts here would
//! decrypt on the wire too.

use std::sync::Arc;

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::ns;
use xmpp_parsers::stanza::Stanza;
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use super::xml::{
    self, BundleXml, Device, DeviceList, Encrypted, NODE_BUNDLES, NODE_DEVICES, NS as NS_OMEMO,
};
use super::{Command, OmemoTrust, Pending, State, crypto, receive, send, store, trust};
use crate::actor::ClientEvent;
use crate::features::testing::Harness;
use crate::features::{self, Effect, IqResponse, Pending as FeaturePending, chat};
use crate::secrets::testing::MemorySecrets;

const ALICE: &str = "alice@chord.localhost";
const BOB: &str = "bob@chord.localhost";

fn bare(jid: &str) -> BareJid {
    BareJid::new(jid).unwrap()
}

fn to_xml(el: &Element) -> String {
    let mut out = Vec::new();
    el.write_to(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn message_xml(m: &Message) -> String {
    to_xml(&Element::from(m.clone()))
}

fn is_list(p: &FeaturePending, who: &str) -> bool {
    matches!(p, FeaturePending::Omemo(Pending::List { jid }) if jid.as_str() == who)
}

fn is_bundle(p: &FeaturePending, who: &str, device: u32) -> bool {
    matches!(p, FeaturePending::Omemo(Pending::Bundle { jid, device: d })
        if jid.as_str() == who && *d == device)
}

fn has_pending(h: &Harness, which: impl Fn(&FeaturePending) -> bool) -> bool {
    h.pending.values().any(|p| which(&p.then))
}

/// The `<pubsub/>` payload of an answer with one item.
fn items_result(node: &str, item_id: &str, payload: &Element) -> Element {
    format!(
        "<pubsub xmlns='{}'><items node='{node}'><item id='{item_id}'>{}</item></items></pubsub>",
        ns::PUBSUB,
        to_xml(payload)
    )
    .parse()
    .unwrap()
}

fn not_found() -> IqResponse {
    IqResponse::Error(StanzaError::new(
        ErrorType::Cancel,
        DefinedCondition::ItemNotFound,
        "en",
        "",
    ))
}

fn answer_list(h: &mut Harness, who: &str, list: Option<&Element>) {
    match list {
        Some(el) => h.answer(
            |p| is_list(p, who),
            Some(items_result(NODE_DEVICES, "current", el)),
        ),
        None => h.respond(|p| is_list(p, who), not_found()),
    }
}

fn answer_bundle(h: &mut Harness, who: &str, device: u32, bundle: &Element) {
    h.answer(
        |p| is_bundle(p, who, device),
        Some(items_result(NODE_BUNDLES, &device.to_string(), bundle)),
    );
}

/// What a client published.
#[derive(Default)]
struct Published {
    list: Option<Element>,
    bundle: Option<Element>,
    device: u32,
}

fn published(iqs: &[Iq]) -> Published {
    let mut out = Published::default();
    for iq in iqs {
        let Iq::Set { payload, .. } = iq else {
            continue;
        };
        let Some(publish) = payload.get_child("publish", ns::PUBSUB) else {
            continue;
        };
        let item = publish.get_child("item", ns::PUBSUB).unwrap();
        let content = item.children().next().cloned().unwrap();
        match publish.attr("node") {
            Some(NODE_DEVICES) => out.list = Some(content),
            Some(NODE_BUNDLES) => {
                out.device = item.attr("id").unwrap().parse().unwrap();
                out.bundle = Some(content);
            }
            _ => {}
        }
    }
    out
}

/// Answer the publishes and bundle checks that wait.
fn settle(h: &mut Harness) {
    for _ in 0..10 {
        let found = h.pending.iter().find_map(|(id, p)| {
            matches!(
                &p.then,
                FeaturePending::Omemo(Pending::PublishList | Pending::PublishBundle)
            )
            .then(|| id.clone())
        });
        let Some(id) = found else { return };
        let pending = h.pending.remove(&id).unwrap();
        h.with_ctx(|ctx| features::on_answer(ctx, pending, IqResponse::Result(None)));
    }
}

/// A new session of the client: load the keys, read our own device list. `own_list` is what
/// the server answers. Returns what the client published.
fn connect(h: &mut Harness, own_list: Option<&Element>) -> Published {
    h.with_ctx(|ctx| {
        super::on_connected(ctx);
        super::on_services_ready(ctx);
    });
    let own = h.account.to_string();
    answer_list(h, &own, own_list);
    let iqs = h.sent_iqs();
    let out = published(&iqs);
    settle(h);
    out
}

fn device_id(h: &Harness) -> u32 {
    store::own_device_id(&h.store, h.account_id)
        .unwrap()
        .unwrap()
}

fn events(h: &mut Harness) -> Vec<ClientEvent> {
    let mut out = Vec::new();
    for effect in std::mem::take(&mut h.effects) {
        match effect {
            Effect::Emit(e) => out.push(e),
            other => h.effects.push(other),
        }
    }
    out
}

fn send_chat(h: &mut Harness, to: &str, body: &str) -> String {
    h.with_ctx(|ctx| chat::send(ctx, Jid::new(to).unwrap(), body.into()))
}

fn enable(h: &mut Harness, peer: &str) {
    let (tx, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        super::on_command(
            ctx,
            Command::SetEnabled {
                peer: bare(peer),
                enabled: true,
                reply: tx,
            },
        )
    });
    assert!(rx.try_recv().unwrap().unwrap().is_ok());
}

fn sent_messages(h: &mut Harness) -> Vec<Message> {
    h.take_sent()
        .into_iter()
        .filter_map(|s| match s {
            Stanza::Message(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn deliver(to: &mut Harness, from: &str, mut m: Message) {
    m.from = Some(Jid::new(from).unwrap());
    if m.to.is_none() {
        m.to = Some(Jid::from(to.account.clone()));
    }
    to.with_ctx(|ctx| features::on_stanza(ctx, Stanza::Message(m)));
}

/// (body, encryption, direction, failed) of the rows with `peer`.
fn rows(h: &Harness, peer: &str) -> Vec<(String, Option<String>, String, bool)> {
    let mut stmt = h
        .store
        .conn()
        .prepare(
            "SELECT body, encryption, direction, failed_at IS NOT NULL FROM messages
             WHERE account_id = ?1 AND peer = ?2 ORDER BY id",
        )
        .unwrap();
    stmt.query_map(rusqlite::params![h.account_id, peer], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn list_of(ids: &[u32]) -> Element {
    Element::from(&DeviceList {
        devices: ids
            .iter()
            .map(|&id| Device {
                id,
                label: None,
                labelsig: None,
            })
            .collect(),
    })
}

fn ids_of(list: &Element) -> Vec<u32> {
    let mut ids: Vec<u32> = DeviceList::try_from(list)
        .unwrap()
        .devices
        .iter()
        .map(|d| d.id)
        .collect();
    ids.sort_unstable();
    ids
}

/// Two connected accounts that know nothing about each other.
struct Pair {
    alice: Harness,
    bob: Harness,
    pa: Published,
    pb: Published,
}

fn pair() -> Pair {
    let mut alice = Harness::for_account(ALICE);
    let mut bob = Harness::for_account(BOB);
    let pa = connect(&mut alice, None);
    let pb = connect(&mut bob, None);
    Pair { alice, bob, pa, pb }
}

/// Alice sends `body` to Bob and answers the PEP requests with Bob's real device list and
/// bundle. Returns the message stanza.
fn alice_sends(p: &mut Pair, body: &str) -> Message {
    send_chat(&mut p.alice, BOB, body);
    serve_bob(p);
    let mut sent = sent_messages(&mut p.alice);
    assert_eq!(sent.len(), 1, "one stanza goes out");
    sent.pop().unwrap()
}

/// Answer the requests of Alice for the devices of Bob.
fn serve_bob(p: &mut Pair) {
    if has_pending(&p.alice, |x| is_list(x, BOB)) {
        answer_list(&mut p.alice, BOB, p.pb.list.as_ref());
    }
    let id = p.pb.device;
    if has_pending(&p.alice, |x| is_bundle(x, BOB, id)) {
        answer_bundle(&mut p.alice, BOB, id, p.pb.bundle.as_ref().unwrap());
    }
}

// -----------------------------------------------------------------------------------------

#[test]
fn a_new_device_publishes_its_list_and_its_bundle() {
    let mut h = Harness::for_account(ALICE);
    let p = connect(&mut h, None);
    let id = device_id(&h);
    assert_eq!(ids_of(p.list.as_ref().unwrap()), vec![id]);
    let bundle = BundleXml::try_from(p.bundle.as_ref().unwrap()).unwrap();
    assert_eq!(p.device, id);
    assert_eq!(bundle.prekeys.len(), 100);
    // Own device row, with the key.
    let own = trust::devices(&h.store, h.account_id, &h.account, &h.account).unwrap();
    assert_eq!(own.len(), 1);
    assert!(own[0].this_device && own[0].own && own[0].active);
    assert!(own[0].fingerprint.is_some());
}

#[test]
fn the_publish_options_ask_for_open_access_and_max_items() {
    let mut h = Harness::for_account(ALICE);
    h.with_ctx(|ctx| {
        super::on_connected(ctx);
        super::on_services_ready(ctx);
    });
    answer_list(&mut h, ALICE, None);
    let text: Vec<String> = h.sent_iqs().iter().map(|i| format!("{i:?}")).collect();
    let all = text.join("\n");
    assert!(all.contains("pubsub#access_model"), "{all}");
    assert!(all.contains("max"), "{all}");
}

#[test]
fn a_restart_keeps_the_keys_and_does_not_publish_the_same_bundle_again() {
    let mut h = Harness::for_account(ALICE);
    let first = connect(&mut h, None);
    let id = device_id(&h);
    let mut again = Harness::with_store(h.store, ALICE);
    let list = list_of(&[id]);
    again.with_ctx(|ctx| {
        super::on_connected(ctx);
        super::on_services_ready(ctx);
    });
    answer_list(&mut again, ALICE, Some(&list));
    assert_eq!(device_id(&again), id);
    // Our id is in the list, so the client only checks the bundle.
    assert!(has_pending(&again, |p| matches!(
        p,
        FeaturePending::Omemo(Pending::OwnBundle)
    )));
    assert!(published(&again.sent_iqs()).list.is_none());
    again.answer(
        |p| matches!(p, FeaturePending::Omemo(Pending::OwnBundle)),
        Some(items_result(
            NODE_BUNDLES,
            &id.to_string(),
            first.bundle.as_ref().unwrap(),
        )),
    );
    let after = published(&again.sent_iqs());
    assert!(after.bundle.is_none(), "the bundle is the same");
}

#[test]
fn a_changed_bundle_is_published_again() {
    let mut h = Harness::for_account(ALICE);
    let first = connect(&mut h, None);
    let id = device_id(&h);
    let mut old = BundleXml::try_from(first.bundle.as_ref().unwrap()).unwrap();
    old.prekeys.pop();
    let mut again = Harness::with_store(h.store, ALICE);
    again.with_ctx(|ctx| {
        super::on_connected(ctx);
        super::on_services_ready(ctx);
    });
    answer_list(&mut again, ALICE, Some(&list_of(&[id])));
    again.answer(
        |p| matches!(p, FeaturePending::Omemo(Pending::OwnBundle)),
        Some(items_result(
            NODE_BUNDLES,
            &id.to_string(),
            &Element::from(&old),
        )),
    );
    assert!(published(&again.sent_iqs()).bundle.is_some());
}

#[test]
fn our_id_is_added_to_a_list_and_other_ids_stay() {
    let mut h = Harness::for_account(ALICE);
    let p = connect(&mut h, Some(&list_of(&[111, 222])));
    let id = device_id(&h);
    let mut want = vec![111, 222, id];
    want.sort_unstable();
    assert_eq!(ids_of(p.list.as_ref().unwrap()), want);
    let own = trust::devices(&h.store, h.account_id, &h.account, &h.account).unwrap();
    assert_eq!(own.iter().filter(|d| d.active).count(), 3);
}

#[test]
fn a_device_list_event_keeps_other_ids_and_adds_ours_back() {
    let mut h = Harness::for_account(ALICE);
    connect(&mut h, None);
    let id = device_id(&h);
    let event = format!(
        "<message xmlns='jabber:client' from='{ALICE}'><event xmlns='http://jabber.org/protocol/pubsub#event'>\
         <items node='{NODE_DEVICES}'><item id='current'>{}</item></items></event></message>",
        to_xml(&list_of(&[999]))
    );
    let message = Message::try_from(event.parse::<Element>().unwrap()).unwrap();
    h.with_ctx(|ctx| features::on_stanza(ctx, Stanza::Message(message)));
    let p = published(&h.sent_iqs());
    let mut want = vec![999, id];
    want.sort_unstable();
    assert_eq!(ids_of(p.list.as_ref().unwrap()), want);
    let own = trust::devices(&h.store, h.account_id, &h.account, &h.account).unwrap();
    let active: Vec<u32> = own
        .iter()
        .filter(|d| d.active)
        .map(|d| d.device_id)
        .collect();
    assert!(active.contains(&999) && active.contains(&id));
}

#[test]
fn a_device_list_event_of_a_contact_marks_missing_ids_inactive() {
    let mut h = Harness::for_account(ALICE);
    connect(&mut h, None);
    for list in [list_of(&[1, 2, 3]), list_of(&[2])] {
        let event = format!(
            "<message xmlns='jabber:client' from='{BOB}'><event xmlns='http://jabber.org/protocol/pubsub#event'>\
             <items node='{NODE_DEVICES}'><item id='current'>{}</item></items></event></message>",
            to_xml(&list)
        );
        let message = Message::try_from(event.parse::<Element>().unwrap()).unwrap();
        h.with_ctx(|ctx| features::on_stanza(ctx, Stanza::Message(message)));
    }
    let devices = trust::devices(&h.store, h.account_id, &h.account, &bare(BOB)).unwrap();
    let state: Vec<(u32, bool)> = devices.iter().map(|d| (d.device_id, d.active)).collect();
    assert_eq!(state, vec![(1, false), (2, true), (3, false)]);
    assert!(
        events(&mut h)
            .iter()
            .any(|e| matches!(e, ClientEvent::OmemoDevicesChanged(j) if j.as_str() == BOB))
    );
}

#[test]
fn the_caps_list_has_the_devices_notify_feature() {
    assert!(crate::features::disco::FEATURES.contains(&xml::NOTIFY_DEVICES));
}

#[test]
fn encryption_is_off_by_default_and_the_message_goes_out_as_plain_text() {
    let mut h = Harness::for_account(ALICE);
    connect(&mut h, None);
    send_chat(&mut h, BOB, "plain hello");
    let sent = sent_messages(&mut h);
    assert_eq!(sent.len(), 1);
    assert!(message_xml(&sent[0]).contains("plain hello"));
    assert_eq!(rows(&h, BOB)[0].1, None);
}

#[test]
fn alice_sends_to_bob_and_bob_reads_it_and_answers() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    enable(&mut p.bob, ALICE);
    // Turning it on warms up: the device list of Bob is read at once. The answer comes now,
    // and the later send waits only for the bundle.
    let m = alice_sends(&mut p, "hello bob, this is secret");
    let xml_text = message_xml(&m);
    assert!(
        !xml_text.contains("hello bob") && !xml_text.contains("secret"),
        "plain text in the stanza: {xml_text}"
    );
    assert!(xml_text.contains("urn:xmpp:omemo:2"));
    assert!(
        xml_text.contains(&format!("sid=\"{}\"", p.pa.device))
            || xml_text.contains(&format!("sid='{}'", p.pa.device))
    );
    assert!(xml_text.contains(xml::FALLBACK_BODY));
    // Alice's own row.
    let a = rows(&p.alice, BOB);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].0, "hello bob, this is secret");
    assert_eq!(a[0].1.as_deref(), Some("omemo:2"));
    assert!(!a[0].3);

    // Bob gets it. It is a key exchange.
    let enc = Encrypted::find(&m).unwrap().unwrap();
    assert!(enc.key_for(&bare(BOB), p.pb.device).unwrap().kex);
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    let b = rows(&p.bob, ALICE);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "hello bob, this is secret");
    assert_eq!(b[0].1.as_deref(), Some("omemo:2"));
    assert_eq!(b[0].2, "in");

    // Bob answers with an empty message (key exchange ack). It makes no row.
    let sent = sent_messages(&mut p.bob);
    assert_eq!(sent.len(), 1);
    let ack = Encrypted::find(&sent[0]).unwrap().unwrap();
    assert!(ack.payload.is_none());
    deliver(&mut p.alice, "bob@chord.localhost/laptop", sent[0].clone());
    assert_eq!(rows(&p.alice, BOB).len(), 1);

    // Bob replies. His client reads the device list of Alice first.
    send_chat(&mut p.bob, ALICE, "hi alice");
    if has_pending(&p.bob, |x| is_list(x, ALICE)) {
        answer_list(&mut p.bob, ALICE, p.pa.list.as_ref());
    }
    let mut sent = sent_messages(&mut p.bob);
    let reply = sent.pop().unwrap();
    assert!(!message_xml(&reply).contains("hi alice"));
    let enc = Encrypted::find(&reply).unwrap().unwrap();
    assert!(!enc.key_for(&bare(ALICE), p.pa.device).unwrap().kex);
    deliver(&mut p.alice, "bob@chord.localhost/laptop", reply);
    let a = rows(&p.alice, BOB);
    assert_eq!(a.len(), 2);
    assert_eq!(a[1].0, "hi alice");
    assert_eq!(a[1].1.as_deref(), Some("omemo:2"));
    assert_eq!(a[1].2, "in");
}

#[test]
fn a_correction_a_reaction_and_a_retraction_are_encrypted_too() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    alice_sends(&mut p, "first");
    // The same peer, now with a session: send other kinds of messages.
    let reaction: Element = "<reactions xmlns='urn:xmpp:reactions:0' id='x1'><reaction>\u{1f44d}</reaction></reactions>"
        .parse()
        .unwrap();
    let correction: Element = "<replace xmlns='urn:xmpp:message-correct:0' id='x1'/>"
        .parse()
        .unwrap();
    let retract: Element = "<retract xmlns='urn:xmpp:message-retract:1' id='x1'/>"
        .parse()
        .unwrap();
    for (payload, body, name) in [
        (reaction, None, "reactions"),
        (correction, Some("the fixed text"), "message-correct"),
        (retract, None, "retract"),
    ] {
        let mut m = Message::chat(Jid::new(BOB).unwrap());
        if let Some(body) = body {
            m = m.with_body("".into(), body.into());
        }
        m.payloads.push(payload);
        m.payloads
            .push(Element::builder("store", "urn:xmpp:hints").build());
        p.alice
            .with_ctx(|ctx| send::outgoing(ctx, &bare(BOB), m))
            .unwrap();
        serve_bob(&mut p);
        let sent = sent_messages(&mut p.alice);
        assert_eq!(sent.len(), 1, "{name}");
        let text = message_xml(&sent[0]);
        assert!(
            !text.contains(name) && !text.contains("fixed text") && !text.contains("\u{1f44d}"),
            "{name}: {text}"
        );
        assert!(text.contains("urn:xmpp:omemo:2"));
    }
}

#[test]
fn a_marker_has_nothing_secret_and_goes_out_as_it_is() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let mut m = Message::chat(Jid::new(BOB).unwrap());
    m.payloads.push(
        "<displayed xmlns='urn:xmpp:chat-markers:0' id='x1'/>"
            .parse()
            .unwrap(),
    );
    p.alice
        .with_ctx(|ctx| send::outgoing(ctx, &bare(BOB), m))
        .unwrap();
    let sent = sent_messages(&mut p.alice);
    assert!(message_xml(&sent[0]).contains("displayed"));
}

#[test]
fn a_peer_with_no_device_fails_the_send_and_never_sends_plain_text() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let id = send_chat(&mut p.alice, BOB, "no way");
    answer_list(&mut p.alice, BOB, None);
    assert!(sent_messages(&mut p.alice).is_empty());
    let a = rows(&p.alice, BOB);
    assert_eq!(a.len(), 1);
    assert!(a[0].3, "the row is failed");
    let ev = events(&mut p.alice);
    assert!(ev.iter().any(|e| matches!(e,
        ClientEvent::OmemoSendFailed { peer, message_id, reason }
        if peer.as_str() == BOB && *message_id == id && reason.contains("no OMEMO device"))));
}

#[test]
fn a_device_with_no_bundle_is_left_out_and_with_no_other_device_the_send_fails() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    send_chat(&mut p.alice, BOB, "x");
    answer_list(&mut p.alice, BOB, p.pb.list.as_ref());
    let id = p.pb.device;
    p.alice.respond(|x| is_bundle(x, BOB, id), not_found());
    assert!(sent_messages(&mut p.alice).is_empty());
    assert!(rows(&p.alice, BOB)[0].3);
}

#[test]
fn a_lost_connection_fails_a_message_that_waits() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    send_chat(&mut p.alice, BOB, "x");
    p.alice.respond(|x| is_list(x, BOB), IqResponse::Lost);
    assert!(sent_messages(&mut p.alice).is_empty());
    assert!(rows(&p.alice, BOB)[0].3);
}

#[test]
fn a_tampered_payload_gives_a_failed_row_with_no_fallback_text() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let mut m = alice_sends(&mut p, "tamper me");
    let mut enc = Encrypted::find(&m).unwrap().unwrap();
    let payload = enc.payload.as_mut().unwrap();
    let last = payload.len() - 1;
    payload[last] ^= 1;
    m.payloads.retain(|x| !x.is("encrypted", NS_OMEMO));
    m.payloads.push(Element::from(&enc));
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    let b = rows(&p.bob, ALICE);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, receive::CANNOT_READ);
    assert_eq!(b[0].1.as_deref(), Some("omemo:2:failed"));
    assert!(!b[0].0.contains("encrypted with OMEMO"));
}

#[test]
fn a_message_without_a_key_for_this_device_gives_one_failed_row() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let mut m = alice_sends(&mut p, "for another device");
    let mut enc = Encrypted::find(&m).unwrap().unwrap();
    for group in &mut enc.keys {
        for key in &mut group.keys {
            key.rid += 1;
        }
    }
    m.payloads.retain(|x| !x.is("encrypted", NS_OMEMO));
    m.payloads.push(Element::from(&enc));
    deliver(&mut p.bob, "alice@chord.localhost/phone", m.clone());
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    let b = rows(&p.bob, ALICE);
    // The same stanza twice has the same id and gives one row.
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].1.as_deref(), Some("omemo:2:failed"));
}

#[test]
fn a_replayed_message_makes_no_second_row() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let m = alice_sends(&mut p, "once");
    deliver(&mut p.bob, "alice@chord.localhost/phone", m.clone());
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    assert_eq!(rows(&p.bob, ALICE).len(), 1);
}

#[test]
fn an_archive_copy_from_before_this_device_gives_the_failed_row_once() {
    // Bob has two devices. The message is for the first one. The second is new.
    let mut p = pair();
    let mut bob2 = Harness::for_account(BOB);
    connect(&mut bob2, Some(p.pb.list.as_ref().unwrap()));
    enable(&mut p.alice, BOB);
    let m = alice_sends(&mut p, "old news");
    for _ in 0..2 {
        let archived = m.clone();
        let mut archived = archived;
        archived.from = Some(Jid::new("alice@chord.localhost/phone").unwrap());
        bob2.with_ctx(|ctx| chat::store_archived(ctx, &archived, "arch-1", Some(1_000)));
    }
    let b = rows(&bob2, ALICE);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, receive::CANNOT_READ);
    assert_eq!(b[0].1.as_deref(), Some("omemo:2:failed"));
}

#[test]
fn an_archive_copy_of_a_message_we_sent_here_only_changes_its_key() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let m = alice_sends(&mut p, "mine");
    let mut archived = m.clone();
    archived.from = Some(Jid::new("alice@chord.localhost/phone").unwrap());
    p.alice
        .with_ctx(|ctx| chat::store_archived(ctx, &archived, "arch-9", Some(5)));
    let a = rows(&p.alice, BOB);
    assert_eq!(a.len(), 1, "no failed row beside our own message");
    assert_eq!(a[0].0, "mine");
    let key: String = p
        .alice
        .store
        .conn()
        .query_row("SELECT key FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(key, "arch-9");
}

#[test]
fn a_restart_keeps_the_session() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let m = alice_sends(&mut p, "one");
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    let acks = sent_messages(&mut p.bob);
    deliver(&mut p.alice, "bob@chord.localhost/laptop", acks[0].clone());

    // Alice restarts with the same database.
    let Pair {
        alice, mut bob, pb, ..
    } = p;
    let mut alice = Harness::with_store(alice.store, ALICE);
    let own = list_of(&[device_id(&alice)]);
    alice.with_ctx(|ctx| {
        super::on_connected(ctx);
        super::on_services_ready(ctx);
    });
    answer_list(&mut alice, ALICE, Some(&own));
    settle(&mut alice);
    send_chat(&mut alice, BOB, "two");
    answer_list(&mut alice, BOB, pb.list.as_ref());
    // No bundle is needed: the session is in the store.
    assert!(
        !alice
            .pending
            .values()
            .any(|x| matches!(&x.then, FeaturePending::Omemo(Pending::Bundle { .. })))
    );
    let sent = sent_messages(&mut alice);
    assert_eq!(sent.len(), 1);
    let enc = Encrypted::find(&sent[0]).unwrap().unwrap();
    assert!(!enc.key_for(&bare(BOB), pb.device).unwrap().kex);
    deliver(&mut bob, "alice@chord.localhost/phone", sent[0].clone());
    let b = rows(&bob, ALICE);
    assert_eq!(b.last().unwrap().0, "two");
}

#[test]
fn a_second_device_of_ours_gets_the_message_in_a_carbon_and_reads_it() {
    let mut a2 = Harness::for_account(ALICE);
    let pa2 = connect(&mut a2, None);
    let mut a1 = Harness::for_account(ALICE);
    let pa1 = connect(&mut a1, Some(pa2.list.as_ref().unwrap()));
    let mut bob = Harness::for_account(BOB);
    let pb = connect(&mut bob, None);
    enable(&mut a1, BOB);
    send_chat(&mut a1, BOB, "from my phone");
    answer_list(&mut a1, BOB, pb.list.as_ref());
    // The device a2 is one of our own devices: its bundle is needed too.
    answer_bundle(&mut a1, ALICE, pa2.device, pa2.bundle.as_ref().unwrap());
    answer_bundle(&mut a1, BOB, pb.device, pb.bundle.as_ref().unwrap());
    let mut sent = sent_messages(&mut a1);
    let m = sent.pop().unwrap();
    let enc = Encrypted::find(&m).unwrap().unwrap();
    assert!(enc.key_for(&bare(ALICE), pa2.device).is_some());
    assert!(enc.key_for(&bare(ALICE), pa1.device).is_none());
    assert!(enc.key_for(&bare(BOB), pb.device).is_some());

    // The carbon of the sent message goes to a2.
    let mut inner = m.clone();
    inner.from = Some(Jid::new("alice@chord.localhost/phone").unwrap());
    let carbon = format!(
        "<message xmlns='jabber:client' from='{ALICE}' to='{ALICE}/desktop' type='chat'>\
         <sent xmlns='urn:xmpp:carbons:2'><forwarded xmlns='urn:xmpp:forward:0'>{}</forwarded></sent></message>",
        to_xml(&Element::from(inner))
    );
    let carbon = Message::try_from(carbon.parse::<Element>().unwrap()).unwrap();
    a2.with_ctx(|ctx| features::on_stanza(ctx, Stanza::Message(carbon)));
    let rows = rows(&a2, BOB);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "from my phone");
    assert_eq!(rows[0].1.as_deref(), Some("omemo:2"));
    assert_eq!(rows[0].2, "out");
}

#[test]
fn an_undecided_device_gets_no_key() {
    // Bob has two devices. Alice verifies the first one. The second is a new key: undecided.
    let mut p = pair();
    let mut bob2 = Harness::for_account(BOB);
    let pb2 = connect(&mut bob2, Some(p.pb.list.as_ref().unwrap()));
    enable(&mut p.alice, BOB);
    // Alice knows only the first device of Bob for now.
    send_chat(&mut p.alice, BOB, "first");
    answer_list(&mut p.alice, BOB, Some(&list_of(&[p.pb.device])));
    serve_bob(&mut p);
    let _ = sent_messages(&mut p.alice);
    let devices = trust::devices(
        &p.alice.store,
        p.alice.account_id,
        &p.alice.account,
        &bare(BOB),
    )
    .unwrap();
    let fingerprint = devices[0].fingerprint.clone().unwrap();
    assert_eq!(devices[0].trust, Some(OmemoTrust::Blind));
    p.alice.with_ctx(|ctx| {
        super::on_command(ctx, {
            let (tx, _rx) = oneshot::channel();
            Command::SetTrust {
                jid: bare(BOB),
                fingerprint: fingerprint.to_uppercase().replace(' ', ""),
                trust: OmemoTrust::Verified,
                reply: tx,
            }
        })
    });
    // Bob adds a device. Alice reads the new list and the bundle of the new device.
    let both = list_of(&[p.pb.device, pb2.device]);
    let event = format!(
        "<message xmlns='jabber:client' from='{BOB}'><event xmlns='http://jabber.org/protocol/pubsub#event'>\
         <items node='{NODE_DEVICES}'><item id='current'>{}</item></items></event></message>",
        to_xml(&both)
    );
    let event = Message::try_from(event.parse::<Element>().unwrap()).unwrap();
    p.alice
        .with_ctx(|ctx| features::on_stanza(ctx, Stanza::Message(event)));
    send_chat(&mut p.alice, BOB, "second");
    answer_bundle(&mut p.alice, BOB, pb2.device, pb2.bundle.as_ref().unwrap());
    // The old device needs no new bundle: it has a session.
    let sent = sent_messages(&mut p.alice);
    assert_eq!(sent.len(), 1);
    let enc = Encrypted::find(&sent[0]).unwrap().unwrap();
    assert!(enc.key_for(&bare(BOB), p.pb.device).is_some());
    assert!(
        enc.key_for(&bare(BOB), pb2.device).is_none(),
        "undecided gets no key"
    );
    let ev = events(&mut p.alice);
    assert!(ev.iter().any(|e| matches!(e,
        ClientEvent::OmemoTrustNeeded { jid, device_id, .. }
        if jid.as_str() == BOB && *device_id == pb2.device)));

    // The user decides: verified. The next message goes to both devices.
    let devices = trust::devices(
        &p.alice.store,
        p.alice.account_id,
        &p.alice.account,
        &bare(BOB),
    )
    .unwrap();
    let new = devices.iter().find(|d| d.device_id == pb2.device).unwrap();
    assert_eq!(new.trust, Some(OmemoTrust::Undecided));
    let (tx, mut rx) = oneshot::channel();
    p.alice.with_ctx(|ctx| {
        super::on_command(
            ctx,
            Command::SetTrust {
                jid: bare(BOB),
                fingerprint: new.fingerprint.clone().unwrap(),
                trust: OmemoTrust::Verified,
                reply: tx,
            },
        )
    });
    assert!(rx.try_recv().unwrap().unwrap().is_ok());
    send_chat(&mut p.alice, BOB, "third");
    // The bundle was used for no session yet: it is still here, so no new request.
    let sent = sent_messages(&mut p.alice);
    assert_eq!(sent.len(), 1);
    let enc = Encrypted::find(&sent[0]).unwrap().unwrap();
    assert!(enc.key_for(&bare(BOB), pb2.device).is_some());
}

#[test]
fn when_every_device_is_undecided_the_send_fails_at_once() {
    let mut h = Harness::for_account(ALICE);
    connect(&mut h, None);
    enable(&mut h, BOB);
    // Bob has one key, the user verified it, and then a new device came: only undecided.
    let first = [1u8; 32];
    let second = [2u8; 32];
    h.with_ctx(|ctx| {
        trust::learn_key(ctx, &bare(BOB), 1, &first);
        store::set_trust(
            ctx.store,
            ctx.account_id,
            BOB,
            &first,
            store::TrustLevel::Verified,
        )
        .unwrap();
        trust::learn_key(ctx, &bare(BOB), 2, &second);
        store::mark_devices_inactive_except(ctx.store, ctx.account_id, BOB, &[2]).unwrap();
    });
    let id = send_chat(&mut h, BOB, "nope");
    assert!(sent_messages(&mut h).is_empty());
    assert!(
        h.pending
            .values()
            .all(|p| !matches!(&p.then, FeaturePending::Omemo(Pending::Bundle { .. })))
    );
    assert!(rows(&h, BOB)[0].3);
    assert!(events(&mut h).iter().any(|e| matches!(e,
        ClientEvent::OmemoSendFailed { message_id, reason, .. }
        if *message_id == id && reason.contains("No trusted"))));
}

#[test]
fn blind_trust_before_verification() {
    let h = Harness::for_account(ALICE);
    let j = BOB;
    let key = |n: u8| [n; 32];
    let level = |k: &[u8; 32]| trust::level_for(&h.store, h.account_id, j, k).unwrap();
    use store::TrustLevel::*;
    assert_eq!(level(&key(1)), Blind);
    assert_eq!(level(&key(2)), Blind);
    // The user verifies one key. New keys are undecided from now on.
    store::set_trust(&h.store, h.account_id, j, &key(1), Verified).unwrap();
    assert_eq!(level(&key(3)), Undecided);
    assert_eq!(level(&key(4)), Undecided);
    // Known keys keep their level.
    assert_eq!(level(&key(1)), Verified);
    assert_eq!(level(&key(2)), Blind);
    assert_eq!(level(&key(3)), Undecided);
    // The user distrusts a key.
    store::set_trust(&h.store, h.account_id, j, &key(2), Distrusted).unwrap();
    assert_eq!(level(&key(2)), Distrusted);
    // Another contact is not touched.
    let other = trust::level_for(&h.store, h.account_id, "carol@chord.localhost", &key(9)).unwrap();
    assert_eq!(other, Blind);
    assert!(trust::usable(Blind) && trust::usable(Verified));
    assert!(!trust::usable(Undecided) && !trust::usable(Distrusted));
}

#[test]
fn the_trust_command_finds_a_key_by_its_fingerprint_and_rejects_an_unknown_one() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    alice_sends(&mut p, "x");
    let (tx, mut rx) = oneshot::channel();
    p.alice.with_ctx(|ctx| {
        super::on_command(
            ctx,
            Command::SetTrust {
                jid: bare(BOB),
                fingerprint: "00000000 11111111".into(),
                trust: OmemoTrust::Distrusted,
                reply: tx,
            },
        )
    });
    assert!(rx.try_recv().unwrap().unwrap().is_err());
    let devices = trust::devices(
        &p.alice.store,
        p.alice.account_id,
        &p.alice.account,
        &bare(BOB),
    )
    .unwrap();
    let (tx, mut rx) = oneshot::channel();
    p.alice.with_ctx(|ctx| {
        super::on_command(
            ctx,
            Command::SetTrust {
                jid: bare(BOB),
                fingerprint: devices[0].fingerprint.clone().unwrap(),
                trust: OmemoTrust::Distrusted,
                reply: tx,
            },
        )
    });
    assert!(rx.try_recv().unwrap().unwrap().is_ok());
    let devices = trust::devices(
        &p.alice.store,
        p.alice.account_id,
        &p.alice.account,
        &bare(BOB),
    )
    .unwrap();
    assert_eq!(devices[0].trust, Some(OmemoTrust::Distrusted));
}

#[test]
fn a_distrusted_sender_gives_a_failed_row() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let m = alice_sends(&mut p, "from a bad device");
    // Bob already distrusts the key of Alice.
    let key =
        p.pa.bundle
            .as_ref()
            .map(|b| BundleXml::try_from(b).unwrap().ik)
            .unwrap();
    store::set_trust(
        &p.bob.store,
        p.bob.account_id,
        ALICE,
        &key,
        store::TrustLevel::Distrusted,
    )
    .unwrap();
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    let b = rows(&p.bob, ALICE);
    assert_eq!(b[0].1.as_deref(), Some("omemo:2:failed"));
    assert!(!b[0].0.contains("from a bad device"));
}

#[test]
fn prekeys_are_refilled_after_a_key_exchange_uses_one() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    // Bob has 25 prekeys. One use leaves 24, which is under the minimum.
    {
        let engine = p.bob.state.omemo.engine.as_mut().unwrap();
        let bundle = engine.keys.bundle().unwrap();
        for (id, _) in bundle.prekeys.iter().take(75) {
            engine.keys.remove_prekey(*id);
        }
        assert_eq!(engine.keys.prekey_count(), 25);
        p.pb.bundle = Some(Element::from(&engine.keys.bundle().unwrap()));
    }
    let m = alice_sends(&mut p, "use a prekey");
    let _ = p.bob.sent_iqs();
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    assert_eq!(rows(&p.bob, ALICE).len(), 1);
    let engine = p.bob.state.omemo.engine.as_ref().unwrap();
    assert_eq!(engine.keys.prekey_count(), 100);
    let again = published(&p.bob.sent_iqs());
    let bundle = BundleXml::try_from(again.bundle.as_ref().unwrap()).unwrap();
    assert_eq!(bundle.prekeys.len(), 100);
    // The keys are saved: a restart has them.
    let id = device_id(&p.bob);
    let restarted = Harness::with_store(p.bob.store, BOB);
    let sealer =
        store::Sealer::open_or_create(&restarted.store, restarted.account_id, BOB, None).unwrap();
    let (stored_id, blob) = store::load_identity(&restarted.store, &sealer, restarted.account_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored_id, id);
    let keys = crypto::Keys::from_bytes(&blob).unwrap();
    assert_eq!(keys.prekey_count(), 100);
}

#[test]
fn the_signed_prekey_rotates_after_seven_days_and_old_ones_go_after_thirty() {
    let mut h = Harness::for_account(ALICE);
    connect(&mut h, None);
    let first = h
        .state
        .omemo
        .engine
        .as_ref()
        .unwrap()
        .keys
        .current_signed_prekey_id();
    let day = 24 * 60 * 60 * 1000;
    let now = super::now_ms(&h.store);
    let current = |h: &Harness| {
        h.state
            .omemo
            .engine
            .as_ref()
            .unwrap()
            .keys
            .current_signed_prekey_id()
    };
    let count = |h: &Harness| {
        h.state
            .omemo
            .engine
            .as_ref()
            .unwrap()
            .keys
            .signed_prekeys()
            .unwrap()
            .len()
    };
    h.with_ctx(|ctx| super::device::maintain_at(ctx, now + 6 * day));
    assert_eq!(current(&h), first, "a young key stays");
    assert!(published(&h.sent_iqs()).bundle.is_none());
    // After 8 days the key is replaced, the old one stays, and the bundle goes out again.
    h.with_ctx(|ctx| super::device::maintain_at(ctx, now + 8 * day));
    assert_ne!(current(&h), first);
    assert_eq!(count(&h), 2);
    assert!(published(&h.sent_iqs()).bundle.is_some());
    // 31 days on, the first key has been replaced for more than 30 days: it goes.
    h.with_ctx(|ctx| super::device::maintain_at(ctx, now + 31 * day));
    let keys = h
        .state
        .omemo
        .engine
        .as_ref()
        .unwrap()
        .keys
        .signed_prekeys()
        .unwrap();
    assert!(keys.iter().all(|(id, _)| *id != first));
}

#[test]
fn lost_keys_start_a_new_device_and_keep_the_choices_of_the_user() {
    let secrets = Arc::new(MemorySecrets::default());
    let mut h = Harness::for_account(ALICE);
    h.state.omemo.set_secret_store(secrets.clone());
    connect(&mut h, None);
    let old = device_id(&h);
    enable(&mut h, BOB);
    store::set_trust(
        &h.store,
        h.account_id,
        BOB,
        &[7u8; 32],
        store::TrustLevel::Verified,
    )
    .unwrap();
    secrets.map.lock().unwrap().clear();
    // A new session through the real wiring.
    h.with_ctx(|ctx| features::on_connected(ctx, false, &[]));
    assert!(
        events(&mut h)
            .iter()
            .any(|e| matches!(e, ClientEvent::OmemoKeysReset))
    );
    assert_ne!(device_id(&h), old);
    assert!(h.state.omemo.engine.is_some());
    // The switch and the trust stay.
    assert!(store::conversation_enabled(&h.store, h.account_id, BOB).unwrap());
    assert_eq!(
        store::get_trust(&h.store, h.account_id, BOB, &[7u8; 32]).unwrap(),
        Some(store::TrustLevel::Verified)
    );
}

#[test]
fn the_session_start_wires_omemo_in_and_keeps_the_secret_store() {
    let secrets = Arc::new(MemorySecrets::default());
    let mut h = Harness::for_account(ALICE);
    h.state.omemo.set_secret_store(secrets.clone());
    h.with_ctx(|ctx| features::on_connected(ctx, false, &[]));
    assert!(h.state.omemo.engine.is_some());
    // The data key went to the secret store, and not into the table.
    assert_eq!(secrets.map.lock().unwrap().len(), 1);
    // A second session keeps the store and the keys.
    let id = device_id(&h);
    h.with_ctx(|ctx| features::on_connected(ctx, false, &[]));
    assert_eq!(device_id(&h), id);
    assert_eq!(secrets.map.lock().unwrap().len(), 1);
}

#[test]
fn we_can_remove_another_device_of_ours_and_not_this_one() {
    let mut h = Harness::for_account(ALICE);
    connect(&mut h, Some(&list_of(&[777])));
    let id = device_id(&h);
    let (tx, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        super::on_command(
            ctx,
            Command::RemoveOwnDevice {
                device_id: id,
                reply: tx,
            },
        )
    });
    assert!(rx.try_recv().unwrap().unwrap().is_err());

    let (tx, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        super::on_command(
            ctx,
            Command::RemoveOwnDevice {
                device_id: 777,
                reply: tx,
            },
        )
    });
    h.answer(
        |p| matches!(p, FeaturePending::Omemo(Pending::RemoveFetch { .. })),
        Some(items_result(NODE_DEVICES, "current", &list_of(&[777, id]))),
    );
    let p = published(&h.sent_iqs());
    assert_eq!(ids_of(p.list.as_ref().unwrap()), vec![id]);
    h.answer(
        |p| matches!(p, FeaturePending::Omemo(Pending::RemovePublish { .. })),
        None,
    );
    assert!(rx.try_recv().unwrap().unwrap().is_ok());
    let own = trust::devices(&h.store, h.account_id, &h.account, &h.account).unwrap();
    assert!(
        own.iter()
            .find(|d| d.device_id == 777)
            .is_some_and(|d| !d.active)
    );
    assert!(
        own.iter()
            .find(|d| d.device_id == id)
            .is_some_and(|d| d.active)
    );
}

#[test]
fn commands_work_offline_from_the_store() {
    let h = Harness::for_account(ALICE);
    let (tx, mut rx) = oneshot::channel();
    super::offline(
        &h.store,
        h.account_id,
        Command::SetEnabled {
            peer: bare(BOB),
            enabled: true,
            reply: tx,
        },
    );
    assert!(rx.try_recv().unwrap().unwrap().is_ok());
    let (tx, mut rx) = oneshot::channel();
    super::offline(
        &h.store,
        h.account_id,
        Command::Enabled {
            peer: bare(BOB),
            reply: tx,
        },
    );
    assert!(rx.try_recv().unwrap().unwrap().unwrap());
    let (tx, mut rx) = oneshot::channel();
    super::offline(&h.store, h.account_id, Command::OwnDevices { reply: tx });
    assert!(rx.try_recv().unwrap().unwrap().unwrap().is_empty());
    let (tx, mut rx) = oneshot::channel();
    super::offline(
        &h.store,
        h.account_id,
        Command::Refresh {
            jid: bare(BOB),
            reply: tx,
        },
    );
    assert!(rx.try_recv().unwrap().unwrap().is_err());
}

#[test]
fn the_switch_is_off_by_default() {
    let h = Harness::for_account(ALICE);
    assert!(!store::conversation_enabled(&h.store, h.account_id, BOB).unwrap());
    let _ = State::default();
}

#[test]
fn a_refresh_reads_the_list_and_the_bundles_and_answers() {
    let mut p = pair();
    let (tx, mut rx) = oneshot::channel();
    p.alice.with_ctx(|ctx| {
        super::on_command(
            ctx,
            Command::Refresh {
                jid: bare(BOB),
                reply: tx,
            },
        )
    });
    serve_bob(&mut p);
    assert!(rx.try_recv().unwrap().unwrap().is_ok());
    let devices = trust::devices(
        &p.alice.store,
        p.alice.account_id,
        &p.alice.account,
        &bare(BOB),
    )
    .unwrap();
    assert_eq!(devices.len(), 1);
    assert!(devices[0].fingerprint.is_some());
    assert_eq!(devices[0].trust, Some(OmemoTrust::Blind));
}

#[test]
fn an_encrypted_message_of_a_legacy_namespace_keeps_the_notice() {
    let mut h = Harness::for_account(ALICE);
    connect(&mut h, None);
    let m: Element = format!(
        "<message xmlns='jabber:client' from='{BOB}/x' to='{ALICE}' type='chat' id='l1'>\
         <body>I sent you an OMEMO message</body>\
         <encryption xmlns='urn:xmpp:eme:0' namespace='eu.siacs.conversations.axolotl'/></message>"
    )
    .parse()
    .unwrap();
    h.with_ctx(|ctx| features::on_stanza(ctx, Stanza::Message(Message::try_from(m).unwrap())));
    let r = rows(&h, BOB);
    assert_eq!(r[0].0, "Encrypted message (OMEMO) that Chord cannot read.");
    assert_eq!(r[0].1, None);
}

#[test]
fn a_forged_reaction_outside_the_envelope_does_not_pass() {
    // The server adds a <replace/> next to the <encrypted/>. It must not reach the store.
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let mut m = alice_sends(&mut p, "real text");
    m.payloads.push(
        "<replace xmlns='urn:xmpp:message-correct:0' id='victim'/>"
            .parse()
            .unwrap(),
    );
    deliver(&mut p.bob, "alice@chord.localhost/phone", m);
    let b = rows(&p.bob, ALICE);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].0, "real text");
}

#[test]
fn a_real_correction_goes_encrypted_and_changes_the_row_of_the_peer() {
    use crate::features::corrections;
    let mut p = pair();
    enable(&mut p.alice, BOB);
    let id = send_chat(&mut p.alice, BOB, "a typpo");
    serve_bob(&mut p);
    let first = sent_messages(&mut p.alice).pop().unwrap();
    deliver(&mut p.bob, "alice@chord.localhost/phone", first);
    let (tx, mut rx) = oneshot::channel();
    p.alice.with_ctx(|ctx| {
        corrections::on_command(
            ctx,
            corrections::Command::Edit {
                item_id: format!("origin-id:{id}"),
                body: "a typo".into(),
                reply: tx,
            },
        )
    });
    assert!(rx.try_recv().unwrap().unwrap().is_ok());
    let sent = sent_messages(&mut p.alice);
    assert_eq!(sent.len(), 1);
    let text = message_xml(&sent[0]);
    assert!(
        !text.contains("typo") && !text.contains("message-correct"),
        "{text}"
    );
    deliver(&mut p.bob, "alice@chord.localhost/phone", sent[0].clone());
    let shown: String = p
        .bob
        .store
        .conn()
        .query_row(
            "SELECT COALESCE(edited_body, body) FROM messages",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(shown, "a typo");
    assert_eq!(rows(&p.bob, ALICE).len(), 1, "the correction has no row");
}

#[test]
fn a_chat_with_ourselves_does_not_double_the_keys() {
    let mut a2 = Harness::for_account(ALICE);
    let pa2 = connect(&mut a2, None);
    let mut a1 = Harness::for_account(ALICE);
    connect(&mut a1, Some(pa2.list.as_ref().unwrap()));
    enable(&mut a1, ALICE);
    send_chat(&mut a1, ALICE, "note to self");
    // Turning it on made a refresh of our own list. The answer has both devices.
    let both = list_of(&[pa2.device, device_id(&a1)]);
    answer_list(&mut a1, ALICE, Some(&both));
    answer_bundle(&mut a1, ALICE, pa2.device, pa2.bundle.as_ref().unwrap());
    let m = sent_messages(&mut a1).pop().unwrap();
    let enc = Encrypted::find(&m).unwrap().unwrap();
    assert_eq!(enc.keys.len(), 1);
    assert_eq!(enc.keys[0].keys.len(), 1);
}

#[test]
fn a_heartbeat_goes_to_the_sender_device_once_at_counter_53() {
    let mut p = pair();
    enable(&mut p.alice, BOB);
    enable(&mut p.bob, ALICE);
    // Alice sends 60 messages and never reads an answer from Bob, so the ratchet key of
    // Alice stays the same.
    let first = alice_sends(&mut p, "message 0");
    let mut empty_after = Vec::new();
    for n in 0..60 {
        let m = if n == 0 {
            first.clone()
        } else {
            send_chat(&mut p.alice, BOB, &format!("message {n}"));
            sent_messages(&mut p.alice).pop().unwrap()
        };
        deliver(&mut p.bob, "alice@chord.localhost/phone", m);
        let out = sent_messages(&mut p.bob);
        for o in &out {
            let enc = Encrypted::find(o).unwrap().unwrap();
            assert!(enc.payload.is_none(), "an answer is an empty message");
            assert_eq!(o.to.as_ref().unwrap().to_bare(), bare(ALICE));
            assert_eq!(enc.keys.len(), 1);
            assert_eq!(
                enc.keys[0].keys.len(),
                1,
                "only the sender device gets a key"
            );
            assert_eq!(enc.keys[0].keys[0].rid, p.pa.device);
        }
        if !out.is_empty() {
            empty_after.push((n, out.len()));
        }
    }
    // The key exchange answer after message 0, and one heartbeat at message 53.
    assert_eq!(empty_after, vec![(0, 1), (53, 1)]);
    assert_eq!(rows(&p.bob, ALICE).len(), 60, "empty messages make no rows");
}
