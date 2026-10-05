// Copyright 2021 Damir Jelić
// Copyright 2021 The Matrix.org Foundation C.I.C.
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

mod chain_key;
mod double_ratchet;
pub(crate) mod message_key;
pub(crate) mod ratchet;
mod receiver_chain;
mod root_key;

use std::fmt::Debug;

use aes::cipher::block_padding::Error as UnpadError;
use arrayvec::ArrayVec;
use double_ratchet::DoubleRatchet;
use hmac::digest::MacError;
use ratchet::RemoteRatchetKey;
use receiver_chain::{MAX_SESSION_MESSAGE_KEYS, ReceiverChain};
use root_key::RootKey;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

use super::{
    messages::{Message, PreKeyMessage},
    shared_secret::X3dhSecret,
};
use crate::ratchet::{Curve25519PublicKey, types::Curve25519SecretKey};

const MAX_RECEIVING_CHAINS: usize = 5;

/// The first byte of a pickled session. A new format gets a new number.
const PICKLE_VERSION: u8 = 1;
/// The biggest pickle that we read, in bytes.
const MAX_PICKLE_LEN: usize = 4 * 1024 * 1024;

/// Error type for decryption failures.
#[derive(Error, Debug)]
pub enum DecryptionError {
    /// The message authentication code of the message was invalid.
    #[error("Failed decrypting OMEMO message, invalid MAC: {0}")]
    InvalidMAC(#[from] MacError),
    /// The ciphertext of the message isn't padded correctly.
    #[error("Failed decrypting OMEMO message, invalid padding")]
    InvalidPadding(#[from] UnpadError),
    /// One or more keys lacked contributory behavior in the Diffie-Hellman
    /// operation, resulting in an insecure shared secret.
    #[error(
        "One or more keys lacked contributory behavior in the Diffie-Hellman operation, \
         resulting in an insecure shared secret"
    )]
    NonContributoryKey,
    /// The session is missing the correct message key to decrypt the message,
    /// either because it was already used up (a replay), or because the Session has
    /// been ratcheted forwards and the message key has been discarded.
    #[error("The message key with the given key can't be created, message index: {0}")]
    MissingMessageKey(u64),
    /// Too many messages have been skipped to attempt decrypting this message.
    #[error("The message gap was too big, got {0}, max allowed {1}")]
    TooBigMessageGap(u64, u64),
    /// The random source failed.
    #[error("The random source failed")]
    Rng,
}

/// Error type for encryption failures.
#[derive(Error, Debug)]
pub enum EncryptionError {
    /// One or more keys lacked contributory behavior in the Diffie-Hellman
    /// operation, resulting in an insecure shared secret.
    #[error(
        "One or more keys lacked contributory behavior in the Diffie-Hellman operation, \
         resulting in an insecure shared secret"
    )]
    NonContributoryKey,
    /// The random source failed.
    #[error("The random source failed")]
    Rng,
    /// The counter of the chain does not fit in the uint32 of the message.
    #[error("The sending chain is too long")]
    ChainTooLong,
}

/// An error from a step of the ratchet.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatchetError {
    /// One or more keys lacked contributory behavior in the Diffie-Hellman operation.
    #[error("A key was not contributory")]
    NonContributoryKey,
    /// The random source failed.
    #[error("The random source failed")]
    Rng,
}

impl From<RatchetError> for DecryptionError {
    fn from(e: RatchetError) -> Self {
        match e {
            RatchetError::NonContributoryKey => Self::NonContributoryKey,
            RatchetError::Rng => Self::Rng,
        }
    }
}

impl From<RatchetError> for EncryptionError {
    fn from(e: RatchetError) -> Self {
        match e {
            RatchetError::NonContributoryKey => Self::NonContributoryKey,
            RatchetError::Rng => Self::Rng,
        }
    }
}

