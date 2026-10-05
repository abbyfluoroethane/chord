//! Interop with python-twomemo (OMEMO 2, MIT license). Both directions, both ways to start.
//!
//! These tests are `#[ignore]` because they need a Python venv. To run them:
//!
//! ```text
//! python3 -m venv chord-omemo/tests/interop/venv
//! chord-omemo/tests/interop/venv/bin/pip install -r chord-omemo/tests/interop/requirements.txt
//! cargo test -p chord-omemo --test interop -- --ignored --nocapture
//! ```
//!
//! Set `CHORD_OMEMO_PYTHON` to use a Python from another place. See
//! `tests/interop/README.md`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use chord_omemo::{Bundle, Identity, decrypt_key, encrypt_key, payload};
use serde_json::{Value, json};

struct Peer {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Peer {
    fn start() -> Peer {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/interop");
        let python = std::env::var("CHORD_OMEMO_PYTHON")
            .unwrap_or_else(|_| dir.join("venv/bin/python").to_string_lossy().into_owned());
        let mut child = Command::new(&python)
            .arg(dir.join("twomemo_peer.py"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {python}: {e}. See tests/interop/README.md"));
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Peer {
            child,
            stdin,
            stdout,
        }
    }

    /// Send one command. The reply must be good.
    fn ok(&mut self, cmd: Value) -> Value {
        let reply = self.raw(cmd);
        assert_eq!(reply["ok"], json!(true), "python peer said: {reply}");
        reply
    }

    fn raw(&mut self, cmd: Value) -> Value {
        writeln!(self.stdin, "{cmd}").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad reply {line:?}: {e}"))
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hx(v: &Value) -> Vec<u8> {
    hex::decode(
        v.as_str()
            .unwrap_or_else(|| panic!("not a hex string: {v}")),
    )
    .unwrap()
}

fn bundle_from_json(j: &Value) -> Bundle {
    Bundle {
        ik: hx(&j["ik"]),
        spk_id: j["spk_id"].as_u64().unwrap() as u32,
        spk: hx(&j["spk"]),
        spks: hx(&j["spks"]),
        prekeys: j["prekeys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_u64().unwrap() as u32, hx(&p[1])))
            .collect(),
    }
}

fn bundle_to_json(b: &Bundle) -> Value {
    json!({
        "ik": hex::encode(&b.ik),
        "spk_id": b.spk_id,
        "spk": hex::encode(&b.spk),
        "spks": hex::encode(&b.spks),
        "prekeys": b.prekeys.iter().map(|(i, k)| json!([i, hex::encode(k)])).collect::<Vec<_>>(),
    })
}

/// The Rust side of one conversation with the Python device.
struct Rust {
    me: Identity,
    session: Option<Vec<u8>>,
    py_bundle: Option<Bundle>,
    py: Peer,
    py_ik: Vec<u8>,
}

impl Rust {
    /// Encrypt `text` for Python and let Python decrypt it. Returns the `kex` flag.
    fn send(&mut self, text: &str) -> bool {
        let (material, ciphertext) = payload::encrypt(text.as_bytes()).unwrap();
        let enc = encrypt_key(
            &self.me,
            self.session.as_deref(),
            self.py_bundle.as_ref(),
            &material,
        )
        .unwrap();
        self.session = Some(enc.session.clone());
        self.deliver(&enc.data, enc.kex, Some(&ciphertext), text);
        enc.kex
    }

    fn deliver(&mut self, data: &[u8], kex: bool, ciphertext: Option<&[u8]>, text: &str) {
        let reply = self.py.ok(json!({
            "cmd": "decrypt", "device_id": self.me.device_id(), "data": hex::encode(data), "kex": kex,
            "payload": ciphertext.map(hex::encode),
        }));
        if ciphertext.is_some() {
            assert_eq!(
                hx(&reply["plaintext"]),
                text.as_bytes(),
                "python read another text"
            );
        } else {
            assert_eq!(reply["empty"], json!(true));
        }
    }

    /// Ask Python for a message and decrypt it here. Returns the `kex` flag.
    fn receive(&mut self, text: &str) -> bool {
        let m = self.py.ok(json!({"cmd": "encrypt", "device_id": self.me.device_id(), "plaintext": hex::encode(text)}));
        self.read(&m, text)
    }

    fn read(&mut self, m: &Value, text: &str) -> bool {
        let kex = m["kex"].as_bool().unwrap();
        let dec = decrypt_key(&self.me, self.session.as_deref(), &hx(&m["data"]), kex)
            .expect("rust cannot decrypt the key");
        let plain = payload::decrypt(&dec.key_material, &hx(&m["payload"]))
            .expect("rust cannot decrypt the payload");
        assert_eq!(&*plain, text.as_bytes());
        assert_eq!(
            dec.remote_identity_key, self.py_ik,
            "wrong identity key of the python device"
        );
        if let Some(id) = dec.used_prekey_id {
            assert!(self.me.remove_prekey(id));
        }
        self.session = Some(dec.session);
        kex
    }

    /// Five messages each way, and one out-of-order round each way.
    fn conversation(&mut self) {
        for i in 0..5 {
            assert!(!self.receive(&format!("py to rust {i}")));
            assert!(
                !self.send(&format!("rust to py {i}")),
                "no key exchange after an answer"
            );
        }
        // Out of order, Python to Rust: Python makes three, Rust reads 3, 1, 2.
        let m: Vec<Value> = (1..=3)
            .map(|i| self.py.ok(json!({"cmd": "encrypt", "device_id": self.me.device_id(), "plaintext": hex::encode(format!("late {i}"))})))
            .collect();
        for i in [2, 0, 1] {
            self.read(&m[i], &format!("late {}", i + 1));
        }
        // Out of order, Rust to Python: Rust makes three, Python reads 3, 1, 2.
        let mut sent = Vec::new();
        for i in 1..=3 {
            let text = format!("early {i}");
            let (material, ct) = payload::encrypt(text.as_bytes()).unwrap();
            let enc = encrypt_key(&self.me, self.session.as_deref(), None, &material).unwrap();
            self.session = Some(enc.session.clone());
            sent.push((enc, ct, text));
        }
        for i in [2, 0, 1] {
            let (enc, ct, text) = &sent[i];
            self.deliver(&enc.data, enc.kex, Some(ct), text);
        }
        // And the conversation goes on.
        assert!(!self.receive("after the gap"));
        assert!(!self.send("after the gap, too"));
    }
}

fn start(initiator_is_rust: bool) -> Rust {
    let mut py = Peer::start();
    let init = py.ok(json!({"cmd": "init"}));
    let py_bundle = bundle_from_json(&init["bundle"]);
    assert_eq!(py_bundle.prekeys.len(), 100);
    let me = Identity::generate().unwrap();
    if !initiator_is_rust {
        py.ok(json!({"cmd": "set_remote", "device_id": me.device_id(), "bundle": bundle_to_json(&me.bundle().unwrap())}));
    }
    let py_ik = py_bundle.ik.clone();
    Rust {
        me,
        session: None,
        py_bundle: initiator_is_rust.then_some(py_bundle),
        py,
        py_ik,
    }
}

/// Case a: Python publishes a bundle. Rust builds a session from it and sends a payload.
#[test]
#[ignore = "needs the python venv, see tests/interop/README.md"]
fn rust_starts_the_session() {
    let mut t = start(true);
    assert!(
        t.send("hello from rust"),
        "the first message is a key exchange"
    );
    // Python has not answered yet: Rust keeps sending key exchanges.
    assert!(t.send("second message before any answer"));
    // Python answers. Rust reads it and leaves the key exchange state.
    assert!(!t.receive("hello from python"));
    assert!(!t.send("now a plain message"));
    t.conversation();
}

/// Case b: Rust publishes a bundle. Python sends a key exchange. Rust decrypts it.
#[test]
#[ignore = "needs the python venv, see tests/interop/README.md"]
fn python_starts_the_session() {
    let mut t = start(false);
    assert!(
        t.receive("hello from python"),
        "the first message is a key exchange"
    );
    // Python has not heard from Rust: the next message is a key exchange again.
    assert!(t.receive("python, once more"));
    // An empty OMEMO message from Python (it holds 32 zero bytes).
    let e =
        t.py.ok(json!({"cmd": "empty", "device_id": t.me.device_id()}));
    let kex = e["kex"].as_bool().unwrap();
    let dec = decrypt_key(&t.me, t.session.as_deref(), &hx(&e["data"]), kex).unwrap();
    assert_eq!(&*dec.key_material, &payload::EMPTY_KEY_MATERIAL[..]);
    t.session = Some(dec.session);
    // Rust answers. Python reads it, and now stops key exchanges.
    assert!(!t.send("hello from rust"));
    assert!(!t.receive("python, plain now"));
    // An empty OMEMO message from Rust.
    let enc = encrypt_key(
        &t.me,
        t.session.as_deref(),
        None,
        &payload::EMPTY_KEY_MATERIAL,
    )
    .unwrap();
    t.session = Some(enc.session.clone());
    t.deliver(&enc.data, enc.kex, None, "");
    t.conversation();
}

/// Python sends 60 messages and never hears from Rust. Its counter and ours must agree, so
/// the heartbeat (XEP-0384, counter 53 or higher) is due at message 53 and not before.
#[test]
#[ignore = "needs the python venv, see tests/interop/README.md"]
fn counters_from_python_show_the_heartbeat_at_53() {
    let mut t = start(false);
    let mut first_due = None;
    for n in 0..60u32 {
        let m = t.py.ok(
            json!({"cmd": "encrypt", "device_id": t.me.device_id(), "plaintext": hex::encode("x")}),
        );
        let kex = m["kex"].as_bool().unwrap();
        let dec = decrypt_key(&t.me, t.session.as_deref(), &hx(&m["data"]), kex).unwrap();
        if let Some(id) = dec.used_prekey_id {
            assert!(t.me.remove_prekey(id));
        }
        t.session = Some(dec.session);
        assert_eq!(dec.counter, n, "python and chord-omemo count the same");
        assert_eq!(dec.heartbeat_due, n >= 53);
        if dec.heartbeat_due && first_due.is_none() {
            first_due = Some(n);
        }
    }
    assert_eq!(first_due, Some(53));
}

/// Python refuses damaged data from Rust, and Rust refuses damaged data from Python.
#[test]
#[ignore = "needs the python venv, see tests/interop/README.md"]
fn damaged_data_is_refused_on_both_sides() {
    let mut t = start(true);
    let (material, ct) = payload::encrypt(b"x").unwrap();
    let enc = encrypt_key(&t.me, None, t.py_bundle.as_ref(), &material).unwrap();
    let mut bad = enc.data.clone();
    let mid = bad.len() / 2;
    bad[mid] ^= 1;
    let reply = t.py.raw(json!({"cmd": "decrypt", "device_id": t.me.device_id(), "data": hex::encode(&bad), "kex": true, "payload": hex::encode(&ct)}));
    assert!(
        reply.get("error").is_some(),
        "python accepted damaged data: {reply}"
    );
    // The good data still works after the failure.
    t.session = Some(enc.session.clone());
    t.deliver(&enc.data, true, Some(&ct), "x");

    let m = t.py.ok(
        json!({"cmd": "encrypt", "device_id": t.me.device_id(), "plaintext": hex::encode("y")}),
    );
    let mut bad = hx(&m["data"]);
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert!(decrypt_key(&t.me, t.session.as_deref(), &bad, false).is_err());
    t.read(&m, "y");
}
