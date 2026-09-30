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
    assert_eq!(state_of(&h), ["2"], "kept as a pending join");
    h.answer(
        is_info,
        Some(node_info("dev", NS_SPACES, "Dev", "authorize")),
    );

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
    let access = if private {
        SpaceAccess::Whitelist
    } else {
        SpaceAccess::Open
    };
    create_with(h, name, access)
}

fn create_with(
    h: &mut Harness,
    name: &str,
    access: SpaceAccess,
) -> oneshot::Receiver<Result<(String, String), ClientError>> {
    let (reply, rx) = oneshot::channel();
    h.with_ctx(|ctx| {
        on_command(
            ctx,
            Command::Create {
                name: name.into(),
                access,
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
fn browse_lists_the_open_and_authorize_spaces_only() {
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
               <item jid='pubsub.chord.localhost' node='c-ask'/>
               <item jid='pubsub.chord.localhost' node='other'/>
               <item jid='pubsub.chord.localhost' node='broken'/></query>",
        )),
    );
    assert_eq!(h.sent_iqs().len(), 6);
    let info = |node: &str, kind: &str, access: &str| {
        let e = node_info(node, kind, &format!("Space {node}"), access);
        (node.to_owned(), e)
    };
    for (node, e) in [
        info("b-open", NS_SPACES, "open"),
        info("a-open", NS_SPACES, "open"),
        info("closed", NS_SPACES, "whitelist"),
        info("c-ask", NS_SPACES, "authorize"),
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
    // The last query fails with a server error: skip that node.
    h.respond(is_browse_info, forbidden());
    let Ok(Some(Ok(found))) = rx.try_recv() else {
        panic!("expected the list")
    };
    let nodes: Vec<&str> = found.iter().map(|s| s.node.as_str()).collect();
    assert_eq!(nodes, ["a-open", "b-open", "c-ask"]);
    assert_eq!(found[0].service, SERVICE);
    assert_eq!(found[0].name, "Space a-open");
    assert!(h.state.spaces.browses.is_empty());
}

#[test]
fn browse_info_lost_fails_the_browse() {
    let mut h = harness();
    let (reply, mut rx) = oneshot::channel();
    h.with_ctx(|ctx| on_command(ctx, Command::Browse { reply }));
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::BrowseItems { .. })),
        Some(xml("<query xmlns='http://jabber.org/protocol/disco#items'>
               <item jid='pubsub.chord.localhost' node='a'/>
               <item jid='pubsub.chord.localhost' node='b'/></query>")),
    );
    h.respond(is_browse_info, IqResponse::Lost);
    assert_eq!(rx.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    assert!(h.state.spaces.browses.is_empty());
    // The other query gets the same answer later. It changes nothing.
    h.respond(is_browse_info, IqResponse::Lost);
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
        access: SpaceAccess::Open,
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

// --- Browse with paging and extensions ---

const RSM: &str = "http://jabber.org/protocol/rsm";

/// A harness whose pubsub service also advertises `features`.
fn harness_with(features: &[&str]) -> Harness {
    let mut h = harness();
    h.state.disco.services[0]
        .1
        .features
        .extend(features.iter().map(|f| (*f).to_owned()));
    h
}

type Found = Result<Vec<SpaceInfo>, ClientError>;

fn browse(h: &mut Harness) -> oneshot::Receiver<Found> {
    let (reply, rx) = oneshot::channel();
    h.with_ctx(|ctx| on_command(ctx, Command::Browse { reply }));
    rx
}

fn is_page(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::BrowseItems { .. }))
}

/// A disco#items result. `rsm` is the `last` and the `count` of the RSM element.
fn page(nodes: &[&str], rsm: Option<(&str, Option<usize>)>) -> Element {
    let items: String = nodes
        .iter()
        .map(|n| format!("<item jid='{SERVICE}' node='{n}'/>"))
        .collect();
    page_xml(&items, rsm)
}

fn page_xml(items: &str, rsm: Option<(&str, Option<usize>)>) -> Element {
    let set = rsm
        .map(|(last, count)| {
            let count = count
                .map(|c| format!("<count>{c}</count>"))
                .unwrap_or_default();
            format!(
                "<set xmlns='http://jabber.org/protocol/rsm'><first>x</first>\
                 <last>{last}</last>{count}</set>"
            )
        })
        .unwrap_or_default();
    xml(&format!(
        "<query xmlns='http://jabber.org/protocol/disco#items'>{items}{set}</query>"
    ))
}

/// The one IQ that the harness sent, as its payload element.
fn sent_query(h: &mut Harness) -> Element {
    let mut iqs = h.sent_iqs();
    assert_eq!(iqs.len(), 1, "{iqs:?}");
    match iqs.remove(0) {
        Iq::Get { payload, .. } => payload,
        other => panic!("{other:?}"),
    }
}

fn open_info(h: &mut Harness, node: &str) {
    let node = node.to_owned();
    let info = node_info(&node, NS_SPACES, &format!("Space {node}"), "open");
    h.answer(
        move |p| {
            matches!(p, FeaturePending::Spaces(Pending::BrowseInfo { node: n, .. }) if *n == node)
        },
        Some(info),
    );
}

fn rsm_after(query: &Element) -> Option<String> {
    let set = query.get_child("set", ns::RSM)?;
    Some(set.get_child("after", ns::RSM)?.text())
}

fn nodes_of(found: &[SpaceInfo]) -> Vec<&str> {
    found.iter().map(|s| s.node.as_str()).collect()
}

#[test]
fn browse_pages_with_rsm_until_the_count_is_reached() {
    let mut h = harness_with(&[RSM]);
    let mut rx = browse(&mut h);
    let first = sent_query(&mut h);
    let set = first.get_child("set", ns::RSM).expect("an RSM element");
    assert_eq!(set.get_child("max", ns::RSM).unwrap().text(), "100");
    assert_eq!(rsm_after(&first), None);
    assert!(first.get_child("filter", NS_TYPE_FILTER).is_none());

    h.answer(is_page, Some(page(&["a", "b"], Some(("b", Some(3))))));
    let second = sent_query(&mut h);
    assert_eq!(rsm_after(&second).as_deref(), Some("b"));
    h.answer(is_page, Some(page(&["c"], Some(("c", Some(3))))));
    // The count is reached: three disco#info queries follow, and no third page.
    let iqs = h.sent_iqs();
    assert_eq!(iqs.len(), 3);
    for node in ["a", "b", "c"] {
        open_info(&mut h, node);
    }
    let Ok(Some(Ok(found))) = rx.try_recv() else {
        panic!("expected the list")
    };
    assert_eq!(nodes_of(&found), ["a", "b", "c"]);
}

#[test]
fn browse_stops_paging_on_a_page_with_nothing_new_or_a_repeated_last() {
    // No count: the second page repeats the first, so it adds nothing.
    let mut h = harness_with(&[RSM]);
    let mut rx = browse(&mut h);
    h.sent_iqs();
    h.answer(is_page, Some(page(&["a"], Some(("a", None)))));
    assert_eq!(h.sent_iqs().len(), 1);
    h.answer(is_page, Some(page(&["a"], Some(("a2", None)))));
    let infos = h.sent_iqs();
    assert_eq!(infos.len(), 1, "{infos:?}");
    assert!(matches!(&infos[0], Iq::Get { payload, .. } if payload.is("query", ns::DISCO_INFO)));
    open_info(&mut h, "a");
    assert!(matches!(rx.try_recv(), Ok(Some(Ok(_)))));

    // A `last` that equals the last request would loop, so it stops the paging.
    let mut h = harness_with(&[RSM]);
    let _rx = browse(&mut h);
    h.sent_iqs();
    h.answer(is_page, Some(page(&["a"], Some(("a", None)))));
    h.sent_iqs();
    h.answer(is_page, Some(page(&["b"], Some(("a", None)))));
    let iqs = h.sent_iqs();
    assert_eq!(iqs.len(), 2);
    assert!(
        iqs.iter()
            .all(|iq| matches!(iq, Iq::Get { payload, .. } if payload.is("query", ns::DISCO_INFO)))
    );
}

#[test]
fn browse_without_the_rsm_feature_sends_one_plain_request() {
    let mut h = harness();
    let mut rx = browse(&mut h);
    let query = sent_query(&mut h);
    assert!(query.get_child("set", ns::RSM).is_none());
    assert!(query.children().next().is_none());
    // The answer carries an RSM element and a metadata form. We use neither.
    let items = format!(
        "<item jid='{SERVICE}' node='a'>
           <x xmlns='jabber:x:data' type='result'>
             <field var='FORM_TYPE' type='hidden'><value>{NS_META}</value></field>
             <field var='pubsub#type'><value>{NS_SPACES}</value></field>
             <field var='pubsub#access_model'><value>open</value></field>
           </x></item>"
    );
    h.answer(is_page, Some(page_xml(&items, Some(("a", Some(9))))));
    assert_eq!(h.sent_iqs().len(), 1);
    open_info(&mut h, "a");
    assert!(matches!(rx.try_recv(), Ok(Some(Ok(found))) if found.len() == 1));
}

#[test]
fn browse_stops_at_the_cap() {
    let mut h = harness_with(&[RSM]);
    let _rx = browse(&mut h);
    h.sent_iqs();
    let names: Vec<String> = (0..1200).map(|i| format!("n{i:04}")).collect();
    let refs = |from: usize, to: usize| -> Vec<&str> {
        names[from..to].iter().map(String::as_str).collect()
    };
    h.answer(is_page, Some(page(&refs(0, 600), Some(("n0599", None)))));
    assert_eq!(h.sent_iqs().len(), 1);
    h.answer(is_page, Some(page(&refs(600, 1200), Some(("n1199", None)))));
    let iqs = h.sent_iqs();
    // No third page. The disco#info queries start, up to the window.
    assert_eq!(iqs.len(), BROWSE_WINDOW);
    assert!(
        iqs.iter()
            .all(|iq| matches!(iq, Iq::Get { payload, .. } if payload.is("query", ns::DISCO_INFO)))
    );
    let browse = h.state.spaces.browses.values().next().unwrap();
    assert_eq!(browse.known.len(), MAX_BROWSE_NODES);
    assert_eq!(browse.queue.len() + browse.in_flight, MAX_BROWSE_NODES);
}

#[test]
fn browse_keeps_a_window_of_info_queries() {
    let mut h = harness();
    let mut rx = browse(&mut h);
    h.sent_iqs();
    let names: Vec<String> = (0..25).map(|i| format!("n{i:02}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    h.answer(is_page, Some(page(&refs, None)));
    assert_eq!(h.sent_iqs().len(), BROWSE_WINDOW);
    // Each answer lets one more query go out.
    open_info(&mut h, "n00");
    assert_eq!(h.sent_iqs().len(), 1);
    for name in &names[1..] {
        open_info(&mut h, name);
        h.sent_iqs();
    }
    let Ok(Some(Ok(found))) = rx.try_recv() else {
        panic!("expected the list")
    };
    assert_eq!(found.len(), 25);
    assert!(h.state.spaces.browses.is_empty());
}

#[test]
fn browse_asks_for_the_space_type_when_the_service_has_type_filtering() {
    let mut h = harness_with(&[NS_TYPE_FILTER]);
    let mut rx = browse(&mut h);
    let query = sent_query(&mut h);
    assert!(query.get_child("set", ns::RSM).is_none());
    let filter = query.get_child("filter", NS_TYPE_FILTER).expect("a filter");
    let form = DataForm::try_from(filter.children().next().unwrap().clone()).unwrap();
    assert_eq!(form.type_, DataFormType::Submit);
    assert_eq!(form.form_type(), Some(NS_TYPE_FILTER));
    let field = form
        .fields
        .iter()
        .find(|f| f.var.as_deref() == Some("included-types"))
        .unwrap();
    assert_eq!(field.values, [NS_SPACES]);
    // The service may ignore the filter, so the info check stays.
    h.answer(is_page, Some(page(&["a", "b"], None)));
    assert_eq!(h.sent_iqs().len(), 2);
    open_info(&mut h, "a");
    let other = node_info("b", "other", "B", "open");
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::BrowseInfo { node, .. }) if node == "b"),
        Some(other),
    );
    let Ok(Some(Ok(found))) = rx.try_recv() else {
        panic!("expected the list")
    };
    assert_eq!(nodes_of(&found), ["a"]);
}

fn meta_item(node: &str, type_: Option<&str>, title: &str, access: &str) -> String {
    let type_ = type_
        .map(|t| format!("<field var='pubsub#type'><value>{t}</value></field>"))
        .unwrap_or_default();
    format!(
        "<item jid='{SERVICE}' node='{node}'>
           <x xmlns='jabber:x:data' type='result'>
             <field var='FORM_TYPE' type='hidden'><value>{NS_META}</value></field>
             {type_}
             <field var='pubsub#title'><value>{title}</value></field>
             <field var='pubsub#description'><value>About {node}</value></field>
             <field var='pubsub#access_model'><value>{access}</value></field>
           </x></item>"
    )
}

#[test]
fn browse_with_extended_disco_reads_the_metadata_from_the_items() {
    for feature in [NS_EXT_DISCO, NS_EXT_DISCO_SHORT] {
        let mut h = harness_with(&[feature, NS_TYPE_FILTER, RSM]);
        let mut rx = browse(&mut h);
        let query = sent_query(&mut h);
        assert!(query.get_child("filter", NS_TYPE_FILTER).is_some());
        assert!(query.get_child("set", ns::RSM).is_some());
        let form = query
            .children()
            .find(|c| c.is("x", ns::DATA_FORMS))
            .expect("the XEP-0499 form");
        let form = DataForm::try_from(form.clone()).unwrap();
        assert_eq!(form.type_, DataFormType::Submit);
        assert_eq!(form.form_type(), Some(NS_EXT_DISCO));
        let value = |var: &str| {
            form.fields
                .iter()
                .find(|f| f.var.as_deref() == Some(var))
                .and_then(|f| f.values.first().cloned())
        };
        assert_eq!(value("type").as_deref(), Some("nodes"));
        assert_eq!(value("full_metadata").as_deref(), Some("true"));

        let items = [
            meta_item("b-open", Some(NS_SPACES), "Bee", "open"),
            meta_item("closed", Some(NS_SPACES), "Closed", "whitelist"),
            meta_item("other", Some("x"), "Other", "open"),
            // A metadata form with no type: ask disco#info for it.
            meta_item("no-type", None, "No type", "open"),
            // No form at all: ask disco#info for it.
            format!("<item jid='{SERVICE}' node='plain'/>"),
            meta_item("a-open", Some(NS_SPACES), "Ay", "open"),
        ]
        .concat();
        h.answer(is_page, Some(page_xml(&items, Some(("a-open", Some(6))))));
        // Only the two items without a type need a disco#info query.
        let infos = h.sent_iqs();
        assert_eq!(infos.len(), 2, "{infos:?}");
        assert!(rx.try_recv().unwrap().is_none());
        open_info(&mut h, "no-type");
        h.respond(
            |p| matches!(p, FeaturePending::Spaces(Pending::BrowseInfo { node, .. }) if node == "plain"),
            IqResponse::Error(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::ItemNotFound,
                "en",
                "",
            )),
        );
        let Ok(Some(Ok(found))) = rx.try_recv() else {
            panic!("expected the list")
        };
        assert_eq!(nodes_of(&found), ["a-open", "b-open", "no-type"]);
        assert_eq!(found[0].name, "Ay");
        assert_eq!(found[0].description.as_deref(), Some("About a-open"));
        assert_eq!(found[0].access_model.as_deref(), Some("open"));
        assert_eq!(found[0].service, SERVICE);
    }
}

