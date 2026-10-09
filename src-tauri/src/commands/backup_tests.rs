//! M6 backups through the commands: the second copy after every kind of backup, its status when
//! the stick is gone, the backup on close, and the fatal-screen recovery listing.

use super::backup;
use super::recovery;
use super::tests::first_run;
use crate::state::{AppPaths, AppState, SharedState};
use pos_core::audit::Actor;
use pos_core::auth::Permission;
use pos_core::cash::CloseInput;
use pos_core::db::{BackupInfo, BackupKind, backup_file_name, list_backups_in};
use pos_core::error::AppError;
use pos_core::local::MirrorFolder;
use pos_core::money::Money;
use pos_core::time::now_ms;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn file_state(dir: &Path) -> SharedState {
    Arc::new(AppState::open(&AppPaths::new(dir), crate::printing::silent()).unwrap())
}

fn mirror_to(st: &AppState, folder: &Path) {
    let path = folder.to_string_lossy().into_owned();
    st.local
        .update(&st.db, Actor::system(None), |c| {
            c.backup_mirror = Some(MirrorFolder { path, label: None })
        })
        .unwrap();
}

fn mirror_actions(st: &AppState) -> Vec<String> {
    st.db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT action FROM audit_log WHERE action LIKE 'backup.mirror%' ORDER BY seq",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

#[test]
fn every_backup_path_mirrors_and_journals_the_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let st = file_state(dir.path());
    first_run(&st);
    let stick = dir.path().join("stick");
    std::fs::create_dir_all(&stick).unwrap();
    mirror_to(&st, &stick);
    let manual = backup::backup_now_impl(&st).unwrap();
    assert!(stick.join(&manual.file_name).exists(), "the manual backup");
    crate::state::run_daily_backup(&st);
    let ctx = st.till(Permission::CashOpen).unwrap();
    st.db
        .write(move |tx| pos_core::cash::open_session(tx, &ctx, Money::ZERO).map(drop))
        .unwrap();
    let closed = super::cash::close_session(&st, CloseInput::default()).unwrap();
    let z_copy = closed.backup.unwrap();
    assert!(stick.join(&z_copy.file_name).exists(), "the Z close backup");
    assert_eq!(
        list_backups_in(&stick).unwrap().len(),
        3,
        "manual, daily, Z"
    );
    std::fs::remove_dir_all(&stick).unwrap(); // the stick is pulled out
    backup::backup_now_impl(&st).unwrap(); // the local backup still works
    assert_eq!(
        st.backup_status(now_ms()).mirror_error.as_deref(),
        Some("mirror_unavailable")
    );
    assert_eq!(
        mirror_actions(&st),
        [
            "backup.mirrored",
            "backup.mirrored",
            "backup.mirrored",
            "backup.mirror_failed"
        ]
    );
}

#[test]
fn the_mirror_status_says_unavailable_when_the_stick_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let st = file_state(dir.path());
    first_run(&st);
    assert_eq!(backup::mirror_status(&st).unwrap().folder, None);
    let stick = dir.path().join("stick");
    std::fs::create_dir_all(&stick).unwrap();
    mirror_to(&st, &stick);
    backup::backup_now_impl(&st).unwrap();
    let there = backup::mirror_status(&st).unwrap();
    assert!(there.available && there.last_copy_at.is_some());
    std::fs::remove_dir_all(&stick).unwrap();
    let gone = backup::mirror_status(&st).unwrap();
    assert!(!gone.available, "unplugged");
    assert_eq!(
        gone.last_copy_at, there.last_copy_at,
        "the last good copy is still named"
    );
    let cleared = backup::mirror_clear(&st).unwrap();
    assert_eq!(cleared.folder, None);
}

#[test]
fn closing_backs_up_only_when_enabled_and_nothing_is_recent() {
    use crate::backups::{CLOSE_BACKUP_AFTER_MS, should_backup_on_close};
    let now = now_ms();
    assert!(should_backup_on_close(true, None, now));
    assert!(should_backup_on_close(
        true,
        Some(now - CLOSE_BACKUP_AFTER_MS),
        now
    ));
    assert!(
        !should_backup_on_close(true, Some(now - 60_000), now),
        "a Z close just made one"
    );
    assert!(!should_backup_on_close(false, None, now));
    assert!(
        should_backup_on_close(true, Some(now + 86_400_000), now),
        "a copy dated in the future is not recent"
    );
    let dir = tempfile::tempdir().unwrap();
    let st = file_state(dir.path());
    first_run(&st);
    assert!(crate::backups::close_backup_due(&st));
    crate::backups::backup_on_close(&st);
    assert!(!crate::backups::close_backup_due(&st));
    let reason: String = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'backup.created' ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(reason.contains("on_close"), "{reason}");
}

