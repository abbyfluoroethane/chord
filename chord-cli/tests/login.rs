use std::process::Command;
use std::time::{Duration, Instant};

#[test]
#[ignore = "needs the dev Prosody server: ./dev/prosody/setup.sh"]
fn wrong_password_exits_with_2() {
    let server =
        std::env::var("CHORD_TEST_SERVER").unwrap_or_else(|_| "tcp://localhost:5222".to_owned());
    let start = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_chord-cli"))
        .arg("login")
        .env("CHORD_JID", "alice@chord.localhost")
        .env("CHORD_PASSWORD", "definitely-wrong")
        .env("CHORD_SERVER", server)
        .output()
        .expect("run chord-cli");
    let elapsed = start.elapsed();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("wrong username or password"),
        "stderr: {stderr}"
    );
    assert!(elapsed < Duration::from_secs(15), "took {elapsed:?}");
}
