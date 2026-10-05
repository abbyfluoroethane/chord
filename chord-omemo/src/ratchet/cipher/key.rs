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

#[allow(deprecated)]
use aes::{
    Aes256,
    cipher::{Array, IvSizeUser, KeySizeUser},
};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::Aes256CbcEnc;

#[allow(deprecated)]
type Aes256Key = Array<u8, <Aes256 as KeySizeUser>::KeySize>;
#[allow(deprecated)]
type Aes256Iv = Array<u8, <Aes256CbcEnc as IvSizeUser>::IvSize>;
type HmacSha256Key = [u8; 32];

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct ExpandedKeys(Box<[u8; 80]>);

impl ExpandedKeys {
    /// The HKDF info of the message key expansion.
    /// Source: python-twomemo, `AEADImpl._get_info`; XEP-0384 0.9, section 4.3 (ENCRYPT).
    const OMEMO_HKDF_INFO: &'static [u8] = b"OMEMO Message Key Material";

    fn new(message_key: &[u8; 32]) -> Self {
        let mut expanded_keys = [0u8; 80];

        // The salt is 32 zero bytes, the size of the SHA-256 output.
        // Source: python-twomemo, `AEADImpl.__derive`; XEP-0384 0.9, section 4.3 (ENCRYPT).
        let hkdf: Hkdf<Sha256> = Hkdf::new(Some(&[0u8; 32]), message_key);

        #[allow(clippy::expect_used)]
        hkdf.expand(Self::OMEMO_HKDF_INFO, &mut expanded_keys)
            .expect("HKDF should be able to expand a 32 byte key into 80 bytes");

        Self(Box::new(expanded_keys))
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct CipherKeys {
    aes_key: Box<[u8; 32]>,
    aes_iv: Box<[u8; 16]>,
    mac_key: Box<[u8; 32]>,
}

impl CipherKeys {
    pub(crate) fn new(message_key: &[u8; 32]) -> Self {
        let expanded_keys = ExpandedKeys::new(message_key);

        Self::from_expanded_keys(expanded_keys)
    }

    pub(crate) fn from_expanded_keys(expanded_keys: ExpandedKeys) -> Self {
        let mut aes_key = Box::new([0u8; 32]);
        let mut mac_key = Box::new([0u8; 32]);
        let mut aes_iv = Box::new([0u8; 16]);

        // Split of the 80 bytes: 32 bytes encryption key, 32 bytes authentication key,
        // 16 bytes IV. Source: python-twomemo, `AEADImpl.__derive`.
        aes_key.copy_from_slice(&expanded_keys.0[0..32]);
        mac_key.copy_from_slice(&expanded_keys.0[32..64]);
        aes_iv.copy_from_slice(&expanded_keys.0[64..80]);

        Self {
            aes_key,
            aes_iv,
            mac_key,
        }
    }

    pub(crate) fn aes_key(&self) -> &Aes256Key {
        #[allow(deprecated)]
        Aes256Key::from_slice(self.aes_key.as_slice())
    }

    pub(crate) const fn mac_key(&self) -> &HmacSha256Key {
        &self.mac_key
    }

    pub(crate) fn iv(&self) -> &Aes256Iv {
        #[allow(deprecated)]
        Aes256Iv::from_slice(self.aes_iv.as_slice())
    }
}
