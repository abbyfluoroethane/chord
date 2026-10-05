//! A fork of the Double Ratchet code of vodozemac (Apache-2.0), changed for OMEMO 2.
//! Read `FORK.md` in this folder for the upstream version and for every change.
//!
//! The crate uses this code through `crate::session` and `crate::identity`. Nothing here
//! is public API.

pub(crate) mod cipher;
pub(crate) mod olm;
pub(crate) mod types;

pub(crate) use types::{Curve25519PublicKey, Curve25519SecretKey};
