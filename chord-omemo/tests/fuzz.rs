//! Feed random and damaged bytes to every parser of the public API. None of the calls may
//! panic. They may return an error, and most do.

use chord_omemo::{Bundle, Identity, decrypt_key, encrypt_key, fingerprint, payload};

/// A small random generator (xorshift64*). The tests need the same bytes in every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let len = self.below(max + 1);
        (0..len).map(|_| self.next() as u8).collect()
    }

    /// A copy of `data` with a few changes: flips, a cut, or extra bytes.
    fn damage(&mut self, data: &[u8]) -> Vec<u8> {
        let mut out = data.to_vec();
        match self.below(4) {
            0 if !out.is_empty() => {
                for _ in 0..=self.below(3) {
                    let i = self.below(out.len());
                    out[i] ^= 1 << self.below(8);
                }
            }
            1 if !out.is_empty() => out.truncate(self.below(out.len())),
            2 => out.extend(self.bytes(8)),
            _ if !out.is_empty() => {
                let i = self.below(out.len());
                out[i] = self.next() as u8;
            }
            _ => {}
        }
        out
    }
}

const ROUNDS: usize = 3000;

/// Two devices, a session, and valid messages as a start for the damage.
struct Fixture {
    alice: Identity,
    bob: Identity,
    kex: Vec<u8>,
    message: Vec<u8>,
    alice_session: Vec<u8>,
    bob_session: Vec<u8>,
}

fn fixture() -> Fixture {
    let alice = Identity::generate().unwrap();
    let bob = Identity::generate().unwrap();
    let first = encrypt_key(&alice, None, Some(&bob.bundle().unwrap()), &[1; 48]).unwrap();
    let bob_session = decrypt_key(&bob, None, &first.data, true).unwrap().session;
    let reply = encrypt_key(&bob, Some(&bob_session), None, &[2; 48]).unwrap();
    let alice_session = decrypt_key(&alice, Some(&first.session), &reply.data, false)
        .unwrap()
        .session;
    let message = encrypt_key(&alice, Some(&alice_session), None, &[3; 48]).unwrap();
    Fixture {
        alice,
        bob,
        kex: first.data,
        message: message.data,
        alice_session,
        bob_session: reply.session,
    }
}

#[test]
fn random_bytes_do_not_panic_any_parser() {
    let f = fixture();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..ROUNDS {
        let data = rng.bytes(300);
        let record = rng.bytes(300);
        for kex in [true, false] {
            let _ = decrypt_key(&f.bob, None, &data, kex);
            let _ = decrypt_key(&f.bob, Some(&record), &data, kex);
        }
        let _ = Identity::from_bytes(&data);
        let _ = fingerprint(&data);
        let _ = payload::decrypt(&data, &record);
        let bundle = Bundle {
            ik: data.clone(),
            spk_id: rng.next() as u32,
            spk: rng.bytes(40),
            spks: rng.bytes(70),
            prekeys: vec![(rng.next() as u32, rng.bytes(40))],
        };
        let _ = encrypt_key(&f.alice, None, Some(&bundle), &data);
        let _ = encrypt_key(&f.alice, Some(&record), None, &data);
    }
}

#[test]
fn damaged_messages_do_not_panic_and_do_not_open() {
    let f = fixture();
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for _ in 0..ROUNDS {
        let bad = rng.damage(&f.message);
        // A damaged copy may in rare cases decode to the same message (an extra field that
        // the parser skips). The MAC covers everything else, so it never opens with other
        // content.
        if let Ok(d) = decrypt_key(&f.bob, Some(&f.bob_session), &bad, false) {
            assert_eq!(&*d.key_material, &[3; 48][..], "{bad:?}");
        }
        let bad = rng.damage(&f.kex);
        if let Ok(d) = decrypt_key(&f.bob, None, &bad, true) {
            assert_eq!(&*d.key_material, &[1; 48][..], "{bad:?}");
        }
    }
}

#[test]
fn damaged_stored_state_does_not_panic() {
    let f = fixture();
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    let identity = f.bob.to_bytes();
    for _ in 0..ROUNDS {
        let bad = rng.damage(&f.bob_session);
        let _ = decrypt_key(&f.bob, Some(&bad), &f.message, false);
        let _ = encrypt_key(&f.bob, Some(&bad), None, &[0; 32]);
        let bad = rng.damage(&f.alice_session);
        let _ = encrypt_key(&f.alice, Some(&bad), None, &[0; 32]);
        let bad = rng.damage(&identity);
        let _ = Identity::from_bytes(&bad);
    }
}

#[test]
fn damaged_payloads_do_not_panic() {
    let (material, ciphertext) = payload::encrypt(b"a secret text").unwrap();
    let mut rng = Rng(0xFEED_FACE_CAFE_BEEF);
    for _ in 0..ROUNDS {
        let bad_key = rng.damage(&material);
        let bad_text = rng.damage(&ciphertext);
        let _ = payload::decrypt(&bad_key, &ciphertext);
        let _ = payload::decrypt(&material, &bad_text);
        let _ = payload::decrypt(&bad_key, &bad_text);
    }
}
