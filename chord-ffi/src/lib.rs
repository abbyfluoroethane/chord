//! UniFFI bindings over the public API of `chord-core`.
//!
//! The foreign code creates a `ChordClient`. The client owns a tokio runtime and runs the
//! actor of `chord-core` on it. Async methods use the UniFFI async support. Events and
//! views reach the foreign code through listener traits: subscribe, and keep the returned
//! `Subscription` alive until you want the calls to stop.
//!
//! Generate Kotlin bindings with:
//! `cargo run -p chord-ffi --bin uniffi-bindgen -- generate --library
//! target/debug/libchord_ffi.so --language kotlin --out-dir <dir>`

mod client;
mod error;
mod types;

pub use client::*;
pub use error::ChordError;
pub use types::*;

pub use chord_core;

uniffi::setup_scaffolding!();
