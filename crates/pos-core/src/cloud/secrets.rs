//! Where the refresh token lives: Windows Credential Manager (generic credential, persistence
//! `Local`: never roams), through `keyring-core` and the Windows store directly — the `keyring`
//! façade needs its all-platform feature. Tests use [`MemorySecrets`]. Nothing here is ever
//! logged, and `Debug` prints no value.

use crate::error::AppError;
use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

pub const SECRET_SERVICE: &str = "DZBuild POS";

/// The account of a link's refresh token: `dzbuild-refresh/<link id>`.
pub fn refresh_account(link_id: &str) -> String {
    format!("dzbuild-refresh/{link_id}")
}

pub trait SecretStore: Send + Sync + std::fmt::Debug {
    fn get(&self, account: &str) -> Result<Option<String>, AppError>;
    fn set(&self, account: &str, secret: &str) -> Result<(), AppError>;
    /// Deleting a missing secret succeeds.
    fn delete(&self, account: &str) -> Result<(), AppError>;
}

#[derive(Default)]
pub struct MemorySecrets(Mutex<BTreeMap<String, String>>);

impl std::fmt::Debug for MemorySecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.0.lock().unwrap_or_else(PoisonError::into_inner).len();
        write!(f, "MemorySecrets({count} accounts)")
    }
}

impl SecretStore for MemorySecrets {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(account)
            .cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(account.to_owned(), secret.to_owned());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), AppError> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(account);
        Ok(())
    }
}

/// A store that refuses to keep anything (no Credential Manager on this machine): linking fails
/// with `cloud_secret_store_failed` instead of keeping a token somewhere weaker.
#[derive(Debug, Default)]
pub struct NoSecretStore;

impl SecretStore for NoSecretStore {
    fn get(&self, _: &str) -> Result<Option<String>, AppError> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &str) -> Result<(), AppError> {
        Err(AppError::sync("cloud_secret_store_failed"))
    }
    fn delete(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
}

#[cfg(windows)]
pub use windows_store::CredentialManager;

#[cfg(windows)]
mod windows_store {
    use super::{SECRET_SERVICE, SecretStore};
    use crate::error::AppError;
    use keyring_core::api::CredentialStoreApi;
    use std::collections::HashMap;
    use std::sync::Arc;

    pub struct CredentialManager {
        store: Arc<windows_native_keyring_store::Store>,
    }

    impl std::fmt::Debug for CredentialManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("CredentialManager")
        }
    }

    /// The variant only: some carry the bytes that failed to decode.
    fn kind(error: &keyring_core::Error) -> &'static str {
        match error {
            keyring_core::Error::PlatformFailure(_) => "platform_failure",
            keyring_core::Error::NoStorageAccess(_) => "no_storage_access",
            keyring_core::Error::NoEntry => "no_entry",
            keyring_core::Error::BadEncoding(_) => "bad_encoding",
            keyring_core::Error::TooLong(_, _) => "too_long",
            keyring_core::Error::Invalid(_, _) => "invalid",
            _ => "other",
        }
    }

    fn failed(error: &keyring_core::Error) -> AppError {
        tracing::warn!(target: "cloud", kind = kind(error), "Windows Credential Manager refused");
        AppError::sync("cloud_secret_store_failed")
    }

    impl CredentialManager {
        pub fn open() -> Result<CredentialManager, AppError> {
            windows_native_keyring_store::Store::new()
                .map(|store| CredentialManager { store })
                .map_err(|e| failed(&e))
        }

        fn entry(&self, account: &str) -> Result<keyring_core::Entry, AppError> {
            let target = format!("{SECRET_SERVICE}/{account}");
            let modifiers = HashMap::from([("target", target.as_str()), ("persistence", "Local")]);
            self.store
                .build(SECRET_SERVICE, account, Some(&modifiers))
                .map_err(|e| failed(&e))
        }
    }

    impl SecretStore for CredentialManager {
        fn get(&self, account: &str) -> Result<Option<String>, AppError> {
            match self.entry(account)?.get_password() {
                Ok(secret) => Ok(Some(secret)),
                Err(keyring_core::Error::NoEntry) => Ok(None),
                Err(e) => Err(failed(&e)),
            }
        }
        fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
            self.entry(account)?
                .set_password(secret)
                .map_err(|e| failed(&e))
        }
        fn delete(&self, account: &str) -> Result<(), AppError> {
            match self.entry(account)?.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
                Err(e) => Err(failed(&e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_secrets_round_trip_and_never_print_their_values() {
        let secrets = MemorySecrets::default();
        let account = refresh_account("00000000-0000-7000-8000-00000000c001");
        assert_eq!(
            account,
            "dzbuild-refresh/00000000-0000-7000-8000-00000000c001"
        );
        assert_eq!(secrets.get(&account).unwrap(), None);
        secrets.set(&account, "rt_SECRET").unwrap();
        assert_eq!(secrets.get(&account).unwrap().as_deref(), Some("rt_SECRET"));
        assert!(!format!("{secrets:?}").contains("rt_SECRET"));
        secrets.delete(&account).unwrap();
        secrets.delete(&account).unwrap(); // deleting twice is fine
        assert_eq!(secrets.get(&account).unwrap(), None);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "writes to this Windows user's Credential Manager: cargo test -p pos-core credential_manager_round_trips -- --ignored"]
    fn credential_manager_round_trips() {
        let store = CredentialManager::open().unwrap();
        let account = format!("dzbuild-refresh/test-{}", crate::ids::new_id());
        assert_eq!(store.get(&account).unwrap(), None);
        store.set(&account, "rt_test_1").unwrap();
        store.set(&account, "rt_test_2").unwrap();
        assert_eq!(
            store.get(&account).unwrap().as_deref(),
            Some("rt_test_2"),
            "rotation overwrites"
        );
        store.delete(&account).unwrap();
        store.delete(&account).unwrap();
        assert_eq!(store.get(&account).unwrap(), None);
    }
}
