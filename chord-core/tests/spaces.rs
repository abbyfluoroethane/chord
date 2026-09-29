//! Live test of server-side spaces (XEP-0503) against the dev Prosody server.

use std::pin::Pin;
use std::time::Duration;

use chord_core::actor::{self, ClientError, ClientHandle};
use chord_core::features::spaces::JoinOutcome;
use chord_core::jid::BareJid;
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig, Stream};
use chord_core::store::Store;
use chord_core::views::{ChannelItem, ChannelScope, ListDiff, ViewItem, ViewStream};

const TIMEOUT: Duration = Duration::from_secs(15);

fn password(name: &str) -> String {
    let env = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../dev/prosody/.env"))
        .expect("dev/prosody/.env: run ./dev/prosody/setup.sh");
    env.lines()
        .find_map(|l| l.strip_prefix(&format!("{name}_PASSWORD=")))
        .expect("password in .env")
        .trim()
        .trim_matches('"')
        .to_owned()
}

async fn login(user: &str) -> ClientHandle {
    let jid = BareJid::new(&format!("{user}@chord.localhost")).unwrap();
    let store = Store::open_in_memory().unwrap();
    let (handle, _events, actor) = actor::new::<NativeSession>(store, jid.clone()).unwrap();
    tokio::spawn(actor.run());
    let server = ServerAddr::StartTls {
        host: "localhost".into(),
        port: 5222,
    };
    let config = SessionConfig::new(jid, password(&user.to_uppercase()), server);
    handle.login(config).await.expect("login");
    handle
}

/// Apply diffs to `list` until `done` is true.
async fn wait_for<T: ViewItem>(
    stream: &mut ViewStream<T>,
    list: &mut Vec<T>,
    done: impl Fn(&[T]) -> bool,
) {
    let run = async {
        while !done(list) {
            let diff = std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx))
                .await
                .expect("the view stream ended");
            match diff {
                ListDiff::Reset(items) => *list = items,
                ListDiff::Insert { index, item } => list.insert(index, item),
                ListDiff::Update { index, item } => list[index] = item,
                ListDiff::Remove { index } => {
                    list.remove(index);
                }
            }
        }
    };
    tokio::time::timeout(TIMEOUT, run)
        .await
        .expect("the view did not reach the expected state");
}

/// Create the space. The disco of the server may not be complete right after login.
async fn create_space(alice: &ClientHandle, name: &str, private: bool) -> (String, String) {
    for _ in 0..50 {
        match alice.create_space(name, private).await {
            Err(ClientError::Unsupported(_)) => {
                tokio::time::sleep(Duration::from_millis(200)).await
            }
            other => return other.expect("create_space"),
        }
    }
    panic!("no pubsub service");
}

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn private_space_with_a_room_and_a_member() {
    let alice = login("alice").await;
    let bob = login("bob").await;
    let suffix = &uuid_suffix();
    let (service, node) = create_space(&alice, &format!("Live space {suffix}"), true).await;

    let result = scenario(&alice, &bob, &service, &node, suffix).await;

    // Clean up, also after a failure.
    let _ = bob.leave_space(&service, &node).await;
    let deleted = alice.delete_space(&service, &node).await;
    alice.logout().await;
    bob.logout().await;
    result.expect("scenario");
    deleted.expect("delete_space");
}