#[test]
fn browse_with_extended_disco_needs_no_info_query_when_the_items_hold_the_metadata() {
    let mut h = harness_with(&[NS_EXT_DISCO]);
    let mut rx = browse(&mut h);
    h.sent_iqs();
    let items = meta_item("a", Some(NS_SPACES), "Ay", "open");
    h.answer(is_page, Some(page_xml(&items, None)));
    assert!(h.sent_iqs().is_empty());
    assert!(matches!(rx.try_recv(), Ok(Some(Ok(found))) if found.len() == 1));
    assert!(h.state.spaces.browses.is_empty());
}

#[test]
fn browse_starts_again_with_a_plain_request_when_an_extension_fails() {
    let mut h = harness_with(&[NS_EXT_DISCO, NS_TYPE_FILTER, RSM]);
    let mut rx = browse(&mut h);
    h.sent_iqs();
    let bad = StanzaError::new(ErrorType::Modify, DefinedCondition::BadRequest, "en", "");
    h.respond(is_page, IqResponse::Error(bad));
    let plain = sent_query(&mut h);
    assert!(plain.children().next().is_none());
    assert!(rx.try_recv().unwrap().is_none());
    h.answer(is_page, Some(page(&["a"], None)));
    assert_eq!(h.sent_iqs().len(), 1);
    open_info(&mut h, "a");
    assert!(matches!(rx.try_recv(), Ok(Some(Ok(found))) if found.len() == 1));

    // A plain request that fails is an error.
    let mut h = harness();
    let mut rx = browse(&mut h);
    h.sent_iqs();
    h.respond(is_page, forbidden());
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Server(_))))
    ));
    assert!(h.state.spaces.browses.is_empty());
}

