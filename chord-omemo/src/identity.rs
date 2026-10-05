//! The keys of this device: identity key, signed prekeys, one-time prekeys.
//!
//! The identity key is an Ed25519 key pair (XEP-0384 0.9, section 4.2, "XEdDSA": a library
//! may use an Ed25519 key pair as its internal identity key). The public key goes on the
//! wire in its Ed25519 form. For Diffie-Hellman and for the fingerprint, the key is
//! converted to its Curve25519 form with the dalek functions `SigningKey::to_scalar_bytes`
//! and `VerifyingKey::to_montgomery`. There is no curve arithmetic of our own here.
//!
//! An [`Identity`] is plain data. It turns into bytes with [`Identity::to_bytes`] and back
//! with [`Identity::from_bytes`]. The caller stores the bytes. The bytes hold private keys,
//! so the caller must protect them (see the secrets section of `docs/omemo-plan.md`).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::Error;
use crate::ratchet::{Curve25519PublicKey, Curve25519SecretKey};

/// How many one-time prekeys a new identity makes. The XEP says "around 100".
pub const PREKEY_COUNT: usize = 100;
/// The XEP says a bundle must always hold at least 25 prekeys.
pub const PREKEY_MIN: usize = 25;

/// Prekey ids wrap at this number. The ids stay small, as in the bundles of other clients.
const PREKEY_ID_LIMIT: u32 = 0xFF_FFFE;
/// The most prekeys and signed prekeys that we read from stored bytes.
const MAX_STORED_PREKEYS: usize = 10_000;
const MAX_STORED_SIGNED_PREKEYS: usize = 1_000;
/// The biggest stored identity that we read, in bytes.
const MAX_STORED_LEN: usize = 8 * 1024 * 1024;

/// The first byte of the stored bytes. A new format gets a new number.
const FORMAT_VERSION: u8 = 1;

/// The public part of a device, in the byte forms of the OMEMO 2 `<bundle>` element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    /// `<ik>`: the Ed25519 public identity key, 32 bytes.
    pub ik: Vec<u8>,
    /// `<spk id>`: the id of the signed prekey.
    pub spk_id: u32,
    /// `<spk>`: the Curve25519 public signed prekey, 32 bytes.
    pub spk: Vec<u8>,
    /// `<spks>`: the Ed25519 signature of `spk` by `ik`, 64 bytes.
    pub spks: Vec<u8>,
    /// `<pk id>` elements: one-time prekeys, Curve25519 public keys of 32 bytes.
    pub prekeys: Vec<(u32, Vec<u8>)>,
}

struct SignedPreKey {
    secret: Curve25519SecretKey,
    /// The Ed25519 signature of the public key of `secret` by the identity key.
    signature: [u8; 64],
    /// Milliseconds since the Unix epoch.
    created_ms: u64,
}

/// The keys of this device.
pub struct Identity {
    device_id: u32,
    ik: SigningKey,
    /// The identity key as a Curve25519 key, for Diffie-Hellman.
    ik_dh: Curve25519SecretKey,
    /// The Ed25519 public identity key.
    ik_public: [u8; 32],
    current_spk: u32,
    next_spk_id: u32,
    next_prekey_id: u32,
    signed_prekeys: BTreeMap<u32, SignedPreKey>,
    prekeys: BTreeMap<u32, Curve25519SecretKey>,
}

impl core::fmt::Debug for Identity {
    // Never print the keys.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Identity")
            .field("device_id", &self.device_id)
            .field("prekeys", &self.prekeys.len())
            .finish_non_exhaustive()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The Curve25519 form of an Ed25519 public key: the birational map of RFC 7748, section 4.1,
/// as XEP-0384 0.9, section 4.2 says. Fails if the bytes are not a point on the curve.
pub(crate) fn curve25519_of_ed25519(ik: &[u8]) -> Result<Curve25519PublicKey, Error> {
    let bytes: [u8; 32] = ik
        .try_into()
        .map_err(|_| Error::Malformed("identity key"))?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| Error::Malformed("identity key"))?;
    Ok(Curve25519PublicKey::from_bytes(
        key.to_montgomery().to_bytes(),
    ))
}

