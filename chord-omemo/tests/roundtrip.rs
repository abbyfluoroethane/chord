//! Rust-to-Rust tests of the wrapper. libomemo-c does the protocol. These tests check that
//! the wrapper passes bytes and state through correctly.

use chord_omemo::{Error, Identity, decrypt_key, encrypt_key, fingerprint, payload};

/// Alice sent the first message to Bob, and Bob read it.
struct Pair {
    alice: Identity,
    bob: Identity,
    alice_session: Vec<u8>,
    bob_session: Vec<u8>,
}

fn key_material() -> Vec<u8> {
    let (material, _) = payload::encrypt(b"hello").unwrap();
    material.to_vec()
}

fn connect() -> Pair {
    let alice = Identity::generate().unwrap();
    let mut bob = Identity::generate().unwrap();
    let material = key_material();
    let enc = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &material).unwrap();
    assert!(enc.kex);
    let dec = decrypt_key(&bob, None, &enc.data, true).unwrap();
    assert_eq!(&*dec.key_material, &material[..]);
    assert_eq!(dec.remote_identity_key, alice.identity_key());
    assert!(bob.remove_prekey(dec.used_prekey_id.expect("a key exchange uses a prekey")));
    Pair {
        alice,
        bob,
        alice_session: enc.session,
        bob_session: dec.session,
    }
}

#[test]
fn new_identity_has_the_xep_sizes() {
    let id = Identity::generate().unwrap();
    assert!((1..=0x7FFF_FFFF).contains(&id.device_id()));
    let b = id.bundle().unwrap();
    assert_eq!(b.ik.len(), 32);
    assert_eq!(b.spk.len(), 32);
    assert_eq!(b.spks.len(), 64);
    assert_eq!(b.prekeys.len(), 100);
    assert!(b.prekeys.iter().all(|(_, k)| k.len() == 32));
    let mut ids: Vec<u32> = b.prekeys.iter().map(|(i, _)| *i).collect();
    ids.dedup();
    assert_eq!(ids.len(), 100);
}

#[test]
fn identity_survives_serialization() {
    let id = Identity::generate().unwrap();
    let again = Identity::from_bytes(&id.to_bytes()).unwrap();
    assert_eq!(id.bundle().unwrap(), again.bundle().unwrap());
    assert_eq!(id.device_id(), again.device_id());
    assert!(Identity::from_bytes(&id.to_bytes()[..100]).is_err());
    assert!(Identity::from_bytes(b"").is_err());
}

#[test]
fn fingerprint_has_eight_groups_of_eight() {
    let id = Identity::generate().unwrap();
    let fp = fingerprint(id.identity_key()).unwrap();
    let groups: Vec<&str> = fp.split(' ').collect();
    assert_eq!(groups.len(), 8);
    assert!(groups.iter().all(|g| {
        g.len() == 8
            && g.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    }));
    assert_eq!(fp, id.fingerprint().unwrap());
    assert!(fingerprint(&[1, 2, 3]).is_err());
}

#[test]
fn round_trip_both_ways() {
    let Pair {
        alice,
        bob,
        mut alice_session,
        mut bob_session,
    } = connect();
    for i in 0..5 {
        let m = format!("bob {i}").into_bytes();
        let enc = encrypt_key(&bob, Some(&bob_session), None, &m).unwrap();
        assert!(!enc.kex);
        bob_session = enc.session;
        let dec = decrypt_key(&alice, Some(&alice_session), &enc.data, false).unwrap();
        assert_eq!(&*dec.key_material, &m[..]);
        alice_session = dec.session;
        assert_eq!(dec.used_prekey_id, None);
        assert_eq!(dec.remote_identity_key, bob.identity_key());

        let m = format!("alice {i}").into_bytes();
        let enc = encrypt_key(&alice, Some(&alice_session), None, &m).unwrap();
        assert!(!enc.kex, "after the first answer there is no key exchange");
        alice_session = enc.session;
        let dec = decrypt_key(&bob, Some(&bob_session), &enc.data, false).unwrap();
        assert_eq!(&*dec.key_material, &m[..]);
        bob_session = dec.session;
    }
}