/// An error from the build or the load of a session.
#[derive(Error, Debug)]
pub enum SessionError {
    /// A key lacked contributory behavior, or the random source failed.
    #[error(transparent)]
    Ratchet(#[from] RatchetError),
    /// The stored session does not parse.
    #[error("The stored session is not valid")]
    BadPickle,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChainStore {
    inner: ArrayVec<ReceiverChain, MAX_RECEIVING_CHAINS>,
}

impl ChainStore {
    fn new() -> Self {
        Self {
            inner: ArrayVec::new(),
        }
    }

    fn push(&mut self, ratchet: ReceiverChain) {
        if self.inner.is_full() {
            self.inner.pop_at(0);
        }

        self.inner.push(ratchet)
    }

    const fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn find_ratchet(&mut self, ratchet_key: &RemoteRatchetKey) -> Option<&mut ReceiverChain> {
        self.inner.iter_mut().find(|r| r.belongs_to(ratchet_key))
    }

    /// The number of skipped message keys in all chains.
    fn skipped_key_count(&self) -> usize {
        self.inner
            .iter()
            .map(ReceiverChain::skipped_key_count)
            .sum()
    }

    /// Keep at most `limit` skipped message keys. The oldest key goes first: the chains
    /// are in the order of their creation, and each chain keeps its keys in order.
    /// Source: XEP-0384 0.9, section 4.3 ("deletion policy for skipped message keys").
    fn enforce_key_limit(&mut self, limit: usize) {
        let mut count = self.skipped_key_count();
        while count > limit {
            if !self
                .inner
                .iter_mut()
                .any(ReceiverChain::drop_oldest_skipped_key)
            {
                break;
            }
            count -= 1;
        }
    }
}

impl Default for ChainStore {
    fn default() -> Self {
        Self::new()
    }
}

/// The data of the key exchange that Alice sends until Bob answers (XEP-0384 0.9,
/// section 4.2 and section 6, "Business Rules").
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct KeyExchangeInfo {
    pub(crate) pre_key_id: u32,
    pub(crate) signed_pre_key_id: u32,
    /// The Ed25519 identity key of Alice.
    pub(crate) identity_key: [u8; 32],
    /// The ephemeral key `ek` of Alice.
    pub(crate) base_key: Curve25519PublicKey,
}

/// The result of `Session::encrypt`.
pub(crate) struct Encrypted {
    /// An `OMEMOKeyExchange` when `kex` is true, else an `OMEMOAuthenticatedMessage`.
    pub(crate) data: Vec<u8>,
    pub(crate) kex: bool,
}

/// One end of an encrypted communication channel with one device: X3DH and the Double
/// Ratchet.
///
/// A session is made with [`Session::new_active`] (we start) or [`Session::new_passive`]
/// (we answer a key exchange).
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Session {
    /// `AD = Encode(IK_A) || Encode(IK_B)`: the Ed25519 identity keys of Alice, who
    /// started the key exchange, and of Bob. Both sides use the same bytes for all messages.
    /// Source: XEP-0384 0.9, section 4.2 ("associated data").
    associated_data: Vec<u8>,
    remote_identity_key: [u8; 32],
    sending_ratchet: DoubleRatchet,
    receiving_chains: ChainStore,
    /// Set on Alice's side until she decrypts a message of Bob. Every message goes out
    /// as a key exchange while it is set.
    key_exchange: Option<KeyExchangeInfo>,
    /// Set on Bob's side: the key exchange that made this session. A repeat of it (Alice
    /// sends it until she hears from Bob) is a normal message here, and no new session.
    built_from: Option<KeyExchangeInfo>,
}

impl Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            associated_data: _,
            remote_identity_key: _,
            sending_ratchet,
            receiving_chains,
            key_exchange,
            built_from,
        } = self;

        f.debug_struct("Session")
            .field("sending_ratchet", &sending_ratchet)
            .field("receiving_chains", &receiving_chains.inner)
            .field("key_exchange_pending", &key_exchange.is_some())
            .field("passive", &built_from.is_some())
            .finish_non_exhaustive()
    }
}