/// The fingerprint of an identity key, in the form that XEP-0384 says to show to the user:
/// the Curve25519 form of the key, as lowercase hex in 8 groups of 8 characters.
///
/// `ik` is the Ed25519 key of the `<ik>` element (32 bytes).
pub fn fingerprint(ik: &[u8]) -> Result<String, Error> {
    let mont = curve25519_of_ed25519(ik)?;
    let hex = mont.to_hex();
    let groups: Vec<&str> = (0..8).map(|i| &hex[i * 8..i * 8 + 8]).collect();
    Ok(groups.join(" "))
}

fn random_u32() -> Result<u32, Error> {
    let mut raw = [0u8; 4];
    getrandom::fill(&mut raw).map_err(|_| Error::Rng)?;
    Ok(u32::from_le_bytes(raw))
}

fn new_secret() -> Result<Curve25519SecretKey, Error> {
    Curve25519SecretKey::new().map_err(|_| Error::Rng)
}

fn ik_dh(ik: &SigningKey) -> Curve25519SecretKey {
    let mut scalar = Zeroizing::new(ik.to_scalar_bytes());
    let key = Curve25519SecretKey::from_slice(&scalar);
    scalar.zeroize();
    key
}

/// Verify the signature of a signed prekey: an Ed25519 signature by the identity key `ik`
/// over the 32 bytes of the Curve25519 public signed prekey `spk`.
///
/// What is signed: the encoding of the public key, "Encode(SPK)" of the X3DH spec; for a
/// Curve25519 key it is the 32 bytes of the u-coordinate (XEP-0384 0.9, section 4.2,
/// "Encode(PK)"). python-x3dh signs `_encode_public_key(CURVE_25519, spk)` which is those
/// bytes (`BaseState.__generate_spk`), and verifies the same bytes
/// (`get_shared_secret_active`).
pub(crate) fn verify_signed_prekey(ik: &[u8], spk: &[u8], spks: &[u8]) -> Result<(), Error> {
    let ik: [u8; 32] = ik.try_into().map_err(|_| Error::Malformed("bundle"))?;
    if spk.len() != 32 {
        return Err(Error::Malformed("bundle"));
    }
    let signature: [u8; 64] = spks.try_into().map_err(|_| Error::Malformed("bundle"))?;
    let key = VerifyingKey::from_bytes(&ik).map_err(|_| Error::BadSignature)?;
    key.verify_strict(spk, &Signature::from_bytes(&signature))
        .map_err(|_| Error::BadSignature)
}

impl Identity {
    /// Make a new identity: a random device id, an identity key pair, one signed prekey
    /// with its signature, and [`PREKEY_COUNT`] one-time prekeys.
    pub fn generate() -> Result<Identity, Error> {
        // A device id in 1..=2^31-1.
        let device_id = (random_u32()? % 0x7FFF_FFFF) + 1;

        let mut seed = Zeroizing::new([0u8; 32]);
        getrandom::fill(seed.as_mut_slice()).map_err(|_| Error::Rng)?;
        let ik = SigningKey::from_bytes(&seed);

        let mut id = Identity::from_signing_key(device_id, ik);
        id.rotate_signed_prekey()?;
        id.refill_prekeys(PREKEY_COUNT)?;
        Ok(id)
    }

    fn from_signing_key(device_id: u32, ik: SigningKey) -> Identity {
        Identity {
            device_id,
            ik_dh: ik_dh(&ik),
            ik_public: ik.verifying_key().to_bytes(),
            ik,
            current_spk: 0,
            next_spk_id: 1,
            next_prekey_id: 1,
            signed_prekeys: BTreeMap::new(),
            prekeys: BTreeMap::new(),
        }
    }

    /// The device id: a number in 1..=2^31-1.
    pub fn device_id(&self) -> u32 {
        self.device_id
    }

    /// The Ed25519 public identity key (the `<ik>` bytes).
    pub fn identity_key(&self) -> &[u8] {
        &self.ik_public
    }

    /// The fingerprint of this device, for display.
    pub fn fingerprint(&self) -> Result<String, Error> {
        fingerprint(&self.ik_public)
    }

    /// The bundle to publish: the current signed prekey and all unused prekeys.
    pub fn bundle(&self) -> Result<Bundle, Error> {
        let spk = self
            .signed_prekeys
            .get(&self.current_spk)
            .ok_or(Error::Malformed("signed prekey"))?;
        let prekeys = self
            .prekeys
            .iter()
            .map(|(id, secret)| (*id, Curve25519PublicKey::from(secret).to_bytes().to_vec()))
            .collect();
        Ok(Bundle {
            ik: self.ik_public.to_vec(),
            spk_id: self.current_spk,
            spk: Curve25519PublicKey::from(&spk.secret).to_bytes().to_vec(),
            spks: spk.signature.to_vec(),
            prekeys,
        })
    }

