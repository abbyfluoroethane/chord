//! One session with one remote device: build it (X3DH) and run the Double Ratchet. The
//! work is done by the fork of vodozemac in `ratchet`. This file checks the bundles and the
//! inputs, and maps the results to the API of this crate.
//!
//! The state is a byte string: the pickled session (one version byte, then JSON). The
//! caller stores it and passes it back. A call never changes its inputs. It returns the new
//! state, and a failed call returns no state, so a failure cannot corrupt a session.

use zeroize::Zeroizing;

use crate::Error;
use crate::identity::{Bundle, Identity, curve25519_of_ed25519, verify_signed_prekey};
use crate::ratchet::Curve25519PublicKey;
use crate::ratchet::olm::{
    DecodeError, DecryptionError, EncryptionError, Message, PreKeyMessage, Session, SessionError,
    session::RatchetError,
};

/// The counter at which XEP-0384 asks for a heartbeat (section 6, business rules): "When a
/// client receives the first message for a given ratchet key with a counter of 53 or
/// higher, it MUST send a heartbeat message."
pub const HEARTBEAT_COUNTER: u32 = 53;

/// The longest key material that we encrypt, in bytes. A real message has 48 bytes (a
/// 32-byte key and a 16-byte MAC) and an empty message has 32 bytes.
const MAX_KEY_MATERIAL: usize = 512;

/// The result of [`encrypt_key`].
#[derive(Debug)]
pub struct EncryptedKey {
    /// The bytes for the `<key>` element: an `OMEMOKeyExchange` when `kex` is true,
    /// otherwise an `OMEMOAuthenticatedMessage`. Both are protobuf, as XEP-0384 says.
    pub data: Vec<u8>,
    /// The value of the `kex` attribute of the `<key>` element.
    pub kex: bool,
    /// The new session record. Store it.
    pub session: Vec<u8>,
}

/// The result of [`decrypt_key`].
pub struct DecryptedKey {
    /// The decrypted bytes. For a message with a payload: the 32-byte key and the 16-byte
    /// MAC that [`crate::payload`] makes.
    pub key_material: Zeroizing<Vec<u8>>,
    /// The new session record. Store it.
    pub session: Vec<u8>,
    /// The prekey that the key exchange used. Remove it with [`Identity::remove_prekey`]
    /// and publish the bundle again. `None` when the message was not a new key exchange.
    pub used_prekey_id: Option<u32>,
    /// The Ed25519 identity key of the other device (as in its `<ik>` element). Check it
    /// against the trust decision for that device.
    pub remote_identity_key: Vec<u8>,
    /// The counter of the message in its sending chain (`n` of the `OMEMOMessage`). It
    /// starts at 0.
    pub counter: u32,
    /// The ratchet key of the sender that this message uses (`dh_pub` of the
    /// `OMEMOMessage`, 32 bytes). A new key means a new sending chain, and the counter
    /// starts again at 0.
    pub ratchet_key: Vec<u8>,
    /// True when `counter` is [`HEARTBEAT_COUNTER`] or more. The caller must send an empty
    /// OMEMO message to the sender, but only for the first such message of a ratchet key.
    /// The caller tracks that with `ratchet_key`: this crate keeps no state between calls.
    pub heartbeat_due: bool,
}

impl core::fmt::Debug for DecryptedKey {
    // Never print the key material.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DecryptedKey")
            .field("used_prekey_id", &self.used_prekey_id)
            .field("counter", &self.counter)
            .finish_non_exhaustive()
    }
}

impl From<SessionError> for Error {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::Ratchet(RatchetError::NonContributoryKey) => Error::BadSignature,
            SessionError::Ratchet(RatchetError::Rng) => Error::Rng,
            SessionError::BadPickle => Error::Malformed("session record"),
        }
    }
}

impl From<EncryptionError> for Error {
    fn from(e: EncryptionError) -> Self {
        match e {
            EncryptionError::NonContributoryKey => Error::BadSignature,
            EncryptionError::Rng => Error::Rng,
            EncryptionError::ChainTooLong => Error::Malformed("sending chain"),
        }
    }
}

impl From<DecryptionError> for Error {
    fn from(e: DecryptionError) -> Self {
        match e {
            DecryptionError::InvalidMAC(_) => Error::BadMac,
            DecryptionError::InvalidPadding(_) => Error::Malformed("padding"),
            DecryptionError::NonContributoryKey => Error::BadSignature,
            // The key of the message is used up (a replay) or was dropped.
            DecryptionError::MissingMessageKey(_) => Error::Duplicate,
            DecryptionError::TooBigMessageGap(..) => Error::Malformed("message gap"),
            DecryptionError::Rng => Error::Rng,
        }
    }
}

impl From<DecodeError> for Error {
    fn from(_: DecodeError) -> Self {
        Error::Malformed("message")
    }
}

