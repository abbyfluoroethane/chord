//! OMEMO 2 (XEP-0384 version 0.9.1, namespace `urn:xmpp:omemo:2`): the protocol parts
//! with no XMPP and no I/O. See `docs/omemo-plan.md`.
//!
//! - `keys`: identity key, signed prekey, one-time prekeys, and the fingerprint.
//! - `proto`: the protobuf messages of the XEP (OMEMOMessage, OMEMOAuthenticatedMessage,
//!   OMEMOKeyExchange).
//! - `x3dh`: the key agreement for a new session.
//! - `ratchet`: the Double Ratchet, with the skipped message keys.
//! - `session`: one session with one remote device. It builds and reads the bytes of a
//!   `<key>` element.
//! - `payload`: the encryption of the `<payload>` (HKDF, AES-256-CBC, HMAC-SHA-256).
//! - `aesgcm`: the file encryption of XEP-0454.
//!
//! The caller keeps all state. Each state type turns into bytes and back, so the core can
//! store it.

pub mod aesgcm;
pub mod keys;
pub mod payload;
pub mod proto;
pub mod ratchet;
pub mod session;
pub mod x3dh;

/// An error of the OMEMO layer. The text never holds key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The bytes do not parse: protobuf, key length, or a stored state.
    Malformed(&'static str),
    /// A signature or a MAC is wrong.
    BadSignature,
    BadMac,
    /// A key exchange names a prekey that we do not have, or that is used.
    UnknownPreKey(u32),
    UnknownSignedPreKey(u32),
    /// The message needs more skipped keys than the limit allows.
    TooManySkipped,
    /// A message that we read before (a replay), or a message key that is gone.
    Duplicate,
    /// The random source failed.
    Rng,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Malformed(what) => write!(f, "malformed {what}"),
            Error::BadSignature => f.write_str("bad signature"),
            Error::BadMac => f.write_str("bad MAC"),
            Error::UnknownPreKey(id) => write!(f, "unknown prekey {id}"),
            Error::UnknownSignedPreKey(id) => write!(f, "unknown signed prekey {id}"),
            Error::TooManySkipped => f.write_str("too many skipped messages"),
            Error::Duplicate => f.write_str("duplicate message"),
            Error::Rng => f.write_str("random source failed"),
        }
    }
}

impl std::error::Error for Error {}
