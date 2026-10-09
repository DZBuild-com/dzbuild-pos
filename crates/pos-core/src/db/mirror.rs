//! The second copy of the backups (spec §13, DAT-008/010): a folder this PC chose (a USB stick,
//! an external disk, a network share) or a stick plugged in for a one-off copy. Each copy is
//! written under `<name>.tmp`, synced, reopened read-only and checked (`integrity_check`, a POS
//! schema this build can open), then renamed. A stick pulled out mid-copy, or one that hands
//! back garbage, leaves no file under a backup name; the local backup is never touched.
//! Rotation removes only the automatic copies this database logged making there: the folder
//! may be shared with another PC's copies.

use super::Db;
use super::backup::{
    BackupInfo, BackupKind, Retention, kind_of, list_backups_in, remove_quietly, select_to_prune,
    time_of, validate_backup_file,
};
use crate::audit::Actor;
use crate::error::AppError;
use crate::ids::new_id;
use crate::time::now_ms;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Where a one-off copy lands on a stick: `<mount>\DZBuild POS\sauvegardes`.
pub fn drive_folder(mount: &Path) -> PathBuf {
    mount.join("DZBuild POS").join("sauvegardes")
}

/// The drive folder of `mount`, created when the stick is there; `mirror_unavailable` when it
/// is not (a drive letter that no longer answers).
pub fn prepare_drive_folder(mount: &Path) -> Result<PathBuf, AppError> {
    if !mount.is_dir() {
        return Err(AppError::conflict("mirror_unavailable"));
    }
    let folder = drive_folder(mount);
    std::fs::create_dir_all(&folder).map_err(|_| AppError::conflict("mirror_unavailable"))?;
    Ok(folder)
}

/// Copies `source` (a verified local backup) into `dir`, which must exist now.
pub fn copy_verified(source: &Path, dir: &Path) -> Result<BackupInfo, AppError> {
    copy_verified_with(source, dir, copy_synced)
}

/// [`copy_verified`] with the byte copy given (tests stand in for a stick pulled out or one
/// that lies). The copy goes to `<name>.tmp`; it is checked, then renamed. Any failure removes
/// the temporary file (quietly: the stick may be gone) and returns `mirror_unavailable` when
/// the folder vanished or the device stopped answering, `backup_invalid` when the copy fails
/// its check, `io_error` otherwise.
pub fn copy_verified_with(
    source: &Path,
    dir: &Path,
    copy: impl FnOnce(&Path, &Path) -> std::io::Result<u64>,
) -> Result<BackupInfo, AppError> {
    if !dir.is_dir() {
        return Err(AppError::conflict("mirror_unavailable"));
    }
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::validation("source", "value_invalid"))?;
    let kind =
        kind_of(&file_name).ok_or_else(|| AppError::validation("source", "value_invalid"))?;
    let final_path = dir.join(&file_name);
    let tmp = dir.join(format!("{file_name}.tmp"));
    let outcome = copy(source, &tmp)
        .map_err(|error| io_error(error, dir))
        .and_then(|_| validate_backup_file(&tmp))
        .and_then(|()| std::fs::rename(&tmp, &final_path).map_err(|error| io_error(error, dir)));
    if let Err(error) = outcome {
        remove_quietly(&tmp);
        return Err(error);
    }
    let size = std::fs::metadata(&final_path)
        .map_err(|error| io_error(error, dir))?
        .len();
    Ok(BackupInfo {
        path: final_path.to_string_lossy().into_owned(),
        at: time_of(&file_name).unwrap_or_else(now_ms),
        size: i64::try_from(size).unwrap_or(i64::MAX),
        kind,
        file_name,
    })
}

/// A removed stick answers "not found" or "device not ready/connected" (Windows 21, 55, 1167).
fn io_error(error: std::io::Error, dir: &Path) -> AppError {
    let gone = !dir.is_dir()
        || error.kind() == std::io::ErrorKind::NotFound
        || matches!(error.raw_os_error(), Some(21 | 55 | 1167));
    if gone {
        AppError::conflict("mirror_unavailable")
    } else {
        AppError::from(error)
    }
}

