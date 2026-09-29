//! Live test of the message archive against the dev Prosody server.

use std::path::PathBuf;
use std::time::Duration;

use chord_core::actor::{ClientHandle, new};
use chord_core::features::new_id;
use chord_core::jid::{BareJid, Jid};
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig};
use chord_core::store::{Store, queries};

const ENV_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../dev/prosody/.env");

fn password(name: &str) -> String {
    let env =
        std::fs::read_to_string(ENV_FILE).expect("dev/prosody/.env: run ./dev/prosody/setup.sh");
    env.lines()
        .find_map(|l| l.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("{name} is not in dev/prosody/.env"))
        .trim()
        .trim_matches('"')
        .to_owned()
}

fn config(user: &str, pass: &str) -> SessionConfig {
    let server = ServerAddr::StartTls {
        host: "localhost".into(),
        port: 5222,
    };
    let jid = BareJid::new(&format!("{user}@chord.localhost")).unwrap();
    SessionConfig::new(jid, password(pass), server)
}

fn temp_db(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("chord-mam-{name}-{}.db", new_id()))
}

/// Start an actor for `user` and log in.
async fn login(user: &str, pass: &str, db: &PathBuf) -> ClientHandle {
    let config = config(user, pass);
    let store = Store::open(db).unwrap();
    let (handle, _events, actor) = new::<NativeSession>(store, config.jid.clone()).unwrap();
    tokio::task::spawn_local(actor.run());
    handle.login(config).await.unwrap();
    handle
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
async fn a_new_client_catches_up_from_the_account_archive() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let marker = new_id();
            let bob_jid = BareJid::new("bob@chord.localhost").unwrap();
            let db_bob = temp_db("bob");
            let bob = login("bob", "BOB_PASSWORD", &db_bob).await;

            // Alice sends three messages. The server archives them.
            let db_a1 = temp_db("alice1");
            let alice1 = login("alice", "ALICE_PASSWORD", &db_a1).await;
            for i in 0..3 {
                let to = Jid::from(bob_jid.clone());
                alice1.send_chat(to, format!("{marker} {i}")).await.unwrap();
            }
            alice1.logout().await;

            // A second client of alice has an empty store. Its login fetches the newest
            // page of the archive.
            let db_a2 = temp_db("alice2");
            let alice2 = login("alice", "ALICE_PASSWORD", &db_a2).await;
            let reader = Store::open(&db_a2).unwrap();
            let account = queries::ensure_account(reader.conn(), "alice@chord.localhost").unwrap();
            let mut found = Vec::new();
            for _ in 0..50 {
                let all = queries::messages_with(reader.conn(), account, bob_jid.as_str()).unwrap();
                found = all
                    .into_iter()
                    .filter(|m| m.body.starts_with(&marker))
                    .collect();
                if found.len() == 3 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            assert_eq!(found.len(), 3, "{found:?}");
            assert!(
                found
                    .iter()
                    .all(|m| m.key_kind == queries::KeyKind::StanzaId)
            );

            let (newest, oldest): (Option<String>, Option<String>) = reader
                .conn()
                .query_row(
                    "SELECT newest_id, oldest_id FROM sync_cursors
                     WHERE account_id = ?1 AND archive = ''",
                    [account],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert!(newest.is_some() && oldest.is_some());

            // Older history for the chat with bob.
            alice2.load_older(bob_jid.clone()).await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
            // A new sync works and finds nothing new.
            alice2.sync_archive().await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;

            alice2.logout().await;
            bob.logout().await;
            for db in [db_bob, db_a1, db_a2] {
                let _ = std::fs::remove_file(db);
            }
        })
        .await;
}