#[test]
fn recovery_answers_only_after_a_failed_start_and_lists_every_place() {
    assert_eq!(
        recovery::recovery_allowed(false).unwrap_err().code(),
        "recovery_not_available"
    );
    recovery::recovery_allowed(true).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let now = now_ms();
    let write = |folder: &Path, at: i64, kind: BackupKind| {
        std::fs::create_dir_all(folder).unwrap();
        std::fs::write(folder.join(backup_file_name(at, kind)), b"x").unwrap();
    };
    write(Path::new(&paths.backups_dir), now - 1_000, BackupKind::Auto);
    let stick = dir.path().join("stick");
    write(&stick, now - 1_000, BackupKind::Auto); // the same name: listed from each place
    write(&stick, now - 86_400_000, BackupKind::Auto);
    pos_core::local::LocalConfig {
        backup_mirror: Some(MirrorFolder {
            path: stick.to_string_lossy().into_owned(),
            label: None,
        }),
        ..Default::default()
    }
    .save(Path::new(&paths.local_config))
    .unwrap();
    let usb = dir.path().join("usb");
    write(
        &pos_core::db::mirror::drive_folder(&usb),
        now - 2 * 86_400_000,
        BackupKind::Manual,
    );
    let found = recovery::collect(&paths, std::slice::from_ref(&usb));
    let places: Vec<recovery::RecoveryLocation> = found.iter().map(|(b, _)| b.location).collect();
    assert_eq!(
        places,
        [
            recovery::RecoveryLocation::Local,
            recovery::RecoveryLocation::Mirror,
            recovery::RecoveryLocation::Mirror,
            recovery::RecoveryLocation::Drive
        ]
    );
    let paths_found: Vec<PathBuf> = found.into_iter().map(|(_, p)| p).collect();
    assert!(paths_found[1].starts_with(&stick));
    assert!(paths_found[3].starts_with(&usb));
}

/// Finding 4: a failing local disk leaves a damaged copy under the same name as the good one on
/// the second copy; both are listed with their own ids, the local one previews as invalid and
/// the mirror's restores.
#[test]
fn a_damaged_local_copy_never_hides_the_good_one_of_the_same_name() {
    let dir = tempfile::tempdir().unwrap();
    let st = file_state(dir.path());
    first_run(&st);
    let stick = dir.path().join("stick");
    std::fs::create_dir_all(&stick).unwrap();
    mirror_to(&st, &stick);
    let good = backup::backup_now_impl(&st).unwrap();
    assert!(stick.join(&good.file_name).exists());
    // The local disk fails: the local copy is no longer a database.
    std::fs::write(&good.path, b"not a database any more").unwrap();
    let paths = AppPaths::new(dir.path());
    let found = recovery::collect(&paths, &[]);
    let same: Vec<_> = found
        .iter()
        .filter(|(b, _)| b.file_name == good.file_name)
        .collect();
    assert_eq!(same.len(), 2, "{found:?}");
    let catalog = recovery::RecoveryCatalog::default();
    let listed = catalog.list(found).unwrap();
    let local = listed
        .iter()
        .find(|b| b.file_name == good.file_name && b.location == recovery::RecoveryLocation::Local)
        .unwrap();
    let mirror = listed
        .iter()
        .find(|b| b.file_name == good.file_name && b.location == recovery::RecoveryLocation::Mirror)
        .unwrap();
    assert_ne!(local.id, mirror.id);
    let preview = |id: &str| pos_core::db::recovery::inspect_backup(&catalog.path_of(id).unwrap());
    assert_eq!(
        preview(&local.id).status,
        pos_core::db::recovery::PreviewStatus::Invalid
    );
    assert_eq!(
        preview(&mirror.id).status,
        pos_core::db::recovery::PreviewStatus::Valid
    );
    let source = catalog.path_of(&mirror.id).unwrap();
    pos_core::db::recovery::restore_guard(&source, false).unwrap();
}

