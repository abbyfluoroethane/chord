//! Live test of XEP-0363 upload against the dev Prosody server.

use std::time::Duration;

use chord_core::actor::{self, ClientError};
use chord_core::jid::{BareJid, Jid};
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig};
use chord_core::store::Store;

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
async fn alice_uploads_a_file_for_bob() {
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

    let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let to = Jid::new("bob@chord.localhost").unwrap();
    let mut url = None;
    for _ in 0..40 {
        match handle
            .upload(
                to.clone(),
                "chord-test.bin".into(),
                "application/octet-stream".into(),
                data.clone(),
            )
            .await
        {
            Err(ClientError::Unsupported(e)) if e.contains("not discovered yet") => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            other => {
                url = Some(other.expect("upload"));
                break;
            }
        }
    }
    let url = url.expect("the upload service was never discovered");

    let body = reqwest::get(&url).await.unwrap();
    assert!(body.status().is_success(), "GET {url}: {}", body.status());
    assert_eq!(body.bytes().await.unwrap().as_ref(), data.as_slice());

    handle.logout().await;
    drop(handle);
    task.await.unwrap();
}
