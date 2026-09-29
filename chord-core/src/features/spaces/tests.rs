use super::*;
use crate::features::testing::Harness;
use xmpp_parsers::disco::Identity;
use xmpp_parsers::pubsub::event::Event;
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType};

const SERVICE: &str = "pubsub.chord.localhost";

type Answer = Result<JoinOutcome, ClientError>;

fn xml(s: &str) -> Element {
    s.parse().unwrap()
}

/// A harness whose disco state has a pubsub service. Spaces are not started.
fn harness() -> Harness {
    let mut h = Harness::new();
    let mut info = DiscoInfoResult {
        node: None,
        identities: vec![Identity::new("pubsub", "service", "en", "Pubsub")],
        features: Default::default(),
        extensions: vec![],
    };
    info.features
        .insert("http://jabber.org/protocol/pubsub#subscribe".into());
    h.state
        .disco
        .services
        .push((Jid::new(SERVICE).unwrap(), info));
    h.state.disco.complete = true;
    h
}

fn is_subscriptions(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::Subscriptions { .. }))
}

fn is_info(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::NodeInfo { .. }))
}

fn is_items(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::Items { .. }))
}

fn is_done(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::Done { .. }))
}

fn is_subscribe(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::Subscribe { .. }))
}

fn is_browse_info(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::BrowseInfo { .. }))
}

/// A harness after the start: the space `dev` is followed and has one room.
fn followed() -> Harness {
    let mut h = harness();
    h.with_ctx(on_connected);
    h.answer(
        is_subscriptions,
        Some(xml(&format!(
            "<pubsub xmlns='{0}'><subscriptions>
               <subscription node='dev' jid='alice@chord.localhost' subscription='subscribed'/>
               </subscriptions></pubsub>",
            ns::PUBSUB
        ))),
    );
    h.answer(
        is_info,
        Some(node_info("dev", NS_SPACES, "The dev corner", "open")),
    );
    h.answer(
        is_items,
        Some(items_result(
            "dev",
            "<item id='room1@rooms.chord.localhost'>
               <conference xmlns='urn:xmpp:bookmarks:1' name='Room One'/></item>",
        )),
    );
    h.take_dirty();
    h.take_sent();
    h
}

fn node_info(node: &str, type_: &str, title: &str, access: &str) -> Element {
    xml(&format!(
        "<query xmlns='http://jabber.org/protocol/disco#info' node='{node}'>
           <identity category='pubsub' type='leaf'/>
           <x xmlns='jabber:x:data' type='result'>
             <field var='FORM_TYPE' type='hidden'><value>{NS_META}</value></field>
             <field var='pubsub#type'><value>{type_}</value></field>
             <field var='pubsub#title'><value>{title}</value></field>
             <field var='pubsub#access_model'><value>{access}</value></field>
           </x></query>"
    ))
}

fn items_result(node: &str, items: &str) -> Element {
    xml(&format!(
        "<pubsub xmlns='{}'><items node='{node}'>{items}</items></pubsub>",
        ns::PUBSUB
    ))
}

fn event(inner: &str) -> Payload {
    let e = xml(&format!(
        "<event xmlns='http://jabber.org/protocol/pubsub#event'>{inner}</event>"
    ));
    Event::try_from(e).unwrap().payload
}

fn column(h: &Harness, sql: &str) -> Vec<String> {
    let mut stmt = h.store.conn().prepare(sql).unwrap();
    stmt.query_map([], |r| r.get::<_, Option<String>>(0))
        .unwrap()
        .map(|v| v.unwrap().unwrap_or_else(|| "NULL".into()))
        .collect()
}

fn service_jid() -> Jid {
    Jid::new(SERVICE).unwrap()
}

fn service_bare() -> BareJid {
    BareJid::new(SERVICE).unwrap()
}

fn payload_of(iq: &Iq) -> String {
    match iq {
        Iq::Get { payload, .. } | Iq::Set { payload, .. } => String::from(payload),
        other => panic!("{other:?}"),
    }
}

fn forbidden() -> IqResponse {
    IqResponse::Error(StanzaError::new(
        ErrorType::Auth,
        DefinedCondition::Forbidden,
        "en",
        "",
    ))
}

