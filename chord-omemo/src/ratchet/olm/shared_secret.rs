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

//! An X3DH implementation following XEP-0384 0.9, section 4.2 (it was a 3DH
//! implementation following the Olm spec in vodozemac).
//!
//! The setup takes the identity keys of Alice and Bob (IK_A, IK_B), the signed prekey of
//! Bob (SPK_B), one one-time prekey of Bob (OPK_B), and an ephemeral key of Alice (EK_A).
//! All keys are Curve25519 keys here. The caller converts the Ed25519 identity keys: that
//! is the rule of the XEP (section 4.2, "XEdDSA" and "DH(PK1, PK2)").
//!
//! ```text
//!     DH1 = DH(IK_A, SPK_B)
//!     DH2 = DH(EK_A, IK_B)
//!     DH3 = DH(EK_A, SPK_B)
//!     DH4 = DH(EK_A, OPK_B)
//!
//!     SK = HKDF(salt = 32 zero bytes, ikm = 0xFF * 32 || DH1 || DH2 || DH3 || DH4,
//!               info = "OMEMO X3DH", length = 32)
//! ```
//!
//! The order of the DH values, the 32 bytes of 0xFF in front, the zero salt of the size of
//! the hash output, and the info string are in the X3DH spec (section 2.2, 3.3) and in
//! python-x3dh (`BaseState.get_shared_secret_active` and `get_shared_secret_passive`,
//! `padding = b"\xFF" * 32`, `salt = b"\x00" * hash_size`). The info string is in
//! python-twomemo (`StateImpl.INFO`) and the XEP (section 4.2, "info string").
//! The XEP says that a key exchange MUST use a one-time prekey: there is no three-DH
//! variant here.

use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::SharedSecret;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::ratchet::{
    Curve25519PublicKey as PublicKey, types::Curve25519SecretKey as StaticSecret,
};

/// The HKDF info of X3DH. Source: XEP-0384 0.9, section 4.2; python-twomemo `StateImpl.INFO`.
const INFO: &[u8] = b"OMEMO X3DH";

/// The shared secret SK of X3DH, 32 bytes. It is the first root key of the ratchet.
#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct X3dhSecret(Box<[u8; 32]>);

impl X3dhSecret {
    pub(crate) fn into_key(self) -> Box<[u8; 32]> {
        let mut key = Box::new([0u8; 32]);
        key.copy_from_slice(self.0.as_ref());
        key
    }
}

fn expand(
    dh1: SharedSecret,
    dh2: SharedSecret,
    dh3: SharedSecret,
    dh4: SharedSecret,
) -> X3dhSecret {
    // 32 bytes of 0xFF, then the four DH outputs.
    let mut input = Box::new([0xFFu8; 32 + 4 * 32]);
    input[32..64].copy_from_slice(dh1.as_bytes());
    input[64..96].copy_from_slice(dh2.as_bytes());
    input[96..128].copy_from_slice(dh3.as_bytes());
    input[128..160].copy_from_slice(dh4.as_bytes());

    let hkdf: Hkdf<Sha256> = Hkdf::new(Some(&[0u8; 32]), input.as_ref());
    let mut key = Box::new([0u8; 32]);

    #[allow(clippy::expect_used)]
    hkdf.expand(INFO, key.as_mut_slice())
        .expect("We should be able to expand the X3DH input into 32 bytes");

    input.zeroize();

    X3dhSecret(key)
}

impl X3dhSecret {
    /// The side of Alice, who starts the key exchange.
    ///
    /// Returns `None` if one of the DH values is not contributory.
    pub(crate) fn active(
        identity_key: &StaticSecret,
        ephemeral_key: &StaticSecret,
        remote_identity_key: &PublicKey,
        remote_signed_pre_key: &PublicKey,
        remote_one_time_key: &PublicKey,
    ) -> Option<Self> {
        let dh1 = identity_key.diffie_hellman(remote_signed_pre_key)?;
        let dh2 = ephemeral_key.diffie_hellman(remote_identity_key)?;
        let dh3 = ephemeral_key.diffie_hellman(remote_signed_pre_key)?;
        let dh4 = ephemeral_key.diffie_hellman(remote_one_time_key)?;

        Some(expand(dh1, dh2, dh3, dh4))
    }