/// Copies and syncs the file, so a stick that is pulled right after a success still holds it.
fn copy_synced(from: &Path, to: &Path) -> std::io::Result<u64> {
    let mut input = std::fs::File::open(from)?;
    let mut output = std::fs::File::create(to)?;
    let copied = std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    Ok(copied)
}

/// Rotates the automatic copies this database made in a mirror folder (`owned`, from
/// [`logged_copies_in`]) like the local ones. Every other file stays: manual, pre-migration and
/// pre-restore copies, and the copies another PC keeps in the same folder (tills of one shop on
/// a network share, one stick for two shops), whose names do not say whose they are. Returns
/// how many files were removed.
pub fn prune(
    dir: &Path,
    retention: Retention,
    today: NaiveDate,
    owned: &HashSet<PathBuf>,
) -> usize {
    let autos: Vec<(i64, PathBuf)> = match list_backups_in(dir) {
        Ok(list) => list
            .into_iter()
            .filter(|b| b.kind == BackupKind::Auto && owned.contains(Path::new(&b.path)))
            .map(|b| (time_of(&b.file_name).unwrap_or(b.at), PathBuf::from(b.path)))
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "could not list the mirror folder for rotation");
            return 0;
        }
    };
    let doomed = select_to_prune(autos, retention, today);
    for path in &doomed {
        remove_quietly(path);
    }
    doomed.len()
}

/// The copies this database made directly in `folder`: its `backups_log` rows with `ok = 1`
/// (written by [`Db::record_mirror`]). The ownership [`prune`] needs; a path spelled another way
/// than when it was logged is simply not owned, so it is kept.
pub fn logged_copies_in(conn: &Connection, folder: &Path) -> Result<HashSet<PathBuf>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT path FROM backups_log WHERE ok = 1 AND substr(path, 1, length(?1)) = ?1",
    )?;
    let paths = stmt
        .query_map([folder.to_string_lossy()], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path.parent() == Some(folder))
        .collect())
}