#[test]
fn start_loads_subscribed_spaces_and_items() {
    let mut h = harness();
    h.with_ctx(on_connected);
    let sent = h.sent_iqs();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to().unwrap().as_str(), SERVICE);
    assert!(payload_of(&sent[0]).contains("<subscriptions"));
    // A second call does not send again.
    h.with_ctx(on_disco_complete);
    assert!(h.sent_iqs().is_empty());

    // A subscription of another JID, a pending one, and a node that is no space.
    h.answer(
        is_subscriptions,
        Some(xml(&format!(
            "<pubsub xmlns='{0}'><subscriptions>
               <subscription node='dev' jid='alice@chord.localhost' subscription='subscribed'/>
               <subscription node='other' jid='alice@chord.localhost' subscription='subscribed'/>
               <subscription node='theirs' jid='bob@chord.localhost' subscription='subscribed'/>
               <subscription node='wait' jid='alice@chord.localhost' subscription='pending'/>
               </subscriptions></pubsub>",
            ns::PUBSUB
        ))),
    );
    assert_eq!(h.sent_iqs().len(), 2, "one disco#info for dev and other");
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::NodeInfo { node, .. }) if node == "other"),
        Some(node_info("other", "http://example.org/x", "X", "open")),
    );
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::NodeInfo { node, .. }) if node == "dev"),
        Some(node_info("dev", NS_SPACES, "The dev corner", "open")),
    );
    assert_eq!(column(&h, "SELECT name FROM spaces"), ["The dev corner"]);
    assert_eq!(column(&h, "SELECT access_model FROM spaces"), ["open"]);
    assert_eq!(
        column(&h, "SELECT CAST(subscribed AS TEXT) FROM spaces"),
        ["1"]
    );
    let dirty = h.take_dirty();
    assert!(dirty.contains(&ViewKey::SpaceList));
    assert!(dirty.contains(&ViewKey::ChannelList(ChannelScope::Space {
        service: SERVICE.into(),
        node: "dev".into()
    })));

    h.answer(
        is_items,
        Some(items_result(
            "dev",
            &format!(
                "<item id='b@rooms.chord.localhost'>
                   <conference xmlns='urn:xmpp:bookmarks:1' name='Bee'/></item>
                 <item id='a@rooms.chord.localhost'>
                   <conference xmlns='urn:xmpp:bookmarks:1'/></item>
                 <item id='https://example.org/x'><x xmlns='jabber:x:oob'><url>https://example.org/x</url></x></item>
                 <item id='not a jid'><conference xmlns='urn:xmpp:bookmarks:1' name='Bad'/></item>
                 <item id='{AVATAR_ITEM}'>
                   <metadata xmlns='urn:xmpp:avatar:metadata'>
                     <info bytes='4' id='abc123' type='image/png'/></metadata></item>"
            ),
        )),
    );
    assert_eq!(
        column(
            &h,
            "SELECT room_jid || '/' || COALESCE(name, '-') FROM space_items
             WHERE room_jid IS NOT NULL ORDER BY position"
        ),
        ["b@rooms.chord.localhost/Bee", "a@rooms.chord.localhost/-"]
    );
    // The other items keep their XML and have no room.
    assert_eq!(
        column(
            &h,
            "SELECT item_id FROM space_items WHERE room_jid IS NULL AND payload IS NOT NULL
             ORDER BY item_id"
        ),
        ["https://example.org/x", "not a jid"]
    );
    assert_eq!(
        column(
            &h,
            "SELECT owner || ' ' || hash || ' ' || mime FROM avatars"
        ),
        [format!("{SERVICE}/dev abc123 image/png")]
    );
}

#[test]
fn start_forgets_a_space_that_we_do_not_follow() {
    let mut h = followed();
    h.state.spaces.started = false;
    h.with_ctx(on_disco_complete);
    h.answer(
        is_subscriptions,
        Some(xml(&format!(
            "<pubsub xmlns='{}'><subscriptions/></pubsub>",
            ns::PUBSUB
        ))),
    );
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
    assert!(column(&h, "SELECT item_id FROM space_items").is_empty());
}

#[test]
fn start_errors_change_nothing() {
    let mut h = followed();
    h.state.spaces.started = false;
    h.with_ctx(on_disco_complete);
    h.respond(is_subscriptions, IqResponse::Lost);
    assert_eq!(column(&h, "SELECT node FROM spaces"), ["dev"]);
    h.state.spaces.started = false;
    h.with_ctx(on_disco_complete);
    h.respond(is_subscriptions, forbidden());
    assert_eq!(column(&h, "SELECT node FROM spaces"), ["dev"]);
}

