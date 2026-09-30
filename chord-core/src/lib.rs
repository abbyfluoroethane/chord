//! Chord core: XMPP connection, protocol features, local storage, and view state.

pub mod actor;
pub mod features;
pub mod forms;
pub mod runtime;
pub mod session;
pub mod store;
pub mod views;

// Frontends use the same stanza and JID types as the core.
pub use jid;
pub use xmpp_parsers;

#[cfg(test)]
mod test_support;
