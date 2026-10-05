//! The one place that calls `chord-omemo`. Chord writes no crypto of its own: every key,
//! session, and payload operation goes through this file into libomemo-c and the payload
//! code of `chord-omemo`. The rest of the feature only handles bytes and XML.
//!
//! The wasm build has no `chord-omemo`. It uses `crypto_off.rs`, which has the same names
//! and says "not available".

use zeroize::Zeroizing;

use super::xml::BundleXml;

/// True when this build can do OMEMO.
pub(crate) const AVAILABLE: bool = true;

/// The keys of this device.
pub(crate) struct Keys(chord_omemo::Identity);

fn text(e: chord_omemo::Error) -> String {
    e.to_string()
}

impl Keys {
    pub(crate) fn generate() -> Result<Self, String> {
        chord_omemo::Identity::generate().map(Self).map_err(text)
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        chord_omemo::Identity::from_bytes(bytes)
            .map(Self)
            .map_err(text)
    }

    pub(crate) fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        self.0.to_bytes()
    }

    pub(crate) fn device_id(&self) -> u32 {
        self.0.device_id()
    }

    pub(crate) fn identity_key(&self) -> &[u8] {
        self.0.identity_key()
    }

    pub(crate) fn bundle(&self) -> Result<BundleXml, String> {
        let b = self.0.bundle().map_err(text)?;
        Ok(BundleXml {
            spk_id: b.spk_id,
            spk: b.spk,
            spks: b.spks,
            ik: b.ik,
            prekeys: b.prekeys,
        })
    }

    pub(crate) fn prekey_count(&self) -> usize {
        self.0.prekey_count()
    }

    pub(crate) fn remove_prekey(&mut self, id: u32) -> bool {
        self.0.remove_prekey(id)
    }

    /// Make new prekeys up to the usual count.
    pub(crate) fn refill_prekeys(&mut self) -> Result<(), String> {
        self.0
            .refill_prekeys(chord_omemo::PREKEY_COUNT)
            .map(|_| ())
            .map_err(text)
    }

    pub(crate) fn rotate_signed_prekey(&mut self) -> Result<(), String> {
        self.0.rotate_signed_prekey().map(|_| ()).map_err(text)
    }

    /// The signed prekeys that we hold: the id and the creation time (Unix ms).
    pub(crate) fn signed_prekeys(&self) -> Result<Vec<(u32, u64)>, String> {
        self.0.signed_prekeys().map_err(text)
    }

    pub(crate) fn current_signed_prekey_id(&self) -> u32 {
        self.0.current_signed_prekey_id()
    }

    pub(crate) fn discard_signed_prekey(&mut self, id: u32) -> bool {
        self.0.discard_signed_prekey(id)
    }
}

/// The minimum of one-time prekeys. Under this number we make new ones.
pub(crate) const PREKEY_MIN: usize = chord_omemo::PREKEY_MIN;

/// The fingerprint to show for an `<ik>`: 8 groups of 8 hex characters.
pub(crate) fn fingerprint(ik: &[u8]) -> Option<String> {
    chord_omemo::fingerprint(ik).ok()
}

/// Encrypt the bytes of an envelope. Returns the 48 bytes of key material and the
/// `<payload>` bytes.
pub(crate) fn encrypt_payload(plain: &[u8]) -> Result<(Zeroizing<Vec<u8>>, Vec<u8>), String> {
    chord_omemo::payload::encrypt(plain).map_err(text)
}

/// The key material of an empty OMEMO message.
pub(crate) fn empty_material() -> Vec<u8> {
    chord_omemo::payload::EMPTY_KEY_MATERIAL.to_vec()
}

/// One encrypted `<key>`.
pub(crate) struct SealedKey {
    pub data: Vec<u8>,
    pub kex: bool,
    /// The new session record. Store it.
    pub session: Zeroizing<Vec<u8>>,
}

/// Encrypt `material` for one device. With a `session` the call continues it. Without,
/// `bundle` makes a new session.
pub(crate) fn encrypt_key(
    keys: &Keys,
    session: Option<&[u8]>,
    bundle: Option<&BundleXml>,
    material: &[u8],
) -> Result<SealedKey, String> {
    let bundle = bundle.map(|b| chord_omemo::Bundle {
        ik: b.ik.clone(),
        spk_id: b.spk_id,
        spk: b.spk.clone(),
        spks: b.spks.clone(),
        prekeys: b.prekeys.clone(),
    });
    let sealed =
        chord_omemo::encrypt_key(&keys.0, session, bundle.as_ref(), material).map_err(text)?;
    Ok(SealedKey {
        data: sealed.data,
        kex: sealed.kex,
        session: Zeroizing::new(sealed.session),
    })
}

/// Why a `<key>` did not open.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OpenError {
    /// We read this message before.
    Duplicate,
    Other(String),
}

pub(crate) struct OpenedKey {
    pub material: Zeroizing<Vec<u8>>,
    /// The new session record. Store it.
    pub session: Zeroizing<Vec<u8>>,
    /// The one-time prekey that a key exchange used.
    pub used_prekey: Option<u32>,
    /// The `<ik>` of the sender device.
    pub remote_identity_key: Vec<u8>,
    /// The ratchet key of the sender, when the message asks for a heartbeat (XEP-0384:
    /// counter 53 or higher). `None` otherwise.
    pub heartbeat_ratchet_key: Option<Vec<u8>>,
}

pub(crate) fn decrypt_key(
    keys: &Keys,
    session: Option<&[u8]>,
    data: &[u8],
    kex: bool,
) -> Result<OpenedKey, OpenError> {
    match chord_omemo::decrypt_key(&keys.0, session, data, kex) {
        Ok(d) => Ok(OpenedKey {
            material: d.key_material,
            session: Zeroizing::new(d.session),
            used_prekey: d.used_prekey_id,
            remote_identity_key: d.remote_identity_key,
            heartbeat_ratchet_key: d.heartbeat_due.then_some(d.ratchet_key),
        }),
        Err(chord_omemo::Error::Duplicate) => Err(OpenError::Duplicate),
        Err(e) => Err(OpenError::Other(e.to_string())),
    }
}

/// Decrypt the `<payload>` with the key material of `decrypt_key`.
pub(crate) fn decrypt_payload(
    material: &[u8],
    payload: &[u8],
) -> Result<Zeroizing<Vec<u8>>, String> {
    chord_omemo::payload::decrypt(material, payload).map_err(text)
}