#[test]
fn browse_keeps_the_nodes_of_earlier_pages_when_a_later_page_fails() {
    let mut h = harness_with(&[RSM]);
    let mut rx = browse(&mut h);
    h.sent_iqs();
    h.answer(is_page, Some(page(&["a"], Some(("a", None)))));
    h.sent_iqs();
    h.respond(is_page, forbidden());
    assert_eq!(h.sent_iqs().len(), 1);
    open_info(&mut h, "a");
    assert!(matches!(rx.try_recv(), Ok(Some(Ok(found))) if found.len() == 1));
}

#[test]
fn browse_lost_ends_the_browse_at_any_stage() {
    let mut h = harness_with(&[RSM]);
    let mut rx = browse(&mut h);
    h.sent_iqs();
    h.answer(is_page, Some(page(&["a"], Some(("a", None)))));
    h.sent_iqs();
    h.respond(is_page, IqResponse::Lost);
    assert_eq!(rx.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    assert!(h.state.spaces.browses.is_empty());

    // A bad answer is a server error.
    let mut h = harness();
    let mut rx = browse(&mut h);
    h.sent_iqs();
    h.answer(is_page, Some(xml("<query xmlns='other'/>")));
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Server(_))))
    ));
}

// --- Space avatars ---

const IMAGE: &[u8] = b"\x89PNG\r\n\x1a\nspace image";

