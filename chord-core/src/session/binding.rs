//! The choice of the SCRAM channel binding (RFC 5802, RFC 9266, XEP-0440).
//!
//! The TLS layer gives at most one kind of binding data: `tls-exporter`, and only on TLS 1.3.
//! The sasl crate names SCRAM as `-PLUS` when it has binding data. So the binding decides
//! the mechanism. The rules:
//!
//! - The server offers none of the `-PLUS` mechanisms: send `y` (`Unsupported`), because the
//!   client can do binding and the server does not offer it. Without data, leave it as is.
//! - The server offers `-PLUS` for a hash that we have (`SCRAM-SHA-256-PLUS` or
//!   `SCRAM-SHA-1-PLUS`), and its `sasl-channel-binding` list is absent or has our type: use
//!   the binding.
//! - The server offers `-PLUS`, but we cannot use any of it (another hash, or a list with
//!   no `tls-exporter`): send `n` (`None`) and use plain SCRAM. Never `y`: a server that
//!   offered `-PLUS` must reject `y` as a downgrade (RFC 5802, 6).
//!
//! A server that offers a type that we have always gets binding. So this never turns
//! binding off for a server that supports it. The mechanism list and the feature list
//! come inside the TLS stream, so an attacker on the network cannot change them.

use std::collections::BTreeSet;

use sasl::common::ChannelBinding;
use xmpp_parsers::sasl_cb::{SaslChannelBinding, Type};

/// The `-PLUS` mechanisms that the sasl crate has.
const PLUS_MECHANISMS: [&str; 2] = ["SCRAM-SHA-256-PLUS", "SCRAM-SHA-1-PLUS"];

/// The channel binding for this login.
///
/// `offered` is the mechanism list of the server, and `listed` is its
/// `sasl-channel-binding` feature, if it sends one.
pub(super) fn choose_binding(
    tls: ChannelBinding,
    offered: &BTreeSet<String>,
    listed: Option<&SaslChannelBinding>,
) -> ChannelBinding {
    let any_plus = offered.iter().any(|m| m.ends_with("-PLUS"));
    let usable_plus = PLUS_MECHANISMS.iter().any(|m| offered.contains(*m));
    let supported = |kind: Type| listed.is_none_or(|l| l.types.contains(&kind));
    match tls {
        ChannelBinding::TlsExporter(_) | ChannelBinding::TlsUnique(_) if !any_plus => {
            ChannelBinding::Unsupported
        }
        ChannelBinding::TlsExporter(_) if !usable_plus || !supported(Type::TlsExporter) => {
            ChannelBinding::None
        }
        ChannelBinding::TlsUnique(_) if !usable_plus || !supported(Type::TlsUnique) => {
            ChannelBinding::None
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn listed(types: &[Type]) -> Option<SaslChannelBinding> {
        Some(SaslChannelBinding {
            types: types.to_vec(),
        })
    }

    fn exporter() -> ChannelBinding {
        ChannelBinding::TlsExporter(vec![1, 2, 3])
    }

    fn kind(binding: &ChannelBinding) -> &'static str {
        match binding {
            ChannelBinding::None => "none",
            ChannelBinding::Unsupported => "unsupported",
            ChannelBinding::TlsUnique(_) => "tls-unique",
            ChannelBinding::TlsExporter(_) => "tls-exporter",
        }
    }

    #[test]
    fn the_selection_table() {
        let plus = offered(&["SCRAM-SHA-256", "SCRAM-SHA-256-PLUS", "PLAIN"]);
        let plain = offered(&["SCRAM-SHA-256", "PLAIN"]);
        let other_plus = offered(&["SCRAM-SHA-256", "SCRAM-SHA-512-PLUS"]);
        let all = [Type::TlsUnique, Type::TlsServerEndPoint, Type::TlsExporter];
        let cases = [
            // (name, tls data, mechanisms, feature list, expected)
            ("exporter, no list", exporter(), &plus, None, "tls-exporter"),
            (
                "exporter, in list",
                exporter(),
                &plus,
                listed(&all),
                "tls-exporter",
            ),
            (
                "exporter, list has only tls-unique",
                exporter(),
                &plus,
                listed(&[Type::TlsUnique]),
                "none",
            ),
            (
                "exporter, list has tls-unique and end-point",
                exporter(),
                &plus,
                listed(&[Type::TlsUnique, Type::TlsServerEndPoint]),
                "none",
            ),
            (
                "exporter, empty list",
                exporter(),
                &plus,
                listed(&[]),
                "none",
            ),
            (
                "exporter, no PLUS offered",
                exporter(),
                &plain,
                None,
                "unsupported",
            ),
            (
                "exporter, no PLUS offered, list is there",
                exporter(),
                &plain,
                listed(&[Type::TlsExporter]),
                "unsupported",
            ),
            // A -PLUS mechanism that we do not have: binding would name a mechanism that the
            // server lacks, and `y` would look like a downgrade.
            (
                "exporter, only a foreign PLUS",
                exporter(),
                &other_plus,
                None,
                "none",
            ),
            (
                "no TLS data, PLUS offered",
                ChannelBinding::None,
                &plus,
                None,
                "none",
            ),
            (
                "no TLS data, no PLUS",
                ChannelBinding::None,
                &plain,
                None,
                "none",
            ),
            (
                "tls-unique, list has it",
                ChannelBinding::TlsUnique(vec![9]),
                &plus,
                listed(&[Type::TlsUnique]),
                "tls-unique",
            ),
            (
                "tls-unique, list lacks it",
                ChannelBinding::TlsUnique(vec![9]),
                &plus,
                listed(&[Type::TlsExporter]),
                "none",
            ),
        ];
        for (name, tls, mechanisms, list, expected) in cases {
            let got = choose_binding(tls, mechanisms, list.as_ref());
            assert_eq!(kind(&got), expected, "{name}");
        }
    }

    #[test]
    fn binding_data_survives_when_used() {
        let plus = offered(&["SCRAM-SHA-1-PLUS"]);
        let got = choose_binding(exporter(), &plus, listed(&[Type::TlsExporter]).as_ref());
        assert_eq!(got, exporter());
    }
}