#[test]
fn no_pubsub_service_means_no_request() {
    let mut h = Harness::new();
    h.state.disco.complete = true;
    h.with_ctx(on_connected);
    assert!(h.sent_iqs().is_empty());
}

#[test]
fn item_events_from_the_service_update_the_tables() {
    let mut h = followed();
    h.with_ctx(|ctx| {
        on_event(
            ctx,
            &service_jid(),
            event(
                "<items node='dev'>
                   <item id='room2@rooms.chord.localhost'>
                     <conference xmlns='urn:xmpp:bookmarks:1' name='Room Two'/></item>
                   <retract id='room1@rooms.chord.localhost'/></items>",
            ),
        )
    });
    assert_eq!(
        column(&h, "SELECT room_jid FROM space_items"),
        ["room2@rooms.chord.localhost"]
    );
    assert!(h.take_dirty().contains(&ViewKey::SpaceList));
    // A new item goes last, and a publish of the same item keeps its place.
    h.with_ctx(|ctx| {
        on_event(
            ctx,
            &service_jid(),
            event(
                "<items node='dev'><item id='room3@rooms.chord.localhost'>
                   <conference xmlns='urn:xmpp:bookmarks:1' name='Three'/></item>
                   <item id='room2@rooms.chord.localhost'>
                   <conference xmlns='urn:xmpp:bookmarks:1' name='Two again'/></item></items>",
            ),
        )
    });
    assert_eq!(
        column(&h, "SELECT name FROM space_items ORDER BY position"),
        ["Two again", "Three"]
    );
}

#[test]
fn avatar_retract_and_purge_and_delete() {
    let mut h = followed();
    h.with_ctx(|ctx| {
        on_event(
            ctx,
            &service_jid(),
            event(&format!(
                "<items node='dev'><item id='{AVATAR_ITEM}'>
                   <metadata xmlns='urn:xmpp:avatar:metadata'>
                     <info bytes='4' id='h1' type='image/png'/></metadata></item></items>"
            )),
        )
    });
    assert_eq!(column(&h, "SELECT hash FROM avatars"), ["h1"]);
    h.with_ctx(|ctx| {
        on_event(
            ctx,
            &service_jid(),
            event(&format!(
                "<items node='dev'><retract id='{AVATAR_ITEM}'/></items>"
            )),
        )
    });
    assert!(column(&h, "SELECT hash FROM avatars").is_empty());

    h.with_ctx(|ctx| on_event(ctx, &service_jid(), event("<purge node='dev'/>")));
    assert!(column(&h, "SELECT item_id FROM space_items").is_empty());
    assert_eq!(column(&h, "SELECT node FROM spaces"), ["dev"]);

    h.with_ctx(|ctx| on_event(ctx, &service_jid(), event("<delete node='dev'/>")));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
    assert!(h.take_dirty().contains(&ViewKey::SpaceList));
}

#[test]
fn configuration_event_asks_for_the_metadata_again() {
    let mut h = followed();
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), event("<configuration node='dev'/>")));
    assert_eq!(h.sent_iqs().len(), 1);
    h.answer(
        is_info,
        Some(node_info("dev", NS_SPACES, "New name", "whitelist")),
    );
    assert_eq!(column(&h, "SELECT name FROM spaces"), ["New name"]);
    assert_eq!(column(&h, "SELECT access_model FROM spaces"), ["whitelist"]);
    assert!(h.sent_iqs().is_empty(), "no new items request");
}

#[test]
fn events_from_a_foreign_jid_or_for_an_unknown_node_are_dropped() {
    let mut h = followed();
    let evil = Jid::new("evil.example.org").unwrap();
    for payload in [
        "<delete node='dev'/>",
        "<purge node='dev'/>",
        "<items node='dev'><retract id='room1@rooms.chord.localhost'/></items>",
        "<configuration node='dev'/>",
    ] {
        h.with_ctx(|ctx| on_event(ctx, &evil, event(payload)));
    }
    // A user of the service, not the service itself.
    let user = Jid::new("pubsub.chord.localhost/x").unwrap();
    h.with_ctx(|ctx| on_event(ctx, &user, event("<delete node='dev'/>")));
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), event("<delete node='unknown'/>")));
    assert_eq!(column(&h, "SELECT node FROM spaces"), ["dev"]);
    assert_eq!(column(&h, "SELECT item_id FROM space_items").len(), 1);
    assert!(h.sent_iqs().is_empty());
    assert!(h.take_dirty().is_empty());
}

