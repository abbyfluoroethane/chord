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

use prost::Message as ProstMessage;

use super::DecodeError;
use crate::ratchet::{Curve25519PublicKey, cipher::MAC_LEN};

/// The longest `OMEMOAuthenticatedMessage` that we parse, in bytes. The key material of
/// an OMEMO message is 48 bytes, so a real message is about 130 bytes long.
pub(crate) const MAX_AUTHENTICATED_MESSAGE_LEN: usize = 2048;
/// The longest ciphertext that we accept in an `OMEMOMessage`, in bytes.
pub(crate) const MAX_CIPHERTEXT_LEN: usize = 1024;

/// Read a Curve25519 public key from the wire: 32 bytes, with the top bit clear.
///
/// X25519 ignores the top bit of the u-coordinate (RFC 7748, section 5), so a key with the
/// bit set would give the same shared secret as the key without it. A key that a real
/// client sends never has the bit set. We refuse it: no message has two valid encodings.
pub(super) fn parse_public_key(
    bytes: &[u8],
    field: &'static str,
) -> Result<Curve25519PublicKey, DecodeError> {
    let key = Curve25519PublicKey::from_slice(bytes).map_err(|_| DecodeError::BadLength(field))?;
    if key.as_bytes_ref()[31] & 0x80 != 0 {
        return Err(DecodeError::InvalidKey(field));
    }
    Ok(key)
}

/// The protobuf message `OMEMOMessage`. All fields are optional here, and
/// `Message::from_bytes` checks that the required ones are there: the prost derive
/// does not check `required` fields.
///
/// Source of the field numbers and types: XEP-0384 0.9, section 12 (Protobuf Schema) (the proto file of
/// namespace `urn:xmpp:omemo:2`), and the descriptor of python-twomemo (`twomemo.proto`):
/// `n` uint32 = 1, `pn` uint32 = 2, `dh_pub` bytes = 3, `ciphertext` bytes = 4 (optional).
#[derive(Clone, PartialEq, ProstMessage)]
struct ProtoOmemoMessage {
    #[prost(uint32, optional, tag = "1")]
    n: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pn: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "3")]
    dh_pub: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "4")]
    ciphertext: Option<Vec<u8>>,
}

/// The protobuf message `OMEMOAuthenticatedMessage`: `mac` bytes = 1, `message` bytes = 2
/// (the serialized `OMEMOMessage`). Same sources as above.
#[derive(Clone, PartialEq, ProstMessage)]
pub(super) struct ProtoAuthenticatedMessage {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub(super) mac: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub(super) message: Option<Vec<u8>>,
}

/// An encrypted OMEMO message: an `OMEMOAuthenticatedMessage` with the `OMEMOMessage` in
/// it.
///
/// It holds the header (`n`, `pn`, the ratchet key) and the ciphertext. The MAC covers
/// the associated data and `message_bytes`, the exact bytes of the `OMEMOMessage`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) ratchet_key: Curve25519PublicKey,
    /// `n`: the index of the message in its chain.
    pub(crate) chain_index: u32,
    /// `pn`: the length of the previous sending chain of the sender.
    pub(crate) previous_chain_length: u32,
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) mac: [u8; MAC_LEN],
    /// The serialized `OMEMOMessage`. The MAC covers these bytes.
    pub(crate) message_bytes: Vec<u8>,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("ratchet_key", &self.ratchet_key)
            .field("chain_index", &self.chain_index)
            .field("previous_chain_length", &self.previous_chain_length)
            .finish_non_exhaustive()
    }
}

impl Message {
    /// Make a message from its parts. The MAC is zero until `set_mac` sets it.
    pub(crate) fn new(
        ratchet_key: Curve25519PublicKey,
        chain_index: u32,
        previous_chain_length: u32,
        ciphertext: Vec<u8>,
    ) -> Self {
        let message_bytes = ProtoOmemoMessage {
            n: Some(chain_index),
            pn: Some(previous_chain_length),
            dh_pub: Some(ratchet_key.to_bytes().to_vec()),
            ciphertext: Some(ciphertext.clone()),
        }
        .encode_to_vec();

        Self {
            ratchet_key,
            chain_index,
            previous_chain_length,
            ciphertext,
            mac: [0u8; MAC_LEN],
            message_bytes,
        }
    }

    /// The bytes that the MAC covers: the associated data, then the `OMEMOMessage`.
    /// Source: python-twomemo, `AEADImpl.encrypt`; XEP-0384 0.9, section 4.3 (ENCRYPT).
    pub(crate) fn mac_bytes(&self, associated_data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(associated_data.len() + self.message_bytes.len());
        bytes.extend_from_slice(associated_data);
        bytes.extend_from_slice(&self.message_bytes);
        bytes
    }

    pub(crate) fn set_mac(&mut self, mac: [u8; MAC_LEN]) {
        self.mac = mac;
    }

