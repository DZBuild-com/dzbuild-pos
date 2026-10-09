//! Files opened for an import, held in memory for the user who opened them (the webview gets a
//! token, never a path). A dry run records the exact options it checked; the apply refuses any
//! other (spec §12: imports never overwrite silently). A file leaves after its apply, when its
//! user discards it or signs out, or 30 minutes after it was opened.

use pos_core::error::AppError;
use pos_core::imports::engine::ImportOptions;
use pos_docs::import::RawFile;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

pub(crate) const STAGING_TTL_MS: i64 = 30 * 60 * 1_000;

pub(crate) struct Staged {
    pub user_id: String,
    pub file_name: String,
    pub file: RawFile,
    pub opened_at: i64,
    /// The options of the last dry run and the fingerprint it produced, spent by one apply.
    pub checked: Option<(ImportOptions, String)>,
}

#[derive(Default)]
pub struct Staging(Mutex<BTreeMap<String, Staged>>);

impl Staging {
    fn lock(&self) -> Result<MutexGuard<'_, BTreeMap<String, Staged>>, AppError> {
        self.0
            .lock()
            .map_err(|_| AppError::Internal("staging lock".into()))
    }

    /// Keeps `staged` under `token`, dropping the files opened more than 30 minutes ago.
    pub(crate) fn put(&self, token: String, staged: Staged, now: i64) -> Result<(), AppError> {
        let mut map = self.lock()?;
        map.retain(|_, s| now - s.opened_at < STAGING_TTL_MS);
        map.insert(token, staged);
        Ok(())
    }

    /// Runs `f` on the staged file of `token` if it belongs to `user_id` and is not older than
    /// 30 minutes at `now` (`import_not_found` otherwise).
    pub(crate) fn with<T>(
        &self,
        token: &str,
        user_id: &str,
        now: i64,
        f: impl FnOnce(&mut Staged) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut map = self.lock()?;
        match map.get_mut(token) {
            Some(staged)
                if staged.user_id == user_id && now - staged.opened_at < STAGING_TTL_MS =>
            {
                f(staged)
            }
            _ => Err(AppError::not_found("import")),
        }
    }

    pub(crate) fn remove(&self, token: &str) -> Result<(), AppError> {
        self.lock()?.remove(token);
        Ok(())
    }

    /// Drops the file of `token` if it is `user_id`'s (a discarded wizard).
    pub(crate) fn discard(&self, token: &str, user_id: &str) -> Result<(), AppError> {
        let mut map = self.lock()?;
        if map.get(token).is_some_and(|s| s.user_id == user_id) {
            map.remove(token);
        }
        Ok(())
    }

    /// Every file of the user signing out.
    pub(crate) fn clear_user(&self, user_id: &str) -> Result<(), AppError> {
        self.lock()?.retain(|_, s| s.user_id != user_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_docs::import::{FileFormat, RawSheet};

    fn staged(user: &str, opened_at: i64) -> Staged {
        Staged {
            user_id: user.into(),
            file_name: "articles.csv".into(),
            file: RawFile {
                format: FileFormat::Csv,
                sheets: vec![RawSheet {
                    name: String::new(),
                    rows: vec![vec!["a".into()]],
                }],
                encoding: None,
                delimiter: Some(';'),
            },
            opened_at,
            checked: None,
        }
    }

    fn found(staging: &Staging, token: &str, user: &str, now: i64) -> bool {
        staging.with(token, user, now, |_| Ok(())).is_ok()
    }

    #[test]
    fn a_file_is_its_users_for_thirty_minutes() {
        let staging = Staging::default();
        staging.put("t1".into(), staged("u1", 0), 0).unwrap();
        staging.put("t2".into(), staged("u2", 0), 0).unwrap();
        assert!(found(&staging, "t1", "u1", STAGING_TTL_MS - 1));
        assert!(!found(&staging, "t1", "u2", 0), "another user's file");
        assert!(!found(&staging, "t1", "u1", STAGING_TTL_MS), "too old");
        assert_eq!(
            staging.with("t1", "u2", 0, |_| Ok(())).unwrap_err().code(),
            "import_not_found"
        );
        staging.discard("t2", "u1").unwrap();
        assert!(found(&staging, "t2", "u2", 0), "only its user discards it");
        staging.clear_user("u2").unwrap();
        assert!(!found(&staging, "t2", "u2", 0));
        staging
            .put("t3".into(), staged("u1", STAGING_TTL_MS), STAGING_TTL_MS)
            .unwrap();
        assert!(
            !found(&staging, "t1", "u1", 0),
            "an old file leaves when another is opened"
        );
    }
}