fn join(h: &mut Harness) -> oneshot::Receiver<Answer> {
    let (reply, rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::Join {
                service: service_bare(),
                node: "dev".into(),
                reply,
            },
        )
    });
    rx
}

fn subscription(state: &str) -> Element {
    xml(&format!(
        "<pubsub xmlns='{}'><subscription node='dev' jid='alice@chord.localhost'
           subscription='{state}'/></pubsub>",
        ns::PUBSUB
    ))
}

#[test]
fn join_subscribes_then_loads_info_and_items() {
    let mut h = harness();
    let mut rx = join(&mut h);
    let sent = h.sent_iqs();
    let text = payload_of(&sent[0]);
    assert!(text.contains("<subscribe") && text.contains("jid='alice@chord.localhost'"));
    h.answer(is_subscribe, Some(subscription("subscribed")));
    assert_eq!(rx.try_recv(), Ok(None), "not before info and items");
    h.answer(
        is_info,
        Some(node_info("dev", NS_SPACES, "Dev", "whitelist")),
    );
    h.answer(
        is_items,
        Some(items_result(
            "dev",
            "<item id='r@rooms.chord.localhost'>
               <conference xmlns='urn:xmpp:bookmarks:1' name='R'/></item>",
        )),
    );
    assert_eq!(rx.try_recv(), Ok(Some(Ok(JoinOutcome::Joined))));
    assert_eq!(column(&h, "SELECT name FROM spaces"), ["Dev"]);
    assert_eq!(column(&h, "SELECT name FROM space_items"), ["R"]);
}

#[test]
fn join_pending_error_and_lost() {
    let mut h = harness();
    let mut rx = join(&mut h);
    h.answer(is_subscribe, Some(subscription("pending")));
    assert_eq!(rx.try_recv(), Ok(Some(Ok(JoinOutcome::Pending))));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());

    let mut rx = join(&mut h);
    h.respond(is_subscribe, forbidden());
    let Ok(Some(Err(ClientError::Server(text)))) = rx.try_recv() else {
        panic!("expected a server error")
    };
    assert!(text.contains("Forbidden"), "{text}");

    let mut rx = join(&mut h);
    h.respond(is_subscribe, IqResponse::Lost);
    assert_eq!(rx.try_recv(), Ok(Some(Err(ClientError::NotConnected))));

    // Lost while the items load.
    let mut rx = join(&mut h);
    h.answer(is_subscribe, Some(subscription("subscribed")));
    h.answer(is_info, Some(node_info("dev", NS_SPACES, "Dev", "open")));
    h.respond(is_items, IqResponse::Lost);
    assert_eq!(rx.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
}

#[test]
fn join_keeps_the_space_when_the_metadata_fails() {
    let mut h = harness();
    let mut rx = join(&mut h);
    h.answer(is_subscribe, Some(subscription("subscribed")));
    h.respond(is_info, forbidden());
    h.answer(is_items, Some(items_result("dev", "")));
    assert_eq!(rx.try_recv(), Ok(Some(Ok(JoinOutcome::Joined))));
    assert_eq!(column(&h, "SELECT node FROM spaces"), ["dev"]);
}

#[test]
fn join_of_a_node_that_is_no_space_fails_and_unsubscribes() {
    let mut h = harness();
    let mut rx = join(&mut h);
    h.answer(is_subscribe, Some(subscription("subscribed")));
    h.take_sent();
    h.answer(
        is_info,
        Some(node_info("dev", "http://example.org/x", "X", "open")),
    );
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Invalid(_))))
    ));
    let sent = h.sent_iqs();
    assert!(payload_of(&sent[0]).contains("<unsubscribe"));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
}

fn leave(h: &mut Harness) -> oneshot::Receiver<Result<(), ClientError>> {
    let (reply, rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::Leave {
                service: service_bare(),
                node: "dev".into(),
                reply,
            },
        )
    });
    rx
}