fn avatar_event(info: &str) -> Payload {
    event(&format!(
        "<items node='dev'><item id='{AVATAR_ITEM}'>
           <metadata xmlns='urn:xmpp:avatar:metadata'>{info}</metadata></item></items>"
    ))
}

fn is_avatar_data(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::AvatarData { .. }))
}

fn avatar_data_result(data: &[u8]) -> Element {
    let item = xmpp_parsers::pubsub::pubsub::Item::new(
        None,
        None,
        Some(AvatarData {
            data: data.to_vec(),
        }),
    );
    xmpp_parsers::pubsub::pubsub::PubSub::Items(xmpp_parsers::pubsub::pubsub::Items {
        max_items: None,
        node: xmpp_parsers::pubsub::NodeName(NS_AVATAR_DATA.to_owned()),
        subid: None,
        items: vec![item],
    })
    .into()
}

fn stored_avatar(h: &Harness) -> Option<avatars::Avatar> {
    load_avatar(&h.store, h.account_id, SERVICE, "dev").unwrap()
}

fn deliver_avatar(h: &mut Harness, info: &str) {
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), avatar_event(info)));
}

#[test]
fn space_avatar_without_a_url_is_fetched_from_the_data_node() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_avatar(
        &mut h,
        &format!(
            "<info bytes='{}' id='{hash}' type='image/png'/>",
            IMAGE.len()
        ),
    );
    let avatar = stored_avatar(&h).unwrap();
    assert_eq!(avatar.hash, hash);
    assert_eq!(avatar.mime.as_deref(), Some("image/png"));
    assert_eq!(avatar.data, None);
    let sent = h.sent_iqs();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to().unwrap().as_str(), SERVICE);
    let text = payload_of(&sent[0]);
    assert!(text.contains(NS_AVATAR_DATA), "{text}");
    assert!(text.contains(&format!("id=\"{hash}\"")) || text.contains(&format!("id='{hash}'")));

    // The same event again: the fetch runs already.
    deliver_avatar(
        &mut h,
        &format!(
            "<info bytes='{}' id='{hash}' type='image/png'/>",
            IMAGE.len()
        ),
    );
    assert!(h.sent_iqs().is_empty());

    h.take_dirty();
    h.answer(is_avatar_data, Some(avatar_data_result(IMAGE)));
    assert_eq!(stored_avatar(&h).unwrap().data.as_deref(), Some(IMAGE));
    assert!(h.take_dirty().contains(&ViewKey::SpaceList));

    // The image is here: a new event with the same hash fetches nothing.
    deliver_avatar(
        &mut h,
        &format!(
            "<info bytes='{}' id='{hash}' type='image/png'/>",
            IMAGE.len()
        ),
    );
    assert!(h.sent_iqs().is_empty());
}

#[test]
fn space_avatar_hash_in_upper_case_is_stored_in_lower_case() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_avatar(
        &mut h,
        &format!("<info id='{}' type='image/png'/>", hash.to_uppercase()),
    );
    assert_eq!(stored_avatar(&h).unwrap().hash, hash);
    h.answer(is_avatar_data, Some(avatar_data_result(IMAGE)));
    assert!(stored_avatar(&h).unwrap().data.is_some());
}

#[test]
fn space_avatar_image_with_the_wrong_hash_or_an_error_is_not_stored() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    let info = format!("<info bytes='4' id='{hash}' type='image/png'/>");
    deliver_avatar(&mut h, &info);
    h.answer(is_avatar_data, Some(avatar_data_result(b"forged")));
    assert_eq!(stored_avatar(&h).unwrap().data, None);

    // The fetch is over, so a new event can start it again. Error and Lost end it too.
    deliver_avatar(&mut h, &info);
    h.respond(is_avatar_data, forbidden());
    assert_eq!(stored_avatar(&h).unwrap().data, None);
    deliver_avatar(&mut h, &info);
    h.respond(is_avatar_data, IqResponse::Lost);
    deliver_avatar(&mut h, &info);
    h.answer(is_avatar_data, None);
    assert_eq!(stored_avatar(&h).unwrap().data, None);
    assert!(h.state.spaces.avatar_fetching.len() <= 1);
}

