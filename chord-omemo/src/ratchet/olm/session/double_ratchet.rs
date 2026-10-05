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

use std::fmt::{Debug, Formatter};

use serde::{Deserialize, Serialize};

use super::{
    EncryptionError, RatchetError,
    chain_key::ChainKey,
    message_key::MessageKey,
    ratchet::{Ratchet, RatchetPublicKey, RemoteRatchetKey},
    receiver_chain::ReceiverChain,
    root_key::RemoteRootKey,
};
use crate::ratchet::olm::messages::Message;

/// The sender side of a double-ratchet implementation.
///
/// We always have a sending chain. We keep track of the latest chain key
/// `C`<sub>`i`,`j`</sub> (so that we can advance to the next one), and also the current
/// root key `R`<sub>`i`</sub> and our most recent ratchet key `T`<sub>`i`</sub> (so that we
/// can calculate the *next* root key).
///
/// When a message with a new ratchet key of the other side arrives, we make a new
/// sending chain at once, with a new ratchet key of ours: that is the "DH ratchet step" of
/// the Double Ratchet spec (section 3.5). vodozemac makes the sending chain only when
/// the next message goes out. That is a problem for OMEMO: a stored session that has
/// not seen our reply would not know the ratchet key that the other device uses for its next
/// chain. python-doubleratchet makes the chain at once (`DiffieHellmanRatchet.next_decryption_key`),
/// and so does Signal.
///
/// We don't handle decryption here (that's done in [`ReceiverChain`]).
#[derive(Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub(super) struct DoubleRatchet {
    inner: ActiveDoubleRatchet,
}

impl DoubleRatchet {
    pub(super) fn next_message_key(&mut self) -> MessageKey {
        self.inner.next_message_key()
    }

    pub(super) fn encrypt(
        &mut self,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> Result<Message, EncryptionError> {
        self.next_message_key().encrypt(plaintext, associated_data)
    }

    /// The public ratchet key of our sending chain.
    #[cfg(test)]
    pub(super) fn ratchet_key(&self) -> RatchetPublicKey {
        self.inner.ratchet_key()
    }

    /// Create a new `DoubleRatchet` instance for Alice, the side that starts the session.
    ///
    /// `root_key` is the shared secret SK of X3DH. `remote_ratchet_key` is the signed
    /// prekey of Bob. The Double Ratchet spec (section 3.3, "Initialization") says that
    /// Alice makes a new ratchet key pair and runs one root KDF step with it and the
    /// key of Bob: `RK, CKs = KDF_RK(SK, DH(DHs, DHr))`. XEP-0384 0.9, section 4.3
    /// ("ratchet initialization") says the same; python-doubleratchet does it in
    /// `DiffieHellmanRatchet.create` and `encrypt_initial_message`.
    pub(super) fn active(
        root_key: Box<[u8; 32]>,
        remote_ratchet_key: RemoteRatchetKey,
    ) -> Result<Self, RatchetError> {
        Self::from_remote_root_key(RemoteRootKey::new(root_key), remote_ratchet_key, 0)
    }

    /// Make a new sending chain from a root key of the other side and its ratchet key.
    /// `previous_chain_length` is the length of the sending chain that it replaces.
    pub(super) fn from_remote_root_key(
        root_key: RemoteRootKey,
        remote_ratchet_key: RemoteRatchetKey,
        previous_chain_length: u64,
    ) -> Result<Self, RatchetError> {
        let (root_key, chain_key, ratchet_key) = root_key.advance(&remote_ratchet_key)?;

        let ratchet = ActiveDoubleRatchet {
            parent_ratchet_key: Some(remote_ratchet_key),
            previous_chain_length,
            active_ratchet: Ratchet::new_with_ratchet_key(root_key, ratchet_key),
            symmetric_key_ratchet: chain_key,
        };

        Ok(Self { inner: ratchet })
    }

    /// A message with the new ratchet key `ratchet_key` of the other side arrived. Returns
    /// the ratchet with a new sending chain, and the receiver chain for the new key.
    /// `self` does not change.
    pub(super) fn advance(
        &self,
        ratchet_key: RemoteRatchetKey,
    ) -> Result<(DoubleRatchet, ReceiverChain), RatchetError> {
        let (remote_root_key, remote_chain) = self.inner.active_ratchet.advance(ratchet_key)?;

        let receiver_chain = ReceiverChain::new(ratchet_key, remote_chain);
        let sending = Self::from_remote_root_key(
            remote_root_key,
            ratchet_key,
            self.inner.symmetric_key_ratchet.index(),
        )?;

        Ok((sending, receiver_chain))
    }
}

impl Debug for DoubleRatchet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DoubleRatchet").field(&self.inner).finish()
    }
}

