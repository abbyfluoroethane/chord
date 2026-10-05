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

mod curve25519;

pub use curve25519::{Curve25519PublicKey, Curve25519SecretKey};
use thiserror::Error;

/// Error type for failures that may occur when decoding or using a
/// cryptographic key.
#[derive(Error, Debug)]
pub enum KeyError {
    /// The key does not have the expected number of bytes.
    #[error("Invalid number of bytes for {key_type}, expected {expected_length}, got {length}.")]
    InvalidKeyLength {
        /// The type of key being decoded.
        key_type: &'static str,
        /// The expected length of the key.
        expected_length: usize,
        /// The actual length of the key.
        length: usize,
    },

    /// The random source of the system failed.
    #[error("The random source failed")]
    Rng,
}
