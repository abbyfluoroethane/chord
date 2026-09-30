//! An opt-in pin of the server certificate (SECURITYAUTH-13, RFC 7590).
//!
//! The pin is the SHA-256 fingerprint of the end-entity certificate of the server. It
//! comes on top of the normal validation: a certificate must pass the root store and the
//! JID domain check first, and then match the pin. It does not replace them.
//!
//! The use is trust on first use. The first connection has no pin. The session reports the
//! fingerprint that it saw (`observed`), the UI shows it, and the user stores it. Later
//! connections carry the stored pin. A server that shows another certificate is refused
//! with a `ConnectError::TlsInvalid` that names both fingerprints. The user has to accept
//! the new fingerprint before any password goes out: the check runs at the TLS handshake,
//! before SASL.

use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

/// The SHA-256 fingerprint of DER bytes, as 32 upper-case hex pairs with colons:
/// `AB:CD:...`. It is the form that browsers and `openssl x509 -fingerprint` show.
pub fn fingerprint(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    let mut out = String::with_capacity(32 * 3 - 1);
    for (i, byte) in digest.iter().enumerate() {
        if i > 0 {
            out.push(':');
        }
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

/// Read a fingerprint that a person typed or pasted: 64 hex digits, with or without
/// colons, spaces or dashes, in any case. Returns the normal form of `fingerprint`.
pub fn parse_fingerprint(text: &str) -> Option<String> {
    let digits: Vec<char> = text
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-' | '\t'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if digits.len() != 64 || !digits.iter().all(char::is_ascii_hexdigit) {
        return None;
    }
    let pairs: Vec<String> = digits.chunks(2).map(|p| p.iter().collect()).collect();
    Some(pairs.join(":"))
}

/// The pin of one account. A clone shares `observed`, so the session that runs with a
/// clone reports to the caller that made the pin.
#[derive(Clone, Debug, Default)]
pub struct CertPin {
    expected: Option<String>,
    /// The user chose to trust this one certificate, although the normal validation refuses
    /// it (a self-signed server). See `CertPin::trusting`.
    trust_untrusted: bool,
    observed: Arc<Mutex<Option<String>>>,
}

impl CertPin {
    /// A pin. `expected` is the stored fingerprint, or `None` for the first connection,
    /// which only learns the fingerprint. A fingerprint that does not parse is an error.
    pub fn new(expected: Option<&str>) -> Result<Self, String> {
        let expected = match expected.map(str::trim).filter(|e| !e.is_empty()) {
            Some(text) => Some(
                parse_fingerprint(text)
                    .ok_or_else(|| format!("not a SHA-256 fingerprint: {text:?}"))?,
            ),
            None => None,
        };
        Ok(Self {
            expected,
            trust_untrusted: false,
            observed: Arc::default(),
        })
    }

    /// A pin that also lets this one certificate pass when the normal validation refuses it,
    /// for a self-signed server (CORESESSION-15). The user must have seen the fingerprint and
    /// agreed. `fingerprint` is required. Exactly that end-entity certificate passes, and
    /// nothing else: the handshake signatures, and the rule that a changed certificate stops
    /// the connection, stay as they are. Never call it with a fingerprint that the user did
    /// not confirm.
    pub fn trusting(fingerprint: &str) -> Result<Self, String> {
        if fingerprint.trim().is_empty() {
            return Err("a trusted certificate needs a fingerprint".to_owned());
        }
        let mut pin = Self::new(Some(fingerprint))?;
        pin.trust_untrusted = true;
        Ok(pin)
    }

    /// Whether `der` is the one certificate that the user chose to trust although the normal
    /// validation refuses it. False for a pin that has no such choice.
    pub fn trusts_untrusted(&self, der: &[u8]) -> bool {
        self.trust_untrusted && self.expected.as_deref() == Some(fingerprint(der).as_str())
    }

    /// The fingerprint that the last handshake showed, if there was one.
    pub fn observed(&self) -> Option<String> {
        self.observed.lock().ok()?.clone()
    }

    /// The check for the certificate of the server. It records the fingerprint, and it fails
    /// if a stored fingerprint differs.
    pub fn check(&self, der: &[u8]) -> Result<(), String> {
        let seen = fingerprint(der);
        if let Ok(mut slot) = self.observed.lock() {
            *slot = Some(seen.clone());
        }
        match &self.expected {
            Some(expected) if *expected != seen => Err(format!(
                "the certificate of the server changed: pinned {expected}, the server showed {seen}"
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_is_sha256_in_colon_hex() {
        // SHA-256 of the empty input.
        assert_eq!(
            fingerprint(b""),
            "E3:B0:C4:42:98:FC:1C:14:9A:FB:F4:C8:99:6F:B9:24:27:AE:41:E4:64:9B:93:4C:A4:95:99:1B:78:52:B8:55"
        );
    }

    #[test]
    fn a_typed_fingerprint_has_many_forms() {
        let normal = fingerprint(b"x");
        let lower = normal.to_ascii_lowercase();
        let bare: String = normal.split(':').collect();
        assert_eq!(parse_fingerprint(&normal).as_deref(), Some(normal.as_str()));
        assert_eq!(parse_fingerprint(&lower).as_deref(), Some(normal.as_str()));
        assert_eq!(parse_fingerprint(&bare).as_deref(), Some(normal.as_str()));
        assert_eq!(
            parse_fingerprint(&format!("  {} ", normal.replace(':', " "))).as_deref(),
            Some(normal.as_str())
        );
        for bad in [
            "",
            "abc",
            &bare[..63],
            &format!("{bare}0"),
            &bare.replace('A', "G"),
        ] {
            // Only a string with a changed digit class or length must fail.
            if bad.len() != 64 || bad.contains('G') {
                assert_eq!(parse_fingerprint(bad), None, "{bad}");
            }
        }
    }

    #[test]
    fn the_first_connection_learns_and_accepts() {
        let pin = CertPin::new(None).unwrap();
        assert_eq!(pin.observed(), None);
        assert!(pin.check(b"cert one").is_ok());
        assert_eq!(pin.observed(), Some(fingerprint(b"cert one")));
    }

    #[test]
    fn a_pinned_certificate_passes_and_another_fails() {
        let stored = fingerprint(b"cert one");
        let pin = CertPin::new(Some(&stored.to_ascii_lowercase())).unwrap();
        assert!(pin.check(b"cert one").is_ok());
        let error = pin.check(b"cert two").unwrap_err();
        assert!(error.contains(&stored), "{error}");
        assert!(error.contains(&fingerprint(b"cert two")), "{error}");
        // The pin still reports what it saw, so the UI can offer the new fingerprint.
        assert_eq!(pin.observed(), Some(fingerprint(b"cert two")));
    }

    #[test]
    fn a_clone_reports_to_the_original() {
        let pin = CertPin::new(None).unwrap();
        let copy = pin.clone();
        copy.check(b"c").unwrap();
        assert_eq!(pin.observed(), Some(fingerprint(b"c")));
    }

    #[test]
    fn only_the_trusted_certificate_passes_the_exception() {
        let stored = fingerprint(b"self signed");
        let trusting = CertPin::trusting(&stored).unwrap();
        assert!(trusting.trusts_untrusted(b"self signed"));
        assert!(!trusting.trusts_untrusted(b"another"));
        // A plain pin and a learning pin never give the exception.
        assert!(
            !CertPin::new(Some(&stored))
                .unwrap()
                .trusts_untrusted(b"self signed")
        );
        assert!(!CertPin::new(None).unwrap().trusts_untrusted(b"self signed"));
        assert!(CertPin::trusting("").is_err());
        assert!(CertPin::trusting("nonsense").is_err());
    }

    #[test]
    fn a_bad_stored_fingerprint_is_an_error() {
        assert!(CertPin::new(Some("not a fingerprint")).is_err());
        assert!(CertPin::new(Some("  ")).is_ok());
    }
}
