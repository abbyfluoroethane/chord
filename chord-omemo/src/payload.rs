//! The encryption of the `<payload>` element (XEP-0384, sections 4.4 and 4.5).
//!
//! This is a composition of RustCrypto calls. The Double Ratchet (see `session`) only
//! carries the key material. For the payload the XEP says:
//!
//! 1. Make 32 random bytes, the `key`.
//! 2. HKDF-SHA-256 with `key` as input, 32 zero bytes as salt, and the info string
//!    "OMEMO Payload". Take 80 bytes of output.
//! 3. Split the output: 32 bytes encryption key, 32 bytes authentication key, 16 bytes IV.
//! 4. Encrypt the plaintext with AES-256-CBC and PKCS#7 padding.
//! 5. HMAC-SHA-256 over the ciphertext with the authentication key. Keep the first 16 bytes.
//! 6. `key || mac` (48 bytes) goes through the Double Ratchet, once for each device.
//!
//! Each step below is one call into the `hkdf`, `cbc`, `hmac` crates. There is no own cipher
//! logic. python-twomemo does the same steps (`Twomemo.encrypt_plaintext`).

use aes::Aes256;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::Error;

const INFO: &[u8] = b"OMEMO Payload";
/// Length of the random `key`.
pub const KEY_LEN: usize = 32;
/// Length of the truncated MAC.
pub const MAC_LEN: usize = 16;
/// The bytes that go through the ratchet for a message with a payload: key and MAC.
pub const KEY_MATERIAL_LEN: usize = KEY_LEN + MAC_LEN;
/// The bytes that go through the ratchet for an empty OMEMO message: 32 zero bytes. An
/// empty message has no `<payload>`.
pub const EMPTY_KEY_MATERIAL: [u8; 32] = [0; 32];

struct Keys {
    enc: Zeroizing<[u8; 32]>,
    auth: Zeroizing<[u8; 32]>,
    iv: Zeroizing<[u8; 16]>,
}

fn derive(key: &[u8]) -> Result<Keys, Error> {
    let hk = Hkdf::<Sha256>::new(Some(&[0u8; 32]), key);
    let mut okm = Zeroizing::new([0u8; 80]);
    hk.expand(INFO, &mut *okm)
        .map_err(|_| Error::Malformed("payload key"))?;
    let mut keys = Keys {
        enc: Zeroizing::new([0; 32]),
        auth: Zeroizing::new([0; 32]),
        iv: Zeroizing::new([0; 16]),
    };
    keys.enc.copy_from_slice(&okm[..32]);
    keys.auth.copy_from_slice(&okm[32..64]);
    keys.iv.copy_from_slice(&okm[64..]);
    Ok(keys)
}

fn mac(auth: &[u8; 32], ciphertext: &[u8]) -> Hmac<Sha256> {
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(auth).expect("HMAC takes any key length");
    mac.update(ciphertext);
    mac
}

/// Encrypt the plaintext of a message (the bytes of the XEP-0420 envelope).
///
/// Returns the 48 bytes of key material for [`crate::encrypt_key`] (one call for each
/// recipient device), and the ciphertext for the `<payload>` element.
pub fn encrypt(plaintext: &[u8]) -> Result<(Zeroizing<Vec<u8>>, Vec<u8>), Error> {
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    getrandom::fill(&mut *key).map_err(|_| Error::Rng)?;
    let keys = derive(&*key)?;
    let ciphertext = cbc::Encryptor::<Aes256>::new_from_slices(&*keys.enc, &*keys.iv)
        .map_err(|_| Error::Malformed("payload key"))?
        .encrypt_padded_vec::<Pkcs7>(plaintext);
    let tag = mac(&keys.auth, &ciphertext).finalize().into_bytes();
    let mut material = Zeroizing::new(Vec::with_capacity(KEY_MATERIAL_LEN));
    material.extend_from_slice(&*key);
    material.extend_from_slice(&tag[..MAC_LEN]);
    Ok((material, ciphertext))
}

/// Decrypt the `<payload>` with the 48 bytes of key material that [`crate::decrypt_key`]
/// gave. The MAC is checked first, in constant time.
pub fn decrypt(key_material: &[u8], ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    if key_material.len() != KEY_MATERIAL_LEN {
        return Err(Error::Malformed("payload key material"));
    }
    let (key, tag) = key_material.split_at(KEY_LEN);
    let keys = derive(key)?;
    mac(&keys.auth, ciphertext)
        .verify_truncated_left(tag)
        .map_err(|_| Error::BadMac)?;
    let plain = cbc::Decryptor::<Aes256>::new_from_slices(&*keys.enc, &*keys.iv)
        .map_err(|_| Error::Malformed("payload key"))?
        .decrypt_padded_vec::<Pkcs7>(ciphertext)
        .map_err(|_| Error::Malformed("payload padding"))?;
    Ok(Zeroizing::new(plain))
}
