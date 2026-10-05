//! Known-answer tests against python-twomemo.
//!
//! `tests/interop/kat.json` was made by `tests/interop/kat.py`. It holds a whole conversation
//! of two devices, as python-twomemo (with python-x3dh and python-doubleratchet) made it, with a
//! fixed source of random bytes. This test runs the same conversation with the same random
//! bytes and compares every message byte for byte. It does not need Python.

use std::collections::VecDeque;

use serde_json::Value;

use crate::ratchet::types::TEST_TAPE;
use crate::{Bundle, Identity, decrypt_key, encrypt_key};

fn hx(v: &Value) -> Vec<u8> {
    hex::decode(
        v.as_str()
            .unwrap_or_else(|| panic!("not a hex string: {v}")),
    )
    .unwrap()
}

fn key32(v: &Value) -> [u8; 32] {
    hx(v).try_into().unwrap()
}

fn set_tape(values: &Value) {
    let tape: VecDeque<[u8; 32]> = values.as_array().unwrap().iter().map(key32).collect();
    TEST_TAPE.with(|t| *t.borrow_mut() = tape);
}

fn tape_is_used() -> bool {
    TEST_TAPE.with(|t| t.borrow().is_empty())
}

struct Kat {
    json: Value,
}

fn kat() -> Kat {
    Kat {
        json: serde_json::from_str(include_str!("../tests/interop/kat.json")).unwrap(),
    }
}

impl Kat {
    fn a(&self) -> Identity {
        let a = &self.json["a"];
        let id = Identity::for_tests(
            a["device_id"].as_u64().unwrap() as u32,
            key32(&a["identity_seed"]),
            &[(1, [9u8; 32])],
            &[],
        );
        // The public key of the seed is the one that python made.
        assert_eq!(id.identity_key(), hx(&a["identity_key"]));
        id
    }

    fn b(&self) -> Identity {
        let b = &self.json["b"];
        let id = Identity::for_tests(
            b["device_id"].as_u64().unwrap() as u32,
            key32(&b["identity_seed"]),
            &[(
                b["signed_prekey_id"].as_u64().unwrap() as u32,
                key32(&b["signed_prekey_secret"]),
            )],
            &[(
                b["prekey_id"].as_u64().unwrap() as u32,
                key32(&b["prekey_secret"]),
            )],
        );
        assert_eq!(id.identity_key(), hx(&b["identity_key"]));
        let bundle = id.bundle().unwrap();
        // The public keys of the secrets are the ones that python made.
        assert_eq!(bundle.spk, hx(&b["signed_prekey"]));
        assert_eq!(bundle.prekeys[0].1, hx(&b["prekey"]));
        id
    }

    /// The bundle that python made for B, with the signature that python made.
    fn bundle_b(&self) -> Bundle {
        let b = &self.json["b"];
        Bundle {
            ik: hx(&b["identity_key"]),
            spk_id: b["signed_prekey_id"].as_u64().unwrap() as u32,
            spk: hx(&b["signed_prekey"]),
            spks: hx(&b["signed_prekey_signature"]),
            prekeys: vec![(b["prekey_id"].as_u64().unwrap() as u32, hx(&b["prekey"]))],
        }
    }
}

#[test]
fn the_signature_of_python_verifies() {
    // The signature of the signed prekey comes from python-xeddsa. It is a plain Ed25519
    // signature over the 32 bytes of the public signed prekey.
    let b = kat().bundle_b();
    crate::identity::verify_signed_prekey(&b.ik, &b.spk, &b.spks).unwrap();
}

#[test]
fn a_whole_conversation_matches_python_twomemo_byte_for_byte() {
    let kat = kat();
    let (a, mut b) = (kat.a(), kat.b());
    let bundle_b = kat.bundle_b();
    let (mut a_session, mut b_session): (Option<Vec<u8>>, Option<Vec<u8>>) = (None, None);
    let mut counted = 0;

    for op in kat.json["operations"].as_array().unwrap() {
        let label = op["label"].as_str().unwrap();
        let actor = op["actor"].as_str().unwrap();
        let kex = op["kex"].as_bool().unwrap();
        let material = hx(&op["key_material"]);
        let data = hx(&op["data"]);
        set_tape(&op["tape"]);

        match (op["op"].as_str().unwrap(), actor) {
            ("send", "A") => {
                let bundle = a_session.is_none().then_some(&bundle_b);
                let enc = encrypt_key(&a, a_session.as_deref(), bundle, &material).unwrap();
                assert_eq!(enc.kex, kex, "{label}: kex flag");
                assert_eq!(hex::encode(&enc.data), hex::encode(&data), "{label}: bytes");
                a_session = Some(enc.session);
            }
            ("send", "B") => {
                let enc = encrypt_key(&b, b_session.as_deref(), None, &material).unwrap();
                assert_eq!(enc.kex, kex, "{label}: kex flag");
                assert_eq!(hex::encode(&enc.data), hex::encode(&data), "{label}: bytes");
                b_session = Some(enc.session);
            }
            ("recv", "A") => {
                let dec = decrypt_key(&a, a_session.as_deref(), &data, kex).unwrap();
                assert_eq!(&*dec.key_material, &material, "{label}: key material");
                a_session = Some(dec.session);
            }
            ("recv", "B") => {
                let dec = decrypt_key(&b, b_session.as_deref(), &data, kex).unwrap();
                assert_eq!(&*dec.key_material, &material, "{label}: key material");
                b_session = Some(dec.session);
                if kex {
                    let used = dec.used_prekey_id.expect("a key exchange uses a prekey");
                    assert_eq!(used, kat.json["b"]["prekey_id"].as_u64().unwrap() as u32);
                    assert!(b.remove_prekey(used));
                }
            }
            other => panic!("unknown operation {other:?}"),
        }
        assert!(
            tape_is_used(),
            "{label}: the random bytes were not all used"
        );
        counted += 1;
    }
    assert_eq!(counted, 16);
}

#[test]
fn decode_and_encode_of_python_messages_gives_the_same_bytes() {
    use crate::ratchet::olm::{Message, PreKeyMessage};
    for op in kat().json["operations"].as_array().unwrap() {
        let data = hx(&op["data"]);
        let again = if op["kex"].as_bool().unwrap() {
            PreKeyMessage::from_bytes(&data).unwrap().to_bytes()
        } else {
            Message::from_bytes(&data).unwrap().to_bytes()
        };
        assert_eq!(hex::encode(again), hex::encode(data));
    }
}
