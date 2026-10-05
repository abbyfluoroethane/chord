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

use super::{
    DecodeError,
    message::{
        MAX_AUTHENTICATED_MESSAGE_LEN, Message, ProtoAuthenticatedMessage, parse_public_key,
    },
};
use crate::ratchet::Curve25519PublicKey;

/// The longest `OMEMOKeyExchange` that we parse, in bytes.
const MAX_KEY_EXCHANGE_LEN: usize = MAX_AUTHENTICATED_MESSAGE_LEN + 256;

/// The protobuf message `OMEMOKeyExchange`: `pk_id` uint32 = 1, `spk_id` uint32 = 2,
/// `ik` bytes = 3, `ek` bytes = 4, `message` OMEMOAuthenticatedMessage = 5.
/// Source: XEP-0384 0.9, section 12 (Protobuf Schema), and the descriptor of
/// python-twomemo. All fields are optional here, and `PreKeyMessage::from_bytes`
/// checks that they are there.
#[derive(Clone, PartialEq, ProstMessage)]
struct ProtoKeyExchange {
    #[prost(uint32, optional, tag = "1")]
    pk_id: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    spk_id: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "3")]
    ik: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "4")]
    ek: Option<Vec<u8>>,
    #[prost(message, optional, tag = "5")]
    message: Option<ProtoAuthenticatedMessage>,
}

/// An `OMEMOKeyExchange`: the data that Bob needs to build the session (the ids of
/// the prekey and of the signed prekey, the identity key and the ephemeral key of Alice),
/// and the first message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreKeyMessage {
    /// The id of the one-time prekey of Bob that Alice used.
    pub(crate) pre_key_id: u32,
    /// The id of the signed prekey of Bob that Alice used.
    pub(crate) signed_pre_key_id: u32,
    /// The Ed25519 identity key of Alice, as on the wire.
    pub(crate) identity_key: [u8; 32],
    /// The ephemeral key (`ek`) of Alice. A Curve25519 key.
    pub(crate) base_key: Curve25519PublicKey,
    /// The first message of Alice.
    pub(crate) message: Message,
}

impl PreKeyMessage {
    /// Encode the key exchange as an `OMEMOKeyExchange`.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        ProtoKeyExchange {
            pk_id: Some(self.pre_key_id),
            spk_id: Some(self.signed_pre_key_id),
            ik: Some(self.identity_key.to_vec()),
            ek: Some(self.base_key.to_bytes().to_vec()),
            message: Some(ProtoAuthenticatedMessage {
                mac: Some(self.message.mac.to_vec()),
                message: Some(self.message.message_bytes.clone()),
            }),
        }
        .encode_to_vec()
    }

    /// Try to decode the given byte slice as an `OMEMOKeyExchange`.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() > MAX_KEY_EXCHANGE_LEN {
            return Err(DecodeError::TooLong);
        }
        let proto = ProtoKeyExchange::decode(bytes)?;

        let pre_key_id = proto.pk_id.ok_or(DecodeError::MissingField("pk_id"))?;
        let signed_pre_key_id = proto.spk_id.ok_or(DecodeError::MissingField("spk_id"))?;
        let identity_key: [u8; 32] = proto
            .ik
            .ok_or(DecodeError::MissingField("ik"))?
            .try_into()
            .map_err(|_| DecodeError::BadLength("ik"))?;
        let ek = proto.ek.ok_or(DecodeError::MissingField("ek"))?;
        let base_key = parse_public_key(&ek, "ek")?;
        let message =
            Message::from_proto(proto.message.ok_or(DecodeError::MissingField("message"))?)?;

        Ok(Self {
            pre_key_id,
            signed_pre_key_id,
            identity_key,
            base_key,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratchet::cipher::MAC_LEN;

    fn sample() -> PreKeyMessage {
        let mut message = Message::new(
            Curve25519PublicKey::from_bytes([3u8; 32]),
            0,
            0,
            vec![4u8; 64],
        );
        message.set_mac([5u8; MAC_LEN]);
        PreKeyMessage {
            pre_key_id: 12,
            signed_pre_key_id: 1,
            identity_key: [1u8; 32],
            base_key: Curve25519PublicKey::from_bytes([2u8; 32]),
            message,
        }
    }

    #[test]
    fn round_trip() {
        let m = sample();
        let again = PreKeyMessage::from_bytes(&m.to_bytes()).unwrap();
        assert_eq!(m, again);
        assert_eq!(again.to_bytes(), m.to_bytes());
    }

    #[test]
    fn field_numbers_are_those_of_the_xep() {
        let bytes = sample().to_bytes();
        // pk_id = 1, spk_id = 2 (varints), ik = 3, ek = 4, message = 5.
        assert_eq!(&bytes[..4], &[0x08, 12, 0x10, 1]);
        assert_eq!(bytes[4], 0x1a);
        assert_eq!(bytes[4 + 2 + 32], 0x22);
        assert_eq!(bytes[4 + 2 + 32 + 2 + 32], 0x2a);
    }

    #[test]
    fn missing_and_bad_fields_are_refused() {
        let ok = ProtoKeyExchange::decode(sample().to_bytes().as_slice()).unwrap();
        for (name, mutate) in [
            (
                "pk_id",
                (|p: &mut ProtoKeyExchange| p.pk_id = None) as fn(&mut ProtoKeyExchange),
            ),
            ("spk_id", |p| p.spk_id = None),
            ("ik", |p| p.ik = None),
            ("ek", |p| p.ek = None),
            ("message", |p| p.message = None),
        ] {
            let mut p = ok.clone();
            mutate(&mut p);
            assert!(
                matches!(
                    PreKeyMessage::from_bytes(&p.encode_to_vec()),
                    Err(DecodeError::MissingField(n)) if n == name
                ),
                "{name}"
            );
        }
        let mut p = ok.clone();
        p.ik = Some(vec![0; 31]);
        assert!(matches!(
            PreKeyMessage::from_bytes(&p.encode_to_vec()),
            Err(DecodeError::BadLength("ik"))
        ));
        let mut p = ok;
        p.ek = Some(vec![0; 33]);
        assert!(matches!(
            PreKeyMessage::from_bytes(&p.encode_to_vec()),
            Err(DecodeError::BadLength("ek"))
        ));
    }
}