async fn scenario(
    alice: &ClientHandle,
    bob: &ClientHandle,
    service: &str,
    node: &str,
    suffix: &str,
) -> Result<(), String> {
    let room = BareJid::new(&format!("live-{suffix}@rooms.chord.localhost")).unwrap();
    let bob_jid = BareJid::new("bob@chord.localhost").unwrap();
    let scope = ChannelScope::Space {
        service: service.to_owned(),
        node: node.to_owned(),
    };

    // Alice sees her space.
    let mut alice_spaces = alice.space_list().await.map_err(|e| e.to_string())?;
    let mut list = Vec::new();
    wait_for(&mut alice_spaces, &mut list, |l| {
        l.iter().any(|s| s.node == node)
    })
    .await;

    alice
        .add_room_to_space(service, node, room.clone(), "Live room")
        .await
        .map_err(|e| format!("add_room_to_space: {e}"))?;
    let mut alice_channels = alice.channel_list(scope.clone()).await.unwrap();
    let mut channels: Vec<ChannelItem> = Vec::new();
    wait_for(&mut alice_channels, &mut channels, |l| l.len() == 1).await;
    assert_eq!(channels[0].jid, room.to_string());
    assert_eq!(channels[0].name, "Live room");

    // Bob is not a member yet: a private space refuses him.
    match bob.join_space(service, node).await {
        Err(ClientError::Server(e)) => println!("bob refused as expected: {e}"),
        other => return Err(format!("bob joined before he was a member: {other:?}")),
    }

    alice
        .add_space_member(service, node, bob_jid)
        .await
        .map_err(|e| format!("add_space_member: {e}"))?;
    let outcome = bob
        .join_space(service, node)
        .await
        .map_err(|e| format!("join_space: {e}"))?;
    assert_eq!(outcome, JoinOutcome::Joined);

    // Bob sees the space and its room item, loaded before join_space returned.
    let mut bob_spaces = bob.space_list().await.unwrap();
    let mut spaces = Vec::new();
    wait_for(&mut bob_spaces, &mut spaces, |l| {
        l.iter()
            .any(|s| s.node == node && s.name.starts_with("Live space"))
    })
    .await;
    let mut bob_channels = bob.channel_list(scope).await.unwrap();
    let mut seen: Vec<ChannelItem> = Vec::new();
    wait_for(&mut bob_channels, &mut seen, |l| l.len() == 1).await;
    assert_eq!(seen[0].jid, room.to_string());

    // A room that alice removes disappears at bob's client through the event.
    alice
        .remove_room_from_space(service, node, room)
        .await
        .map_err(|e| format!("remove_room_from_space: {e}"))?;
    wait_for(&mut bob_channels, &mut seen, |l| l.is_empty()).await;
    Ok(())
}

/// A random suffix for room and node names.
fn uuid_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{:x}{:x}", std::process::id(), nanos)
}

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn browse_finds_a_public_space_and_not_a_private_one() {
    let alice = login("alice").await;
    let suffix = uuid_suffix();
    let (service, public) = create_space(&alice, &format!("Public {suffix}"), false).await;
    let (_, private) = create_space(&alice, &format!("Private {suffix}"), true).await;

    let found = alice.browse_spaces().await;
    let _ = alice.delete_space(&service, &public).await;
    let _ = alice.delete_space(&service, &private).await;
    alice.logout().await;

    let found = found.expect("browse_spaces");
    assert!(
        found
            .iter()
            .any(|s| s.node == public && s.name == format!("Public {suffix}"))
    );
    assert!(!found.iter().any(|s| s.node == private));
}

/// More spaces than the browse window (20 disco#info queries at a time). Prosody has no
/// RSM, type filtering, or extended disco for pubsub, so this covers the plain path.
#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn browse_finds_more_spaces_than_the_query_window() {
    const COUNT: usize = 22;
    let alice = login("alice").await;
    let suffix = uuid_suffix();
    let mut nodes = Vec::new();
    let mut service = String::new();
    for i in 0..COUNT {
        let (s, node) = create_space(&alice, &format!("Many {suffix} {i:02}"), false).await;
        service = s;
        nodes.push(node);
    }

    let found = alice.browse_spaces().await;
    for node in &nodes {
        let _ = alice.delete_space(&service, node).await;
    }
    alice.logout().await;

    let found = found.expect("browse_spaces");
    for node in &nodes {
        assert!(found.iter().any(|s| &s.node == node), "missing {node}");
    }
    // The list is sorted by name.
    let names: Vec<&str> = found
        .iter()
        .filter(|s| s.name.starts_with(&format!("Many {suffix}")))
        .map(|s| s.name.as_str())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(names.len(), COUNT);
}
