//! The certificate pin of each account (SECURITYAUTH-13, CORESESSION-15, docs/cert-pin.md).
//!
//! A pin is the SHA-256 fingerprint of the certificate of the server. It is not a secret,
//! so it lives in `certpins.json` in the app config directory, not in the keychain. The UI
//! never writes the fingerprint: it asks Rust to pin the certificate that the last login
//! saw (`cert_trust`). A script in the page cannot pin a fingerprint of its own choice.
//!
//! The file is apart from `settings.json` on purpose. The UI owns that file and replaces
//! it as a whole, so a pin in it could be lost or forged by a write of the page.
//!
//! Every login builds a `CertPin`, also when nothing is stored: then it only learns the
//! fingerprint, and the settings screen can show it. With a stored pin, a changed
//! certificate stops the login before the password goes out. A pin that the user made
//! for a certificate that the system refuses (a self-signed server) carries the flag
//! `trust_untrusted`: exactly that certificate passes. See `CertPin::trusting`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chord_core::session::CertPin;
use chord_core::session::cert_pin::parse_fingerprint;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::error::{ChordError, Res};
use crate::state::lock;

const FILE: &str = "certpins.json";

fn bare(account: &str) -> Res<chord_core::jid::BareJid> {
    account
        .parse()
        .map_err(|e| ChordError::invalid(format!("not a bare JID ({account:?}): {e}")))
}

/// A pin on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPin {
    /// `AB:CD:...`, upper case.
    pub fingerprint: String,
    /// The user trusted this certificate although the system does not trust its issuer.
    #[serde(default)]
    pub trust_untrusted: bool,
}

type Pins = BTreeMap<String, StoredPin>;

/// Read the pins of `dir`. A missing or damaged file gives no pins. A damaged entry is
/// dropped: a pin that cannot be read must not become a weaker pin.
fn read_from(dir: &Path) -> Pins {
    let Ok(bytes) = std::fs::read(dir.join(FILE)) else {
        return Pins::new();
    };
    let Ok(mut pins) = serde_json::from_slice::<Pins>(&bytes) else {
        log::warn!("certpins.json cannot be read: no pins");
        return Pins::new();
    };
    pins.retain(|_, pin| parse_fingerprint(&pin.fingerprint).is_some());
    pins
}

fn write_to(dir: &Path, pins: &Pins) -> Res<()> {
    std::fs::create_dir_all(dir).map_err(|e| ChordError::io("cannot create the config dir", e))?;
    let bytes = serde_json::to_vec_pretty(pins)
        .map_err(|e| ChordError::new("settings", format!("cannot write the pins: {e}")))?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| ChordError::io("cannot write the pins", e))?;
    std::fs::rename(&tmp, dir.join(FILE)).map_err(|e| ChordError::io("cannot replace the pins", e))
}

/// What the last login of an account did with the certificate.
struct Attempt {
    pin: CertPin,
    /// The last login failed because of the certificate.
    failed: bool,
}

/// The managed state: the pin of the last login of each account.
#[derive(Default)]
pub struct CertPins {
    attempts: Mutex<HashMap<String, Attempt>>,
}

/// What the UI shows about the certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertStatus {
    /// The stored fingerprint, if the user pinned one.
    pub pinned: Option<String>,
    /// The pin lets a certificate that the system refuses pass.
    pub trust_untrusted: bool,
    /// The fingerprint that the last connection saw.
    pub observed: Option<String>,
    /// `none`, `untrusted` (the system refuses the certificate, no pin covers it) or
    /// `changed` (the server shows another certificate than the pin).
    pub problem: &'static str,
}

fn status_of(stored: Option<&StoredPin>, observed: Option<String>, failed: bool) -> CertStatus {
    let pinned = stored.map(|p| p.fingerprint.clone());
    let changed = matches!((&pinned, &observed), (Some(p), Some(o)) if p != o);
    let problem = if changed {
        "changed"
    } else if failed {
        "untrusted"
    } else {
        "none"
    };
    CertStatus {
        pinned,
        trust_untrusted: stored.is_some_and(|p| p.trust_untrusted),
        observed,
        problem,
    }
}

