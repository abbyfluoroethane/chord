//! One session with one remote device: build it (X3DH) and run the Double Ratchet. All of
//! it happens inside libomemo-c at protocol version 4 (OMEMO 2). This file only moves bytes
//! in and out.
//!
//! The state is a byte string: the serialized session record of libomemo-c. The caller
//! stores it and passes it back. A call never changes its inputs. It returns the new state,
//! and a failed call returns no state, so a failure cannot corrupt a session.

use zeroize::Zeroizing;

use crate::Error;
use crate::ffi::{self, signal_buffer};
use crate::identity::{Bundle, Identity};
use crate::native::{Buf, Ctx, Rc, address, check};

/// The counter at which XEP-0384 asks for a heartbeat (section 6, business rules): "When a
/// client receives the first message for a given ratchet key with a counter of 53 or
/// higher, it MUST send a heartbeat message."
pub const HEARTBEAT_COUNTER: u32 = 53;

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
    /// The counter of the message in its sending chain. It starts at 0. libomemo-c reads it
    /// from the message, we only pass it on.
    pub counter: u32,
    /// The ratchet key of the sender that this message uses (Ed25519 form). A new key
    /// means a new sending chain, and the counter starts again at 0.
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

struct Builder(*mut ffi::session_builder);

impl Drop for Builder {
    fn drop(&mut self) {
        // SAFETY: we own the builder and free it once.
        unsafe { ffi::session_builder_free(self.0) }
    }
}

struct Cipher(*mut ffi::session_cipher);

impl Drop for Cipher {
    fn drop(&mut self) {
        // SAFETY: we own the cipher and free it once.
        unsafe { ffi::session_cipher_free(self.0) }
    }
}

fn cipher(ctx: &Ctx) -> Result<Cipher, Error> {
    let addr = address();
    let mut raw = std::ptr::null_mut();
    // SAFETY: all pointers are valid. `addr` is copied by the library.
    unsafe {
        check(ffi::session_cipher_create(
            &mut raw, ctx.store, &addr, ctx.global,
        ))?;
        let cipher = Cipher(raw);
        ffi::session_cipher_set_version(cipher.0, ffi::CIPHERTEXT_OMEMO_VERSION);
        Ok(cipher)
    }
}