#[test]
fn space_avatar_that_a_new_hash_replaced_is_dropped() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_avatar(&mut h, &format!("<info id='{hash}' type='image/png'/>"));
    let other = avatars::sha1_hex(b"other");
    deliver_avatar(&mut h, &format!("<info id='{other}' type='image/png'/>"));
    h.answer(is_avatar_data, Some(avatar_data_result(IMAGE)));
    let avatar = stored_avatar(&h).unwrap();
    assert_eq!(avatar.hash, other);
    assert_eq!(avatar.data, None);
}

#[test]
fn space_avatar_too_big_or_with_a_bad_hash_is_not_fetched() {
    let mut h = followed();
    h.sent_iqs();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_avatar(&mut h, &format!("<info id='{hash}' type='image/png'/>"));
    h.answer(is_avatar_data, None);
    h.sent_iqs();
    deliver_avatar(
        &mut h,
        &format!(
            "<info id='{hash}' type='image/png' bytes='{}'/>",
            MAX_AVATAR_BYTES + 1
        ),
    );
    deliver_avatar(&mut h, "<info id='not a hash' type='image/png'/>");
    assert!(h.sent_iqs().is_empty());
}

#[test]
fn empty_avatar_metadata_removes_the_space_avatar() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_avatar(&mut h, &format!("<info id='{hash}' type='image/png'/>"));
    assert!(stored_avatar(&h).is_some());
    deliver_avatar(&mut h, "");
    assert_eq!(stored_avatar(&h), None);
}

#[test]
fn store_avatar_image_checks_the_hash_and_the_size() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_avatar(&mut h, &format!("<info id='{hash}' type='image/png'/>"));
    let store = |hash: &str, data: &[u8]| {
        store_avatar_image(&h.store, h.account_id, SERVICE, "dev", hash, data)
    };
    assert!(store(&hash, b"forged").is_err());
    assert!(store(&hash, &vec![0; MAX_AVATAR_BYTES + 1]).is_err());
    // A hash that the space does not have any more stores nothing.
    let other = avatars::sha1_hex(b"other");
    assert_eq!(store(&other, b"other"), Ok(false));
    assert_eq!(store(&hash, IMAGE), Ok(true));
    assert_eq!(stored_avatar(&h).unwrap().data.as_deref(), Some(IMAGE));
}

// --- Space avatars at a URL ---

const AVATAR_URL: &str = "https://example.org/a.png";

fn take_downloads(h: &mut Harness) -> Vec<DownloadRequest> {
    let mut found = Vec::new();
    for effect in std::mem::take(&mut h.effects) {
        match effect {
            crate::features::Effect::Download { request } => found.push(request),
            other => h.effects.push(other),
        }
    }
    found
}

fn deliver_url_avatar(h: &mut Harness, hash: &str) {
    deliver_avatar(
        h,
        &format!("<info id='{hash}' type='image/png' url='{AVATAR_URL}'/>"),
    );
}

#[test]
fn space_avatar_at_a_url_queues_a_download() {
    let mut h = followed();
    h.sent_iqs();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_url_avatar(&mut h, &hash);
    assert_eq!(stored_avatar(&h).unwrap().hash, hash);
    assert!(h.sent_iqs().is_empty());
    let downloads = take_downloads(&mut h);
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].url, AVATAR_URL);
    assert_eq!(downloads[0].max_bytes, MAX_AVATAR_BYTES);
    assert_eq!(downloads[0].hash, hash);
    assert_eq!(downloads[0].node, "dev");

    // The same event again: the download runs already.
    deliver_url_avatar(&mut h, &hash);
    assert!(take_downloads(&mut h).is_empty());
}

#[test]
fn finished_download_stores_the_image_and_marks_the_view() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    deliver_url_avatar(&mut h, &hash);
    let request = take_downloads(&mut h).remove(0);
    h.take_dirty();
    h.with_ctx(|ctx| {
        on_download_done(
            ctx,
            DownloadDone {
                request,
                result: Ok(IMAGE.to_vec()),
            },
        );
    });
    assert_eq!(stored_avatar(&h).unwrap().data.as_deref(), Some(IMAGE));
    assert!(h.take_dirty().contains(&ViewKey::SpaceList));
    assert!(h.state.spaces.avatar_fetching.is_empty());
}

#[test]
fn download_with_the_wrong_hash_or_an_error_keeps_the_hash_only() {
    let mut h = followed();
    let hash = avatars::sha1_hex(IMAGE);
    for result in [
        Ok(b"forged".to_vec()),
        Err("the server answered 404".into()),
    ] {
        deliver_url_avatar(&mut h, &hash);
        let request = take_downloads(&mut h).remove(0);
        h.take_dirty();
        h.with_ctx(|ctx| on_download_done(ctx, DownloadDone { request, result }));
        let avatar = stored_avatar(&h).unwrap();
        assert_eq!(avatar.hash, hash);
        assert_eq!(avatar.data, None);
        assert!(!h.take_dirty().contains(&ViewKey::SpaceList));
        // The fetch is over, so a new event can start it again.
        assert!(h.state.spaces.avatar_fetching.is_empty());
    }
}

