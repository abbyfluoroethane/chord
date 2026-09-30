//! A place for secrets that should not sit in the database, such as a room password. The
//! desktop app backs it with the system keychain. A build with no store keeps the secrets
//! in the database, as before.

use std::fmt::Debug;

/// A store of secrets, by key. The keys are plain text, the values are secrets: no
/// implementation may log a value.
pub trait SecretStore: Send + Sync + Debug {
    /// The secret under `key`, or `None` if there is none.
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    /// Delete the secret. A missing secret is not an error.
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// The key of the password of `room` for `account`.
pub fn room_password_key(account: &str, room: &str) -> String {
    format!("{account}#room-password#{room}")
}

#[cfg(test)]
pub(crate) mod testing {
    use super::SecretStore;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// A store in memory. `fail` makes every write fail.
    #[derive(Debug, Default)]
    pub struct MemorySecrets {
        pub map: Mutex<HashMap<String, String>>,
        pub fail: AtomicBool,
        /// How many times `get` ran.
        pub gets: AtomicUsize,
    }

    impl SecretStore for MemorySecrets {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            if self.fail.load(Ordering::SeqCst) {
                return Err("locked".into());
            }
            self.map.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), String> {
            self.map.lock().unwrap().remove(key);
            Ok(())
        }
    }
}