fn associated_data(initiator: &[u8; 32], responder: &[u8; 32]) -> Vec<u8> {
    let mut ad = Vec::with_capacity(64);
    ad.extend_from_slice(initiator);
    ad.extend_from_slice(responder);
    ad
}

impl Session {
    /// Start a session (the side of Alice).
    ///
    /// The keys of Bob come from his bundle. The caller checked the signature of the signed
    /// prekey. `own_identity_key` is our identity key in Curve25519 form.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_active(
        own_identity_key: &Curve25519SecretKey,
        own_identity_public_ed: [u8; 32],
        remote_identity_key_ed: [u8; 32],
        remote_identity_key: &Curve25519PublicKey,
        signed_pre_key: Curve25519PublicKey,
        signed_pre_key_id: u32,
        one_time_key: Curve25519PublicKey,
        one_time_key_id: u32,
    ) -> Result<Self, SessionError> {
        let ephemeral_key = Curve25519SecretKey::new().map_err(|_| RatchetError::Rng)?;
        let secret = X3dhSecret::active(
            own_identity_key,
            &ephemeral_key,
            remote_identity_key,
            &signed_pre_key,
            &one_time_key,
        )
        .ok_or(RatchetError::NonContributoryKey)?;

        let sending_ratchet =
            DoubleRatchet::active(secret.into_key(), RemoteRatchetKey::from(signed_pre_key))?;

        Ok(Self {
            associated_data: associated_data(&own_identity_public_ed, &remote_identity_key_ed),
            remote_identity_key: remote_identity_key_ed,
            sending_ratchet,
            receiving_chains: ChainStore::new(),
            key_exchange: Some(KeyExchangeInfo {
                pre_key_id: one_time_key_id,
                signed_pre_key_id,
                identity_key: own_identity_public_ed,
                base_key: Curve25519PublicKey::from(&ephemeral_key),
            }),
            built_from: None,
        })
    }

    /// Answer a key exchange (the side of Bob).
    ///
    /// This builds the session and its first receiver chain. The caller decrypts the first
    /// message with [`Session::decrypt`]. `remote_identity_key` is the Curve25519 form of
    /// the identity key in the key exchange.
    pub(crate) fn new_passive(
        own_identity_key: &Curve25519SecretKey,
        own_identity_public_ed: [u8; 32],
        signed_pre_key: &Curve25519SecretKey,
        one_time_key: &Curve25519SecretKey,
        remote_identity_key: &Curve25519PublicKey,
        key_exchange: &PreKeyMessage,
    ) -> Result<Self, SessionError> {
        let secret = X3dhSecret::passive(
            own_identity_key,
            signed_pre_key,
            one_time_key,
            remote_identity_key,
            &key_exchange.base_key,
        )
        .ok_or(RatchetError::NonContributoryKey)?;

        // The ratchet key of Alice is in her first message. Bob starts with the signed prekey
        // as his own ratchet key, and runs one root KDF step with it (XEP-0384 0.9, section
        // 4.3, "ratchet initialization"; python-doubleratchet `DiffieHellmanRatchet.create`).
        let remote_ratchet_key = RemoteRatchetKey::from(key_exchange.message.ratchet_key);
        let (root_key, remote_chain_key) = RootKey::new(secret.into_key()).advance(
            &ratchet::RatchetKey::from(signed_pre_key.clone()),
            &remote_ratchet_key,
        )?;

        // Bob makes his sending chain at once, with a new ratchet key (see `DoubleRatchet`).
        let sending_ratchet = DoubleRatchet::from_remote_root_key(root_key, remote_ratchet_key, 0)?;
        let receiver_chain = ReceiverChain::new(remote_ratchet_key, remote_chain_key);

        let mut receiving_chains = ChainStore::new();
        receiving_chains.push(receiver_chain);

        Ok(Self {
            associated_data: associated_data(&key_exchange.identity_key, &own_identity_public_ed),
            remote_identity_key: key_exchange.identity_key,
            sending_ratchet,
            receiving_chains,
            key_exchange: None,
            built_from: Some(KeyExchangeInfo {
                pre_key_id: key_exchange.pre_key_id,
                signed_pre_key_id: key_exchange.signed_pre_key_id,
                identity_key: key_exchange.identity_key,
                base_key: key_exchange.base_key,
            }),
        })
    }

    /// The Ed25519 identity key of the other device.
    pub(crate) const fn remote_identity_key(&self) -> [u8; 32] {
        self.remote_identity_key
    }

    /// True if this session was built from the key exchange `key_exchange`. Alice sends the
    /// same key exchange until she hears from Bob. Python-twomemo checks the same four
    /// values (`KeyExchangeImpl.builds_same_session`).
    pub(crate) fn built_from(&self, key_exchange: &PreKeyMessage) -> bool {
        self.built_from.as_ref().is_some_and(|b| {
            b.pre_key_id == key_exchange.pre_key_id
                && b.signed_pre_key_id == key_exchange.signed_pre_key_id
                && b.identity_key == key_exchange.identity_key
                && b.base_key == key_exchange.base_key
        })
    }

    /// Have we ever received and decrypted a message from the other side?
    ///
    /// Used to decide if outgoing messages should be sent as normal or key exchange messages.
    pub(crate) const fn has_received_message(&self) -> bool {
        !self.receiving_chains.is_empty()
    }

    /// Encrypt the `plaintext`. The result is a key exchange message until the other side
    /// answered, else a normal message.
    pub(crate) fn encrypt(&mut self, plaintext: &[u8]) -> Result<Encrypted, EncryptionError> {
        let message = self
            .sending_ratchet
            .encrypt(plaintext, &self.associated_data)?;

        match (&self.key_exchange, self.has_received_message()) {
            (Some(info), false) => {
                let key_exchange = PreKeyMessage {
                    pre_key_id: info.pre_key_id,
                    signed_pre_key_id: info.signed_pre_key_id,
                    identity_key: info.identity_key,
                    base_key: info.base_key,
                    message,
                };
                Ok(Encrypted {
                    data: key_exchange.to_bytes(),
                    kex: true,
                })
            }
            _ => Ok(Encrypted {
                data: message.to_bytes(),
                kex: false,
            }),
        }
    }

    /// Try to decrypt a message. A failure changes nothing in the session.
    pub(crate) fn decrypt(&mut self, message: &Message) -> Result<Vec<u8>, DecryptionError> {
        let ratchet_key = RemoteRatchetKey::from(message.ratchet_key);

        let plaintext = if let Some(ratchet) = self.receiving_chains.find_ratchet(&ratchet_key) {
            ratchet.decrypt(message, &self.associated_data)?
        } else {
            let (sending_ratchet, mut remote_ratchet) =
                self.sending_ratchet.advance(ratchet_key)?;

            let plaintext = remote_ratchet.decrypt(message, &self.associated_data)?;

            self.sending_ratchet = sending_ratchet;
            self.receiving_chains.push(remote_ratchet);

            plaintext
        };

        // The message is good: Bob answered, and Alice stops the key exchange.
        self.key_exchange = None;
        self.receiving_chains
            .enforce_key_limit(MAX_SESSION_MESSAGE_KEYS);

        Ok(plaintext)
    }

    /// The session as bytes: a version byte, then JSON. The bytes hold secret keys.
    pub(crate) fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(vec![PICKLE_VERSION]);
        // The types of the session serialize with no error.
        #[allow(clippy::expect_used)]
        serde_json::to_writer(&mut *out, self).expect("A session should serialize");
        out
    }

    /// Read a session from the bytes of [`Session::to_bytes`].
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, SessionError> {
        let Some((version, json)) = bytes.split_first() else {
            return Err(SessionError::BadPickle);
        };
        if *version != PICKLE_VERSION || bytes.len() > MAX_PICKLE_LEN {
            return Err(SessionError::BadPickle);
        }
        let mut session: Session =
            serde_json::from_slice(json).map_err(|_| SessionError::BadPickle)?;
        if session.associated_data.len() != 64 {
            return Err(SessionError::BadPickle);
        }
        for chain in &mut session.receiving_chains.inner {
            chain.trim();
        }
        session
            .receiving_chains
            .enforce_key_limit(MAX_SESSION_MESSAGE_KEYS);
        Ok(session)
    }
}