    /// How many unused one-time prekeys are left.
    pub fn prekey_count(&self) -> usize {
        self.prekeys.len()
    }

    /// Remove a prekey after a key exchange used it. Returns false if it was not there.
    pub fn remove_prekey(&mut self, id: u32) -> bool {
        self.prekeys.remove(&id).is_some()
    }

    /// Make new prekeys until there are `target`. Returns the new public keys. The caller
    /// must publish the bundle again. Does nothing if there are enough.
    pub fn refill_prekeys(&mut self, target: usize) -> Result<Vec<(u32, Vec<u8>)>, Error> {
        let missing = target.saturating_sub(self.prekeys.len());
        let mut added = Vec::with_capacity(missing);
        for _ in 0..missing {
            // Find a free id. With at most a few thousand keys this ends at once.
            let mut id = self.next_prekey_id;
            while self.prekeys.contains_key(&id) {
                id = (id % PREKEY_ID_LIMIT) + 1;
            }
            let secret = new_secret()?;
            added.push((id, Curve25519PublicKey::from(&secret).to_bytes().to_vec()));
            self.prekeys.insert(id, secret);
            self.next_prekey_id = (id % PREKEY_ID_LIMIT) + 1;
        }
        Ok(added)
    }

    /// Make a new signed prekey and use it for the bundle. The old ones stay, because
    /// delayed messages can still use them (XEP-0384, section 4.2). Returns the id, the
    /// public key and the signature.
    pub fn rotate_signed_prekey(&mut self) -> Result<(u32, Vec<u8>, Vec<u8>), Error> {
        let mut id = self.next_spk_id;
        while self.signed_prekeys.contains_key(&id) {
            id = (id % PREKEY_ID_LIMIT) + 1;
        }
        let secret = new_secret()?;
        let public = Curve25519PublicKey::from(&secret).to_bytes();
        // The signature is over the 32 bytes of the public key (see `verify_signed_prekey`).
        let signature = self.ik.sign(&public).to_bytes();
        self.signed_prekeys.insert(
            id,
            SignedPreKey {
                secret,
                signature,
                created_ms: now_ms(),
            },
        );
        self.current_spk = id;
        self.next_spk_id = (id % PREKEY_ID_LIMIT) + 1;
        Ok((id, public.to_vec(), signature.to_vec()))
    }

    /// The id of the signed prekey that the bundle uses now.
    pub fn current_signed_prekey_id(&self) -> u32 {
        self.current_spk
    }

    /// The ids of all signed prekeys that we still hold, with the creation time in
    /// milliseconds since the Unix epoch. The caller decides when to drop an old one.
    pub fn signed_prekeys(&self) -> Result<Vec<(u32, u64)>, Error> {
        Ok(self
            .signed_prekeys
            .iter()
            .map(|(id, k)| (*id, k.created_ms))
            .collect())
    }

    /// Drop an old signed prekey. The current one stays. Returns true if one was dropped.
    pub fn discard_signed_prekey(&mut self, id: u32) -> bool {
        if id == self.current_spk {
            return false;
        }
        self.signed_prekeys.remove(&id).is_some()
    }

    /// An identity with fixed keys, for the known-answer tests. The first signed prekey is the
    /// current one.
    #[cfg(test)]
    pub(crate) fn for_tests(
        device_id: u32,
        seed: [u8; 32],
        signed_prekeys: &[(u32, [u8; 32])],
        prekeys: &[(u32, [u8; 32])],
    ) -> Identity {
        let mut id = Identity::from_signing_key(device_id, SigningKey::from_bytes(&seed));
        for (n, (spk_id, secret)) in signed_prekeys.iter().enumerate() {
            let secret = Curve25519SecretKey::from_slice(secret);
            let signature = id
                .ik
                .sign(Curve25519PublicKey::from(&secret).as_bytes_ref())
                .to_bytes();
            id.signed_prekeys.insert(
                *spk_id,
                SignedPreKey {
                    secret,
                    signature,
                    created_ms: 0,
                },
            );
            if n == 0 {
                id.current_spk = *spk_id;
            }
        }
        for (pk_id, secret) in prekeys {
            id.prekeys
                .insert(*pk_id, Curve25519SecretKey::from_slice(secret));
        }
        id
    }