#[test]
fn key_exchange_repeats_until_answered() {
    let Pair {
        alice,
        bob,
        alice_session,
        bob_session,
    } = connect();
    // Alice has not heard from Bob. The next message is still a key exchange.
    let enc = encrypt_key(&alice, Some(&alice_session), None, b"again").unwrap();
    assert!(enc.kex);
    // Bob has the session already and reads the key exchange with the known ephemeral key.
    let dec = decrypt_key(&bob, Some(&bob_session), &enc.data, true).unwrap();
    assert_eq!(&*dec.key_material, b"again");
}

#[test]
fn out_of_order_messages_work() {
    let Pair {
        alice,
        bob,
        mut alice_session,
        mut bob_session,
    } = connect();
    // Bob answers once, so that Alice leaves the key exchange state.
    let enc = encrypt_key(&bob, Some(&bob_session), None, b"hi").unwrap();
    alice_session = decrypt_key(&alice, Some(&alice_session), &enc.data, false)
        .unwrap()
        .session;

    let mut sent = Vec::new();
    for i in 0..3u8 {
        let enc = encrypt_key(&alice, Some(&alice_session), None, &[i; 32]).unwrap();
        alice_session = enc.session;
        sent.push(enc.data);
    }
    for i in [2usize, 0, 1] {
        let dec = decrypt_key(&bob, Some(&bob_session), &sent[i], false).unwrap();
        assert_eq!(&*dec.key_material, &[i as u8; 32][..]);
        bob_session = dec.session;
    }
}

#[test]
fn tampered_data_fails() {
    let alice = Identity::generate().unwrap();
    let bob = Identity::generate().unwrap();
    let enc = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &key_material()).unwrap();
    // Flip each byte in turn. None of the results may decrypt.
    for i in 0..enc.data.len() {
        let mut bad = enc.data.clone();
        bad[i] ^= 0x01;
        assert!(
            decrypt_key(&bob, None, &bad, true).is_err(),
            "byte {i} was accepted"
        );
    }
    assert!(decrypt_key(&bob, None, &enc.data[..enc.data.len() - 1], true).is_err());
    assert!(decrypt_key(&bob, None, b"", true).is_err());
    // The right bytes still work.
    assert!(decrypt_key(&bob, None, &enc.data, true).is_ok());
}

#[test]
fn tampered_message_after_session_fails() {
    let Pair {
        alice,
        bob,
        mut alice_session,
        bob_session,
    } = connect();
    let enc = encrypt_key(&bob, Some(&bob_session), None, b"x").unwrap();
    alice_session = decrypt_key(&alice, Some(&alice_session), &enc.data, false)
        .unwrap()
        .session;
    let enc = encrypt_key(&alice, Some(&alice_session), None, &key_material()).unwrap();
    let mut bad = enc.data.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    let err = decrypt_key(&bob, Some(&bob_session), &bad, false).unwrap_err();
    assert!(
        matches!(err, Error::BadMac | Error::Malformed(_)),
        "{err:?}"
    );
}

#[test]
fn used_prekey_fails() {
    let alice = Identity::generate().unwrap();
    let mut bob = Identity::generate().unwrap();
    let mut one = bob.bundle().unwrap();
    one.prekeys.truncate(1);
    let used_id = one.prekeys[0].0;

    let first = encrypt_key(&alice, None, Some(&one), b"first").unwrap();
    let dec = decrypt_key(&bob, None, &first.data, true).unwrap();
    assert_eq!(dec.used_prekey_id, Some(used_id));
    assert!(bob.remove_prekey(used_id));

    // Another device builds a session with the same old bundle.
    let mallory = Identity::generate().unwrap();
    let second = encrypt_key(&mallory, None, Some(&one), b"second").unwrap();
    assert_eq!(
        decrypt_key(&bob, None, &second.data, true).unwrap_err(),
        Error::UnknownPreKey(used_id)
    );
}