#[test]
fn spaces_prefer_the_spaces_service() {
    let mut h = Harness::new();
    let pubsub = DiscoInfoResult {
        node: None,
        identities: vec![Identity::new("pubsub", "service", "en", "Pubsub")],
        features: Default::default(),
        extensions: vec![],
    };
    // ejabberd for Movim: the answers can come in any order.
    for jid in [
        "comments.chat.foid.space",
        "pubsub.chat.foid.space",
        "spaces.chat.foid.space",
    ] {
        h.state
            .disco
            .services
            .push((Jid::new(jid).unwrap(), pubsub.clone()));
    }
    h.state.disco.complete = true;
    h.with_ctx(on_disco_complete);
    assert_eq!(
        h.state.spaces.service.as_ref().map(|s| s.as_str()),
        Some("spaces.chat.foid.space")
    );

    // Without a spaces. service, pubsub. comes before the others.
    h.state.disco.services.pop();
    h.state.spaces = Default::default();
    h.with_ctx(on_disco_complete);
    assert_eq!(
        h.state.spaces.service.as_ref().map(|s| s.as_str()),
        Some("pubsub.chat.foid.space")
    );
}

// --- Join approval ---

fn notices(h: &mut Harness) -> Vec<String> {
    let effects = std::mem::take(&mut h.effects);
    let mut out = Vec::new();
    for e in effects {
        match e {
            crate::features::Effect::Emit(ClientEvent::Notice(text)) => out.push(text),
            other => h.effects.push(other),
        }
    }
    out
}

/// A harness where our join of `dev` is pending.
fn pending_join() -> Harness {
    let mut h = harness();
    let mut rx = join(&mut h);
    h.answer(is_subscribe, Some(subscription("pending")));
    assert_eq!(rx.try_recv(), Ok(Some(Ok(JoinOutcome::Pending))));
    h.answer(
        is_info,
        Some(node_info("dev", NS_SPACES, "Dev", "authorize")),
    );
    h.take_dirty();
    h.take_sent();
    h
}

fn state_of(h: &Harness) -> Vec<String> {
    column(h, "SELECT CAST(subscribed AS TEXT) FROM spaces")
}

#[test]
fn a_pending_join_is_stored_with_its_name_and_is_not_followed() {
    let h = pending_join();
    assert_eq!(state_of(&h), ["2"]);
    assert_eq!(column(&h, "SELECT name FROM spaces"), ["Dev"]);
    assert_eq!(column(&h, "SELECT access_model FROM spaces"), ["authorize"]);
    let pending = pending_joins(&h.store, h.account_id).unwrap();
    assert_eq!(pending, [(SERVICE.into(), "dev".into(), "Dev".into())]);
    // The space list query reads `subscribed = 1` only.
    assert!(!db::is_followed(h.store.conn(), h.account_id, SERVICE, "dev").unwrap());
}

#[test]
fn an_approval_notification_loads_the_space_and_emits_a_notice() {
    let mut h = pending_join();
    let payload =
        event("<subscription node='dev' jid='alice@chord.localhost' subscription='subscribed'/>");
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), payload));
    assert_eq!(notices(&mut h), ["Your request to join Dev was approved"]);
    h.answer(
        is_info,
        Some(node_info("dev", NS_SPACES, "Dev", "authorize")),
    );
    h.answer(
        is_items,
        Some(items_result(
            "dev",
            "<item id='r@rooms.chord.localhost'>
               <conference xmlns='urn:xmpp:bookmarks:1' name='R'/></item>",
        )),
    );
    assert_eq!(state_of(&h), ["1"]);
    assert_eq!(column(&h, "SELECT name FROM space_items"), ["R"]);
    assert!(h.take_dirty().contains(&ViewKey::SpaceList));
    assert!(pending_joins(&h.store, h.account_id).unwrap().is_empty());
}

#[test]
fn a_denial_notification_drops_the_pending_join_and_emits_a_notice() {
    let mut h = pending_join();
    let payload =
        event("<subscription node='dev' jid='alice@chord.localhost' subscription='none'/>");
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), payload));
    assert_eq!(notices(&mut h), ["Your request to join Dev was denied"]);
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
    assert!(h.take_dirty().contains(&ViewKey::SpaceList));
}

#[test]
fn a_subscription_notification_that_we_did_not_ask_for_is_dropped() {
    let mut h = pending_join();
    let ours = "<subscription node='dev' jid='alice@chord.localhost' subscription='subscribed'/>";
    // No pending join for this node.
    let other =
        event("<subscription node='other' jid='alice@chord.localhost' subscription='subscribed'/>");
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), other));
    // A user of the service.
    let user = Jid::new("pubsub.chord.localhost/x").unwrap();
    h.with_ctx(|ctx| on_event(ctx, &user, event(ours)));
    // Another subscriber.
    let bob =
        event("<subscription node='dev' jid='bob@chord.localhost' subscription='subscribed'/>");
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), bob));
    // A foreign service.
    let evil = Jid::new("evil.example.org").unwrap();
    h.with_ctx(|ctx| on_event(ctx, &evil, event(ours)));
    // The pending state changes nothing.
    let still =
        event("<subscription node='dev' jid='alice@chord.localhost' subscription='pending'/>");
    h.with_ctx(|ctx| on_event(ctx, &service_jid(), still));
    assert!(notices(&mut h).is_empty());
    assert!(h.sent_iqs().is_empty());
    assert_eq!(state_of(&h), ["2"]);
}

fn start_with_subscriptions(h: &mut Harness, list: &str) {
    h.state.spaces = State::default();
    h.with_ctx(on_connected);
    h.answer(
        is_subscriptions,
        Some(xml(&format!(
            "<pubsub xmlns='{}'><subscriptions>{list}</subscriptions></pubsub>",
            ns::PUBSUB
        ))),
    );
}