    /// Encode the message as an `OMEMOAuthenticatedMessage`.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        ProtoAuthenticatedMessage {
            mac: Some(self.mac.to_vec()),
            message: Some(self.message_bytes.clone()),
        }
        .encode_to_vec()
    }

    /// Try to decode the given byte slice as an `OMEMOAuthenticatedMessage`.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() > MAX_AUTHENTICATED_MESSAGE_LEN {
            return Err(DecodeError::TooLong);
        }
        let outer = ProtoAuthenticatedMessage::decode(bytes)?;
        Self::from_proto(outer)
    }

    pub(super) fn from_proto(outer: ProtoAuthenticatedMessage) -> Result<Self, DecodeError> {
        let mac = outer.mac.ok_or(DecodeError::MissingField("mac"))?;
        let message_bytes = outer.message.ok_or(DecodeError::MissingField("message"))?;
        let mac: [u8; MAC_LEN] = mac.try_into().map_err(|_| DecodeError::BadLength("mac"))?;

        if message_bytes.len() > MAX_AUTHENTICATED_MESSAGE_LEN {
            return Err(DecodeError::TooLong);
        }
        let inner = ProtoOmemoMessage::decode(message_bytes.as_slice())?;
        let chain_index = inner.n.ok_or(DecodeError::MissingField("n"))?;
        let previous_chain_length = inner.pn.ok_or(DecodeError::MissingField("pn"))?;
        let dh_pub = inner.dh_pub.ok_or(DecodeError::MissingField("dh_pub"))?;
        let ciphertext = inner
            .ciphertext
            .ok_or(DecodeError::MissingField("ciphertext"))?;
        let ratchet_key = parse_public_key(&dh_pub, "dh_pub")?;

        if ciphertext.len() > MAX_CIPHERTEXT_LEN {
            return Err(DecodeError::TooLong);
        }

        Ok(Self {
            ratchet_key,
            chain_index,
            previous_chain_length,
            ciphertext,
            mac,
            message_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Message {
        let mut m = Message::new(
            Curve25519PublicKey::from_bytes([9u8; 32]),
            3,
            2,
            vec![5u8; 48],
        );
        m.set_mac([7u8; MAC_LEN]);
        m
    }

    #[test]
    fn round_trip() {
        let m = sample();
        let again = Message::from_bytes(&m.to_bytes()).unwrap();
        assert_eq!(m, again);
        assert_eq!(again.to_bytes(), m.to_bytes());
    }

    #[test]
    fn field_numbers_are_those_of_the_xep() {
        let m = sample();
        let outer = m.to_bytes();
        // mac: tag 1, length 16.
        assert_eq!(&outer[..2], &[0x0a, 16]);
        // message: tag 2.
        assert_eq!(outer[18], 0x12);
        // n: tag 1 (varint), pn: tag 2 (varint), dh_pub: tag 3, ciphertext: tag 4.
        assert_eq!(&m.message_bytes[..4], &[0x08, 3, 0x10, 2]);
        assert_eq!(m.message_bytes[4], 0x1a);
        assert_eq!(m.message_bytes[4 + 2 + 32], 0x22);
    }

    #[test]
    fn a_zero_counter_is_still_written() {
        // A required field is on the wire even when its value is 0.
        let m = Message::new(
            Curve25519PublicKey::from_bytes([1u8; 32]),
            0,
            0,
            vec![1u8; 16],
        );
        assert_eq!(&m.message_bytes[..4], &[0x08, 0, 0x10, 0]);
    }

    #[test]
    fn missing_fields_are_refused() {
        let no_mac = ProtoAuthenticatedMessage {
            mac: None,
            message: Some(vec![]),
        }
        .encode_to_vec();
        assert!(matches!(
            Message::from_bytes(&no_mac),
            Err(DecodeError::MissingField("mac"))
        ));
        let inner = ProtoOmemoMessage {
            n: Some(1),
            pn: None,
            dh_pub: Some(vec![0; 32]),
            ciphertext: Some(vec![0; 16]),
        };
        let outer = ProtoAuthenticatedMessage {
            mac: Some(vec![0; 16]),
            message: Some(inner.encode_to_vec()),
        };
        assert!(matches!(
            Message::from_bytes(&outer.encode_to_vec()),
            Err(DecodeError::MissingField("pn"))
        ));
    }

    #[test]
    fn bad_lengths_are_refused() {
        let mut m = sample();
        m.mac = [0; MAC_LEN];
        let mut outer = ProtoAuthenticatedMessage::decode(m.to_bytes().as_slice()).unwrap();
        outer.mac = Some(vec![0; 8]);
        assert!(matches!(
            Message::from_bytes(&outer.encode_to_vec()),
            Err(DecodeError::BadLength("mac"))
        ));
        let long = vec![0u8; MAX_AUTHENTICATED_MESSAGE_LEN + 1];
        assert!(matches!(
            Message::from_bytes(&long),
            Err(DecodeError::TooLong)
        ));
    }
}
