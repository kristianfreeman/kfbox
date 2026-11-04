//! Where the API key is kept: the macOS Keychain, behind a trait so tests use
//! memory instead.

use std::sync::Mutex;

use kfbox_core::session::APP_ID;

use crate::AssistError;

/// The Keychain account the Anthropic key is filed under.
const ACCOUNT: &str = "anthropic-api-key";

pub trait KeyStore: Send + Sync {
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
