//! Avatars (XEP-0084) against the dev Prosody server. Alice publishes an avatar, bob
//! fetches it. The test ends with an empty metadata element, so alice has no avatar.

use std::time::Duration;

use chord_core::actor::{self, ClientHandle};
use chord_core::features::avatars::sha1_hex;
use chord_core::jid::BareJid;
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig};
use chord_core::store::Store;

fn password(name: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../dev/prosody/.env");
    let env = std::fs::read_to_string(path).expect("dev/prosody/.env");
    env.lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("{name} is missing in dev/prosody/.env"))
        .trim()
        .trim_matches('"')
        .to_owned()
}

async fn login(user: &str, name: &str) -> ClientHandle {
    let server = std::env::var("CHORD_TEST_SERVER").unwrap_or_else(|_| "localhost:5222".into());
    let (host, port) = server.rsplit_once(':').unwrap();
    let jid = BareJid::new(&format!("{user}@chord.localhost")).unwrap();
    let store = Store::open_in_memory().unwrap();
    let (handle, _events, actor) = actor::new::<NativeSession>(store, jid.clone()).unwrap();
    tokio::spawn(actor.run());
    let config = SessionConfig::new(
        jid,
        password(name),
        ServerAddr::StartTls {
            host: host.to_owned(),
            port: port.parse().unwrap(),
        },
    );
    handle.login(config).await.expect("login");
    // Let the session settle: the first stanzas of a session get their answers.
    tokio::time::sleep(Duration::from_millis(500)).await;
    handle
}

#[tokio::test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn alice_publishes_and_bob_fetches_the_avatar() {
    let alice_jid = BareJid::new("alice@chord.localhost").unwrap();
    let alice = login("alice", "ALICE_PASSWORD").await;
    let bob = login("bob", "BOB_PASSWORD").await;

    // Not a real PNG, but the server does not look at the bytes.
    let suffix = uuid_suffix();
    let image = format!("chord avatar test {suffix}").into_bytes();
    alice
        .set_avatar("image/png".into(), image.clone(), 8, 8)
        .await
        .expect("set_avatar");

    let mine = alice.avatar(alice_jid.clone()).await.unwrap().unwrap();
    assert_eq!(mine.hash, sha1_hex(&image));
    assert_eq!(mine.data.as_deref(), Some(&image[..]));

    let result = bob.refresh_avatar(alice_jid.clone()).await;
    let got = bob.avatar(alice_jid.clone()).await;

    // Clean up before the asserts, so a failure leaves no avatar.
    let removed = alice.remove_avatar().await;
    let after = bob.refresh_avatar(alice_jid.clone()).await;
    let after_avatar = bob.avatar(alice_jid).await;
    alice.logout().await;
    bob.logout().await;

    result.expect("refresh_avatar");
    let got = got.unwrap().expect("bob has the avatar");
    assert_eq!(got.hash, sha1_hex(&image));
    assert_eq!(got.mime.as_deref(), Some("image/png"));
    assert_eq!(got.data.as_deref(), Some(&image[..]));
    removed.expect("remove_avatar");
    after.expect("refresh after remove");
    assert_eq!(after_avatar.unwrap(), None);
}

fn uuid_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{nanos:x}")
}
