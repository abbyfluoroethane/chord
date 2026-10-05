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

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, digest::CtOutput};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{
    message_key::{MessageKey, RemoteMessageKey},
    ratchet::RatchetPublicKey,
};

/// KDF_CK: HMAC-SHA-256 with the chain key as the key. The single byte 0x01 as the input
/// gives the message key, the single byte 0x02 gives the next chain key.
/// Source: XEP-0384 0.9, section 4.3 (KDF_CK); python-twomemo
/// `DoubleRatchetImpl.MESSAGE_CHAIN_CONSTANT = b"\x02\x01"` (the first HMAC is the new
/// chain key, the second is the message key).
const MESSAGE_KEY_SEED: &[u8; 1] = b"\x01";
const ADVANCEMENT_SEED: &[u8; 1] = b"\x02";

fn expand_chain_key(key: &[u8; 32]) -> Box<[u8; 32]> {
    #[allow(clippy::expect_used)]
    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .expect("We should be able to create a HMAC object from a 32-byte key");
    mac.update(MESSAGE_KEY_SEED);

    let mut output = mac.finalize().into_bytes();

    let mut key = Box::new([0u8; 32]);
    #[allow(deprecated)]
    key.copy_from_slice(output.as_slice());

    output.zeroize();

    key
}

fn advance(key: &[u8; 32]) -> CtOutput<Hmac<Sha256>> {
    #[allow(clippy::expect_used)]
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect(
        "We should be able to create a HMAC object from a 32-byte key to advance the ratchet",
    );
    mac.update(ADVANCEMENT_SEED);

    mac.finalize()
}

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub(super) struct ChainKey {
    pub(super) key: Box<[u8; 32]>,
    pub(super) index: u64,
}

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub(super) struct RemoteChainKey {
    key: Box<[u8; 32]>,
    index: u64,
}

impl RemoteChainKey {
    pub(super) const fn new(bytes: Box<[u8; 32]>) -> Self {
        Self {
            key: bytes,
            index: 0,
        }
    }

    pub(super) const fn chain_index(&self) -> u64 {
        self.index
    }

    pub(super) fn advance(&mut self) {
        let output = advance(&self.key).into_bytes();
        #[allow(deprecated)]
        self.key.copy_from_slice(output.as_slice());
        self.index += 1;
    }

    pub(super) fn create_message_key(&mut self) -> RemoteMessageKey {
        let key = expand_chain_key(&self.key);
        let message_key = RemoteMessageKey::new(key, self.index);

        self.advance();

        message_key
    }
}

impl ChainKey {
    pub(super) const fn new(bytes: Box<[u8; 32]>) -> Self {
        Self {
            key: bytes,
            index: 0,
        }
    }

    pub(super) fn advance(&mut self) {
        let output = advance(&self.key).into_bytes();
        #[allow(deprecated)]
        self.key.copy_from_slice(output.as_slice());
        self.index += 1;
    }

    pub(super) const fn index(&self) -> u64 {
        self.index
    }

    /// The next message key. `previous_chain_length` is `pn`, the length of the
    /// sending chain before this one: it goes into the message header.
    pub(super) fn create_message_key(
        &mut self,
        ratchet_key: RatchetPublicKey,
        previous_chain_length: u64,
    ) -> MessageKey {
        let key = expand_chain_key(&self.key);
        let message_key = MessageKey::new(key, ratchet_key, self.index, previous_chain_length);

        self.advance();

        message_key
    }
}

impl std::fmt::Debug for ChainKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainKey")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for RemoteChainKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteChainKey")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::ChainKey;
    use crate::ratchet::olm::session::chain_key::RemoteChainKey;

    #[test]
    fn advancing_chain_key_increments_index() {
        let mut key = ChainKey::new(Box::new(*b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(key.index(), 0);
        key.advance();
        assert_eq!(key.index(), 1);
    }

    #[test]
    fn advancing_remote_chain_key_increments_index() {
        let mut key = RemoteChainKey::new(Box::new(*b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(key.chain_index(), 0);
        key.advance();
        assert_eq!(key.chain_index(), 1);
    }

    #[test]
    fn both_sides_make_the_same_keys() {
        let mut ours = ChainKey::new(Box::new([5u8; 32]));
        let mut theirs = RemoteChainKey::new(Box::new([5u8; 32]));
        let ratchet_key = super::RatchetPublicKey::from([1u8; 32]);
        for i in 0..4 {
            let a = ours.create_message_key(ratchet_key, 0);
            let b = theirs.create_message_key();
            assert_eq!(b.chain_index(), i);
            assert_eq!(a.key(), b.key.as_ref());
        }
    }
}