#[test]
fn leave_removes_the_space_only_on_success() {
    let mut h = followed();
    let mut rx = leave(&mut h);
    assert!(payload_of(&h.sent_iqs()[0]).contains("<unsubscribe"));
    h.respond(is_done, IqResponse::Lost);
    assert_eq!(rx.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    assert_eq!(column(&h, "SELECT node FROM spaces"), ["dev"]);

    let mut rx = leave(&mut h);
    h.answer(is_done, None);
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
    assert!(column(&h, "SELECT item_id FROM space_items").is_empty());
}

fn create(
    h: &mut Harness,
    name: &str,
    private: bool,
) -> oneshot::Receiver<Result<(String, String), ClientError>> {
    let (reply, rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::Create {
                name: name.into(),
                private,
                reply,
            },
        )
    });
    rx
}

#[test]
fn create_private_space_sends_a_whitelist_form_and_subscribes() {
    let mut h = harness();
    let mut rx = create(&mut h, "Secret <club>", true);
    let sent = h.sent_iqs();
    let text = payload_of(&sent[0]);
    assert!(text.contains("<create node='space-"), "{text}");
    assert!(
        text.contains("whitelist") && text.contains(NS_SPACES),
        "{text}"
    );
    assert!(text.contains("Secret &lt;club&gt;"), "{text}");
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::Create { .. })),
        None,
    );
    assert!(payload_of(&h.sent_iqs()[0]).contains("<subscribe"));
    assert_eq!(rx.try_recv(), Ok(None));
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::CreateSubscribe { .. })),
        Some(subscription("subscribed")),
    );
    let Ok(Some(Ok((service, node)))) = rx.try_recv() else {
        panic!("expected a space")
    };
    assert_eq!(service, SERVICE);
    assert!(node.starts_with("space-"));
    assert_eq!(column(&h, "SELECT name FROM spaces"), ["Secret <club>"]);
    assert_eq!(column(&h, "SELECT access_model FROM spaces"), ["whitelist"]);
}

#[test]
fn create_errors_and_missing_service() {
    let mut h = harness();
    let mut rx = create(&mut h, "x", false);
    assert!(payload_of(&h.sent_iqs()[0]).contains("open"));
    h.respond(
        |p| matches!(p, FeaturePending::Spaces(Pending::Create { .. })),
        IqResponse::Lost,
    );
    assert_eq!(rx.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());

    // The subscribe after the create fails.
    let mut rx = create(&mut h, "x", false);
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::Create { .. })),
        None,
    );
    h.respond(
        |p| matches!(p, FeaturePending::Spaces(Pending::CreateSubscribe { .. })),
        forbidden(),
    );
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Server(_))))
    ));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());

    let mut none = Harness::new();
    let mut rx = create(&mut none, "x", false);
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Unsupported(_))))
    ));
}

#[test]
fn owner_commands_send_the_right_requests() {
    let mut h = followed();
    let room = BareJid::new("new@rooms.chord.localhost").unwrap();
    let bob = BareJid::new("bob@chord.localhost").unwrap();

    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::AddRoom {
                service: service_bare(),
                node: "dev".into(),
                room: room.clone(),
                name: "New".into(),
                reply,
            },
        )
    });
    let text = payload_of(&h.sent_iqs()[0]);
    assert!(text.contains("<publish node='dev'>"), "{text}");
    assert!(
        text.contains("item id='new@rooms.chord.localhost'"),
        "{text}"
    );
    h.answer(is_done, None);
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert_eq!(
        column(
            &h,
            "SELECT name FROM space_items WHERE room_jid LIKE 'new@%'"
        ),
        ["New"]
    );

    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::RemoveRoom {
                service: service_bare(),
                node: "dev".into(),
                room,
                reply,
            },
        )
    });
    assert!(payload_of(&h.sent_iqs()[0]).contains("<retract"));
    h.answer(is_done, None);
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert!(
        column(
            &h,
            "SELECT name FROM space_items WHERE room_jid LIKE 'new@%'"
        )
        .is_empty()
    );

    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::AddMember {
                service: service_bare(),
                node: "dev".into(),
                member: bob,
                reply,
            },
        )
    });
    let sent = h.sent_iqs();
    let text = payload_of(&sent[0]);
    assert!(text.contains(ns::PUBSUB_OWNER) && text.contains("affiliation='member'"));
    assert!(text.contains("jid='bob@chord.localhost'"));
    h.respond(is_done, forbidden());
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Server(_))))
    ));

    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::Delete {
                service: service_bare(),
                node: "dev".into(),
                reply,
            },
        )
    });
    assert!(payload_of(&h.sent_iqs()[0]).contains("<delete"));
    h.answer(is_done, None);
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
}