/// Build a session from the bundle of the other device (the sending side of X3DH).
fn build_session(own: &Identity, bundle: &Bundle) -> Result<Session, Error> {
    if bundle.ik.len() != 32 || bundle.spk.len() != 32 || bundle.spks.len() != 64 {
        return Err(Error::Malformed("bundle"));
    }
    // XEP-0384, section 4.2: a key exchange must use a prekey.
    if bundle.prekeys.is_empty() {
        return Err(Error::Malformed("bundle without prekeys"));
    }
    // The signature first: nothing of an unsigned bundle is used.
    verify_signed_prekey(&bundle.ik, &bundle.spk, &bundle.spks)?;

    let mut raw = [0u8; 4];
    getrandom::fill(&mut raw).map_err(|_| Error::Rng)?;
    let (pk_id, pk) = &bundle.prekeys[u32::from_le_bytes(raw) as usize % bundle.prekeys.len()];

    let remote_identity_key =
        curve25519_of_ed25519(&bundle.ik).map_err(|_| Error::Malformed("bundle identity key"))?;
    let spk = Curve25519PublicKey::from_slice(&bundle.spk)
        .map_err(|_| Error::Malformed("bundle signed prekey"))?;
    let pk = Curve25519PublicKey::from_slice(pk).map_err(|_| Error::Malformed("bundle prekey"))?;
    let mut remote_ik = [0u8; 32];
    remote_ik.copy_from_slice(&bundle.ik);
    let mut own_ik = [0u8; 32];
    own_ik.copy_from_slice(own.identity_key());

    Ok(Session::new_active(
        own.identity_dh_key(),
        own_ik,
        remote_ik,
        &remote_identity_key,
        spk,
        bundle.spk_id,
        pk,
        *pk_id,
    )?)
}

/// Encrypt `key_material` for one remote device.
///
/// - With `session_record` you continue a session. `remote_bundle` is not used then.
/// - Without it, `remote_bundle` must be the bundle of the device. The call builds a new
///   session (X3DH) with one of the prekeys of the bundle, and `kex` is true.
///
/// `kex` stays true until the other device answers and you decrypt that answer. Send
/// every message as a key exchange until then (XEP-0384, section 4.2 and section 6).
///
/// For a message with a payload, `key_material` is the output of
/// [`crate::payload::encrypt`]. For an empty OMEMO message it is 32 zero bytes.
pub fn encrypt_key(
    own: &Identity,
    session_record: Option<&[u8]>,
    remote_bundle: Option<&Bundle>,
    key_material: &[u8],
) -> Result<EncryptedKey, Error> {
    if key_material.len() > MAX_KEY_MATERIAL {
        return Err(Error::Malformed("key material"));
    }
    let mut session = match session_record {
        Some(record) => Session::from_bytes(record)?,
        None => build_session(own, remote_bundle.ok_or(Error::NoSession)?)?,
    };

    let out = session.encrypt(key_material)?;

    Ok(EncryptedKey {
        data: out.data,
        kex: out.kex,
        session: session.to_bytes().to_vec(),
    })
}

/// Make the session for a key exchange that we answer (the receiving side of X3DH).
fn answer_key_exchange(own: &Identity, key_exchange: &PreKeyMessage) -> Result<Session, Error> {
    let one_time_key = own
        .prekey_secret(key_exchange.pre_key_id)
        .ok_or(Error::UnknownPreKey(key_exchange.pre_key_id))?;
    let signed_pre_key = own
        .signed_prekey_secret(key_exchange.signed_pre_key_id)
        .ok_or(Error::Malformed("unknown signed prekey"))?;
    let remote_identity_key = curve25519_of_ed25519(&key_exchange.identity_key)?;
    let mut own_ik = [0u8; 32];
    own_ik.copy_from_slice(own.identity_key());

    Ok(Session::new_passive(
        own.identity_dh_key(),
        own_ik,
        signed_pre_key,
        one_time_key,
        &remote_identity_key,
        key_exchange,
    )?)
}

/// Decrypt the bytes of one `<key>` element.
///
/// `kex` is the `kex` attribute of the element. `session_record` is the session with this
/// device, if you have one. A key exchange builds the session (the receiving side of
/// X3DH) and does not need one. A key exchange that the stored session was built from is
/// read with that session: the other device repeats it until it hears from us. On failure
/// nothing changes: `own` and `session_record` are only read, and no new state is returned.
///
/// On success, remove the used prekey from `own` and store the new session.
pub fn decrypt_key(
    own: &Identity,
    session_record: Option<&[u8]>,
    data: &[u8],
    kex: bool,
) -> Result<DecryptedKey, Error> {
    let mut used_prekey_id = None;
    let (mut session, message) = if kex {
        let key_exchange = PreKeyMessage::from_bytes(data)?;
        // A stored session that is no good does not stop a new key exchange.
        let existing = session_record.and_then(|r| Session::from_bytes(r).ok());
        match existing {
            Some(session) if session.built_from(&key_exchange) => (session, key_exchange.message),
            _ => {
                let session = answer_key_exchange(own, &key_exchange)?;
                used_prekey_id = Some(key_exchange.pre_key_id);
                (session, key_exchange.message)
            }
        }
    } else {
        let record = session_record.ok_or(Error::NoSession)?;
        (Session::from_bytes(record)?, Message::from_bytes(data)?)
    };

    let plaintext = Zeroizing::new(session.decrypt(&message)?);

    Ok(DecryptedKey {
        key_material: plaintext,
        session: session.to_bytes().to_vec(),
        used_prekey_id,
        remote_identity_key: session.remote_identity_key().to_vec(),
        counter: message.chain_index,
        ratchet_key: message.ratchet_key.to_bytes().to_vec(),
        heartbeat_due: message.chain_index >= HEARTBEAT_COUNTER,
    })
}
