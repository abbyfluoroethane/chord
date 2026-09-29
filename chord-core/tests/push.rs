//! Live test of XEP-0357 push registration against the dev Prosody server.

use std::time::Duration;

use chord_core::actor;
use chord_core::jid::{BareJid, Jid};
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig};
use chord_core::store::Store;

const TIMEOUT: Duration = Duration::from_secs(15);

fn password(name: &str) -> String {
    let env = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../dev/prosody/.env"))
        .expect("dev/prosody/.env");
    env.lines()
        .find_map(|l| l.strip_prefix(&format!("{name}=")))
        .expect("password in .env")
        .trim()
        .trim_matches('"')
        .to_owned()
}

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn enable_and_disable_push() {
    let alice = BareJid::new("alice@chord.localhost").unwrap();
    let (handle, _events, actor) =
        actor::new::<NativeSession>(Store::open_in_memory().unwrap(), alice.clone()).unwrap();
    let task = tokio::spawn(actor.run());
    let config = SessionConfig::new(
        alice,
        password("ALICE_PASSWORD"),
        ServerAddr::StartTls {
            host: "localhost".into(),
            port: 5222,
        },
    );
    handle.login(config).await.unwrap();

    // The app server does not exist. Prosody stores the registration all the same.
    let service = Jid::new("push.example").unwrap();
    let node = format!("chord-test-{}", std::process::id());
    let form = Some(vec![("secret".to_owned(), "not-a-real-secret".to_owned())]);
    tokio::time::timeout(
        TIMEOUT,
        handle.enable_push(service.clone(), node.clone(), form),
    )
    .await
    .expect("enable in time")
    .expect("enable");
    let registrations = handle.push_registrations().await.unwrap();
    assert!(
        registrations
            .iter()
            .any(|r| r.service == "push.example" && r.node == node),
        "{registrations:?}"
    );

    tokio::time::timeout(TIMEOUT, handle.disable_push(service, Some(node.clone())))
        .await
        .expect("disable in time")
        .expect("disable");
    let registrations = handle.push_registrations().await.unwrap();
    assert!(registrations.iter().all(|r| r.node != node));

    let _ = tokio::time::timeout(TIMEOUT, handle.logout()).await;
    drop(handle);
    let _ = tokio::time::timeout(TIMEOUT, task).await;
}