    /// The identity key as a Curve25519 key, for Diffie-Hellman.
    pub(crate) fn identity_dh_key(&self) -> &Curve25519SecretKey {
        &self.ik_dh
    }

    pub(crate) fn signed_prekey_secret(&self, id: u32) -> Option<&Curve25519SecretKey> {
        self.signed_prekeys.get(&id).map(|k| &k.secret)
    }

    pub(crate) fn prekey_secret(&self, id: u32) -> Option<&Curve25519SecretKey> {
        self.prekeys.get(&id)
    }

    /// Serialize all of this identity, private keys included.
    ///
    /// Format: one version byte (1), then JSON of the device id, the 32-byte seed of the
    /// identity key, the three id counters, the signed prekeys (id, 32-byte secret,
    /// signature, creation time) and the prekeys (id, 32-byte secret).
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let pickle = IdentityPickle {
            device_id: self.device_id,
            identity_seed: self.ik.to_bytes().to_vec(),
            current_spk: self.current_spk,
            next_spk_id: self.next_spk_id,
            next_prekey_id: self.next_prekey_id,
            signed_prekeys: self
                .signed_prekeys
                .iter()
                .map(|(id, k)| SignedPreKeyPickle {
                    id: *id,
                    secret: k.secret.to_bytes().to_vec(),
                    signature: k.signature.to_vec(),
                    created_ms: k.created_ms,
                })
                .collect(),
            prekeys: self
                .prekeys
                .iter()
                .map(|(id, k)| PreKeyPickle {
                    id: *id,
                    secret: k.to_bytes().to_vec(),
                })
                .collect(),
        };
        let mut out = Zeroizing::new(vec![FORMAT_VERSION]);
        // The pickle types serialize with no error.
        #[allow(clippy::expect_used)]
        serde_json::to_writer(&mut *out, &pickle).expect("An identity should serialize");
        out
    }

    /// Read an identity from the bytes of [`Identity::to_bytes`]. It checks every key and
    /// every signature, so that a broken record fails here and not at the first message.
    pub fn from_bytes(bytes: &[u8]) -> Result<Identity, Error> {
        let bad = Error::Malformed("identity record");
        let (version, json) = bytes.split_first().ok_or(bad.clone())?;
        if *version != FORMAT_VERSION || bytes.len() > MAX_STORED_LEN {
            return Err(bad);
        }
        let pickle: IdentityPickle = serde_json::from_slice(json).map_err(|_| bad.clone())?;

        if pickle.device_id == 0
            || pickle.device_id > 0x7FFF_FFFF
            || pickle.signed_prekeys.len() > MAX_STORED_SIGNED_PREKEYS
            || pickle.prekeys.len() > MAX_STORED_PREKEYS
        {
            return Err(bad);
        }
        let seed: [u8; 32] = pickle
            .identity_seed
            .as_slice()
            .try_into()
            .map_err(|_| bad.clone())?;
        let mut id = Identity::from_signing_key(pickle.device_id, SigningKey::from_bytes(&seed));
        id.current_spk = pickle.current_spk;
        id.next_spk_id = pickle.next_spk_id;
        id.next_prekey_id = pickle.next_prekey_id;

        for k in &pickle.signed_prekeys {
            let secret: [u8; 32] = k.secret.as_slice().try_into().map_err(|_| bad.clone())?;
            let signature: [u8; 64] = k.signature.as_slice().try_into().map_err(|_| bad.clone())?;
            let secret = Curve25519SecretKey::from_slice(&secret);
            // The signature must be good for the key: a bundle with a bad one is useless.
            let public = Curve25519PublicKey::from(&secret).to_bytes();
            id.ik
                .verifying_key()
                .verify(&public, &Signature::from_bytes(&signature))
                .map_err(|_| bad.clone())?;
            if id
                .signed_prekeys
                .insert(
                    k.id,
                    SignedPreKey {
                        secret,
                        signature,
                        created_ms: k.created_ms,
                    },
                )
                .is_some()
            {
                return Err(bad);
            }
        }
        for k in &pickle.prekeys {
            let secret: [u8; 32] = k.secret.as_slice().try_into().map_err(|_| bad.clone())?;
            if id
                .prekeys
                .insert(k.id, Curve25519SecretKey::from_slice(&secret))
                .is_some()
            {
                return Err(bad);
            }
        }
        if !id.signed_prekeys.contains_key(&id.current_spk) {
            return Err(bad);
        }
        Ok(id)
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct IdentityPickle {
    #[zeroize(skip)]
    device_id: u32,
    identity_seed: Vec<u8>,
    #[zeroize(skip)]
    current_spk: u32,
    #[zeroize(skip)]
    next_spk_id: u32,
    #[zeroize(skip)]
    next_prekey_id: u32,
    signed_prekeys: Vec<SignedPreKeyPickle>,
    prekeys: Vec<PreKeyPickle>,
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct SignedPreKeyPickle {
    #[zeroize(skip)]
    id: u32,
    secret: Vec<u8>,
    #[zeroize(skip)]
    signature: Vec<u8>,
    #[zeroize(skip)]
    created_ms: u64,
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct PreKeyPickle {
    #[zeroize(skip)]
    id: u32,
    secret: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_no_key() {
        let id = Identity::generate().unwrap();
        let text = format!("{id:?}");
        assert!(
            text.contains("device_id") && !text.contains("secret"),
            "{text}"
        );
    }

    #[test]
    fn the_signature_is_over_the_public_prekey() {
        let id = Identity::generate().unwrap();
        let b = id.bundle().unwrap();
        verify_signed_prekey(&b.ik, &b.spk, &b.spks).unwrap();
        let mut other = b.spk.clone();
        other[0] ^= 1;
        assert_eq!(
            verify_signed_prekey(&b.ik, &other, &b.spks),
            Err(Error::BadSignature)
        );
    }

    #[test]
    fn the_fingerprint_is_the_montgomery_form_of_the_identity_key() {
        let id = Identity::generate().unwrap();
        let from_scalar = Curve25519PublicKey::from(id.identity_dh_key()).to_hex();
        assert_eq!(
            fingerprint(id.identity_key()).unwrap().replace(' ', ""),
            from_scalar
        );
    }

    #[test]
    fn prekey_ids_wrap_and_skip_used_ids() {
        let mut id = Identity::generate().unwrap();
        id.next_prekey_id = PREKEY_ID_LIMIT;
        id.refill_prekeys(PREKEY_COUNT + 3).unwrap();
        let ids: Vec<u32> = id.prekeys.keys().copied().collect();
        assert_eq!(ids.len(), PREKEY_COUNT + 3);
        assert!(ids.contains(&PREKEY_ID_LIMIT));
        assert!(ids.iter().all(|i| (1..=PREKEY_ID_LIMIT).contains(i)));
    }

    #[test]
    fn stored_bytes_have_a_version_and_are_checked() {
        let id = Identity::generate().unwrap();
        let bytes = id.to_bytes();
        assert_eq!(bytes[0], FORMAT_VERSION);
        let again = Identity::from_bytes(&bytes).unwrap();
        assert_eq!(again.bundle().unwrap(), id.bundle().unwrap());
        assert_eq!(again.to_bytes().len(), bytes.len());

        let mut wrong = bytes.to_vec();
        wrong[0] = 2;
        assert!(Identity::from_bytes(&wrong).is_err());
        // A signed prekey whose signature is not for its key.
        let text = String::from_utf8_lossy(&bytes[1..]).into_owned();
        let mut parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        parsed["signed_prekeys"][0]["signature"][3] = serde_json::json!(
            (parsed["signed_prekeys"][0]["signature"][3]
                .as_u64()
                .unwrap() as u8
                ^ 1) as u64
        );
        let mut broken = vec![FORMAT_VERSION];
        broken.extend(serde_json::to_vec(&parsed).unwrap());
        assert!(Identity::from_bytes(&broken).is_err());
        // The current signed prekey must exist.
        let mut parsed: serde_json::Value = serde_json::from_slice(&bytes[1..]).unwrap();
        parsed["current_spk"] = serde_json::json!(99999);
        let mut broken = vec![FORMAT_VERSION];
        broken.extend(serde_json::to_vec(&parsed).unwrap());
        assert!(Identity::from_bytes(&broken).is_err());
    }
}