#[cfg(test)]
pub(super) mod testing {
    use super::*;
    use crate::ratchet::olm::messages::PreKeyMessage;

    impl Session {
        pub(super) fn sending_ratchet_key(&self) -> Curve25519PublicKey {
            *self.sending_ratchet.ratchet_key().as_ref()
        }

        pub(super) fn advance_with(&self, key: RemoteRatchetKey) -> Result<(), RatchetError> {
            self.sending_ratchet.advance(key).map(|_| ())
        }
    }

    /// Two sessions that did the first exchange: Alice sent a key exchange, Bob decrypted it.
    pub(in crate::ratchet) fn pair() -> (Session, Session) {
        let (alice_ik, bob_ik, bob_spk, bob_opk) = (
            Curve25519SecretKey::new().unwrap(),
            Curve25519SecretKey::new().unwrap(),
            Curve25519SecretKey::new().unwrap(),
            Curve25519SecretKey::new().unwrap(),
        );
        let (alice_ed, bob_ed) = ([1u8; 32], [2u8; 32]);
        let mut alice = Session::new_active(
            &alice_ik,
            alice_ed,
            bob_ed,
            &Curve25519PublicKey::from(&bob_ik),
            Curve25519PublicKey::from(&bob_spk),
            1,
            Curve25519PublicKey::from(&bob_opk),
            7,
        )
        .unwrap();

        let first = alice.encrypt(b"hello").unwrap();
        assert!(first.kex);
        let kex = PreKeyMessage::from_bytes(&first.data).unwrap();
        let mut bob = Session::new_passive(
            &bob_ik,
            bob_ed,
            &bob_spk,
            &bob_opk,
            &Curve25519PublicKey::from(&alice_ik),
            &kex,
        )
        .unwrap();
        assert_eq!(bob.decrypt(&kex.message).unwrap(), b"hello");
        (alice, bob)
    }

