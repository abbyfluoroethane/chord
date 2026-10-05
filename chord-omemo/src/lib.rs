//! OMEMO 2 (XEP-0384 version 0.9.1, namespace `urn:xmpp:omemo:2`) in pure Rust, with no
//! XMPP and no I/O. See `docs/omemo-plan.md`.
//!
//! The Double Ratchet comes from a fork of vodozemac (Apache-2.0), which was audited by Least
//! Authority in 2022. The fork is in `src/ratchet`. It changes the code for OMEMO 2: the key
//! agreement is X3DH, the KDF strings, the message crypto and the message formats are those
//! of XEP-0384. `src/ratchet/FORK.md` lists every change. The code is tested against
//! python-twomemo (see `tests/interop`). The changes need an outside review before the
//! feature is called stable.
//!
//! The primitives are RustCrypto and dalek crates. This crate has no cipher code of its own.
//!
//! - [`identity`]: the keys of this device, and the bundle to publish.
//! - [`session`]: [`encrypt_key`] and [`decrypt_key`] for one `<key>` element.
//! - [`payload`]: the encryption of the `<payload>` element.
//! - [`aesgcm`]: the file encryption of XEP-0454 (not built yet).
//!
//! The caller keeps all state as bytes: [`Identity::to_bytes`] and the session records. Every
//! call reads its inputs and returns new state. It never changes an input. Trust is not
//! decided here. Every call returns the identity key of the other device, and the caller
//! decides.

pub mod aesgcm;
pub mod identity;
pub mod payload;
mod ratchet;
pub mod session;

pub use identity::{Bundle, Identity, PREKEY_COUNT, PREKEY_MIN, fingerprint};
pub use session::{DecryptedKey, EncryptedKey, HEARTBEAT_COUNTER, decrypt_key, encrypt_key};

/// An error of the OMEMO layer. The text never holds key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The bytes do not parse: protobuf, key length, or a stored state.
    Malformed(&'static str),
    /// A signature is wrong, or a key is not valid.
    BadSignature,
    /// A MAC is wrong.
    BadMac,
    /// A key exchange names a prekey that we do not have, or that is used.
    UnknownPreKey(u32),
    /// A message that we read before (a replay), or a message key that is gone.
    Duplicate,
    /// There is no session, and no bundle to make one.
    NoSession,
    /// The random source failed.
    Rng,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Malformed(what) => write!(f, "malformed {what}"),
            Error::BadSignature => f.write_str("bad signature or invalid key"),
            Error::BadMac => f.write_str("bad MAC"),
            Error::UnknownPreKey(id) => write!(f, "unknown prekey {id}"),
            Error::Duplicate => f.write_str("duplicate message"),
            Error::NoSession => f.write_str("no session"),
            Error::Rng => f.write_str("random source failed"),
        }
    }
}

impl std::error::Error for Error {}
