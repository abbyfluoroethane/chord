//! Presence (RFC 6121).

use xmpp_parsers::presence::Presence;

/// The initial presence after a new session (RFC 6121, 4.2). The server routes chat
/// messages to a resource only after it is available.
pub fn initial() -> Presence {
    Presence::available()
}

/// Whether to send initial presence after `Connected`. After a resumed stream the server
/// keeps the presence (XEP-0198), so a second one is not necessary.
pub fn needs_initial(resumed: bool) -> bool {
    !resumed
}
