//! The three places a record lives, read and written as text: `app_meta` (in `pos.db`;
//! written by the engine inside the transaction of its journal row), the Windows Credential
//! Manager (any `SecretStore`) and a file under `%LOCALAPPDATA%` ([`FileStore`], also a
//! `SecretStore`). A store that cannot be read counts as missing, never as tampered (a locked
//! Credential Manager is not an attack): only what a store holds is judged (`record::read_slot`).

use crate::record::Record;
use pos_core::cloud::secrets::SecretStore;
use pos_core::db::{Db, meta_get};
use pos_core::error::AppError;
use serde::Serialize;
use specta::Type;
use std::io::Write;
use std::path::PathBuf;

pub const META_KEY: &str = "license.record";
pub const ACCOUNT: &str = "dzpos-license";
/// A record is a few hundred bytes; a larger file is not one.
const MAX_FILE_BYTES: u64 = 64 * 1024;

/// Where a record (or, in a tamper report, a witness) comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Meta,
    Vault,
    File,
    /// The audit journal's first `license.activated` trial (tamper reports only).
    Journal,
    /// The built-in public keys (tamper reports only).
    Keyring,
}

impl Source {
    pub const fn as_str(self) -> &'static str {
        match self {
            Source::Meta => "meta",
            Source::Vault => "vault",
            Source::File => "file",
            Source::Journal => "journal",
            Source::Keyring => "keyring",
        }
    }
}

/// A folder of small files, one per account (`<account>.json`), written atomically
/// (`<file>.tmp`, sync, rename).
#[derive(Debug)]
pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new(dir: impl Into<PathBuf>) -> FileStore {
        FileStore { dir: dir.into() }
    }

    /// `%LOCALAPPDATA%\dev.dzbuild.pos`: this PC's own folder, never the roaming one that holds
    /// `pos.db` and travels with a restore.
    pub fn this_pc() -> Option<FileStore> {
        std::env::var_os("LOCALAPPDATA")
            .map(|dir| FileStore::new(PathBuf::from(dir).join("dev.dzbuild.pos")))
    }

    fn path(&self, account: &str) -> Result<PathBuf, AppError> {
        let fine = !account.is_empty()
            && account.len() <= 40
            && account
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !fine {
            return Err(AppError::Internal("file store account".into()));
        }
        Ok(self.dir.join(format!("{account}.json")))
    }
}

impl SecretStore for FileStore {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let path = self.path(account)?;
        match std::fs::metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            // Not a record: read as empty, so that it is judged unreadable.
            Ok(meta) if meta.len() > MAX_FILE_BYTES => return Ok(Some(String::new())),
            Ok(_) => {}
        }
        let bytes = std::fs::read(&path)?;
        Ok(Some(String::from_utf8(bytes).unwrap_or_default()))
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        let path = self.path(account)?;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("json.tmp");
        let written = std::fs::File::create(&tmp).and_then(|mut file| {
            file.write_all(secret.as_bytes())?;
            file.sync_all()
        });
        let result = written.and_then(|()| std::fs::rename(&tmp, path.as_path()));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        Ok(result?)
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        match std::fs::remove_file(self.path(account)?) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }
}

/// What each store holds now (`None`: nothing there). A database error is an error (the shop
/// cannot run anyway); a store that refuses to be read is left out — neither judged nor repaired
/// (a locked Credential Manager is not an attack) — and logged without its content.
pub fn read_texts(
    db: &Db,
    vault: &dyn SecretStore,
    file: &dyn SecretStore,
) -> Result<Vec<(Source, Option<String>)>, AppError> {
    let mut texts = vec![(Source::Meta, db.read(|c| meta_get(c, META_KEY))?)];
    for (source, store) in [(Source::Vault, vault), (Source::File, file)] {
        match store.get(ACCOUNT) {
            Ok(text) => texts.push((source, text)),
            Err(error) => {
                tracing::warn!(target: "license", store = source.as_str(), code = %error.code(), "a licence store could not be read; left out of this check");
            }
        }
    }
    Ok(texts)
}

/// Writes `record` to the Credential Manager and the file (the database copy is the engine's,
/// inside its transaction). A store that refuses is logged; the next check repairs it.
pub fn write_outside(
    vault: &dyn SecretStore,
    file: &dyn SecretStore,
    record: &Record,
) -> Result<(), AppError> {
    let text = record.to_text()?;
    for (source, store) in [(Source::Vault, vault), (Source::File, file)] {
        if let Err(error) = store.set(ACCOUNT, &text) {
            tracing::warn!(target: "license", store = source.as_str(), code = %error.code(), "a licence store could not be written; the next check repairs it");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FailingStore;
    use pos_core::cloud::secrets::MemorySecrets;

    #[test]
    fn the_file_store_writes_atomically_and_reads_garbage_as_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("dev.dzbuild.pos"));
        assert_eq!(store.get(ACCOUNT).unwrap(), None);
        store.set(ACCOUNT, "{\"v\":1}").unwrap();
        assert_eq!(store.get(ACCOUNT).unwrap().as_deref(), Some("{\"v\":1}"));
        let file = dir
            .path()
            .join("dev.dzbuild.pos")
            .join("dzpos-license.json");
        assert!(file.is_file());
        assert!(
            !dir.path()
                .join("dev.dzbuild.pos")
                .join("dzpos-license.json.tmp")
                .exists()
        );
        std::fs::write(&file, [0xff_u8, 0xfe, 0x00]).unwrap();
        assert_eq!(
            store.get(ACCOUNT).unwrap().as_deref(),
            Some(""),
            "not text: unreadable"
        );
        std::fs::write(&file, vec![b'x'; 70_000]).unwrap();
        assert_eq!(
            store.get(ACCOUNT).unwrap().as_deref(),
            Some(""),
            "too large: unreadable"
        );
        store.delete(ACCOUNT).unwrap();
        store.delete(ACCOUNT).unwrap();
        assert_eq!(store.get(ACCOUNT).unwrap(), None);
        assert!(store.get("../escape").is_err());
    }

    #[test]
    fn a_store_that_cannot_be_read_is_left_out() {
        let db = Db::open_memory().unwrap();
        let file = MemorySecrets::default();
        file.set(ACCOUNT, "{}").unwrap();
        let texts = read_texts(&db, &FailingStore, &file).unwrap();
        // a locked Credential Manager is not an attack: neither judged nor repaired
        assert_eq!(
            texts,
            vec![(Source::Meta, None), (Source::File, Some("{}".into()))]
        );
        let vault = MemorySecrets::default();
        assert_eq!(
            read_texts(&db, &vault, &file).unwrap(),
            vec![
                (Source::Meta, None),
                (Source::Vault, None),
                (Source::File, Some("{}".into()))
            ]
        );
        db.write(|tx| pos_core::db::meta_set(tx, META_KEY, "{\"v\":1}"))
            .unwrap();
        assert_eq!(
            read_texts(&db, &vault, &file).unwrap()[0].1.as_deref(),
            Some("{\"v\":1}")
        );
    }
}
