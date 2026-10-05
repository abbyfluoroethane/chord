//! OMEMO 2 (XEP-0384 version 0.9.1, namespace `urn:xmpp:omemo:2`): a safe wrapper around
//! libomemo-c, with no XMPP and no I/O. See `docs/omemo-plan.md`.
//!
//! Chord does not write its own crypto. The protocol parts come from libomemo-c, the C
//! library that Dino and QXmpp use. We vendor it in `vendor/libomemo-c` and build it in
//! `build.rs`. libomemo-c does:
//!
//! - X3DH, the Double Ratchet, and the OMEMO 2 message formats (protocol version 4);
//! - key generation, and the signature of the signed prekey.
//!
//! libomemo-c asks the host for random, HMAC-SHA-256, SHA-512 and AES. `provider.rs` gives
//! them with one call each into RustCrypto crates.
//!
//! The one OMEMO 2 step that libomemo-c does not do is the encryption of the `<payload>`.
//! It is in `payload`, as a composition of RustCrypto calls. Read the note there.
//!
//! - [`identity`]: the keys of this device, and the bundle to publish.
//! - [`session`]: [`encrypt_key`] and [`decrypt_key`] for one `<key>` element.
//! - [`payload`]: the encryption of the `<payload>` element.
//! - [`aesgcm`]: the file encryption of XEP-0454 (not built yet).
//!
//! The caller keeps all state as bytes: [`Identity::to_bytes`] and the session records.
//! Every call reads its inputs and returns new state. It never changes an input. Trust is
//! not decided here. Every call returns the identity key of the other device, and the
//! caller decides.
//!
//! License: libomemo-c is GPL-3.0. A program that links this crate is a covered work. See
//! `vendor/libomemo-c/VENDOR.md`.

pub mod aesgcm;
mod ffi;
pub mod identity;
mod native;
pub mod payload;
mod provider;
pub mod session;

pub use identity::{Bundle, Identity, PREKEY_COUNT, PREKEY_MIN, fingerprint};
pub use session::{DecryptedKey, EncryptedKey, decrypt_key, encrypt_key};

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
    /// Another error of libomemo-c. The number is its result code.
    Library(i32),
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
            Error::Library(code) => write!(f, "libomemo-c error {code}"),
        }
    }
}

impl std::error::Error for Error {}