#[test]
fn browse_lists_the_open_spaces_only() {
    let mut h = harness();
    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| on_command(ctx, Command::Browse { reply }));
    assert_eq!(h.sent_iqs().len(), 1);
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::BrowseItems { .. })),
        Some(xml(
            "<query xmlns='http://jabber.org/protocol/disco#items'>
               <item jid='pubsub.chord.localhost' node='b-open'/>
               <item jid='pubsub.chord.localhost' node='a-open'/>
               <item jid='pubsub.chord.localhost' node='closed'/>
               <item jid='pubsub.chord.localhost' node='other'/>
               <item jid='pubsub.chord.localhost' node='broken'/></query>",
        )),
    );
    assert_eq!(h.sent_iqs().len(), 5);
    let info = |node: &str, kind: &str, access: &str| {
        let e = node_info(node, kind, &format!("Space {node}"), access);
        (node.to_owned(), e)
    };
    for (node, e) in [
        info("b-open", NS_SPACES, "open"),
        info("a-open", NS_SPACES, "open"),
        info("closed", NS_SPACES, "whitelist"),
        info("other", "x", "open"),
    ] {
        h.answer(
            move |p| {
                matches!(p, FeaturePending::Spaces(Pending::BrowseInfo { node: n, .. }) if *n == node)
            },
            Some(e),
        );
    }
    assert_eq!(rx.try_recv(), Ok(None));
    h.respond(is_browse_info, IqResponse::Lost);
    let Ok(Some(Ok(found))) = rx.try_recv() else {
        panic!("expected the list")
    };
    let nodes: Vec<&str> = found.iter().map(|s| s.node.as_str()).collect();
    assert_eq!(nodes, ["a-open", "b-open"]);
    assert_eq!(found[0].service, SERVICE);
    assert_eq!(found[0].name, "Space a-open");
    assert!(h.state.spaces.browses.is_empty());
}

#[test]
fn browse_error_and_empty_list() {
    let mut h = harness();
    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| on_command(ctx, Command::Browse { reply }));
    h.respond(
        |p| matches!(p, FeaturePending::Spaces(Pending::BrowseItems { .. })),
        IqResponse::Lost,
    );
    assert_eq!(rx.try_recv(), Ok(Some(Err(ClientError::NotConnected))));

    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| on_command(ctx, Command::Browse { reply }));
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::BrowseItems { .. })),
        Some(xml(
            "<query xmlns='http://jabber.org/protocol/disco#items'/>",
        )),
    );
    assert_eq!(rx.try_recv(), Ok(Some(Ok(vec![]))));
}

#[test]
fn offline_answers_every_command_with_not_connected() {
    let room = BareJid::new("r@rooms.chord.localhost").unwrap();
    let (a, mut ra) = oneshot::channel::<Result<Vec<SpaceInfo>, ClientError>>();
    offline(Command::Browse { reply: a });
    assert_eq!(ra.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    let (a, mut ra) = oneshot::channel();
    offline(Command::Join {
        service: service_bare(),
        node: "n".into(),
        reply: a,
    });
    assert_eq!(ra.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    let (a, mut ra) = oneshot::channel();
    offline(Command::Create {
        name: "n".into(),
        private: false,
        reply: a,
    });
    assert_eq!(ra.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    let (a, mut ra) = oneshot::channel();
    offline(Command::AddRoom {
        service: service_bare(),
        node: "n".into(),
        room,
        name: "n".into(),
        reply: a,
    });
    assert_eq!(ra.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    let (a, mut ra) = oneshot::channel();
    offline(Command::Delete {
        service: service_bare(),
        node: "n".into(),
        reply: a,
    });
    assert_eq!(ra.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
}

#[test]
fn a_bad_service_jid_is_invalid() {
    assert!(matches!(
        parse_service("not a jid@@"),
        Err(ClientError::Invalid(_))
    ));
}
