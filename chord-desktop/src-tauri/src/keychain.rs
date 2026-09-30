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

/// The room passwords of the core, in the system keychain. The core gives each secret a key
/// (`account#room-password#room`), and the key is the keychain user under the service of
/// the app. A failure says what failed and never the secret.
#[derive(Debug)]
pub struct RoomSecrets;

impl chord_core::secrets::SecretStore for RoomSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SERVICE, key).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, key)
            .and_then(|entry| entry.set_password(value))
            .map_err(|e| e.to_string())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        match keyring::Entry::new(SERVICE, key).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_entry_has_its_own_user() {
        assert_eq!(server_user("a@b.example"), "a@b.example#server");
    }

    #[test]
    fn the_room_secrets_use_the_key_of_the_core_as_the_keychain_user() {
        // The user of a room password has a `#room-password#` part, so that it cannot be
        // the user of the saved password or of the server entry of an account.
        let key = chord_core::secrets::room_password_key("a@b.example", "r@c.example");
        assert_eq!(key, "a@b.example#room-password#r@c.example");
        assert_ne!(key, server_user("a@b.example"));
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