#[test]
fn a_pending_join_survives_a_restart_and_becomes_a_space_when_approved() {
    let mut h = pending_join();
    // Restart: the service still lists the join as pending.
    start_with_subscriptions(
        &mut h,
        "<subscription node='dev' jid='alice@chord.localhost' subscription='pending'/>",
    );
    assert!(h.pending.is_empty(), "no request for a pending space");
    assert_eq!(state_of(&h), ["2"]);
    // Another restart, and the list omits it: it stays.
    start_with_subscriptions(&mut h, "");
    assert_eq!(state_of(&h), ["2"]);
    // The owner approved while we were away.
    start_with_subscriptions(
        &mut h,
        "<subscription node='dev' jid='alice@chord.localhost' subscription='subscribed'/>",
    );
    h.answer(
        is_info,
        Some(node_info("dev", NS_SPACES, "Dev", "authorize")),
    );
    h.answer(is_items, Some(items_result("dev", "")));
    assert_eq!(state_of(&h), ["1"]);
}

#[test]
fn a_pending_join_that_the_list_shows_as_none_is_dropped_with_a_notice() {
    let mut h = pending_join();
    start_with_subscriptions(
        &mut h,
        "<subscription node='dev' jid='alice@chord.localhost' subscription='none'/>",
    );
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
    assert_eq!(notices(&mut h), ["Your request to join dev was denied"]);
}

#[test]
fn leave_cancels_a_pending_join() {
    let mut h = pending_join();
    let mut rx = leave(&mut h);
    h.answer(is_done, None);
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert!(column(&h, "SELECT node FROM spaces").is_empty());
}

fn authorization(from: &str, node: &str, who: &str) -> Message {
    let mut m = Message::new(None);
    m.from = Some(Jid::new(from).unwrap());
    m.payloads.push(xml(&format!(
        "<x xmlns='jabber:x:data' type='form'>
           <title>PubSub subscriber request</title>
           <field var='FORM_TYPE' type='hidden'>
             <value>{FORM_SUBSCRIBE_AUTHORIZATION}</value></field>
           <field var='pubsub#node' type='text-single'><value>{node}</value></field>
           <field var='pubsub#subscriber_jid' type='jid-single'><value>{who}</value></field>
           <field var='pubsub#allow' type='boolean'><value>false</value></field>
         </x>"
    )));
    m
}

#[test]
fn an_authorization_form_from_the_service_emits_a_notice() {
    let mut h = followed();
    h.with_ctx(on_connected);
    let m = authorization(SERVICE, "dev", "bob@chord.localhost");
    assert!(h.with_ctx(|ctx| on_authorization(ctx, &m)));
    assert_eq!(
        notices(&mut h),
        ["bob@chord.localhost asks to join the space The dev corner"]
    );
    // The pubsub router takes it too.
    assert!(h.with_ctx(|ctx| pubsub::on_event(ctx, &m)));
    assert_eq!(notices(&mut h).len(), 1);
}

#[test]
fn an_authorization_form_from_another_sender_or_without_fields_is_dropped() {
    let mut h = followed();
    let m = authorization("evil.example.org", "dev", "bob@chord.localhost");
    assert!(h.with_ctx(|ctx| on_authorization(ctx, &m)));
    let mut empty = Message::new(None);
    empty.from = Some(service_jid());
    empty.payloads.push(xml(&format!(
        "<x xmlns='jabber:x:data' type='form'><field var='FORM_TYPE' type='hidden'>
           <value>{FORM_SUBSCRIBE_AUTHORIZATION}</value></field></x>"
    )));
    assert!(h.with_ctx(|ctx| on_authorization(ctx, &empty)));
    assert!(notices(&mut h).is_empty());
    // A message without the form is not ours.
    let plain = Message::new(Some(Jid::new("alice@chord.localhost").unwrap()));
    assert!(!h.with_ctx(|ctx| on_authorization(ctx, &plain)));
}

fn owner_call<T>(
    h: &mut Harness,
    make: impl FnOnce(Reply<T>) -> Command,
) -> oneshot::Receiver<Result<T, ClientError>> {
    let (reply, rx) = oneshot::channel();
    h.with_ctx(|ctx| on_command(ctx, make(reply)));
    rx
}

fn is_requests(p: &FeaturePending) -> bool {
    matches!(p, FeaturePending::Spaces(Pending::JoinRequests { .. }))
}

fn requests(h: &mut Harness) -> oneshot::Receiver<Result<Vec<JoinRequest>, ClientError>> {
    owner_call(h, |reply| Command::JoinRequests {
        service: service_bare(),
        node: "dev".into(),
        reply,
    })
}

#[test]
fn the_join_requests_are_the_pending_owner_subscriptions() {
    let mut h = followed();
    let mut rx = requests(&mut h);
    let sent = h.sent_iqs();
    assert!(matches!(&sent[0], Iq::Get { .. }));
    let text = payload_of(&sent[0]);
    assert!(text.contains(ns::PUBSUB_OWNER), "{text}");
    assert!(
        text.contains("<subscriptions") && text.contains("dev"),
        "{text}"
    );
    h.answer(
        is_requests,
        Some(xml(&format!(
            "<pubsub xmlns='{}'><subscriptions node='dev'>
               <subscription jid='alice@chord.localhost' subscription='subscribed'/>
               <subscription jid='bob@chord.localhost' subscription='pending' subid='s1'/>
               <subscription jid='carol@chord.localhost' subscription='pending'/>
             </subscriptions></pubsub>",
            ns::PUBSUB_OWNER
        ))),
    );
    let Ok(Some(Ok(list))) = rx.try_recv() else {
        panic!("expected requests")
    };
    assert_eq!(
        list,
        [
            JoinRequest {
                jid: "bob@chord.localhost".into(),
                subid: Some("s1".into())
            },
            JoinRequest {
                jid: "carol@chord.localhost".into(),
                subid: None
            },
        ]
    );
    // An error, and an empty answer.
    let mut rx = requests(&mut h);
    h.respond(is_requests, forbidden());
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Server(_))))
    ));
    let mut rx = requests(&mut h);
    h.answer(is_requests, None);
    assert_eq!(rx.try_recv(), Ok(Some(Ok(vec![]))));
}