    /// Wrap the bytes of an encrypt call into a `Message`.
    pub(in crate::ratchet) fn message(session: &mut Session, text: &[u8]) -> Message {
        let out = session.encrypt(text).unwrap();
        if out.kex {
            PreKeyMessage::from_bytes(&out.data).unwrap().message
        } else {
            Message::from_bytes(&out.data).unwrap()
        }
    }
}

#[cfg(test)]
mod test {
    use super::{
        testing::{message, pair},
        *,
    };

    impl Session {
        fn skipped(&self) -> usize {
            self.receiving_chains.skipped_key_count()
        }
    }

    #[test]
    fn a_conversation_with_turns() {
        let (mut alice, mut bob) = pair();
        for round in 0..5 {
            let m = message(&mut bob, format!("b{round}").as_bytes());
            assert_eq!(alice.decrypt(&m).unwrap(), format!("b{round}").as_bytes());
            for k in 0..3 {
                let m = message(&mut alice, format!("a{round}{k}").as_bytes());
                assert_eq!(bob.decrypt(&m).unwrap(), format!("a{round}{k}").as_bytes());
            }
        }
    }

    #[test]
    fn key_exchange_goes_on_until_the_answer() {
        let (mut alice, mut bob) = pair();
        assert!(alice.encrypt(b"x").unwrap().kex);
        assert!(alice.encrypt(b"x").unwrap().kex);
        let m = message(&mut bob, b"answer");
        alice.decrypt(&m).unwrap();
        assert!(!alice.encrypt(b"x").unwrap().kex);
        // Bob never sends a key exchange.
        assert!(!bob.encrypt(b"y").unwrap().kex);
    }