#[test]
fn failed_decrypt_changes_no_state() {
    let Pair {
        alice,
        bob,
        mut alice_session,
        bob_session,
    } = connect();
    let enc = encrypt_key(&bob, Some(&bob_session), None, b"x").unwrap();
    let bob_session = enc.session;
    alice_session = decrypt_key(&alice, Some(&alice_session), &enc.data, false)
        .unwrap()
        .session;

    let good = encrypt_key(&alice, Some(&alice_session), None, &[7; 32]).unwrap();
    let mut bad = good.data.clone();
    let mid = bad.len() / 2;
    bad[mid] ^= 0x40;

    let identity_before = bob.to_bytes();
    let session_before = bob_session.clone();
    assert!(decrypt_key(&bob, Some(&bob_session), &bad, false).is_err());
    assert!(decrypt_key(&bob, Some(&bob_session), &bad[..10], false).is_err());
    assert_eq!(*identity_before, *bob.to_bytes());
    assert_eq!(session_before, bob_session);

    // The untouched session still reads the good message, once.
    let dec = decrypt_key(&bob, Some(&bob_session), &good.data, false).unwrap();
    assert_eq!(&*dec.key_material, &[7; 32][..]);
    assert_eq!(
        decrypt_key(&bob, Some(&dec.session), &good.data, false).unwrap_err(),
        Error::Duplicate
    );
}

#[test]
fn no_session_and_no_bundle_is_an_error() {
    let alice = Identity::generate().unwrap();
    assert_eq!(
        encrypt_key(&alice, None, None, b"x").unwrap_err(),
        Error::NoSession
    );
    assert_eq!(
        decrypt_key(&alice, None, b"x", false).unwrap_err(),
        Error::NoSession
    );
}

#[test]
fn bad_bundles_are_refused() {
    let alice = Identity::generate().unwrap();
    let bob = Identity::generate().unwrap();
    let mut bundle = bob.bundle().unwrap();
    bundle.spks[10] ^= 1;
    assert_eq!(
        encrypt_key(&alice, None, Some(&bundle), b"x").unwrap_err(),
        Error::BadSignature
    );
    // A signed prekey of another device, with the signature of the right one.
    let other = Identity::generate().unwrap().bundle().unwrap();
    let mut mixed = bob.bundle().unwrap();
    mixed.spk = other.spk;
    assert!(encrypt_key(&alice, None, Some(&mixed), b"x").is_err());
    // A bundle without prekeys is refused: a key exchange must use a prekey.
    let mut none = bob.bundle().unwrap();
    none.prekeys.clear();
    assert!(encrypt_key(&alice, None, Some(&none), b"x").is_err());
}

#[test]
fn prekey_refill_and_signed_prekey_rotation() {
    let mut bob = Identity::generate().unwrap();
    let old = bob.bundle().unwrap();
    // Keep prekey 1 of the old bundle, remove 79 others.
    for (id, _) in old.prekeys.iter().skip(1).take(79) {
        assert!(bob.remove_prekey(*id));
    }
    assert_eq!(bob.prekey_count(), 21);
    let added = bob.refill_prekeys(100).unwrap();
    assert_eq!(added.len(), 79);
    assert_eq!(bob.prekey_count(), 100);
    assert!(bob.refill_prekeys(100).unwrap().is_empty());
    assert!(
        added
            .iter()
            .all(|(id, _)| !old.prekeys.iter().any(|(o, _)| o == id))
    );

    let (id, spk, sig) = bob.rotate_signed_prekey().unwrap();
    assert_ne!(id, old.spk_id);
    let new = bob.bundle().unwrap();
    assert_eq!((new.spk_id, &new.spk, &new.spks), (id, &spk, &sig));
    assert_ne!(new.spk, old.spk);

    // A message built from the old bundle (old signed prekey, kept prekey) still decrypts.
    let mut stale = old.clone();
    stale.prekeys.truncate(1);
    let alice = Identity::generate().unwrap();
    let enc = encrypt_key(&alice, None, Some(&stale), b"late").unwrap();
    assert_eq!(
        &*decrypt_key(&bob, None, &enc.data, true)
            .unwrap()
            .key_material,
        b"late"
    );

    assert_eq!(bob.signed_prekeys().unwrap().len(), 2);
    assert!(!bob.discard_signed_prekey(id));
    assert!(bob.discard_signed_prekey(old.spk_id));
    assert_eq!(bob.signed_prekeys().unwrap().len(), 1);
    // The record with the rotated key reads back.
    assert_eq!(
        Identity::from_bytes(&bob.to_bytes())
            .unwrap()
            .bundle()
            .unwrap(),
        bob.bundle().unwrap()
    );
}