impl CertPins {
    /// The pin for a login of `account`: the stored one, or a learning pin.
    fn begin(&self, dir: &Path, account: &str) -> Res<CertPin> {
        let pin = match read_from(dir).get(account) {
            Some(s) if s.trust_untrusted => CertPin::trusting(&s.fingerprint),
            Some(s) => CertPin::new(Some(&s.fingerprint)),
            None => CertPin::new(None),
        }
        .map_err(|e| ChordError::new("certPin", e))?;
        lock(&self.attempts).insert(
            account.to_owned(),
            Attempt {
                pin: pin.clone(),
                failed: false,
            },
        );
        Ok(pin)
    }

    /// Record that the login failed, and whether the certificate was the reason.
    fn finish(&self, account: &str, cert_failed: bool) {
        if let Some(attempt) = lock(&self.attempts).get_mut(account) {
            attempt.failed = cert_failed;
        }
    }

    fn status(&self, dir: &Path, account: &str) -> CertStatus {
        let (observed, failed) = lock(&self.attempts)
            .get(account)
            .map_or((None, false), |a| (a.pin.observed(), a.failed));
        status_of(read_from(dir).get(account), observed, failed)
    }

    /// Pin the certificate that the last login saw.
    fn trust(&self, dir: &Path, account: &str) -> Res<CertStatus> {
        let status = self.status(dir, account);
        let Some(observed) = status.observed.clone() else {
            return Err(ChordError::invalid(
                "no certificate to pin: sign in first, or try again",
            ));
        };
        // The exception for a certificate that the system refuses is only for the
        // `untrusted` problem, where the user saw the fingerprint and chose to trust it.
        // A plain pin of a good certificate, or a replacement of a pin, has none.
        let trust_untrusted = status.problem == "untrusted";
        let mut pins = read_from(dir);
        pins.insert(
            account.to_owned(),
            StoredPin {
                fingerprint: observed,
                trust_untrusted,
            },
        );
        write_to(dir, &pins)?;
        // The next login reads the new pin. Forget the failure of the old one.
        self.finish(account, false);
        Ok(self.status(dir, account))
    }

    fn clear(&self, dir: &Path, account: &str) -> Res<CertStatus> {
        let mut pins = read_from(dir);
        if pins.remove(account).is_some() {
            write_to(dir, &pins)?;
        }
        self.finish(account, false);
        Ok(self.status(dir, account))
    }
}

fn config_dir(app: &AppHandle) -> Res<PathBuf> {
    app.path()
        .app_config_dir()
        .map_err(|e| ChordError::io("no config dir", e))
}

/// The pin for a login. Called by `login`.
pub fn begin_login(app: &AppHandle, account: &str) -> Res<CertPin> {
    app.state::<CertPins>().begin(&config_dir(app)?, account)
}

/// Record how a login ended. Called by `login`.
pub fn end_login(app: &AppHandle, account: &str, error: Option<&ChordError>) {
    app.state::<CertPins>()
        .finish(account, error.is_some_and(|e| e.code == "tlsInvalid"));
}

/// The pin and the fingerprint that the last connection of `account` saw.
#[tauri::command]
pub async fn cert_status(
    app: AppHandle,
    pins: State<'_, CertPins>,
    account: String,
) -> Res<CertStatus> {
    Ok(pins.status(&config_dir(&app)?, bare(&account)?.as_str()))
}

/// Pin the certificate that the last login of `account` saw (trust on first use). When
/// the system refused that certificate, the pin lets exactly it pass. The UI must have
/// shown the fingerprint and asked the user first.
#[tauri::command]
pub async fn cert_trust(
    app: AppHandle,
    pins: State<'_, CertPins>,
    account: String,
) -> Res<CertStatus> {
    pins.trust(&config_dir(&app)?, bare(&account)?.as_str())
}

/// Remove the pin of `account`. The next login uses the normal checks alone.
#[tauri::command]
pub async fn cert_clear(
    app: AppHandle,
    pins: State<'_, CertPins>,
    account: String,
) -> Res<CertStatus> {
    pins.clear(&config_dir(&app)?, bare(&account)?.as_str())
}

