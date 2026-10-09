//! This PC's `local.json` as managed state (Task 2 `pos_core::local`): read at start, saved and
//! journaled on every change. The M5 pairing writes its `lan` key into the same file:
//! a change starts from the file as it is on disk and `LocalConfig::save` keeps every
//! key it does not own.

use pos_core::audit::Actor;
use pos_core::db::Db;
use pos_core::error::AppError;
use pos_core::local::{LocalConfig, journal_changes};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

pub struct LocalStore {
    /// `None` for the tests' store, never written to disk.
    path: Option<PathBuf>,
    config: RwLock<LocalConfig>,
    /// One change at a time: two commands never interleave their read, change and write.
    writing: Mutex<()>,
}

impl LocalStore {
    pub fn open(path: &Path) -> LocalStore {
        LocalStore {
            path: Some(path.to_path_buf()),
            config: RwLock::new(LocalConfig::load(path)),
            writing: Mutex::new(()),
        }
    }

    /// Tests: never written to disk.
    pub fn in_memory() -> LocalStore {
        LocalStore {
            path: None,
            config: RwLock::new(LocalConfig::default()),
            writing: Mutex::new(()),
        }
    }

    pub fn get(&self) -> LocalConfig {
        match self.config.read() {
            Ok(config) => config.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn set(&self, config: LocalConfig) {
        match self.config.write() {
            Ok(mut slot) => *slot = config,
            Err(poisoned) => *poisoned.into_inner() = config,
        }
    }

    /// Applies `change` to the file as it is now (another writer may have changed it since the
    /// start), saves it, then journals each changed field (`local.updated`). Nothing is written
    /// when nothing changed. Blocking (file, SQLite).
    pub fn update(
        &self,
        db: &Db,
        actor: Actor,
        change: impl FnOnce(&mut LocalConfig),
    ) -> Result<LocalConfig, AppError> {
        let _writing = match self.writing.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let before = match &self.path {
            Some(path) => LocalConfig::load(path),
            None => self.get(),
        };
        let mut after = before.clone();
        change(&mut after);
        if after == before {
            self.set(after.clone());
            return Ok(after);
        }
        if let Some(path) = &self.path {
            after.save(path)?;
        }
        self.set(after.clone());
        let (old, new) = (before, after.clone());
        db.write(move |tx| journal_changes(tx, &actor, &old, &new))?;
        Ok(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::local::TextSize;
    use serde_json::{Value, json};

    /// The M5 pairing writes `lan` into the same file, at any time: a change
    /// made here starts from the file as it is now and keeps every key it does not own.
    #[test]
    fn a_change_keeps_what_another_writer_put_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(pos_core::local::FILE_NAME);
        std::fs::write(
            &path,
            br#"{ "lan": { "role": "till", "host": "CAISSE-1" } }"#,
        )
        .unwrap();
        let store = LocalStore::open(&path);
        // The pairing moves this till to another main PC after the store was opened.
        std::fs::write(
            &path,
            br#"{ "lan": { "role": "till", "host": "BUREAU" }, "lightMode": true }"#,
        )
        .unwrap();
        let db = Db::open_memory().unwrap();
        let saved = store
            .update(&db, Actor::system(None), |c| c.text_size = TextSize::Large)
            .unwrap();
        assert!(
            saved.light_mode,
            "the change starts from the file, not from a stale copy"
        );
        let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(file["lan"], json!({ "role": "till", "host": "BUREAU" }));
        assert_eq!(file["textSize"], json!("large"));
        assert_eq!(store.get(), saved);
        let unchanged = store
            .update(&db, Actor::system(None), |c| c.text_size = TextSize::Large)
            .unwrap();
        assert_eq!(unchanged, saved);
        let rows: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'local.updated'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(rows, 1, "only the text size changed, once");
    }

    #[test]
    fn the_test_store_never_touches_the_disk() {
        let store = LocalStore::in_memory();
        let db = Db::open_memory().unwrap();
        let saved = store
            .update(&db, Actor::system(None), |c| c.onscreen_keyboard = true)
            .unwrap();
        assert!(saved.onscreen_keyboard && store.get().onscreen_keyboard);
    }
}
