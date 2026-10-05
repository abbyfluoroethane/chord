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

use super::{
    DecryptionError, chain_key::RemoteChainKey, message_key::RemoteMessageKey,
    ratchet::RemoteRatchetKey,
};
use crate::ratchet::olm::messages::Message;

/// The maximum number of message keys that can be skipped in a single message.
///
/// Source: XEP-0384 0.9, section 4.3 (MAX_SKIP): "modern hardware is safe to choose values
/// of around 1000"; python-twomemo uses 1000 (`max_num_per_message_skipped_keys`, which
/// defaults to the per-session maximum of 1000).
pub(crate) const MAX_MESSAGE_GAP: u64 = 1000;

/// The maximum number of skipped message keys to retain in one receiver chain.
///
/// Source: XEP-0384 0.9, section 4.3 (MAX_SKIP): "It is RECOMMENDED to keep 1000 skipped
/// message keys around per session"; python-twomemo uses 1000
/// (`max_num_per_session_skipped_keys`). The limit for a whole session is in
/// [`MAX_SESSION_MESSAGE_KEYS`]: the oldest keys go first.
pub(crate) const MAX_MESSAGE_KEYS: usize = 1000;

/// The maximum number of skipped message keys that one session keeps, over all of its
/// receiver chains. The oldest key goes first (XEP-0384 0.9, section 4.3, "deletion policy
/// for skipped message keys": FIFO).
pub(crate) const MAX_SESSION_MESSAGE_KEYS: usize = 1000;

#[derive(Serialize, Deserialize, Clone)]
struct MessageKeyStore {
    inner: Vec<RemoteMessageKey>,
}

impl MessageKeyStore {
    fn new() -> Self {
        Self { inner: Vec::new() }
    }

    fn push(&mut self, message_key: RemoteMessageKey) {
        if self.inner.len() >= MAX_MESSAGE_KEYS {
            self.inner.remove(0);
        }

        self.inner.push(message_key)
    }

    /// Keep at most `MAX_MESSAGE_KEYS` keys. The oldest go first. Used after a load.
    fn trim(&mut self) {
        let extra = self.inner.len().saturating_sub(MAX_MESSAGE_KEYS);
        self.inner.drain(..extra);
    }

    fn merge(&mut self, mut store: MessageKeyStore) {
        for key in store.inner.drain(..) {
            self.push(key);
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    /// Drop the oldest key. Returns false if there is none.
    fn drop_oldest(&mut self) -> bool {
        if self.inner.is_empty() {
            false
        } else {
            self.inner.remove(0);
            true
        }
    }

    fn get_message_key(&self, chain_index: u64) -> Option<&RemoteMessageKey> {
        self.inner.iter().find(|k| k.chain_index() == chain_index)
    }

    fn remove_message_key(&mut self, chain_index: u64) {
        self.inner.retain(|k| k.chain_index() != chain_index);
    }
}

impl Default for MessageKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

enum FoundMessageKey<'a> {
    Existing(&'a RemoteMessageKey),
    New(Box<(RemoteChainKey, MessageKeyStore, RemoteMessageKey)>),
}

impl FoundMessageKey<'_> {
    fn decrypt(
        &self,
        message: &Message,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, DecryptionError> {
        let message_key = match self {
            FoundMessageKey::Existing(m) => m,
            FoundMessageKey::New(m) => &m.2,
        };

        message_key.decrypt(message, associated_data)
    }
}

impl<'a> Debug for FoundMessageKey<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Existing(key) => f.debug_tuple("Existing").field(key).finish(),
            Self::New(found_key) => {
                let (_, _, key) = found_key.as_ref();
                f.debug_tuple("New").field(key).finish()
            }
        }
    }
}

/// The receiver side data on a single chain in the double ratchet.
///
/// Contains the data needed to decrypt a message sent to a given chain.
#[derive(Serialize, Deserialize, Clone)]
pub(super) struct ReceiverChain {
    /// The sender's ratchet key `T`<sub>`i`</sub> for this chain.
    ///
    /// We store this mostly so that we can identify which chain to use to
    /// decrypt a given message.
    ratchet_key: RemoteRatchetKey,

    /// The chain key `C`<sub>`i`,`j`</sub>.
    ///
    /// As we receive more messages on the chain, this is ratcheted forward,
    /// producing a new chain key `C`<sub>`i`,`j+1`</sub>, from which we can
    /// derive a new message key `M`<sub>`i`,`j+1`</sub>.
    hkdf_ratchet: RemoteChainKey,

    /// A stash of message keys which have not yet been used to decrypt a
    /// message.
    ///
    /// This allows us to handle out-of-order messages.
    skipped_message_keys: MessageKeyStore,
}

impl Debug for ReceiverChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            ratchet_key,
            hkdf_ratchet,
            skipped_message_keys,
        } = self;

        f.debug_struct("ReceiverChain")
            .field("ratchet_key", &ratchet_key)
            .field("chain_index", &hkdf_ratchet.chain_index())
            .field("skipped_message_keys", &skipped_message_keys.inner)
            .finish_non_exhaustive()
    }
}

impl ReceiverChain {
    pub(super) fn new(ratchet_key: RemoteRatchetKey, chain_key: RemoteChainKey) -> Self {
        ReceiverChain {
            ratchet_key,
            hkdf_ratchet: chain_key,
            skipped_message_keys: Default::default(),
        }
    }

    fn find_message_key(&self, chain_index: u64) -> Result<FoundMessageKey<'_>, DecryptionError> {
        let message_gap = chain_index.saturating_sub(self.hkdf_ratchet.chain_index());

