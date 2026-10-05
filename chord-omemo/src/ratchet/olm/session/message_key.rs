// Copyright 2021 Damir Jelić, Denis Kasak
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

use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::{DecryptionError, EncryptionError, ratchet::RatchetPublicKey};
use crate::ratchet::{cipher::Cipher, olm::messages::Message};

/// A single-use encryption key for per-message encryption.
///
/// This key is used to encrypt a single message in a session.
pub(crate) struct MessageKey {
    key: Box<[u8; 32]>,
    ratchet_key: RatchetPublicKey,
    index: u64,
    /// `pn`: the length of the previous sending chain.
    previous_chain_length: u64,
}

impl Drop for MessageKey {
    fn drop(&mut self) {
        self.key.zeroize()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct RemoteMessageKey {
    pub key: Box<[u8; 32]>,
    pub index: u64,
}

impl Debug for RemoteMessageKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { key: _, index } = self;

        f.debug_struct("RemoteMessageKey")
            .field("index", index)
            .finish()
    }
}

impl Drop for RemoteMessageKey {
    fn drop(&mut self) {
        self.key.zeroize()
    }
}

impl MessageKey {
    /// Creates a new [`MessageKey`] from the provided raw 32 bytes, along with
    /// a public ratchet key, an index and the length of the previous chain.
    ///
    /// The ratchet key, index and previous chain length are included in the created
    /// [`Message`], allowing the message recipient to derive the same [`MessageKey`].
    pub(super) const fn new(
        key: Box<[u8; 32]>,
        ratchet_key: RatchetPublicKey,
        index: u64,
        previous_chain_length: u64,
    ) -> Self {
        Self {
            key,
            ratchet_key,
            index,
            previous_chain_length,
        }
    }

    /// Encrypts the provided plaintext using this [`MessageKey`].
    ///
    /// The ciphertext is AES-256-CBC with PKCS#7. The MAC is HMAC-SHA-256, cut to 16 bytes,
    /// over `associated_data` followed by the serialized `OMEMOMessage`.
    pub(super) fn encrypt(
        self,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<Message, EncryptionError> {
        let cipher = Cipher::new(&self.key);

        let ciphertext = cipher.encrypt(plaintext);

        // `n` and `pn` are uint32 on the wire.
        let index = u32::try_from(self.index).map_err(|_| EncryptionError::ChainTooLong)?;
        let previous =
            u32::try_from(self.previous_chain_length).map_err(|_| EncryptionError::ChainTooLong)?;

        let mut message = Message::new(*self.ratchet_key.as_ref(), index, previous, ciphertext);

        let mac = cipher.mac(&message.mac_bytes(associated_data));
        message.set_mac(mac);

        Ok(message)
    }

    #[cfg(test)]
    pub(super) fn key(&self) -> &[u8; 32] {
        self.key.as_ref()
    }
}

impl RemoteMessageKey {
    pub(super) const fn new(key: Box<[u8; 32]>, index: u64) -> Self {
        Self { key, index }
    }

    pub(super) const fn chain_index(&self) -> u64 {
        self.index
    }

    /// Checks the MAC of the message in constant time, then decrypts it.
    pub(super) fn decrypt(
        &self,
        message: &Message,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, DecryptionError> {
        let cipher = Cipher::new(&self.key);

        cipher.verify_mac(&message.mac_bytes(associated_data), &message.mac)?;
        Ok(cipher.decrypt(&message.ciphertext)?)
    }
}
