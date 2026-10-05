// Copyright 2021 The Matrix.org Foundation C.I.C.
// Copyright 2021 Damir Jelić
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Modified for OMEMO 2 by the Chord authors, 2026.

pub(crate) mod key;

use aes::{
    Aes256,
    cipher::{
        BlockModeDecrypt, BlockModeEncrypt, KeyIvInit,
        block_padding::{Error as UnpadError, Pkcs7},
    },
};
use hmac::{Hmac, KeyInit as _, Mac as MacT, digest::MacError};
use key::CipherKeys;
use sha2::Sha256;

pub(crate) type Aes256CbcEnc = cbc::Encryptor<Aes256>;
pub(crate) type Aes256CbcDec = cbc::Decryptor<Aes256>;
pub(crate) type HmacSha256 = Hmac<Sha256>;

/// The length of the message authentication code (MAC) of an OMEMO message: the
/// HMAC-SHA-256 output, truncated to 16 bytes.
/// Source: python-twomemo, `AEADImpl.AUTHENTICATION_TAG_TRUNCATED_LENGTH`; XEP-0384 0.9,
/// section 4.3 (ENCRYPT).
pub(crate) const MAC_LEN: usize = 16;

/// A cipher used for encrypting and decrypting messages.
pub(crate) struct Cipher {
    keys: CipherKeys,
}

impl Cipher {
    /// Creates a new [`Cipher`] from the given 32-byte message key.
    ///
    /// The key is deterministically expanded into a 32-byte AES key, a 32-byte
    /// MAC key, and a 16-byte initialization vector (IV) using HKDF-SHA-256, with
    /// 32 zero bytes as the salt and "OMEMO Message Key Material" as the info.
    pub(crate) fn new(key: &[u8; 32]) -> Self {
        let keys = CipherKeys::new(key);

        Self { keys }
    }

    fn get_hmac(&self) -> HmacSha256 {
        // We don't use HmacSha256::new() here because it expects a 64-byte
        // large HMAC key while the key here has 32 bytes.
        #[allow(clippy::expect_used)]
        HmacSha256::new_from_slice(self.keys.mac_key())
            .expect("We should be able to create a HmacSha256 from a 32 byte key")
    }

    /// Encrypts the given plaintext with AES-256-CBC and PKCS#7 padding.
    ///
    /// **Warning**: This is a low-level function and does not provide
    /// authentication for the ciphertext. You must call [`Cipher::mac()`]
    /// separately to generate the message authentication code (MAC).
    pub(crate) fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let cipher = Aes256CbcEnc::new(self.keys.aes_key(), self.keys.iv());
        cipher.encrypt_padded_vec::<Pkcs7>(plaintext)
    }

    /// Generates the truncated message authentication code (MAC) for the given
    /// data. The data is the associated data followed by the serialized
    /// `OMEMOMessage`.
    pub(crate) fn mac(&self, message: &[u8]) -> [u8; MAC_LEN] {
        let mut hmac = self.get_hmac();
        hmac.update(message);

        let mac_bytes = hmac.finalize().into_bytes();

        let mut mac = [0u8; MAC_LEN];
        mac.copy_from_slice(&mac_bytes[..MAC_LEN]);

        mac
    }

    /// Decrypts the provided `ciphertext` using this [`Cipher`].
    ///
    /// **Warning**: This is a low-level function. Before calling this, you must
    /// call [`Cipher::verify_mac()`] to ensure the integrity of the ciphertext.
    pub(crate) fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, UnpadError> {
        let cipher = Aes256CbcDec::new(self.keys.aes_key(), self.keys.iv());
        cipher.decrypt_padded_vec::<Pkcs7>(ciphertext)
    }

    /// Verifies, in constant time, that the provided truncated message
    /// authentication code (MAC) correctly authenticates the given message. The tag must
    /// have the full length of 16 bytes: a shorter tag would be easier to forge.
    ///
    /// **Warning**: This is a low-level function and must be called before
    /// invoking the [`Cipher::decrypt()`] method.
    pub(crate) fn verify_mac(&self, message: &[u8], tag: &[u8; MAC_LEN]) -> Result<(), MacError> {
        let mut hmac = self.get_hmac();

        hmac.update(message);
        hmac.verify_truncated_left(tag)
    }
}

#[cfg(test)]
mod test {
    use super::{Cipher, MAC_LEN};

    #[test]
    fn mac_is_truncated_and_verifies() {
        let cipher = Cipher::new(&[1u8; 32]);
        let mac = cipher.mac(b"data");
        assert_eq!(mac.len(), MAC_LEN);
        assert!(cipher.verify_mac(b"data", &mac).is_ok());
        assert!(cipher.verify_mac(b"datb", &mac).is_err());
        let mut other = mac;
        other[15] ^= 1;
        assert!(cipher.verify_mac(b"data", &other).is_err());
    }

    #[test]
    fn encrypt_and_decrypt() {
        let cipher = Cipher::new(&[2u8; 32]);
        let ciphertext = cipher.encrypt(b"hello");
        assert_eq!(ciphertext.len(), 16);
        assert_eq!(cipher.decrypt(&ciphertext).unwrap(), b"hello");
        assert!(cipher.decrypt(&ciphertext[..15]).is_err());
    }
}