/// Carry from the M6 core review: the second copy rotates only the automatic copies this
/// database logged making in that folder; another PC's old copies in a shared folder stay.
#[test]
fn the_second_copy_rotates_only_the_copies_this_pc_made() {
    let dir = tempfile::tempdir().unwrap();
    let st = file_state(dir.path());
    first_run(&st);
    // Keep the 7 most recent days only (the smallest retention the settings allow).
    for (key, value) in [
        ("backup.keep_daily", 7),
        ("backup.keep_weekly", 0),
        ("backup.keep_monthly", 0),
    ] {
        super::settings::set(&st, key.into(), serde_json::json!(value)).unwrap();
    }
    let stick = dir.path().join("stick");
    std::fs::create_dir_all(&stick).unwrap();
    let day = 86_400_000_i64;
    let start = now_ms() - 30 * day;
    let folder = stick.to_string_lossy().into_owned();
    // Eight days of this PC's copies, logged as it made them; the oldest is one day too many.
    let ours: Vec<PathBuf> = (0..8)
        .map(|days_back| {
            let at = start - days_back * day;
            let path = stick.join(backup_file_name(at, BackupKind::Auto));
            std::fs::write(&path, b"x").unwrap();
            let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
            let copy: Result<BackupInfo, AppError> = Ok(BackupInfo {
                path: path.to_string_lossy().into_owned(),
                at,
                size: 1,
                kind: BackupKind::Auto,
                file_name: file_name.clone(),
            });
            st.db
                .record_mirror(Actor::system(None), &folder, &file_name, &copy)
                .unwrap();
            path
        })
        .collect();
    // Another PC's copy in the same folder, older than all of them: never logged here.
    let theirs = stick.join(backup_file_name(start - 20 * day, BackupKind::Auto));
    std::fs::write(&theirs, b"x").unwrap();
    mirror_to(&st, &stick);
    let manual = backup::backup_now_impl(&st).unwrap();
    assert!(stick.join(&manual.file_name).exists(), "the copy just made");
    assert!(!ours[7].exists(), "this PC's own oldest copy rotates");
    assert!(ours[..7].iter().all(|p| p.exists()), "seven days are kept");
    assert!(theirs.exists(), "a copy this PC never made stays");
}

/// Carry from the M6 core review: a weekly integrity check that cannot run counts as a damaged
/// database: it is journaled and raises the "stop selling" alert.
#[test]
fn an_integrity_check_that_cannot_run_raises_the_damaged_alert_and_is_journaled() {
    let st = AppState::for_tests();
    first_run(&st);
    crate::backups::watch_integrity(&st, Actor::system(None), Ok(Some(true)));
    assert!(!st.backup_status(now_ms()).database_damaged);
    crate::backups::watch_integrity(
        &st,
        Actor::system(None),
        Err(AppError::Io("disk unreadable".into())),
    );
    assert!(st.backup_status(now_ms()).database_damaged);
    let payload: String = st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'db.integrity_checked' ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        payload,
        serde_json::json!({ "ok": false, "scheduled": true, "error": "io_error" })
    );
}

/// Finding 8 (Task 9 carry): the second copy is never the data folder or a folder inside it (the
/// backups folder, the demo, a `corrupt-*` folder): the screen would report a copy that does
/// not exist apart from the PC's disk. Finding 36: nor a folder a sync client sends online.
#[test]
fn the_second_copy_refuses_the_data_folder_and_synced_folders() {
    let dir = tempfile::tempdir().unwrap();
    let st = file_state(dir.path());
    first_run(&st);
    let refused = |folder: &Path| {
        std::fs::create_dir_all(folder).unwrap();
        backup::mirror_set(&st, folder.to_path_buf())
            .unwrap_err()
            .code()
    };
    assert_eq!(refused(Path::new(&st.paths.backups_dir)), "mirror_invalid");
    assert_eq!(refused(dir.path()), "mirror_invalid");
    assert_eq!(
        refused(&dir.path().join("demo").join("backups")),
        "mirror_invalid"
    );
    assert_eq!(
        refused(&dir.path().join("backups").join("corrupt-20261002-101010")),
        "mirror_invalid"
    );
    let elsewhere = tempfile::tempdir().unwrap();
    assert_eq!(
        refused(&elsewhere.path().join("Dropbox").join("caisse")),
        "mirror_cloud_folder"
    );
    assert_eq!(
        refused(&elsewhere.path().join("Google Drive").join("sauvegardes")),
        "mirror_cloud_folder"
    );
    let synced_root = elsewhere.path().join("Entreprise");
    std::fs::create_dir_all(synced_root.join("pos")).unwrap();
    assert_eq!(
        crate::backups::mirror_refusal(
            &synced_root.join("pos"),
            dir.path(),
            std::slice::from_ref(&synced_root)
        ),
        Some("mirror_cloud_folder"),
        "under %OneDrive%"
    );
    assert_eq!(
        backup::mirror_status(&st).unwrap().folder,
        None,
        "nothing kept"
    );
    let stick = elsewhere.path().join("stick");
    std::fs::create_dir_all(&stick).unwrap();
    let kept = backup::mirror_set(&st, stick.clone()).unwrap();
    assert_eq!(
        kept.folder.as_deref(),
        Some(stick.to_string_lossy().as_ref())
    );
}