#[test]
fn approve_and_deny_send_the_authorization_form() {
    for (state, allow) in [("subscribed", "true"), ("none", "false")] {
        let mut h = followed();
        // A stored request from the service goes away after the answer.
        crate::features::spaces::db::add_request(
            h.store.conn(),
            h.account_id,
            SERVICE,
            "dev",
            "bob@chord.localhost",
        )
        .unwrap();
        let mut rx = owner_call(&mut h, |reply| Command::AnswerJoin {
            service: service_bare(),
            node: "dev".into(),
            jid: Jid::new("bob@chord.localhost").unwrap(),
            state,
            reply,
        });
        assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
        let sent = h.take_sent();
        let m = sent
            .iter()
            .find_map(|s| match s {
                xmpp_parsers::stanza::Stanza::Message(m) => Some(m),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected a message: {sent:?}"));
        let form = String::from(&m.payloads[0]);
        assert!(form.contains(FORM_SUBSCRIBE_AUTHORIZATION), "{form}");
        assert!(form.contains("bob@chord.localhost"), "{form}");
        assert!(form.contains(&format!("<value>{allow}</value>")), "{form}");
        let left =
            crate::features::spaces::db::requests(h.store.conn(), h.account_id, SERVICE, "dev")
                .unwrap();
        assert!(left.is_empty());
    }
}

#[test]
fn an_authorize_space_needs_the_service_feature() {
    let mut h = harness();
    let mut rx = create_with(&mut h, "Gate", SpaceAccess::Authorize);
    assert!(matches!(
        rx.try_recv(),
        Ok(Some(Err(ClientError::Unsupported(_))))
    ));
    assert!(h.sent_iqs().is_empty());

    let mut h = harness_with(&[FEATURE_ACCESS_AUTHORIZE]);
    let mut rx = create_with(&mut h, "Gate", SpaceAccess::Authorize);
    let sent = h.sent_iqs();
    let text = payload_of(&sent[0]);
    assert!(
        text.contains("authorize") && text.contains(NS_SPACES),
        "{text}"
    );
    assert_eq!(rx.try_recv(), Ok(None));
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::Create { .. })),
        None,
    );
    h.answer(
        |p| matches!(p, FeaturePending::Spaces(Pending::CreateSubscribe { .. })),
        Some(subscription("subscribed")),
    );
    assert!(matches!(rx.try_recv(), Ok(Some(Ok(_)))));
    assert_eq!(column(&h, "SELECT access_model FROM spaces"), ["authorize"]);
}

#[test]
fn the_access_forms_use_each_access_model() {
    for (access, word) in [
        (SpaceAccess::Open, "open"),
        (SpaceAccess::Authorize, "authorize"),
        (SpaceAccess::Whitelist, "whitelist"),
    ] {
        let form = node_config("x", access);
        assert!(form.contains(&format!("<value>{word}</value>")), "{form}");
    }
}

#[test]
fn the_owner_commands_fail_offline() {
    let (a, mut ra) = oneshot::channel::<Result<Vec<JoinRequest>, ClientError>>();
    offline(Command::JoinRequests {
        service: service_bare(),
        node: "n".into(),
        reply: a,
    });
    assert_eq!(ra.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
    let (a, mut ra) = oneshot::channel();
    offline(Command::AnswerJoin {
        service: service_bare(),
        node: "n".into(),
        jid: Jid::new("b@x").unwrap(),
        state: "none",
        reply: a,
    });
    assert_eq!(ra.try_recv(), Ok(Some(Err(ClientError::NotConnected))));
}

/// The room JIDs of the member grants in the sent stanzas.
fn granted_rooms(h: &mut Harness, jid: &str) -> Vec<String> {
    h.sent_iqs()
        .iter()
        .filter(|iq| payload_of(iq).contains("muc#admin"))
        .filter(|iq| {
            let text = payload_of(iq);
            text.contains("affiliation='member'") && text.contains(jid)
        })
        .filter_map(|iq| match iq {
            Iq::Set { to: Some(to), .. } => Some(to.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn approving_a_join_makes_the_member_a_member_of_each_room() {
    let mut h = followed();
    let mut rx = owner_call(&mut h, |reply| Command::AnswerJoin {
        service: service_bare(),
        node: "dev".into(),
        jid: Jid::new("bob@chord.localhost/phone").unwrap(),
        state: "subscribed",
        reply,
    });
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert_eq!(
        granted_rooms(&mut h, "bob@chord.localhost"),
        ["room1@rooms.chord.localhost"]
    );
}

#[test]
fn denying_a_join_grants_no_membership() {
    let mut h = followed();
    let mut rx = owner_call(&mut h, |reply| Command::AnswerJoin {
        service: service_bare(),
        node: "dev".into(),
        jid: Jid::new("bob@chord.localhost").unwrap(),
        state: "none",
        reply,
    });
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert!(granted_rooms(&mut h, "bob@chord.localhost").is_empty());
}

#[test]
fn adding_a_member_grants_membership_after_the_service_accepts() {
    let mut h = followed();
    let (reply, mut rx) = oneshot::channel();
    let bob = BareJid::new("bob@chord.localhost").unwrap();
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
    h.take_sent();
    h.answer(is_done, None);
    assert_eq!(rx.try_recv(), Ok(Some(Ok(()))));
    assert_eq!(
        granted_rooms(&mut h, "bob@chord.localhost"),
        ["room1@rooms.chord.localhost"]
    );
}
