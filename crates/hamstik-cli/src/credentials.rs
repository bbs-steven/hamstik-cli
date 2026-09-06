// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Credential storage behind a seam.
//!
//! The production store delegates to the OS credential store (SPEC §25) via the
//! `keyring` crate. Tests inject [`MemoryCredentialStore`]. A missing entry is
//! reported as `Ok(None)` and is *not* an error — only genuine store failures
//! map to exit code 10.

use std::collections::HashMap;
use std::sync::Mutex;

use keyring::{Entry, Error as KeyringError};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

/// Stable service identifier for stored Hamstik credentials.
pub const SERVICE: &str = "Hamstik CLI";

/// Credential-store failures (distinct from Hamstik authentication errors).
#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("{0}")]
    Unavailable(String),
}

/// Reads and writes PATs in a secure store.
pub trait CredentialStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<SecretString>, CredentialError>;
    fn set(&self, account: &str, secret: &SecretString) -> Result<(), CredentialError>;
    fn delete(&self, account: &str) -> Result<(), CredentialError>;
}

/// Builds the collision-resistant account key: host + Hamstik user id.
#[must_use]
pub fn account_key(host: &str, user_id: &str) -> String {
    format!("host={host};user={user_id}")
}

/// OS credential store backed by the `keyring` crate.
pub struct KeyringCredentialStore;

impl KeyringCredentialStore {
    fn entry(account: &str) -> Result<Entry, CredentialError> {
        Entry::new(SERVICE, account).map_err(|err| CredentialError::Unavailable(err.to_string()))
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn get(&self, account: &str) -> Result<Option<SecretString>, CredentialError> {
        let entry = Self::entry(account)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(SecretString::from(password))),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(err) => Err(CredentialError::Unavailable(err.to_string())),
        }
    }

    fn set(&self, account: &str, secret: &SecretString) -> Result<(), CredentialError> {
        let entry = Self::entry(account)?;
        entry
            .set_password(secret.expose_secret())
            .map_err(|err| CredentialError::Unavailable(err.to_string()))
    }

    fn delete(&self, account: &str) -> Result<(), CredentialError> {
        let entry = Self::entry(account)?;
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(err) => Err(CredentialError::Unavailable(err.to_string())),
        }
    }
}

/// In-memory store for tests (never used in production).
#[derive(Default)]
pub struct MemoryCredentialStore {
    entries: Mutex<HashMap<String, String>>,
}

impl MemoryCredentialStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds a value for tests that pre-populate credentials.
    pub fn seed(&self, account: &str, secret: &str) {
        self.entries
            .lock()
            .expect("store lock")
            .insert(account.to_string(), secret.to_string());
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn get(&self, account: &str) -> Result<Option<SecretString>, CredentialError> {
        let value = self
            .entries
            .lock()
            .expect("store lock")
            .get(account)
            .cloned();
        Ok(value.map(SecretString::from))
    }

    fn set(&self, account: &str, secret: &SecretString) -> Result<(), CredentialError> {
        self.entries
            .lock()
            .expect("store lock")
            .insert(account.to_string(), secret.expose_secret().to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), CredentialError> {
        self.entries.lock().expect("store lock").remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip() {
        let store = MemoryCredentialStore::new();
        assert!(store.get("acct").unwrap().is_none());
        store.set("acct", &SecretString::from("tok")).unwrap();
        assert_eq!(store.get("acct").unwrap().unwrap().expose_secret(), "tok");
        store.delete("acct").unwrap();
        assert!(store.get("acct").unwrap().is_none());
    }

    #[test]
    fn delete_missing_is_ok() {
        let store = MemoryCredentialStore::new();
        store.delete("nope").unwrap();
    }

    #[test]
    fn account_key_includes_host_and_user() {
        let key = account_key("https://hamstik.com", "abc");
        assert!(key.contains("https://hamstik.com"));
        assert!(key.contains("abc"));
    }
}
