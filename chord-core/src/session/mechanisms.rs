//! The SASL mechanisms that a password login may use (RFC 7590, XEP-0440).
//!
//! `tokio_xmpp::client_login` tries SCRAM-SHA-256, SCRAM-SHA-1, PLAIN and ANONYMOUS in this
//! order, and takes the first one that the server lists. So it falls to PLAIN when the
//! server lists no SCRAM that the login can use, and to ANONYMOUS after that. We filter the
//! list first:
//!
//! - ANONYMOUS is out. A login with a password never wants an anonymous session.
//! - When the server lists a SCRAM variant that we can use, PLAIN is out. The login then
//!   uses SCRAM or fails. A server that hides SCRAM cannot push us down to PLAIN, and a
//!   failed SCRAM login never retries with PLAIN.
//!
//! "A SCRAM variant that we can use" depends on the channel binding that `choose_binding`
//! picked. With binding data the sasl crate names SCRAM as `-PLUS`, and without it as plain
//! SCRAM (sasl-0.5.2 src/client/mechanisms/scram.rs:103-108). A `-PLUS` name that we
//! cannot use does not count, so a server with `SCRAM-SHA-256-PLUS` and PLAIN only, and no
//! binding data, still gets PLAIN (over TLS, as always).

use std::collections::BTreeSet;

use sasl::common::ChannelBinding;

/// The SCRAM names that the login can send, for this channel binding.
fn scram_names(binding: &ChannelBinding) -> [&'static str; 2] {
    match binding {
        ChannelBinding::TlsExporter(_) | ChannelBinding::TlsUnique(_) => {
            ["SCRAM-SHA-256-PLUS", "SCRAM-SHA-1-PLUS"]
        }
        ChannelBinding::None | ChannelBinding::Unsupported => ["SCRAM-SHA-256", "SCRAM-SHA-1"],
    }
}

/// The mechanism list for `client_login`. `binding` is the choice of `choose_binding`.
pub(super) fn password_mechanisms(
    offered: &BTreeSet<String>,
    binding: &ChannelBinding,
) -> BTreeSet<String> {
    let mut mechanisms = offered.clone();
    mechanisms.remove("ANONYMOUS");
    if scram_names(binding).iter().any(|m| offered.contains(*m)) {
        mechanisms.remove("PLAIN");
    }
    mechanisms
}

#[cfg(test)]
mod tests {
    use sasl::client::Mechanism;
    use sasl::client::mechanisms::Scram;
    use sasl::common::Credentials;
    use sasl::common::scram::{Sha1, Sha256};

    use super::super::binding::choose_binding;
    use super::*;

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn exporter() -> ChannelBinding {
        ChannelBinding::TlsExporter(vec![1, 2, 3])
    }

    /// The mechanism that `client_login` picks: the first local one that the list holds.
    /// Same order as tokio-xmpp 6.0.0 src/client/login.rs:36-41.
    fn picked(binding: ChannelBinding, list: &BTreeSet<String>) -> Option<String> {
        let creds = Credentials::default()
            .with_username("alice")
            .with_password("secret")
            .with_channel_binding(binding);
        let scram256 = Scram::<Sha256>::from_credentials(creds.clone()).unwrap();
        let scram1 = Scram::<Sha1>::from_credentials(creds).unwrap();
        [scram256.name(), scram1.name(), "PLAIN", "ANONYMOUS"]
            .into_iter()
            .find(|name| list.contains(*name))
            .map(str::to_owned)
    }

    /// The whole choice for a server list and TLS data.
    fn login_choice(tls: ChannelBinding, offered: &[&str]) -> Option<String> {
        let offered = set(offered);
        let binding = choose_binding(tls, &offered, None);
        let list = password_mechanisms(&offered, &binding);
        picked(binding, &list)
    }

    #[test]
    fn a_server_with_scram_and_scram_plus_gets_binding() {
        // SECURITYAUTH-11: the server offers both. Chord has TLS 1.3 binding data.
        let got = login_choice(
            exporter(),
            &[
                "SCRAM-SHA-256",
                "SCRAM-SHA-256-PLUS",
                "SCRAM-SHA-1",
                "SCRAM-SHA-1-PLUS",
                "PLAIN",
                "ANONYMOUS",
            ],
        );
        assert_eq!(got.as_deref(), Some("SCRAM-SHA-256-PLUS"));
        // The same server, SHA-1 only.
        let got = login_choice(exporter(), &["SCRAM-SHA-1", "SCRAM-SHA-1-PLUS", "PLAIN"]);
        assert_eq!(got.as_deref(), Some("SCRAM-SHA-1-PLUS"));
    }

    #[test]
    fn no_binding_data_uses_plain_scram() {
        let got = login_choice(
            ChannelBinding::None,
            &["SCRAM-SHA-256", "SCRAM-SHA-256-PLUS", "PLAIN"],
        );
        assert_eq!(got.as_deref(), Some("SCRAM-SHA-256"));
    }

    #[test]
    fn a_server_without_plus_gets_scram_with_the_y_flag() {
        let got = login_choice(exporter(), &["SCRAM-SHA-256", "PLAIN"]);
        assert_eq!(got.as_deref(), Some("SCRAM-SHA-256"));
    }

    #[test]
    fn anonymous_is_never_offered_to_a_password_login() {
        let list = password_mechanisms(&set(&["ANONYMOUS"]), &ChannelBinding::None);
        assert!(list.is_empty());
        let got = login_choice(ChannelBinding::None, &["PLAIN", "ANONYMOUS"]);
        assert_eq!(got.as_deref(), Some("PLAIN"));
    }

    #[test]
    fn plain_is_out_when_the_server_offers_a_scram_that_we_can_use() {
        for (binding, offered) in [
            (ChannelBinding::None, vec!["SCRAM-SHA-1", "PLAIN"]),
            (exporter(), vec!["SCRAM-SHA-256-PLUS", "PLAIN"]),
        ] {
            let offered = set(&offered);
            let chosen = choose_binding(binding, &offered, None);
            let list = password_mechanisms(&offered, &chosen);
            assert!(!list.contains("PLAIN"), "{offered:?}");
            assert!(picked(chosen, &list).unwrap().starts_with("SCRAM"));
        }
    }

    #[test]
    fn plain_stays_when_no_scram_is_usable() {
        // Only a -PLUS name, and no binding data: SCRAM is not usable, and PLAIN stays.
        let got = login_choice(ChannelBinding::None, &["SCRAM-SHA-256-PLUS", "PLAIN"]);
        assert_eq!(got.as_deref(), Some("PLAIN"));
        // A hash that Chord does not have.
        let got = login_choice(ChannelBinding::None, &["SCRAM-SHA-512", "PLAIN"]);
        assert_eq!(got.as_deref(), Some("PLAIN"));
        // Nothing usable at all.
        assert_eq!(login_choice(ChannelBinding::None, &["EXTERNAL"]), None);
    }
}