/// Build a session from the bundle of the other device (the sending side of X3DH).
fn build_session(ctx: &Ctx, bundle: &Bundle) -> Result<(), Error> {
    if bundle.ik.len() != 32 || bundle.spk.len() != 32 || bundle.spks.len() != 64 {
        return Err(Error::Malformed("bundle"));
    }
    // XEP-0384: a key exchange must use a prekey.
    if bundle.prekeys.is_empty() {
        return Err(Error::Malformed("bundle without prekeys"));
    }
    let mut raw = [0u8; 4];
    getrandom::fill(&mut raw).map_err(|_| Error::Rng)?;
    let (pk_id, pk) = &bundle.prekeys[u32::from_le_bytes(raw) as usize % bundle.prekeys.len()];
    if pk.len() != 32 {
        return Err(Error::Malformed("bundle"));
    }

    let (mut ik, mut spk, mut pre) = (
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    let mut raw_bundle = std::ptr::null_mut();
    let addr = address();
    let mut builder = std::ptr::null_mut();
    // SAFETY: all slices are valid for their lengths. Each out pointer is owned by `Rc` or
    // `Builder` at once. The library takes its own references to keys in the bundle.
    unsafe {
        check(ffi::curve_decode_point_ed(
            &mut ik,
            bundle.ik.as_ptr(),
            32,
            ctx.global,
        ))
        .map_err(|_| Error::Malformed("bundle identity key"))?;
        let ik = Rc::from_raw(ik).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        check(ffi::curve_decode_point_mont(
            &mut spk,
            bundle.spk.as_ptr(),
            32,
            ctx.global,
        ))
        .map_err(|_| Error::Malformed("bundle signed prekey"))?;
        let spk = Rc::from_raw(spk).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        check(ffi::curve_decode_point_mont(
            &mut pre,
            pk.as_ptr(),
            32,
            ctx.global,
        ))
        .map_err(|_| Error::Malformed("bundle prekey"))?;
        let pre = Rc::from_raw(pre).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        // The registration id and the device id mean nothing in OMEMO 2. Use zero.
        check(ffi::session_pre_key_bundle_create(
            &mut raw_bundle,
            0,
            0,
            *pk_id,
            pre.as_ptr(),
            bundle.spk_id,
            spk.as_ptr(),
            bundle.spks.as_ptr(),
            bundle.spks.len(),
            ik.as_ptr(),
        ))?;
        let raw_bundle = Rc::from_raw(raw_bundle).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        check(ffi::session_builder_create(
            &mut builder,
            ctx.store,
            &addr,
            ctx.global,
        ))?;
        let builder = Builder(builder);
        ffi::session_builder_set_version(builder.0, ffi::CIPHERTEXT_OMEMO_VERSION);
        match ffi::session_builder_process_pre_key_bundle(builder.0, raw_bundle.as_ptr()) {
            // The signature of the signed prekey is wrong, or a key does not parse.
            ffi::SG_ERR_INVALID_KEY => Err(Error::BadSignature),
            code => check(code).map(|_| ()),
        }
    }
}

/// Encrypt `key_material` for one remote device.
///
/// - With `session_record` you continue a session. `remote_bundle` is not used then.
/// - Without it, `remote_bundle` must be the bundle of the device. The call builds a new
///   session (X3DH) with one of the prekeys of the bundle, and `kex` is true.
///
/// `kex` stays true until the other device answers and you decrypt that answer. Send
/// every message as a key exchange until then (XEP-0384, section 4.3).
///
/// For a message with a payload, `key_material` is the output of
/// [`crate::payload::encrypt`]. For an empty OMEMO message it is 32 zero bytes.
pub fn encrypt_key(
    own: &Identity,
    session_record: Option<&[u8]>,
    remote_bundle: Option<&Bundle>,
    key_material: &[u8],
) -> Result<EncryptedKey, Error> {
    let mut state = own.store_state();
    state.session = session_record.map(<[u8]>::to_vec);
    let ctx = Ctx::new(state)?;

    if session_record.is_none() {
        build_session(&ctx, remote_bundle.ok_or(Error::NoSession)?)?;
    }

    let cipher = cipher(&ctx)?;
    let mut message = std::ptr::null_mut();
    // SAFETY: `key_material` is a valid slice. `message` is owned by `Rc` at once. The
    // serialized buffer is borrowed from the message, so we copy it before `message` drops.
    let (data, kex) = unsafe {
        check(ffi::session_cipher_encrypt(
            cipher.0,
            key_material.as_ptr(),
            key_material.len(),
            &mut message,
        ))
        .map_err(|e| {
            if e == Error::Library(ffi::SG_ERR_NO_SESSION) {
                Error::NoSession
            } else {
                e
            }
        })?;
        let message = Rc::from_raw(message).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        let kex = ffi::ciphertext_message_get_type(message.as_ptr()) == ffi::CIPHERTEXT_PREKEY_TYPE;
        let serialized = ffi::ciphertext_message_get_serialized(message.as_ptr());
        if serialized.is_null() {
            return Err(Error::Library(ffi::SG_ERR_UNKNOWN));
        }
        let len = ffi::signal_buffer_len(serialized);
        (
            std::slice::from_raw_parts(ffi::signal_buffer_data(serialized), len).to_vec(),
            kex,
        )
    };
    drop(cipher);

    let session = ctx
        .state()
        .new_session
        .clone()
        .ok_or(Error::Library(ffi::SG_ERR_UNKNOWN))?;
    Ok(EncryptedKey { data, kex, session })
}

/// The Ed25519 identity key of the remote device that a session record holds.
fn remote_identity(ctx: &Ctx, record: &[u8]) -> Result<Vec<u8>, Error> {
    let mut raw = std::ptr::null_mut();
    // SAFETY: `record` is a valid slice. The state and the key are borrowed from `raw`,
    // which lives until the end of the block.
    unsafe {
        check(ffi::session_record_deserialize(
            &mut raw,
            record.as_ptr(),
            record.len(),
            ctx.global,
        ))?;
        let raw = Rc::from_raw(raw).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        let state = ffi::session_record_get_state(raw.as_ptr());
        if state.is_null() {
            return Err(Error::Malformed("session record"));
        }
        let key = ffi::session_state_get_remote_identity_key(state);
        if key.is_null() {
            return Err(Error::Malformed("session record"));
        }
        let buf = Buf::from_raw(ffi::ec_public_key_get_ed(key))
            .ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        Ok(buf.bytes().to_vec())
    }
}

/// The counter and the ratchet key of a parsed message.
///
/// # Safety
/// `message` must be a live `signal_message`.
unsafe fn chain_position(message: *const ffi::signal_message) -> Result<(u32, Vec<u8>), Error> {
    // SAFETY: the caller gives a live message. The key is borrowed from it, and the buffer
    // is a copy that we own.
    unsafe {
        let counter = ffi::signal_message_get_counter(message);
        let key = ffi::signal_message_get_sender_ratchet_key(message);
        if key.is_null() {
            return Err(Error::Malformed("message"));
        }
        let buf = Buf::from_raw(ffi::ec_public_key_get_ed(key))
            .ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
        Ok((counter, buf.bytes().to_vec()))
    }
}

/// Decrypt the bytes of one `<key>` element.
///
/// `kex` is the `kex` attribute of the element. `session_record` is the session with this
/// device, if you have one. A key exchange builds the session (the receiving side of
/// X3DH) and does not need one. On failure nothing changes: `own` and `session_record`
/// are only read, and no new state is returned.
///
/// On success, remove the used prekey from `own` and store the new session.
pub fn decrypt_key(
    own: &Identity,
    session_record: Option<&[u8]>,
    data: &[u8],
    kex: bool,
) -> Result<DecryptedKey, Error> {
    let mut state = own.store_state();
    state.session = session_record.map(<[u8]>::to_vec);
    let ctx = Ctx::new(state)?;
    let cipher = cipher(&ctx)?;

    let mut plaintext: *mut signal_buffer = std::ptr::null_mut();
    let position;
    if kex {
        let mut message = std::ptr::null_mut();
        // SAFETY: `data` is a valid slice. `message` is owned by `Rc` at once. The
        // registration id is zero because OMEMO 2 does not use it.
        let code = unsafe {
            check(ffi::pre_key_signal_message_deserialize_omemo(
                &mut message,
                data.as_ptr(),
                data.len(),
                0,
                ctx.global,
            ))
            .map_err(|_| Error::Malformed("key exchange message"))?;
            let message = Rc::from_raw(message).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            let inner = ffi::pre_key_signal_message_get_signal_message(message.as_ptr());
            if inner.is_null() {
                return Err(Error::Malformed("key exchange message"));
            }
            position = chain_position(inner)?;
            let code = ffi::session_cipher_decrypt_pre_key_signal_message(
                cipher.0,
                message.as_ptr(),
                std::ptr::null_mut(),
                &mut plaintext,
            );
            if code == ffi::SG_ERR_INVALID_KEY_ID {
                return Err(Error::UnknownPreKey(
                    ffi::pre_key_signal_message_get_pre_key_id(message.as_ptr()),
                ));
            }
            code
        };
        finish(code)?;
    } else {
        if session_record.is_none() {
            return Err(Error::NoSession);
        }
        let mut message = std::ptr::null_mut();
        // SAFETY: as above.
        let code = unsafe {
            check(ffi::signal_message_deserialize_omemo(
                &mut message,
                data.as_ptr(),
                data.len(),
                ctx.global,
            ))
            .map_err(|_| Error::Malformed("message"))?;
            let message = Rc::from_raw(message).ok_or(Error::Library(ffi::SG_ERR_NOMEM))?;
            position = chain_position(message.as_ptr())?;
            ffi::session_cipher_decrypt_signal_message(
                cipher.0,
                message.as_ptr(),
                std::ptr::null_mut(),
                &mut plaintext,
            )
        };
        finish(code)?;
    }
    drop(cipher);

    // SAFETY: on success the library wrote a new buffer that we own now.
    let plain = unsafe { Buf::from_raw(plaintext) }.ok_or(Error::Library(ffi::SG_ERR_UNKNOWN))?;
    let key_material = Zeroizing::new(plain.bytes().to_vec());
    let session = ctx
        .state()
        .new_session
        .clone()
        .ok_or(Error::Library(ffi::SG_ERR_UNKNOWN))?;
    let remote_identity_key = remote_identity(&ctx, &session)?;
    Ok(DecryptedKey {
        key_material,
        session,
        used_prekey_id: ctx.state().used_prekey,
        remote_identity_key,
        counter: position.0,
        ratchet_key: position.1,
        heartbeat_due: position.0 >= HEARTBEAT_COUNTER,
    })
}

/// Turn a result code of a decrypt call into an error.
fn finish(code: i32) -> Result<(), Error> {
    match code {
        c if c >= 0 => Ok(()),
        ffi::SG_ERR_INVALID_MAC => Err(Error::BadMac),
        ffi::SG_ERR_DUPLICATE_MESSAGE => Err(Error::Duplicate),
        ffi::SG_ERR_NO_SESSION => Err(Error::NoSession),
        ffi::SG_ERR_INVALID_KEY => Err(Error::BadSignature),
        ffi::SG_ERR_INVALID_MESSAGE
        | ffi::SG_ERR_INVALID_PROTO_BUF
        | ffi::SG_ERR_INVALID_VERSION => Err(Error::Malformed("message")),
        ffi::SG_ERR_LEGACY_MESSAGE => Err(Error::Malformed("legacy message")),
        c => Err(Error::Library(c)),
    }
}