/// State of the sender-side ratchet: we are encrypting our own messages.
///
/// See [`DoubleRatchet`] for more explanation.
#[derive(Serialize, Deserialize, Clone)]
struct ActiveDoubleRatchet {
    /// The other side's most recent ratchet key, which was used to calculate
    /// the root key in `active_ratchet` and the chain key in
    /// `symmetric_key_ratchet`.
    ///
    /// This is not required to implement the algorithm: it is maintained solely
    /// for diagnostic output.
    parent_ratchet_key: Option<RemoteRatchetKey>,

    /// The length of the sending chain before this one, `PN` of the Double Ratchet spec.
    /// Every message of this chain sends it in its `pn` field.
    previous_chain_length: u64,

    active_ratchet: Ratchet,
    symmetric_key_ratchet: ChainKey,
}

impl ActiveDoubleRatchet {
    fn ratchet_key(&self) -> RatchetPublicKey {
        RatchetPublicKey::from(self.active_ratchet.ratchet_key())
    }

    fn next_message_key(&mut self) -> MessageKey {
        self.symmetric_key_ratchet
            .create_message_key(self.ratchet_key(), self.previous_chain_length)
    }
}

impl Debug for ActiveDoubleRatchet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let active_ratchet_public_key: RatchetPublicKey = self.active_ratchet.ratchet_key().into();
        f.debug_struct("ActiveDoubleRatchet")
            .field("parent_ratchet_key", &self.parent_ratchet_key)
            .field("ratchet_key", &active_ratchet_public_key)
            .field("chain_index", &self.symmetric_key_ratchet.index())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod test {
    use super::RemoteRatchetKey;
    use crate::ratchet::olm::session::testing::{message, pair};

    #[test]
    fn every_turn_makes_a_new_ratchet_key() {
        let (mut alice, mut bob) = pair();

        let alice_key = alice.sending_ratchet_key();
        let bob_key = bob.sending_ratchet_key();

        // Bob made his sending chain when he read the first message.
        let reply = message(&mut bob, b"sssh");
        assert_eq!(reply.ratchet_key, bob_key);
        alice.decrypt(&reply).unwrap();
        assert_ne!(alice.sending_ratchet_key(), alice_key);

        let again = message(&mut alice, b"sssh");
        assert_ne!(again.ratchet_key, alice_key);
        bob.decrypt(&again).unwrap();
        assert_ne!(bob.sending_ratchet_key(), bob_key);
    }

    #[test]
    fn pn_is_the_length_of_the_previous_chain() {
        let (mut alice, mut bob) = pair();
        // Alice sent one message in her first chain (the key exchange message).
        let first = message(&mut alice, b"2");
        assert_eq!((first.chain_index, first.previous_chain_length), (1, 0));
        let reply = message(&mut bob, b"r");
        alice.decrypt(&reply).unwrap();
        // Her second chain starts after two messages.
        let next = message(&mut alice, b"3");
        assert_eq!((next.chain_index, next.previous_chain_length), (0, 2));
        bob.decrypt(&next).unwrap();
    }

    #[test]
    fn advance_with_non_contributory_key_fails() {
        let (alice, _) = pair();
        assert!(
            alice
                .advance_with(RemoteRatchetKey::from([0u8; 32]))
                .is_err()
        );
    }
}