        if message_gap > MAX_MESSAGE_GAP {
            Err(DecryptionError::TooBigMessageGap(
                message_gap,
                MAX_MESSAGE_GAP,
            ))
        } else if self.hkdf_ratchet.chain_index() > chain_index {
            self.skipped_message_keys
                .get_message_key(chain_index)
                .map(FoundMessageKey::Existing)
                .ok_or(DecryptionError::MissingMessageKey(chain_index))
        } else {
            let mut ratchet = self.hkdf_ratchet.clone();
            let mut skipped_keys = MessageKeyStore::new();

            // Advance the ratchet up until our desired point.
            while ratchet.chain_index() < chain_index {
                if chain_index - ratchet.chain_index() > MAX_MESSAGE_KEYS as u64 {
                    // If we're still too many messages away for us to save the
                    // skipped message keys just advance the
                    // ratchet by one index. This avoids the expansion of a
                    // message key we're going to throw away.
                    //
                    // NOTE: Messages that were encrypted with the chain index
                    // of this loop iteration will not have
                    // their message key anymore around. This does not mean
                    // that any messages new messages, following the message at
                    // `chain_index`, will be undecryptable.
                    ratchet.advance();
                } else {
                    // Otherwise advance the ratchet using the
                    // `create_message_key()` method and
                    // store the skipped key.
                    let key = ratchet.create_message_key();
                    skipped_keys.push(key);
                }
            }

            // Create now our desired message key
            let message_key = ratchet.create_message_key();

            Ok(FoundMessageKey::New(Box::new((
                ratchet,
                skipped_keys,
                message_key,
            ))))
        }
    }

    /// Cut the skipped keys to the limit of one chain. Used after a load.
    pub(super) fn trim(&mut self) {
        self.skipped_message_keys.trim();
    }

    /// The number of skipped message keys that this chain holds.
    pub(super) fn skipped_key_count(&self) -> usize {
        self.skipped_message_keys.len()
    }

    /// Drop the oldest skipped message key. Returns false if there is none.
    pub(super) fn drop_oldest_skipped_key(&mut self) -> bool {
        self.skipped_message_keys.drop_oldest()
    }

    /// Decrypt a message. A failure changes nothing in this chain: the new state is
    /// kept apart, and it replaces the old one only after the MAC and the padding are good.
    pub(super) fn decrypt(
        &mut self,
        message: &Message,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, DecryptionError> {
        let chain_index = u64::from(message.chain_index);
        let message_key = self.find_message_key(chain_index)?;

        let plaintext = message_key.decrypt(message, associated_data)?;

        match message_key {
            FoundMessageKey::Existing(m) => {
                let chain_index = m.chain_index();
                self.skipped_message_keys.remove_message_key(chain_index)
            }
            FoundMessageKey::New(m) => {
                let (ratchet, skipped_keys, _) = *m;

                self.hkdf_ratchet = ratchet;
                self.skipped_message_keys.merge(skipped_keys);
            }
        }

        Ok(plaintext)
    }

    pub(super) fn belongs_to(&self, ratchet_key: &RemoteRatchetKey) -> bool {
        &self.ratchet_key == ratchet_key
    }
}

#[cfg(test)]
mod test {
    use super::{MAX_MESSAGE_GAP, MessageKeyStore};
    use crate::ratchet::olm::session::{
        DecryptionError, ReceiverChain, chain_key::RemoteChainKey, message_key::RemoteMessageKey,
        ratchet::RemoteRatchetKey,
    };

    #[test]
    fn push_and_remove() {
        let mut store = MessageKeyStore::new();
        let key_bytes = *b"11111111111111111111111111111111";
        let chain_index: u64 = 1;
        let key = RemoteMessageKey::new(Box::new(key_bytes), chain_index);
        assert!(store.get_message_key(chain_index).is_none());
        store.push(key);
        let key = store.get_message_key(chain_index).unwrap();
        assert_eq!(key.key.as_ref(), &key_bytes);
        assert_eq!(key.index, chain_index);
        store.remove_message_key(chain_index);
        assert!(store.get_message_key(chain_index).is_none());
    }

    #[test]
    fn merge() {
        let mut store1 = MessageKeyStore::new();
        store1.push(RemoteMessageKey::new(Box::new([1u8; 32]), 1));
        let mut store2 = MessageKeyStore::new();
        store2.push(RemoteMessageKey::new(Box::new([2u8; 32]), 2));

        assert!(store1.get_message_key(2).is_none());
        store1.merge(store2);
        assert!(store1.get_message_key(1).is_some());
        assert!(store1.get_message_key(2).is_some());
    }

    #[test]
    fn the_store_drops_the_oldest_key_first() {
        let mut store = MessageKeyStore::new();
        for i in 0..(super::MAX_MESSAGE_KEYS as u64 + 5) {
            store.push(RemoteMessageKey::new(Box::new([0u8; 32]), i));
        }
        assert_eq!(store.len(), super::MAX_MESSAGE_KEYS);
        assert!(store.get_message_key(4).is_none());
        assert!(store.get_message_key(5).is_some());
        assert!(store.drop_oldest());
        assert!(store.get_message_key(5).is_none());
        assert!(store.get_message_key(6).is_some());
    }

    #[test]
    fn respect_max_gap() {
        let ratchet_key = RemoteRatchetKey::from([0u8; 32]);
        let chain_key = RemoteChainKey::new([0u8; 32].into());

        let receiver_chain = ReceiverChain::new(ratchet_key, chain_key);

        assert!(matches!(
            receiver_chain.find_message_key(MAX_MESSAGE_GAP + 1),
            Err(DecryptionError::TooBigMessageGap(max_invalid_gap, MAX_MESSAGE_GAP))
                if max_invalid_gap == MAX_MESSAGE_GAP + 1
        ));
        assert!(receiver_chain.find_message_key(MAX_MESSAGE_GAP).is_ok());
    }
}
