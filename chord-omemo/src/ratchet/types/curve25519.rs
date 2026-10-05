// Copyright 2021 Denis Kasak, Damir Jelić
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

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, SharedSecret, StaticSecret};
use zeroize::Zeroize;

use super::KeyError;

/// Struct representing a Curve25519 secret key.
#[derive(Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Curve25519SecretKey(Box<StaticSecret>);

impl std::fmt::Debug for Curve25519SecretKey {
    // Never print a secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Curve25519SecretKey(..)")
    }
}

#[cfg(test)]
thread_local! {
    /// A tape of fixed "random" keys for the known-answer tests. `new` takes the next
    /// entry before it asks the operating system.
    pub(crate) static TEST_TAPE: std::cell::RefCell<std::collections::VecDeque<[u8; 32]>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

impl Curve25519SecretKey {
    /// Generate a new, random, Curve25519SecretKey. Fails if the random source fails.
    pub fn new() -> Result<Self, KeyError> {
        #[cfg(test)]
        if let Some(bytes) = TEST_TAPE.with(|t| t.borrow_mut().pop_front()) {
            return Ok(Self::from_slice(&bytes));
        }

        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| KeyError::Rng)?;
        let key = Self::from_slice(&bytes);
        bytes.zeroize();

        Ok(key)
    }

    /// Create a `Curve25519SecretKey` from the given slice of bytes.
    pub fn from_slice(bytes: &[u8; 32]) -> Self {
        // XXX: Passing in secret array as value.
        Self(Box::new(StaticSecret::from(*bytes)))
    }

    /// Perform a Diffie-Hellman key exchange between the given
    /// `Curve25519PublicKey` and this `Curve25519SecretKey` and return a shared
    /// secret.
    ///
    /// Returns `None` if one of the keys does not show contributory behavior
    /// resulting in an all-zero shared secret.
    pub fn diffie_hellman(&self, their_public_key: &Curve25519PublicKey) -> Option<SharedSecret> {
        let shared_secret = self.0.diffie_hellman(&their_public_key.inner);

        if shared_secret.was_contributory() {
            Some(shared_secret)
        } else {
            None
        }
    }

    /// Convert the `Curve25519SecretKey` to a byte array.
    ///
    /// **Note**: This creates a copy of the key which won't be zeroized, the
    /// caller of the method needs to make sure to zeroize the returned array.
    pub fn to_bytes(&self) -> Box<[u8; 32]> {
        let mut key = Box::new([0u8; 32]);
        let mut bytes = self.0.to_bytes();
        key.copy_from_slice(&bytes);

        bytes.zeroize();

        key
    }
}

/// Struct representing a Curve25519 public key.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Curve25519PublicKey {
    pub(crate) inner: PublicKey,
}

impl Curve25519PublicKey {
    /// The number of bytes a Curve25519 public key has.
    pub const LENGTH: usize = 32;

    /// Convert this public key to a byte array.
    #[inline]
    pub fn to_bytes(&self) -> [u8; Self::LENGTH] {
        self.inner.to_bytes()
    }

    /// View this public key as a byte array.
    #[inline]
    pub(crate) fn as_bytes_ref(&self) -> &[u8; Self::LENGTH] {
        self.inner.as_bytes()
    }

    /// Create a `Curve25519PublicKey` from a byte array.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            inner: PublicKey::from(bytes),
        }
    }

    /// Try to create a `Curve25519PublicKey` from a slice of bytes.
    pub fn from_slice(slice: &[u8]) -> Result<Curve25519PublicKey, KeyError> {
        let key_len = slice.len();

        if key_len == Self::LENGTH {
            let mut key = [0u8; Self::LENGTH];
            key.copy_from_slice(slice);

            Ok(Self::from(key))
        } else {
            Err(KeyError::InvalidKeyLength {
                key_type: "Curve25519",
                expected_length: Self::LENGTH,
                length: key_len,
            })
        }
    }

    /// The key as lowercase hex.
    pub fn to_hex(&self) -> String {
        self.inner
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

impl Display for Curve25519PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl std::fmt::Debug for Curve25519PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!("curve25519:{self}");
        <str as std::fmt::Debug>::fmt(&s, f)
    }
}

impl From<[u8; Self::LENGTH]> for Curve25519PublicKey {
    fn from(bytes: [u8; Self::LENGTH]) -> Curve25519PublicKey {
        Curve25519PublicKey {
            inner: PublicKey::from(bytes),
        }
    }
}

impl<'a> From<&'a Curve25519SecretKey> for Curve25519PublicKey {
    fn from(secret: &'a Curve25519SecretKey) -> Curve25519PublicKey {
        Curve25519PublicKey {
            inner: PublicKey::from(secret.0.as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Curve25519PublicKey, Curve25519SecretKey};

    #[test]
    fn from_slice_checks_the_length() {
        assert!(Curve25519PublicKey::from_slice(&[0u8; 31]).is_err());
        assert!(Curve25519PublicKey::from_slice(&[0u8; 33]).is_err());
        assert!(Curve25519PublicKey::from_slice(&[1u8; 32]).is_ok());
    }

    #[test]
    fn a_non_contributory_key_gives_no_secret() {
        let secret = Curve25519SecretKey::new().unwrap();
        assert!(
            secret
                .diffie_hellman(&Curve25519PublicKey::from_bytes([0u8; 32]))
                .is_none()
        );
    }

    #[test]
    fn debug_hides_the_secret() {
        let secret = Curve25519SecretKey::from_slice(&[7u8; 32]);
        assert_eq!(format!("{secret:?}"), "Curve25519SecretKey(..)");
    }

    #[test]
    fn both_sides_get_the_same_secret() {
        let a = Curve25519SecretKey::new().unwrap();
        let b = Curve25519SecretKey::new().unwrap();
        let ab = a.diffie_hellman(&Curve25519PublicKey::from(&b)).unwrap();
        let ba = b.diffie_hellman(&Curve25519PublicKey::from(&a)).unwrap();
        assert_eq!(ab.as_bytes(), ba.as_bytes());
    }
}