    #[test]
    fn out_of_order_messages() {
        let (mut alice, mut bob) = pair();
        let m: Vec<_> = (0..6).map(|i| message(&mut alice, &[i])).collect();
        for i in [3usize, 1, 5, 0, 2, 4] {
            assert_eq!(bob.decrypt(&m[i]).unwrap(), vec![i as u8]);
        }
        assert_eq!(bob.skipped(), 0);
    }

    #[test]
    fn out_of_order_across_a_turn() {
        let (mut alice, mut bob) = pair();
        let first: Vec<_> = (0..3).map(|i| message(&mut alice, &[i])).collect();
        // Bob gets only the last message, then answers, then Alice sends in a new chain.
        bob.decrypt(&first[2]).unwrap();
        let answer = message(&mut bob, b"answer");
        alice.decrypt(&answer).unwrap();
        let next = message(&mut alice, b"next");
        assert_eq!(bob.decrypt(&next).unwrap(), b"next");
        // The late messages of the old chain still decrypt.
        assert_eq!(bob.decrypt(&first[0]).unwrap(), vec![0]);
        assert_eq!(bob.decrypt(&first[1]).unwrap(), vec![1]);
    }

    #[test]
    fn a_replay_is_refused() {
        let (mut alice, mut bob) = pair();
        let m = message(&mut alice, b"once");
        bob.decrypt(&m).unwrap();
        assert!(matches!(
            bob.decrypt(&m),
            Err(DecryptionError::MissingMessageKey(_))
        ));
        // Also after a gap.
        let late = message(&mut alice, b"late");
        let later = message(&mut alice, b"later");
        bob.decrypt(&later).unwrap();
        bob.decrypt(&late).unwrap();
        assert!(matches!(
            bob.decrypt(&late),
            Err(DecryptionError::MissingMessageKey(_))
        ));
    }

    #[test]
    fn the_gap_limit() {
        let (mut alice, mut bob) = pair();
        let mut last = message(&mut alice, b"x");
        // Skip exactly MAX_MESSAGE_GAP keys: ok.
        for _ in 0..receiver_chain::MAX_MESSAGE_GAP {
            last = message(&mut alice, b"x");
        }
        assert_eq!(bob.decrypt(&last).unwrap(), b"x");
        assert_eq!(bob.skipped(), receiver_chain::MAX_MESSAGE_GAP as usize);

        // One more than the limit is refused, and the session stays as it was.
        let (mut alice, mut bob) = pair();
        let mut last = message(&mut alice, b"x");
        for _ in 0..=receiver_chain::MAX_MESSAGE_GAP {
            last = message(&mut alice, b"x");
        }
        assert!(matches!(
            bob.decrypt(&last),
            Err(DecryptionError::TooBigMessageGap(g, _)) if g == receiver_chain::MAX_MESSAGE_GAP + 1
        ));
        assert_eq!(bob.skipped(), 0);
        let before = bob.to_bytes();
        let _ = bob.decrypt(&last);
        assert_eq!(*before, *bob.to_bytes());
    }

    #[test]
    fn the_session_keeps_at_most_the_limit_and_drops_the_oldest() {
        let (mut alice, mut bob) = pair();
        // Skip 900 keys, then 900 more in the next message: 1800 would be stored.
        let mut msgs = Vec::new();
        for _ in 0..901 {
            msgs.push(message(&mut alice, b"x"));
        }
        bob.decrypt(msgs.last().unwrap()).unwrap();
        assert_eq!(bob.skipped(), 900);
        // A turn: Bob answers, Alice starts a new chain, 900 more are skipped there.
        let answer = message(&mut bob, b"a");
        alice.decrypt(&answer).unwrap();
        let mut more = Vec::new();
        for _ in 0..901 {
            more.push(message(&mut alice, b"y"));
        }
        bob.decrypt(more.last().unwrap()).unwrap();
        assert_eq!(bob.skipped(), MAX_SESSION_MESSAGE_KEYS);
        // The oldest keys are gone: message 0 and 1 of the first chain (index 0 was the
        // message that made the session). The newest chain keeps all of its 900.
        assert!(matches!(
            bob.decrypt(&msgs[1]),
            Err(DecryptionError::MissingMessageKey(_))
        ));
        assert_eq!(bob.decrypt(&more[0]).unwrap(), b"y");
    }

