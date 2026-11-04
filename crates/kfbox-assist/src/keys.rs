//! Where the API key is kept: the macOS Keychain, behind a trait so tests use
//! memory instead.

use std::sync::Mutex;

use kfbox_core::session::APP_ID;

use crate::AssistError;

/// The Keychain account the Anthropic key is filed under.
const ACCOUNT: &str = "anthropic-api-key";

pub trait KeyStore: Send + Sync {
    /// Whether a key is saved, without reading it. Used to answer the page,
    /// so it must never prompt: macOS asks for Keychain access only when a
    /// secret is read.
    fn has_key(&self) -> Result<bool, AssistError> {
        Ok(self.get()?.is_some())
    }
    /// The key itself. Reading it can show the Keychain's access prompt, so
    /// it's only called off the main thread, when a request needs it.
    fn get(&self) -> Result<Option<String>, AssistError>;
    fn set(&self, key: &str) -> Result<(), AssistError>;
    fn delete(&self) -> Result<(), AssistError>;
}

/// The macOS Keychain, through the `keyring` crate.
#[derive(Debug, Default)]
pub struct Keychain;

impl Keychain {
    fn entry() -> Result<keyring::Entry, AssistError> {
        keyring::Entry::new(APP_ID, ACCOUNT).map_err(|e| AssistError::Keychain(e.to_string()))
    }
}

impl KeyStore for Keychain {
    /// Asks for the item's attributes only: no data, so nothing is
    /// decrypted and macOS never asks for access.
    #[cfg(target_os = "macos")]
    fn has_key(&self) -> Result<bool, AssistError> {
        use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
        const NOT_FOUND: i32 = -25_300; // errSecItemNotFound
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(APP_ID)
            .account(ACCOUNT)
            .load_attributes(true)
            .load_data(false)
            .limit(Limit::Max(1))
            .search();
        match found {
            Ok(items) => Ok(!items.is_empty()),
            Err(e) if e.code() == NOT_FOUND => Ok(false),
            Err(e) => Err(AssistError::Keychain(e.to_string())),
        }
    }

    fn get(&self) -> Result<Option<String>, AssistError> {
        match Self::entry()?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AssistError::Keychain(e.to_string())),
        }
    }

    fn set(&self, key: &str) -> Result<(), AssistError> {
        Self::entry()?.set_password(key).map_err(|e| AssistError::Keychain(e.to_string()))
    }

    fn delete(&self) -> Result<(), AssistError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AssistError::Keychain(e.to_string())),
        }
    }
}

/// A key held in memory, for tests.
#[derive(Debug, Default)]
pub struct MemoryKeys(Mutex<Option<String>>);

impl KeyStore for MemoryKeys {
    fn get(&self) -> Result<Option<String>, AssistError> {
        Ok(self.0.lock().map_err(|e| AssistError::Keychain(e.to_string()))?.clone())
    }

    fn set(&self, key: &str) -> Result<(), AssistError> {
        *self.0.lock().map_err(|e| AssistError::Keychain(e.to_string()))? = Some(key.to_owned());
        Ok(())
    }

    fn delete(&self) -> Result<(), AssistError> {
        *self.0.lock().map_err(|e| AssistError::Keychain(e.to_string()))? = None;
        Ok(())
    }
}

/// Checks a key before it's stored. It says only what's wrong, never the key.
pub fn check_key(key: &str) -> Result<String, AssistError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AssistError::BadKey("the key is empty".to_owned()));
    }
    if key.chars().any(char::is_whitespace) {
        return Err(AssistError::BadKey("the key has spaces in it".to_owned()));
    }
    Ok(key.to_owned())
}

/// A key store that takes `delay` to answer, as the Keychain does while its
/// access prompt waits for a person: for the app's smoke test, which checks
/// the window still comes up.
#[derive(Debug)]
pub struct SlowKeys {
    pub delay: std::time::Duration,
    pub keys: MemoryKeys,
}

impl KeyStore for SlowKeys {
    fn has_key(&self) -> Result<bool, AssistError> {
        std::thread::sleep(self.delay);
        self.keys.has_key()
    }

    fn get(&self) -> Result<Option<String>, AssistError> {
        std::thread::sleep(self.delay);
        self.keys.get()
    }

    fn set(&self, key: &str) -> Result<(), AssistError> {
        self.keys.set(key)
    }

    fn delete(&self) -> Result<(), AssistError> {
        self.keys.delete()
    }
}