#[test]
fn payload_round_trip_and_tamper() {
    let (material, ct) = payload::encrypt(b"the envelope").unwrap();
    assert_eq!(material.len(), payload::KEY_MATERIAL_LEN);
    assert_eq!(&*payload::decrypt(&material, &ct).unwrap(), b"the envelope");
    let mut bad = ct.clone();
    bad[0] ^= 1;
    assert_eq!(
        payload::decrypt(&material, &bad).unwrap_err(),
        Error::BadMac
    );
    let mut bad_key = material.to_vec();
    bad_key[40] ^= 1;
    assert_eq!(payload::decrypt(&bad_key, &ct).unwrap_err(), Error::BadMac);
    assert!(payload::decrypt(&material[..40], &ct).is_err());
}

#[test]
fn heartbeat_is_due_from_counter_53_and_the_counter_is_read_from_the_message() {
    use chord_omemo::HEARTBEAT_COUNTER;
    assert_eq!(HEARTBEAT_COUNTER, 53);
    let mut p = connect();
    // `connect` sent message 0. Alice sends 59 more, and Bob never answers.
    let mut ratchet = None;
    let mut first_due = None;
    for n in 1..60u32 {
        let enc = encrypt_key(&p.alice, Some(&p.alice_session), None, &key_material()).unwrap();
        p.alice_session = enc.session;
        let dec = decrypt_key(&p.bob, Some(&p.bob_session), &enc.data, enc.kex).unwrap();
        p.bob_session = dec.session;
        assert_eq!(dec.counter, n);
        // The sending chain does not change while Bob is silent.
        assert_eq!(
            *ratchet.get_or_insert(dec.ratchet_key.clone()),
            dec.ratchet_key
        );
        assert_eq!(dec.heartbeat_due, n >= 53, "message {n}");
        if dec.heartbeat_due && first_due.is_none() {
            first_due = Some(n);
        }
    }
    assert_eq!(first_due, Some(53));
}

#[test]
fn a_reply_starts_a_new_chain_with_a_new_ratchet_key_and_counter() {
    let mut p = connect();
    let enc = encrypt_key(&p.alice, Some(&p.alice_session), None, &key_material()).unwrap();
    p.alice_session = enc.session;
    let first = decrypt_key(&p.bob, Some(&p.bob_session), &enc.data, enc.kex).unwrap();
    p.bob_session = first.session;
    // Bob answers, and Alice reads it. Alice's next message uses a new ratchet key.
    let reply = encrypt_key(&p.bob, Some(&p.bob_session), None, &key_material()).unwrap();
    p.bob_session = reply.session;
    let got = decrypt_key(&p.alice, Some(&p.alice_session), &reply.data, reply.kex).unwrap();
    assert_eq!(got.counter, 0);
    assert!(!got.heartbeat_due);
    p.alice_session = got.session;
    let enc = encrypt_key(&p.alice, Some(&p.alice_session), None, &key_material()).unwrap();
    let next = decrypt_key(&p.bob, Some(&p.bob_session), &enc.data, enc.kex).unwrap();
    assert_eq!(next.counter, 0);
    assert_ne!(next.ratchet_key, first.ratchet_key);
}
