//! OMEMO 2 end-to-end encryption (XEP-0384). See `docs/omemo-plan.md`.
//!
//! - `xml`: the stanza elements (device list, bundle, `<encrypted>`) and the XEP-0420
//!   envelope.
//! - `store`: the tables of keys, devices, sessions, and trust.

pub(crate) mod store;
pub(crate) mod xml;
