//! Live test of private messages between room occupants against the dev Prosody server.

use std::pin::Pin;
use std::time::Duration;

use chord_core::actor::{
    self, ClientError, ClientEvent, ClientEvents, ClientHandle, ConnectionState,
};
use chord_core::jid::BareJid;
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig, Stream};
use chord_core::store::Store;
use chord_core::views::ListDiff;
use chord_core::views::TimelineItem;

const TIMEOUT: Duration = Duration::from_secs(15);

fn password(name: &str) -> String {
    let env = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../dev/prosody/.env"))
        .expect("read dev/prosody/.env");
    env.lines()
        .find_map(|l| l.strip_prefix(&format!("{name}=")))
        .expect("password in .env")
        .trim()
        .trim_matches('"')
        .to_owned()
}

async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

async fn client(
    user: &str,
    pass: &str,
) -> (ClientHandle, ClientEvents, tokio::task::JoinHandle<()>) {
    let jid = BareJid::new(&format!("{user}@chord.localhost")).unwrap();
    let store = Store::open_in_memory().unwrap();
    let (handle, mut events, actor) = actor::new::<NativeSession>(store, jid.clone()).unwrap();
    let task = tokio::spawn(actor.run());
    let server = ServerAddr::StartTls {
        host: "localhost".into(),
        port: 5222,
    };
    handle
        .login(SessionConfig::new(jid, pass.to_owned(), server))
        .await
        .expect("login");
    // The actor runs the features after it gets the `Connected` event.
    let wait = async {
        while let Some(event) = next(&mut events).await {
            if matches!(
                event,
                ClientEvent::ConnectionState(ConnectionState::Connected { .. })
            ) {
                return;
            }
        }
        panic!("events ended");
    };
    tokio::time::timeout(TIMEOUT, wait)
        .await
        .expect("connected");
    (handle, events, task)
}

/// Wait for a message with `body` in the events.
async fn wait_message(events: &mut ClientEvents, body: &str) {
    let wait = async {
        while let Some(event) = next(events).await {
            if let ClientEvent::MessageReceived(m) = event
                && m.body == body
            {
                return;
            }
        }
        panic!("events ended");
    };
    tokio::time::timeout(TIMEOUT, wait)
        .await
        .expect("message did not arrive");
}

fn suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{:x}{:x}", std::process::id(), nanos)
}

/// Collect the timeline until `done` is true for the items, or time out.
async fn wait_items(
    timeline: &mut chord_core::actor::Timeline,
    done: impl Fn(&[TimelineItem]) -> bool,
) -> Vec<TimelineItem> {
    let wait = async {
        let mut items: Vec<TimelineItem> = Vec::new();
        loop {
            match next(&mut timeline.stream).await.expect("timeline ended") {
                ListDiff::Reset(all) => items = all,
                ListDiff::Insert { item, .. } => items.push(item),
                ListDiff::Update { index, item } => items[index] = item,
                ListDiff::Remove { index } => {
                    items.remove(index);
                }
            }
            if done(&items) {
                return items;
            }
        }
    };
    tokio::time::timeout(TIMEOUT, wait)
        .await
        .expect("timeline did not reach the expected state")
}

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn private_message_stays_out_of_the_room_timeline() {
    let suffix = suffix();
    let room = BareJid::new(&format!("chord-pm-{suffix}@rooms.chord.localhost")).unwrap();
    let (alice, _alice_events, alice_task) = client("alice", &password("ALICE_PASSWORD")).await;
    let (bob, mut bob_events, bob_task) = client("bob", &password("BOB_PASSWORD")).await;
    let (a_nick, b_nick) = (format!("alice-{suffix}"), format!("bob-{suffix}"));

    tokio::time::timeout(TIMEOUT, alice.join_room(room.clone(), a_nick.clone(), None))
        .await
        .unwrap()
        .expect("alice joins");
    tokio::time::timeout(TIMEOUT, bob.join_room(room.clone(), b_nick.clone(), None))
        .await
        .unwrap()
        .expect("bob joins");

    // Alice sends the private message when she sees Bob as an occupant.
    let mut sent = None;
    for _ in 0..30 {
        match alice
            .send_private(room.clone(), b_nick.clone(), "psst".into())
            .await
        {
            Ok(id) => {
                sent = Some(id);
                break;
            }
            Err(ClientError::Invalid(_)) => tokio::time::sleep(Duration::from_millis(250)).await,
            Err(e) => panic!("send_private: {e:?}"),
        }
    }
    sent.expect("bob became an occupant");
    // An unknown nick is an error.
    let error = alice
        .send_private(room.clone(), "nobody".into(), "x".into())
        .await
        .expect_err("not an occupant");
    assert!(matches!(error, ClientError::Invalid(_)), "{error:?}");

    wait_message(&mut bob_events, "psst").await;
    // A room message follows, so that the room timeline is not empty.
    alice
        .send_chat(room.clone().into(), "public".into())
        .await
        .unwrap();
    wait_message(&mut bob_events, "public").await;

    let mut private = bob
        .private_timeline(room.clone(), a_nick.clone())
        .await
        .unwrap();
    let items = wait_items(&mut private, |i| i.iter().any(|m| m.body == "psst")).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].sender_name, a_nick);
    assert!(!items[0].outgoing);

    let mut public = bob.timeline(room.clone()).await.unwrap();
    let items = wait_items(&mut public, |i| i.iter().any(|m| m.body == "public")).await;
    assert!(items.iter().all(|m| m.body != "psst"), "{items:?}");

    // Alice has the outgoing row in her own private timeline.
    let mut alice_private = alice
        .private_timeline(room.clone(), b_nick.clone())
        .await
        .unwrap();
    let items = wait_items(&mut alice_private, |i| !i.is_empty()).await;
    assert!(items[0].outgoing);
    assert_eq!(items[0].body, "psst");

    alice.leave_room(room.clone()).await.unwrap();
    bob.leave_room(room.clone()).await.unwrap();
    let _ = tokio::time::timeout(TIMEOUT, alice.logout()).await;
    let _ = tokio::time::timeout(TIMEOUT, bob.logout()).await;
    drop((alice, bob, private, public, alice_private));
    let _ = tokio::time::timeout(TIMEOUT, alice_task).await;
    let _ = tokio::time::timeout(TIMEOUT, bob_task).await;
}
