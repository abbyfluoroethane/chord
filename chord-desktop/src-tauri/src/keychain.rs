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

/// The server that the saved password of `account` is for: `srv` or `starttls://host:port`.
/// It is a second entry, so an old entry with no server stays valid and means `srv`.
pub fn get_server(account: &str) -> Res<Option<String>> {
    match entry(&server_user(account))?.get_password() {
        Ok(server) => Ok(Some(server)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keychain_error(e)),
    }
}

pub fn set_server(account: &str, server: &str) -> Res<()> {
    entry(&server_user(account))?
        .set_password(server)
        .map_err(keychain_error)
}

/// The keychain user of the server entry. A JID has no `#` in its domain, so no other
/// account has this name.
fn server_user(account: &str) -> String {
    format!("{account}#server")
}

pub fn set(account: &str, password: &str) -> Res<()> {
    entry(account)?
        .set_password(password)
        .map_err(keychain_error)
}

/// Delete the saved password. No saved password is not an error.
pub fn delete(account: &str) -> Res<()> {
    for user in [account.to_owned(), server_user(account)] {
        match entry(&user)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(keychain_error(e)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_entry_has_its_own_user() {
        assert_eq!(server_user("a@b.example"), "a@b.example#server");
    }

    #[test]
    fn a_keychain_failure_has_the_keychain_code() {
        let error = keychain_error(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::other("locked"),
        )));
        assert_eq!(error.code, "keychain");
        assert!(error.message.contains("locked"));
    }
}