/// The newest good copy logged in `folder` (its path starts with it), even when the stick is
/// out: the screen shows "last copy on …".
pub fn last_copy_in(conn: &Connection, folder: &str) -> Result<Option<i64>, AppError> {
    Ok(conn
        .query_row(
            "SELECT MAX(at) FROM backups_log WHERE ok = 1 AND substr(path, 1, length(?1)) = ?1",
            [folder],
            |r| r.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten())
}

impl Db {
    /// Journals one mirror attempt. A good copy adds its `backups_log` row (the mirror path, the
    /// source's kind, `ok = 1`) and `backup.mirrored { file, folder }`; a failure adds
    /// `backup.mirror_failed { file, folder, code }`. Both in one writer transaction.
    pub fn record_mirror(
        &self,
        actor: Actor,
        folder: &str,
        source_file: &str,
        outcome: &Result<BackupInfo, AppError>,
    ) -> Result<(), AppError> {
        let folder = folder.to_owned();
        let file = source_file.to_owned();
        let outcome = outcome.clone();
        self.write(move |tx| {
            match &outcome {
                Ok(copy) => {
                    tx.execute(
                        "INSERT INTO backups_log (id, at, path, size, kind, ok) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                        params![new_id(), now_ms(), copy.path, copy.size, copy.kind.as_str()],
                    )?;
                    crate::audit::append(
                        tx,
                        &actor.event(
                            "backup.mirrored",
                            "backup",
                            Some(&file),
                            json!({ "folder": folder }),
                        ),
                    )?;
                }
                Err(error) => {
                    crate::audit::append(
                        tx,
                        &actor.event(
                            "backup.mirror_failed",
                            "backup",
                            Some(&file),
                            json!({ "folder": folder, "code": error.code() }),
                        ),
                    )?;
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, DbOptions, backup_file_name};
    use crate::time::business_date;

    fn made_backup(dir: &Path) -> (Db, BackupInfo) {
        let db = Db::open(&dir.join("pos.db"), DbOptions::default()).unwrap();
        let info = db.backup(BackupKind::Auto).unwrap();
        (db, info)
    }

    #[test]
    fn a_copy_is_verified_then_renamed_under_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, info) = made_backup(dir.path());
        let stick = dir.path().join("stick");
        std::fs::create_dir_all(&stick).unwrap();
        let copy = copy_verified(Path::new(&info.path), &stick).unwrap();
        assert_eq!(copy.file_name, info.file_name);
        assert_eq!(copy.kind, BackupKind::Auto);
        assert_eq!(copy.size, info.size);
        validate_backup_file(Path::new(&copy.path)).unwrap();
        let names: Vec<String> = std::fs::read_dir(&stick)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [info.file_name], "no temporary file left");
    }

    #[test]
    fn an_unplugged_stick_mid_copy_leaves_no_file_and_keeps_the_local_backup() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, info) = made_backup(dir.path());
        let stick = dir.path().join("stick");
        std::fs::create_dir_all(&stick).unwrap();
        let pulled = stick.clone();
        let error = copy_verified_with(Path::new(&info.path), &stick, move |from, to| {
            // Half the file reaches the stick, then the stick is gone.
            let bytes = std::fs::read(from)?;
            std::fs::write(to, &bytes[..bytes.len() / 2])?;
            std::fs::remove_dir_all(&pulled)?;
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        })
        .unwrap_err();
        assert_eq!(error.code(), "mirror_unavailable");
        assert!(
            !stick.exists(),
            "nothing was written back once the stick was gone"
        );
        validate_backup_file(Path::new(&info.path)).unwrap();
    }

    #[test]
    fn a_stick_that_returns_garbage_is_caught_by_the_integrity_check() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, info) = made_backup(dir.path());
        let stick = dir.path().join("stick");
        std::fs::create_dir_all(&stick).unwrap();
        let error = copy_verified_with(Path::new(&info.path), &stick, |from, to| {
            // A counterfeit stick: it accepts the write and hands back zeros.
            let len = std::fs::metadata(from)?.len();
            std::fs::write(to, vec![0_u8; usize::try_from(len).unwrap()])?;
            Ok(len)
        })
        .unwrap_err();
        assert_eq!(error.code(), "backup_invalid");
        assert_eq!(
            std::fs::read_dir(&stick).unwrap().count(),
            0,
            "the bad copy is removed"
        );
    }

    #[test]
    fn a_missing_folder_is_reported_unavailable_not_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, info) = made_backup(dir.path());
        let error =
            copy_verified(Path::new(&info.path), &dir.path().join("E-drive-gone")).unwrap_err();
        assert_eq!(error.code(), "mirror_unavailable");
        assert!(prepare_drive_folder(&dir.path().join("no-such-drive")).is_err());
        let mount = dir.path().join("usb");
        std::fs::create_dir_all(&mount).unwrap();
        let folder = prepare_drive_folder(&mount).unwrap();
        assert!(folder.ends_with(Path::new("DZBuild POS").join("sauvegardes")) && folder.is_dir());
    }

    #[test]
    fn mirror_attempts_are_logged_and_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let (db, info) = made_backup(dir.path());
        let stick = dir.path().join("stick");
        std::fs::create_dir_all(&stick).unwrap();
        let folder = stick.to_string_lossy().into_owned();
        let good = copy_verified(Path::new(&info.path), &stick);
        db.record_mirror(
            crate::audit::Actor::system(None),
            &folder,
            &info.file_name,
            &good,
        )
        .unwrap();
        let bad: Result<BackupInfo, AppError> = Err(AppError::conflict("mirror_unavailable"));
        db.record_mirror(
            crate::audit::Actor::system(None),
            &folder,
            &info.file_name,
            &bad,
        )
        .unwrap();
        let actions: Vec<String> = db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT action FROM audit_log WHERE action LIKE 'backup.mirror%' ORDER BY seq",
                )?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(actions, ["backup.mirrored", "backup.mirror_failed"]);
        let last = db.read(move |c| last_copy_in(c, &folder)).unwrap();
        assert!(
            last.is_some(),
            "the good copy is in backups_log under the mirror path"
        );
    }

    #[test]
    fn the_mirror_folder_rotates_its_automatic_copies_only() {
        let dir = tempfile::tempdir().unwrap();
        let stick = dir.path().join("stick");
        std::fs::create_dir_all(&stick).unwrap();
        let day = 86_400_000_i64;
        let now = now_ms();
        let mut owned = HashSet::new();
        for days_ago in 1..=5 {
            let path = stick.join(backup_file_name(now - days_ago * day, BackupKind::Auto));
            std::fs::write(&path, b"x").unwrap();
            owned.insert(path);
        }
        let manual = stick.join(backup_file_name(now - 30 * day, BackupKind::Manual));
        std::fs::write(&manual, b"x").unwrap();
        owned.insert(manual);
        let removed = prune(
            &stick,
            Retention {
                daily: 2,
                weekly: 0,
                monthly: 0,
            },
            business_date(now),
            &owned,
        );
        assert_eq!(removed, 3);
        let kinds: Vec<BackupKind> = list_backups_in(&stick)
            .unwrap()
            .into_iter()
            .map(|b| b.kind)
            .collect();
        assert_eq!(kinds.iter().filter(|k| **k == BackupKind::Auto).count(), 2);
        assert!(
            kinds.contains(&BackupKind::Manual),
            "a manual copy is never pruned"
        );
    }

    /// Unix milliseconds of `hour`:00 Algiers time on 2026-09-`day`.
    fn at(day: u32, hour: u32) -> i64 {
        chrono::NaiveDate::from_ymd_opt(2026, 9, day)
            .unwrap()
            .and_hms_opt(hour, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis()
            - crate::time::ALGIERS_OFFSET_MS
    }

    /// An automatic backup file of `at` in `dir` (content is irrelevant to rotation).
    fn auto_file(dir: &Path, at: i64) -> PathBuf {
        let path = dir.join(backup_file_name(at, BackupKind::Auto));
        std::fs::write(&path, b"x").unwrap();
        path
    }

    /// What `copy_verified` returns for `path`, to journal it without a real copy.
    fn copied(path: &Path) -> Result<BackupInfo, AppError> {
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
        Ok(BackupInfo {
            path: path.to_string_lossy().into_owned(),
            at: time_of(&file_name).unwrap(),
            size: 1,
            kind: BackupKind::Auto,
            file_name,
        })
    }

    #[test]
    fn another_pcs_copies_in_a_shared_folder_are_never_rotated() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _local) = made_backup(dir.path());
        let stick = dir.path().join("stick");
        let other_folder = dir.path().join("stick2");
        std::fs::create_dir_all(&stick).unwrap();
        std::fs::create_dir_all(&other_folder).unwrap();
        // This PC backs up in the morning, the other till (or the owner's other shop) at night.
        let ours_older = auto_file(&stick, at(27, 8));
        let ours_yesterday = auto_file(&stick, at(28, 8));
        let theirs_yesterday = auto_file(&stick, at(28, 20));
        let theirs_older = auto_file(&stick, at(26, 20));
        let folder = stick.to_string_lossy().into_owned();
        let system = || crate::audit::Actor::system(None);
        for path in [&ours_older, &ours_yesterday] {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            db.record_mirror(system(), &folder, &file, &copied(path))
                .unwrap();
        }
        // A failed attempt at their file, and a good copy of it logged in a folder whose name
        // merely starts with the stick's, own nothing in the stick.
        let theirs = theirs_yesterday
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let failed: Result<BackupInfo, AppError> = Err(AppError::conflict("mirror_unavailable"));
        db.record_mirror(system(), &folder, &theirs, &failed)
            .unwrap();
        let elsewhere = other_folder.join(&theirs);
        db.record_mirror(
            system(),
            &other_folder.to_string_lossy(),
            &theirs,
            &copied(&elsewhere),
        )
        .unwrap();
        let owned = {
            let stick = stick.clone();
            db.read(move |c| logged_copies_in(c, &stick)).unwrap()
        };
        assert_eq!(
            owned,
            HashSet::from([ours_older.clone(), ours_yesterday.clone()]),
            "only the good copies this database logged in the stick itself"
        );
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 29).unwrap();
        let removed = prune(
            &stick,
            Retention {
                daily: 1,
                weekly: 0,
                monthly: 0,
            },
            today,
            &owned,
        );
        assert!(
            ours_yesterday.exists(),
            "this PC keeps its own copy of yesterday"
        );
        assert!(
            theirs_yesterday.exists() && theirs_older.exists(),
            "a copy this database never made stays"
        );
        assert!(!ours_older.exists(), "this PC's own copies still rotate");
        assert_eq!(removed, 1);
    }
}
