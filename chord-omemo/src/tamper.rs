//! Tests of the API with damaged, forged, and replayed input. They use the internal message
//! types to change one field at a time and keep the other fields as they were.

use crate::ratchet::olm::{Message, PreKeyMessage, Session};
use crate::ratchet::{Curve25519PublicKey, Curve25519SecretKey};
use crate::session::MAX_KEY_MATERIAL;
use crate::{Error, Identity, decrypt_key, encrypt_key};

struct Pair {
    alice: Identity,
    bob: Identity,
}

fn pair() -> Pair {
    Pair {
        alice: Identity::generate().unwrap(),
        bob: Identity::generate().unwrap(),
    }
}

/// Make a message with other header fields, and keep the MAC that the sender made.
fn with_header(m: &Message, n: u32, pn: u32, key: [u8; 32], ciphertext: Vec<u8>) -> Message {
    let mut t = Message::new(Curve25519PublicKey::from_bytes(key), n, pn, ciphertext);
    t.set_mac(m.mac);
    t
}

/// Every field of a message, changed one at a time.
fn tampered_messages(m: &Message) -> Vec<(&'static str, Message)> {
    let key = m.ratchet_key.to_bytes();
    let mut other_key = key;
    other_key[0] ^= 1;
    let mut ciphertext = m.ciphertext.clone();
    ciphertext[3] ^= 1;
    let mut with_mac = m.clone();
    with_mac.mac[0] ^= 1;
    let (n, pn, ct) = (m.chain_index, m.previous_chain_length, m.ciphertext.clone());
    vec![
        ("n", with_header(m, n + 1, pn, key, ct.clone())),
        ("pn", with_header(m, n, pn + 1, key, ct.clone())),
        ("dh_pub", with_header(m, n, pn, other_key, ct)),
        ("ciphertext", with_header(m, n, pn, key, ciphertext)),
        ("mac", with_mac),
    ]
}

#[test]
fn a_change_in_any_field_of_a_message_is_refused_and_changes_no_state() {
    let Pair { alice, bob } = pair();
    let first = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &[1; 48]).unwrap();
    let bob_session = decrypt_key(&bob, None, &first.data, true).unwrap().session;
    let bob_reply = encrypt_key(&bob, Some(&bob_session), None, &[2; 48]).unwrap();
    let alice_session = decrypt_key(&alice, Some(&first.session), &bob_reply.data, false)
        .unwrap()
        .session;
    let second = encrypt_key(&alice, Some(&alice_session), None, &[3; 48]).unwrap();
    let message = Message::from_bytes(&second.data).unwrap();

    for (field, bad) in tampered_messages(&message) {
        let err =
            decrypt_key(&bob, Some(&bob_reply.session), &bad.to_bytes(), false).expect_err(field);
        assert!(
            matches!(err, Error::BadMac | Error::Duplicate),
            "{field}: {err:?}"
        );
    }
    // The state of Bob is as it was: the good message still reads.
    let dec = decrypt_key(&bob, Some(&bob_reply.session), &second.data, false).unwrap();
    assert_eq!(&*dec.key_material, &[3; 48][..]);
}

#[test]
fn a_change_in_any_field_of_a_key_exchange_is_refused() {
    let Pair { alice, bob } = pair();
    let first = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &[1; 48]).unwrap();
    let kex = PreKeyMessage::from_bytes(&first.data).unwrap();

    let mut cases: Vec<(String, PreKeyMessage)> = Vec::new();
    let mut c = kex.clone();
    c.pre_key_id += 1;
    cases.push(("pk_id".into(), c));
    let mut c = kex.clone();
    c.signed_pre_key_id += 1;
    cases.push(("spk_id".into(), c));
    let mut c = kex.clone();
    c.identity_key = Identity::generate()
        .unwrap()
        .identity_key()
        .try_into()
        .unwrap();
    cases.push(("ik".into(), c));
    let mut c = kex.clone();
    c.base_key = Curve25519PublicKey::from(&Curve25519SecretKey::new().unwrap());
    cases.push(("ek".into(), c));
    for (field, m) in tampered_messages(&kex.message) {
        let mut c = kex.clone();
        c.message = m;
        cases.push((format!("message.{field}"), c));
    }
    for (field, bad) in cases {
        assert!(
            decrypt_key(&bob, None, &bad.to_bytes(), true).is_err(),
            "{field}"
        );
    }
    assert!(decrypt_key(&bob, None, &first.data, true).is_ok());
}

