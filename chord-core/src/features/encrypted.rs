//! Explicit message encryption (XEP-0380): a message that Chord cannot read.
//!
//! A client that sends an encrypted message adds an `<encryption namespace=.../>` element
//! and a fallback body like "I sent you an OMEMO encrypted message". Chord has no
//! encryption. `notice` gives the text that the timeline shows in place of that fallback,
//! and it names the method.

use xmpp_parsers::eme::ExplicitMessageEncryption;
use xmpp_parsers::message::Message;

/// The name of an encryption method from its XEP-0380 namespace.
fn method(namespace: &str, name: Option<&str>) -> String {
    match namespace {
        "urn:xmpp:omemo:2"
        | "urn:xmpp:omemo:1"
        | "urn:xmpp:omemo:0"
        | "eu.siacs.conversations.axolotl" => "OMEMO".into(),
        "urn:xmpp:otr:0" => "OTR".into(),
        "urn:xmpp:openpgp:0" | "jabber:x:encrypted" => "OpenPGP".into(),
        _ => match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => namespace.to_owned(),
        },
    }
}

/// The notice for a message that says it is encrypted (XEP-0380), or `None` for a plain
/// message. The notice replaces the fallback body, which only asks the reader to get a
/// client with support.
pub(crate) fn notice(message: &Message) -> Option<String> {
    let eme = message
        .payloads
        .iter()
        .find_map(|p| ExplicitMessageEncryption::try_from(p.clone()).ok())?;
    Some(format!(
        "Encrypted message ({}) that Chord cannot read.",
        method(&eme.namespace, eme.name.as_deref())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmpp_parsers::minidom::Element;

    fn message(eme: Option<&str>) -> Message {
        let mut m = Message::chat(None).with_body("".into(), "I sent you an OMEMO message".into());
        if let Some(xml) = eme {
            m.payloads.push(xml.parse::<Element>().unwrap());
        }
        m
    }

    #[test]
    fn names_the_method() {
        for (ns, name) in [
            ("urn:xmpp:omemo:2", "OMEMO"),
            ("eu.siacs.conversations.axolotl", "OMEMO"),
            ("urn:xmpp:otr:0", "OTR"),
            ("urn:xmpp:openpgp:0", "OpenPGP"),
            ("jabber:x:encrypted", "OpenPGP"),
        ] {
            let m = message(Some(&format!(
                "<encryption xmlns='urn:xmpp:eme:0' namespace='{ns}'/>"
            )));
            assert_eq!(
                notice(&m).as_deref(),
                Some(format!("Encrypted message ({name}) that Chord cannot read.").as_str())
            );
        }
    }

    #[test]
    fn an_unknown_method_uses_its_name_or_its_namespace() {
        let m = message(Some(
            "<encryption xmlns='urn:xmpp:eme:0' namespace='urn:x:y' name='SuperMechanism'/>",
        ));
        assert!(notice(&m).unwrap().contains("SuperMechanism"));
        let m = message(Some(
            "<encryption xmlns='urn:xmpp:eme:0' namespace='urn:x:y'/>",
        ));
        assert!(notice(&m).unwrap().contains("urn:x:y"));
    }

    #[test]
    fn a_plain_message_has_no_notice() {
        assert_eq!(notice(&message(None)), None);
    }

    #[test]
    fn the_notice_replaces_the_fallback_body_of_a_stored_message() {
        let m = message(Some(
            "<encryption xmlns='urn:xmpp:eme:0' namespace='urn:xmpp:omemo:2'/>",
        ));
        assert_eq!(
            crate::features::message_ext::body(&m).as_deref(),
            Some("Encrypted message (OMEMO) that Chord cannot read.")
        );
        // A message with no body and an encryption element stays without a row.
        let mut m = Message::chat(None);
        m.payloads.push(
            "<encryption xmlns='urn:xmpp:eme:0' namespace='urn:xmpp:omemo:2'/>"
                .parse()
                .unwrap(),
        );
        assert_eq!(crate::features::message_ext::body(&m), None);
    }
}
