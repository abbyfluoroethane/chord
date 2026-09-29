//! Roster and contact presence against the dev Prosody server.
#![cfg(feature = "native-session")]

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use chord_core::actor::{self, ClientEvent, ClientEvents, ClientHandle, ConnectionState};
use chord_core::features::roster::Subscription;
use chord_core::jid::BareJid;
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, Stream};
use chord_core::store::Store;
use chord_core::views::ListDiff;

const LIMIT: Duration = Duration::from_secs(15);

fn password(name: &str) -> String {
    let env = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../dev/prosody/.env"))
        .expect("read dev/prosody/.env");
    let key = format!("{name}_PASSWORD=");
    env.lines()
        .find_map(|l| l.strip_prefix(&key))
        .expect("password in .env")
        .trim()
        .to_owned()
}

async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

async fn within<T>(what: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(LIMIT, future)
        .await
        .unwrap_or_else(|_| panic!("timeout: {what}"))
}

struct Client {
    handle: ClientHandle,
    events: ClientEvents,
    path: PathBuf,
}

async fn start(user: &str, tag: &str) -> Client {
    let jid = BareJid::new(&format!("{user}@chord.localhost")).unwrap();
    let path = std::env::temp_dir().join(format!("chord-roster-{tag}-{user}.sqlite3"));
    let _ = std::fs::remove_file(&path);
    let store = Store::open(&path).unwrap();
    let (handle, mut events, actor) = actor::new::<NativeSession>(store, jid.clone()).unwrap();
    tokio::spawn(actor.run());
    let server = std::env::var("CHORD_TEST_SERVER").unwrap_or_default();
    let addr = if server.is_empty() {
        ServerAddr::StartTls {
            host: "localhost".into(),
            port: 5222,
        }
    } else {
        ServerAddr::StartTls {
            host: server,
            port: 5222,
        }
    };
    let config = chord_core::session::SessionConfig::new(jid, password(&user.to_uppercase()), addr);
    within("login", handle.login(config)).await.unwrap();
    within("connected", async {
        while let Some(event) = next(&mut events).await {
            if matches!(
                event,
                ClientEvent::ConnectionState(ConnectionState::Connected { .. })
            ) {
                return;
            }
        }
        panic!("closed");
    })
    .await;
    Client {
        handle,
        events,
        path,
    }
}

/// Poll the contacts until `check` holds.
async fn wait_contact(
    handle: &ClientHandle,
    jid: &BareJid,
    check: impl Fn(Option<&chord_core::features::roster::Contact>) -> bool,
) {
    within("contact state", async {
        loop {
            let list = handle.contacts().await.unwrap();
            if check(list.iter().find(|c| &c.jid == jid)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
}

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn subscribe_approve_presence_and_remove() {
    let tag = uuid_tag();
    let name = format!("Bobby {tag}");
    let alice_jid = BareJid::new("alice@chord.localhost").unwrap();
    let bob_jid = BareJid::new("bob@chord.localhost").unwrap();
    let alice = start("alice", &tag).await;
    let mut bob = start("bob", &tag).await;

    // Start clean, in case an earlier run stopped half way.
    let _ = alice.handle.remove_contact(bob_jid.clone()).await;
    let _ = bob.handle.remove_contact(alice_jid.clone()).await;

    // Alice adds bob. Bob sees the request.
    let mut members = alice.handle.member_list(bob_jid.clone()).await.unwrap();
    alice
        .handle
        .add_contact(bob_jid.clone(), Some(name.clone()))
        .await
        .unwrap();
    let requester = within("subscription request", async {
        while let Some(event) = next(&mut bob.events).await {
            if let ClientEvent::SubscriptionRequest(jid) = event {
                return jid;
            }
        }
        panic!("closed");
    })
    .await;
    assert_eq!(requester, alice_jid);
    wait_contact(&alice.handle, &bob_jid, |c| {
        c.is_some_and(|c| c.ask && c.name.as_deref() == Some(&name))
    })
    .await;

    // Bob approves. Alice now sees bob, and bob's presence.
    bob.handle
        .approve_subscription(alice_jid.clone())
        .await
        .unwrap();
    wait_contact(&alice.handle, &bob_jid, |c| {
        c.is_some_and(|c| c.subscription == Subscription::To && !c.ask)
    })
    .await;
    within("bob online in the member list", async {
        let mut online = false;
        while !online {
            match next(&mut members).await.expect("stream ended") {
                ListDiff::Reset(items) => {
                    online = items.iter().any(|m| m.id == bob_jid.as_str() && m.online)
                }
                ListDiff::Update { item, .. } | ListDiff::Insert { item, .. } => {
                    online = item.id == bob_jid.as_str() && item.online
                }
                ListDiff::Remove { .. } => {}
            }
        }
    })
    .await;

    // Bob removes alice. Alice loses the subscription. Then alice removes bob.
    bob.handle.remove_contact(alice_jid.clone()).await.unwrap();
    wait_contact(&alice.handle, &bob_jid, |c| {
        c.is_some_and(|c| c.subscription == Subscription::None)
    })
    .await;
    alice.handle.remove_contact(bob_jid.clone()).await.unwrap();
    wait_contact(&alice.handle, &bob_jid, |c| c.is_none()).await;

    alice.handle.logout().await;
    bob.handle.logout().await;
    let _ = std::fs::remove_file(&alice.path);
    let _ = std::fs::remove_file(&bob.path);
}

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn preapprove_sets_the_approved_flag_and_remove_clears_it() {
    let tag = uuid_tag();
    // A JID that no other test uses. Pre-approval needs no account behind the JID.
    let jid = BareJid::new(&format!("preapprove-{tag}@chord.localhost")).unwrap();
    let alice = start("alice", &tag).await;
    // Remove the items that an earlier run that stopped half way left.
    for c in alice.handle.contacts().await.unwrap() {
        if c.jid.as_str().starts_with("preapprove-") {
            let _ = alice.handle.remove_contact(c.jid).await;
        }
    }

    let result = within(
        "pre-approve",
        alice.handle.preapprove_subscription(jid.clone()),
    )
    .await;
    match result {
        Ok(()) => {
            wait_contact(&alice.handle, &jid, |c| {
                c.is_some_and(|c| c.approved && c.subscription == Subscription::None)
            })
            .await;
            // Pre-approval again is a no-op that succeeds at once.
            alice
                .handle
                .preapprove_subscription(jid.clone())
                .await
                .unwrap();
            // Removing the contact cancels the pre-approval. Prosody keeps the flag after
            // an `unsubscribed` presence, so the test does not use it.
        }
        Err(actor::ClientError::Unsupported(_)) => {
            eprintln!("this server does not support pre-approval");
        }
        Err(e) => panic!("pre-approve: {e}"),
    }
    let _ = alice.handle.remove_contact(jid.clone()).await;
    wait_contact(&alice.handle, &jid, |c| c.is_none()).await;
    alice.handle.logout().await;
    let _ = std::fs::remove_file(&alice.path);
}

fn uuid_tag() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{:x}{:x}", std::process::id(), nanos)
}
