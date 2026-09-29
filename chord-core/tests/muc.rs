//! Live test of rooms and bookmarks against the dev Prosody server.

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

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn rooms_and_bookmarks() {
    let suffix = suffix();
    let room = BareJid::new(&format!("chord-muc-{suffix}@rooms.chord.localhost")).unwrap();
    let (alice, _alice_events, alice_task) = client("alice", &password("ALICE_PASSWORD")).await;
    let (bob, mut bob_events, bob_task) = client("bob", &password("BOB_PASSWORD")).await;
    let (a_nick, b_nick) = (format!("alice-{suffix}"), format!("bob-{suffix}"));

    // Alice creates the room. The client unlocks it.
    tokio::time::timeout(TIMEOUT, alice.join_room(room.clone(), a_nick.clone(), None))
        .await
        .unwrap()
        .expect("alice joins");

    // Bob wants the nick of Alice: conflict. Then he joins with his own nick.
    let error = tokio::time::timeout(TIMEOUT, bob.join_room(room.clone(), a_nick.clone(), None))
        .await
        .unwrap()
        .expect_err("nick in use");
    assert!(
        matches!(&error, ClientError::Server(t) if t.contains("conflict")),
        "{error:?}"
    );
    tokio::time::timeout(TIMEOUT, bob.join_room(room.clone(), b_nick.clone(), None))
        .await
        .unwrap()
        .expect("bob joins");

    // Alice sees both occupants.
    let mut members = alice.member_list(room.clone()).await.unwrap();
    let wait = async {
        let mut nicks: Vec<String> = Vec::new();
        while let Some(diff) = next(&mut members).await {
            match diff {
                ListDiff::Reset(items) => nicks = items.into_iter().map(|m| m.id).collect(),
                ListDiff::Insert { item, .. } => nicks.push(item.id),
                _ => {}
            }
            if nicks.contains(&a_nick) && nicks.contains(&b_nick) {
                return;
            }
        }
        panic!("member list ended");
    };
    tokio::time::timeout(TIMEOUT, wait)
        .await
        .expect("both occupants");

    // A message reaches Bob, and Alice keeps one row for it, keyed by the stanza-id.
    let mut timeline = alice.timeline(room.clone()).await.unwrap();
    alice
        .send_chat(room.clone().into(), "hello room".into())
        .await
        .unwrap();
    wait_message(&mut bob_events, "hello room").await;
    let wait = async {
        loop {
            let diff = next(&mut timeline.stream).await.expect("timeline ended");
            let items = match diff {
                ListDiff::Reset(items) => items,
                ListDiff::Update { item, .. } | ListDiff::Insert { item, .. } => vec![item],
                _ => continue,
            };
            if let Some(item) = items.iter().find(|i| i.body == "hello room")
                && item.id.starts_with("stanza-id:")
            {
                assert!(item.outgoing);
                return;
            }
        }
    };
    tokio::time::timeout(TIMEOUT, wait)
        .await
        .expect("the own message gets its stanza-id");

    // Bookmarks: add, then remove.
    alice
        .add_bookmark(
            room.clone(),
            Some("Test".into()),
            false,
            Some(a_nick.clone()),
        )
        .await
        .expect("add bookmark");
    alice
        .remove_bookmark(room.clone())
        .await
        .expect("remove bookmark");

    alice.leave_room(room.clone()).await.unwrap();
    bob.leave_room(room.clone()).await.unwrap();
    let _ = tokio::time::timeout(TIMEOUT, alice.logout()).await;
    let _ = tokio::time::timeout(TIMEOUT, bob.logout()).await;
    // A Timeline holds a handle too: the actor runs until every handle is gone.
    drop((alice, bob, timeline));
    let _ = tokio::time::timeout(TIMEOUT, alice_task).await;
    let _ = tokio::time::timeout(TIMEOUT, bob_task).await;
}