#[cfg(test)]
mod tests {
    use chord_core::session::cert_pin::fingerprint;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("chord-certpins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    const ACCOUNT: &str = "alice@example.org";

    #[test]
    fn a_learning_pin_shows_the_fingerprint_and_pins_it_on_trust() {
        let dir = temp_dir("learn");
        let pins = CertPins::default();
        let pin = pins.begin(&dir, ACCOUNT).unwrap();
        assert!(pin.check(b"cert one").is_ok());
        let status = pins.status(&dir, ACCOUNT);
        assert_eq!(status.observed, Some(fingerprint(b"cert one")));
        assert_eq!((status.pinned, status.problem), (None, "none"));

        let status = pins.trust(&dir, ACCOUNT).unwrap();
        assert_eq!(status.pinned, Some(fingerprint(b"cert one")));
        assert!(
            !status.trust_untrusted,
            "a good certificate gets no exception"
        );
        // The pin is on disk, and the next login carries it.
        let next = pins.begin(&dir, ACCOUNT).unwrap();
        assert!(next.check(b"cert one").is_ok());
        assert!(next.check(b"cert two").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_certificate_is_untrusted_until_the_user_trusts_it() {
        let dir = temp_dir("untrusted");
        let pins = CertPins::default();
        let pin = pins.begin(&dir, ACCOUNT).unwrap();
        pin.check(b"self signed").unwrap();
        // The login failed because of the certificate.
        pins.finish(ACCOUNT, true);
        let status = pins.status(&dir, ACCOUNT);
        assert_eq!(status.problem, "untrusted");

        let status = pins.trust(&dir, ACCOUNT).unwrap();
        assert!(status.trust_untrusted);
        assert_eq!(status.problem, "none");
        // The next login trusts that one certificate, and no other.
        let next = pins.begin(&dir, ACCOUNT).unwrap();
        assert!(next.trusts_untrusted(b"self signed"));
        assert!(!next.trusts_untrusted(b"other"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_certificate_is_a_problem_and_a_new_trust_has_no_exception() {
        let dir = temp_dir("changed");
        let pins = CertPins::default();
        let mut stored = Pins::new();
        stored.insert(
            ACCOUNT.to_owned(),
            StoredPin {
                fingerprint: fingerprint(b"old"),
                trust_untrusted: true,
            },
        );
        write_to(&dir, &stored).unwrap();
        let pin = pins.begin(&dir, ACCOUNT).unwrap();
        assert!(pin.check(b"new").is_err());
        pins.finish(ACCOUNT, true);
        let status = pins.status(&dir, ACCOUNT);
        assert_eq!(status.problem, "changed");
        assert_eq!(status.pinned, Some(fingerprint(b"old")));
        assert_eq!(status.observed, Some(fingerprint(b"new")));

        // Trusting the new certificate replaces the pin. The exception of the old pin
        // does not carry over.
        let status = pins.trust(&dir, ACCOUNT).unwrap();
        assert_eq!(status.pinned, Some(fingerprint(b"new")));
        assert!(!status.trust_untrusted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_removes_the_pin() {
        let dir = temp_dir("clear");
        let pins = CertPins::default();
        pins.begin(&dir, ACCOUNT).unwrap().check(b"c").unwrap();
        pins.trust(&dir, ACCOUNT).unwrap();
        let status = pins.clear(&dir, ACCOUNT).unwrap();
        assert_eq!(status.pinned, None);
        assert!(read_from(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trust_without_a_seen_certificate_fails() {
        let dir = temp_dir("none");
        let pins = CertPins::default();
        assert_eq!(pins.trust(&dir, ACCOUNT).unwrap_err().code, "invalid");
        pins.begin(&dir, ACCOUNT).unwrap();
        assert_eq!(pins.trust(&dir, ACCOUNT).unwrap_err().code, "invalid");
    }

    #[test]
    fn pins_are_kept_for_each_account() {
        let dir = temp_dir("accounts");
        let pins = CertPins::default();
        pins.begin(&dir, ACCOUNT).unwrap().check(b"a").unwrap();
        pins.trust(&dir, ACCOUNT).unwrap();
        let other = "bob@example.org";
        assert_eq!(pins.status(&dir, other).pinned, None);
        pins.begin(&dir, other).unwrap().check(b"b").unwrap();
        pins.trust(&dir, other).unwrap();
        assert_eq!(read_from(&dir).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_or_entry_gives_no_pin() {
        let dir = temp_dir("damaged");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), b"{not json").unwrap();
        assert!(read_from(&dir).is_empty());
        std::fs::write(
            dir.join(FILE),
            br#"{"alice@example.org":{"fingerprint":"nonsense"}}"#,
        )
        .unwrap();
        assert!(read_from(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