    /// The side of Bob, who answers the key exchange.
    ///
    /// Returns `None` if one of the DH values is not contributory.
    pub(crate) fn passive(
        identity_key: &StaticSecret,
        signed_pre_key: &StaticSecret,
        one_time_key: &StaticSecret,
        remote_identity_key: &PublicKey,
        remote_ephemeral_key: &PublicKey,
    ) -> Option<Self> {
        let dh1 = signed_pre_key.diffie_hellman(remote_identity_key)?;
        let dh2 = identity_key.diffie_hellman(remote_ephemeral_key)?;
        let dh3 = signed_pre_key.diffie_hellman(remote_ephemeral_key)?;
        let dh4 = one_time_key.diffie_hellman(remote_ephemeral_key)?;

        Some(expand(dh1, dh2, dh3, dh4))
    }
}

#[cfg(test)]
mod test {
    use super::X3dhSecret;
    use crate::ratchet::{
        Curve25519PublicKey as PublicKey, types::Curve25519SecretKey as StaticSecret,
    };

    fn key() -> StaticSecret {
        StaticSecret::new().unwrap()
    }

    #[test]
    fn both_sides_get_the_same_secret() {
        let (alice_identity, alice_ephemeral) = (key(), key());
        let (bob_identity, bob_spk, bob_opk) = (key(), key(), key());

        let alice = X3dhSecret::active(
            &alice_identity,
            &alice_ephemeral,
            &PublicKey::from(&bob_identity),
            &PublicKey::from(&bob_spk),
            &PublicKey::from(&bob_opk),
        )
        .unwrap();

        let bob = X3dhSecret::passive(
            &bob_identity,
            &bob_spk,
            &bob_opk,
            &PublicKey::from(&alice_identity),
            &PublicKey::from(&alice_ephemeral),
        )
        .unwrap();

        assert_eq!(alice.into_key(), bob.into_key());
    }

    #[test]
    fn another_prekey_gives_another_secret() {
        let (alice_identity, alice_ephemeral) = (key(), key());
        let (bob_identity, bob_spk, bob_opk, other_opk) = (key(), key(), key(), key());

        let alice = X3dhSecret::active(
            &alice_identity,
            &alice_ephemeral,
            &PublicKey::from(&bob_identity),
            &PublicKey::from(&bob_spk),
            &PublicKey::from(&bob_opk),
        )
        .unwrap();
        let bob = X3dhSecret::passive(
            &bob_identity,
            &bob_spk,
            &other_opk,
            &PublicKey::from(&alice_identity),
            &PublicKey::from(&alice_ephemeral),
        )
        .unwrap();

        assert_ne!(alice.into_key(), bob.into_key());
    }

    #[test]
    fn non_contributory_keys_are_refused() {
        let (alice_identity, alice_ephemeral) = (key(), key());
        let (bob_identity, bob_spk, bob_opk) = (key(), key(), key());
        let zero = PublicKey::from_bytes([0u8; 32]);

        for (ik, spk, opk) in [
            (zero, PublicKey::from(&bob_spk), PublicKey::from(&bob_opk)),
            (
                PublicKey::from(&bob_identity),
                zero,
                PublicKey::from(&bob_opk),
            ),
            (
                PublicKey::from(&bob_identity),
                PublicKey::from(&bob_spk),
                zero,
            ),
        ] {
            assert!(
                X3dhSecret::active(&alice_identity, &alice_ephemeral, &ik, &spk, &opk).is_none()
            );
        }
        assert!(
            X3dhSecret::passive(
                &bob_identity,
                &bob_spk,
                &bob_opk,
                &zero,
                &PublicKey::from(&alice_ephemeral)
            )
            .is_none()
        );
        assert!(
            X3dhSecret::passive(
                &bob_identity,
                &bob_spk,
                &bob_opk,
                &PublicKey::from(&alice_identity),
                &zero
            )
            .is_none()
        );
    }
}
