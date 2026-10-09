//! The till's bearer token (LAN-009): in Windows Credential Manager (this user, this PC, never
//! roaming), never in `pos.db`, a log, a backup or an export. The main PC keeps only its SHA-256.
//! It goes through the store that keeps the DZBuild link's refresh token (M7,
//! `pos_core::cloud::secrets`), at a target of its own.

#[cfg(windows)]
use pos_core::cloud::secrets::{CredentialManager, SecretStore};
use pos_core::error::AppError;
use std::sync::{Arc, Mutex};

/// The token's account in that store; its Credential Manager target is [`TARGET`].
pub const ACCOUNT: &str = "lan-till-token";
pub const TARGET: &str = "DZBuild POS/lan-till-token";

pub trait TokenVault: Send + Sync + 'static {
    fn get(&self) -> Result<Option<String>, AppError>;
    fn set(&self, token: &str) -> Result<(), AppError>;
    /// Deleting a missing token succeeds.
    fn delete(&self) -> Result<(), AppError>;
}

/// Carries neither the token nor the store's own message.
fn unavailable() -> AppError {
    AppError::conflict("lan_vault_unavailable")
}

#[derive(Default)]
pub struct MemoryVault(Mutex<Option<String>>);

impl std::fmt::Debug for MemoryVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemoryVault(<hidden>)")
    }
}

impl TokenVault for MemoryVault {
    fn get(&self) -> Result<Option<String>, AppError> {
        self.0.lock().map(|t| t.clone()).map_err(|_| unavailable())
    }

    fn set(&self, token: &str) -> Result<(), AppError> {
        *self.0.lock().map_err(|_| unavailable())? = Some(token.to_owned());
        Ok(())
    }

    fn delete(&self) -> Result<(), AppError> {
        *self.0.lock().map_err(|_| unavailable())? = None;
        Ok(())
    }
}

/// Windows Credential Manager, through M7's store (generic credential, persistence `Local`).
/// The store logs which kind of failure it met, never the secret.
#[cfg(windows)]
pub struct CredentialVault {
    store: CredentialManager,
    account: String,
}

#[cfg(windows)]
impl CredentialVault {
    pub fn open() -> Result<CredentialVault, AppError> {
        Self::open_named(ACCOUNT)
    }

    /// The vault of `account` (target `DZBuild POS/<account>`); a test uses its own.
    pub fn open_named(account: &str) -> Result<CredentialVault, AppError> {
        let store = CredentialManager::open().map_err(|_| unavailable())?;
        Ok(CredentialVault {
            store,
            account: account.to_owned(),
        })
    }
}

#[cfg(windows)]
impl TokenVault for CredentialVault {
    fn get(&self) -> Result<Option<String>, AppError> {
        self.store.get(&self.account).map_err(|_| unavailable())
    }

    fn set(&self, token: &str) -> Result<(), AppError> {
        self.store
            .set(&self.account, token)
            .map_err(|_| unavailable())
    }

    fn delete(&self) -> Result<(), AppError> {
        self.store.delete(&self.account).map_err(|_| unavailable())
    }
}

/// Windows without a working Credential Manager: the token is neither read nor kept anywhere
/// weaker, and the till's status says `lan_vault_unavailable`.
#[cfg(windows)]
struct UnavailableVault;

#[cfg(windows)]
impl TokenVault for UnavailableVault {
    fn get(&self) -> Result<Option<String>, AppError> {
        Err(unavailable())
    }

    fn set(&self, _: &str) -> Result<(), AppError> {
        Err(unavailable())
    }

    fn delete(&self) -> Result<(), AppError> {
        Err(unavailable())
    }
}

/// The Credential Manager on Windows; memory elsewhere (CI on other systems).
pub fn default_vault() -> Arc<dyn TokenVault> {
    #[cfg(windows)]
    {
        match CredentialVault::open() {
            Ok(vault) => Arc::new(vault),
            Err(_) => {
                tracing::error!("the Credential Manager is unavailable: no till token this run");
                Arc::new(UnavailableVault)
            }
        }
    }
    #[cfg(not(windows))]
    {
        Arc::new(MemoryVault::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_vault_keeps_one_token() {
        let vault = MemoryVault::default();
        assert_eq!(vault.get().unwrap(), None);
        vault.set("a".repeat(64).as_str()).unwrap();
        assert_eq!(
            vault.get().unwrap().as_deref(),
            Some("a".repeat(64).as_str())
        );
        vault.delete().unwrap();
        assert_eq!(vault.get().unwrap(), None);
        assert!(!format!("{vault:?}").contains("aaaa"), "Debug hides it");
    }

    /// The till's token sits under M7's Credential Manager service, at its own target.
    #[test]
    fn the_token_has_its_own_credential() {
        assert_eq!(
            TARGET,
            format!("{}/{ACCOUNT}", pos_core::cloud::secrets::SECRET_SERVICE)
        );
        assert_eq!(TARGET, "DZBuild POS/lan-till-token");
    }

    /// Writes to the real Windows Credential Manager of the user running the tests.
    #[cfg(windows)]
    #[test]
    #[ignore = "touches the user's Credential Manager"]
    fn the_credential_vault_round_trips() {
        let vault = CredentialVault::open_named("lan-till-token-test").unwrap();
        vault.set("0123456789abcdef").unwrap();
        assert_eq!(vault.get().unwrap().as_deref(), Some("0123456789abcdef"));
        vault.delete().unwrap();
        assert_eq!(vault.get().unwrap(), None);
    }
}