#[test]
fn a_key_exchange_with_an_unknown_id_names_it() {
    let Pair { alice, bob } = pair();
    let first = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &[1; 48]).unwrap();
    let mut kex = PreKeyMessage::from_bytes(&first.data).unwrap();
    let known = kex.pre_key_id;
    kex.pre_key_id = 0xFF_FFFF;
    assert_eq!(
        decrypt_key(&bob, None, &kex.to_bytes(), true).unwrap_err(),
        Error::UnknownPreKey(0xFF_FFFF)
    );
    kex.pre_key_id = known;
    kex.signed_pre_key_id = 4242;
    assert!(matches!(
        decrypt_key(&bob, None, &kex.to_bytes(), true).unwrap_err(),
        Error::Malformed(_)
    ));
}

#[test]
fn a_replayed_key_exchange_is_a_duplicate() {
    let Pair { alice, mut bob } = pair();
    let first = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &[1; 48]).unwrap();
    let dec = decrypt_key(&bob, None, &first.data, true).unwrap();
    bob.remove_prekey(dec.used_prekey_id.unwrap());
    // The same key exchange again, with the stored session: no second session.
    assert_eq!(
        decrypt_key(&bob, Some(&dec.session), &first.data, true).unwrap_err(),
        Error::Duplicate
    );
    // And without the stored session the prekey is gone.
    assert!(matches!(
        decrypt_key(&bob, None, &first.data, true).unwrap_err(),
        Error::UnknownPreKey(_)
    ));
}

#[test]
fn a_bundle_with_a_wrong_signature_is_refused() {
    let Pair { alice, bob } = pair();
    let mallory = Identity::generate().unwrap();
    let mut bundle = bob.bundle().unwrap();
    // The signature of Mallory for another key, under the identity key of Bob.
    bundle.spks = mallory.bundle().unwrap().spks;
    assert_eq!(
        encrypt_key(&alice, None, Some(&bundle), &[1; 48]).unwrap_err(),
        Error::BadSignature
    );
    // Mallory puts the signed prekey of Bob in her own bundle.
    let mut forged = mallory.bundle().unwrap();
    forged.spk = bob.bundle().unwrap().spk;
    assert_eq!(
        encrypt_key(&alice, None, Some(&forged), &[1; 48]).unwrap_err(),
        Error::BadSignature
    );
}

#[test]
fn too_long_key_material_is_refused() {
    let Pair { alice, bob } = pair();
    let bundle = bob.bundle().unwrap();
    assert!(encrypt_key(&alice, None, Some(&bundle), &vec![0; MAX_KEY_MATERIAL]).is_ok());
    assert_eq!(
        encrypt_key(&alice, None, Some(&bundle), &vec![0; MAX_KEY_MATERIAL + 1]).unwrap_err(),
        Error::Malformed("key material")
    );
}

#[test]
fn a_session_with_many_skipped_keys_survives_the_round_trip() {
    let Pair { alice, bob } = pair();
    let first = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &[1; 32]).unwrap();
    let mut alice_session = first.session.clone();
    let mut last = first.data;
    for _ in 0..200 {
        let enc = encrypt_key(&alice, Some(&alice_session), None, &[1; 32]).unwrap();
        alice_session = enc.session;
        last = enc.data;
    }
    // Bob reads only the last message: 200 keys are skipped. The session is stored and read
    // again, and a late message still opens.
    let late = encrypt_key(&alice, Some(&first.session), None, &[7; 32]).unwrap();
    let mut bob_session = decrypt_key(&bob, None, &last, true).unwrap().session;
    assert!(bob_session.len() > 10_000);
    bob_session = decrypt_key(&bob, Some(&bob_session), &late.data, true)
        .unwrap()
        .session;
    let again = Session::from_bytes(&bob_session).unwrap();
    assert_eq!(*again.to_bytes(), bob_session);
}
