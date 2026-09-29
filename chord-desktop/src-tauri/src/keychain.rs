//! The saved password, in the system keychain. Service `space.foid.chord`, user = account.
//!
//! The password never goes to the log, to a file, or to the UI: `login` reads it here.

use crate::error::{ChordError, Res};

const SERVICE: &str = "space.foid.chord";

fn entry(account: &str) -> Res<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).map_err(keychain_error)
}

/// The error text names the failure, never the secret.
fn keychain_error(error: keyring::Error) -> ChordError {
    ChordError::new("keychain", format!("the system keychain failed: {error}"))
}

/// The saved password of `account`, if there is one.
pub fn get(account: &str) -> Res<Option<String>> {
    match entry(account)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keychain_error(e)),
    }
}

pub fn set(account: &str, password: &str) -> Res<()> {
    entry(account)?
        .set_password(password)
        .map_err(keychain_error)
}

/// Delete the saved password. No saved password is not an error.
pub fn delete(account: &str) -> Res<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keychain_error(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keychain_failure_has_the_keychain_code() {
        let error = keychain_error(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::other("locked"),
        )));
        assert_eq!(error.code, "keychain");
        assert!(error.message.contains("locked"));
    }
}