    #[test]
    fn a_failed_decrypt_changes_nothing() {
        let (mut alice, mut bob) = pair();
        let good = message(&mut alice, b"good");
        let before = bob.to_bytes();

        let mut bad = good.clone();
        bad.ciphertext[0] ^= 1;
        assert!(bob.decrypt(&bad).is_err());
        assert_eq!(*before, *bob.to_bytes());

        let mut bad = good.clone();
        bad.mac[0] ^= 1;
        assert!(bob.decrypt(&bad).is_err());
        assert_eq!(*before, *bob.to_bytes());

        // A message with a new ratchet key and a bad MAC changes nothing either.
        let answer = message(&mut bob, b"a");
        alice.decrypt(&answer).unwrap();
        let mut next = message(&mut alice, b"n");
        let before = bob.to_bytes();
        next.mac[3] ^= 1;
        assert!(bob.decrypt(&next).is_err());
        assert_eq!(*before, *bob.to_bytes());

        assert_eq!(bob.decrypt(&good).unwrap(), b"good");
    }

    #[test]
    fn a_wrong_header_fails_the_mac() {
        let (mut alice, mut bob) = pair();
        let m = message(&mut alice, b"hello");
        // n, pn and the ratchet key are under the MAC.
        for tamper in [
            (|m: &mut Message| m.message_bytes[1] ^= 1) as fn(&mut Message),
            |m| m.message_bytes[3] ^= 1,
            |m| m.message_bytes[6] ^= 1,
        ] {
            let mut t = m.clone();
            tamper(&mut t);
            // Parse again as a receiver would.
            let wire = {
                let mut mac_copy = t.clone();
                mac_copy.mac = m.mac;
                mac_copy.to_bytes()
            };
            if let Ok(parsed) = Message::from_bytes(&wire) {
                assert!(bob.decrypt(&parsed).is_err());
            }
        }
        assert_eq!(bob.decrypt(&m).unwrap(), b"hello");
    }

    #[test]
    fn the_pickle_round_trips() {
        let (mut alice, mut bob) = pair();
        let m = message(&mut alice, b"one");
        let m2 = message(&mut alice, b"two");
        bob.decrypt(&m2).unwrap();

        let mut bob2 = Session::from_bytes(&bob.to_bytes()).unwrap();
        assert_eq!(*bob.to_bytes(), *bob2.to_bytes());
        assert_eq!(bob2.decrypt(&m).unwrap(), b"one");
        // Alice too, with a pending key exchange.
        let mut alice2 = Session::from_bytes(&alice.to_bytes()).unwrap();
        assert!(alice2.encrypt(b"x").unwrap().kex);
    }

    #[test]
    fn a_bad_pickle_is_refused() {
        let (alice, _) = pair();
        let bytes = alice.to_bytes();
        assert!(Session::from_bytes(&[]).is_err());
        assert!(Session::from_bytes(&[2]).is_err());
        assert!(Session::from_bytes(&bytes[..bytes.len() - 3]).is_err());
        let mut wrong_version = bytes.to_vec();
        wrong_version[0] = 9;
        assert!(Session::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn debug_shows_no_secret() {
        let (alice, _) = pair();
        let text = format!("{alice:?}");
        assert!(!text.contains("key: ["), "{text}");
    }
}
