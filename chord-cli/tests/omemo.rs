//! The OMEMO commands that work offline. They need no server.

use std::path::PathBuf;
use std::process::{Command, Output};

fn db(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chord-cli-omemo-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("db.sqlite3")
}

fn cli(db: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chord-cli"))
        .arg("--offline")
        .args(args)
        .env("CHORD_JID", "alice@chord.localhost")
        .env("CHORD_DB", db)
        .output()
        .expect("run chord-cli")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn omemo_state_is_off_then_on_then_off() {
    let db = db("state");
    let out = cli(&db, &["omemo", "bob@chord.localhost"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("bob@chord.localhost: off"));
    let out = cli(&db, &["omemo", "bob@chord.localhost", "on"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains(": on"));
    // A new process reads the saved state.
    let out = cli(&db, &["--json", "omemo", "bob@chord.localhost"]);
    assert!(
        stdout(&out).contains(r#""enabled":true"#),
        "{}",
        stdout(&out)
    );
    let out = cli(&db, &["omemo", "bob@chord.localhost", "off"]);
    assert!(stdout(&out).contains(": off"));
    let _ = std::fs::remove_dir_all(db.parent().unwrap());
}

#[test]
fn omemo_devices_of_an_unknown_contact_is_empty() {
    let db = db("devices");
    let out = cli(&db, &["omemo-devices", "bob@chord.localhost"]);
    assert!(out.status.success());
    assert!(stdout(&out).starts_with("devices (0)"));
    let _ = std::fs::remove_dir_all(db.parent().unwrap());
}

#[test]
fn omemo_commands_check_their_arguments() {
    let db = db("usage");
    for args in [
        &["omemo"][..],
        &["omemo", "bob@chord.localhost", "maybe"],
        &["omemo-trust", "bob@chord.localhost", "aa", "trusted"],
        &["omemo-trust", "bob@chord.localhost"],
        &["omemo-devices", "not a jid", "x"],
    ] {
        let out = cli(&db, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage"),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    // These need a session.
    for args in [
        &["omemo-refresh", "bob@chord.localhost"][..],
        &["omemo-remove-device", "5"],
    ] {
        let out = cli(&db, args);
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains("needs a session"));
    }
    let _ = std::fs::remove_dir_all(db.parent().unwrap());
}
