//! The fatal screen's backup ids (Task 9 review): an id names one file for the life of the
//! process. The list is polled every 5 s, so a stick plugged in or pulled out between the
//! listing, the preview and the PIN must never make the id the owner chose name another backup.

use super::recovery::{self, RecoveryBackup, RecoveryCatalog, RecoveryLocation};
use crate::state::AppPaths;
use pos_core::db::{BackupKind, backup_file_name, mirror};
use pos_core::time::now_ms;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DAY: i64 = 86_400_000;

fn write(folder: &Path, at: i64, kind: BackupKind) -> PathBuf {
    std::fs::create_dir_all(folder).unwrap();
    let path = folder.join(backup_file_name(at, kind));
    std::fs::write(&path, b"x").unwrap();
    path
}

fn names_by_id(listing: &[RecoveryBackup]) -> HashMap<String, String> {
    listing
        .iter()
        .map(|b| (b.id.clone(), b.file_name.clone()))
        .collect()
}

#[test]
fn an_id_keeps_naming_its_backup_when_a_stick_is_plugged_in_or_pulled_out() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let local = PathBuf::from(&paths.backups_dir);
    let now = now_ms();
    let yesterday = write(&local, now - DAY, BackupKind::Auto);
    write(&local, now - 2 * DAY, BackupKind::Auto);
    let catalog = RecoveryCatalog::default();

    // The owner picks yesterday's local backup, the newest one listed, and previews it.
    let first = catalog.list(recovery::collect(&paths, &[])).unwrap();
    assert_eq!(first.len(), 2);
    let chosen = first[0].id.clone();
    assert_eq!(catalog.path_of(&chosen).unwrap(), yesterday);
    let first_ids = names_by_id(&first);

    // Then plugs in the stick, as the empty-state text says: it holds a newer backup, which now
    // comes first in the list.
    let usb = dir.path().join("usb");
    let on_stick = write(
        &mirror::drive_folder(&usb),
        now - 60_000,
        BackupKind::Manual,
    );
    let second = catalog
        .list(recovery::collect(&paths, std::slice::from_ref(&usb)))
        .unwrap();
    assert_eq!(second.len(), 3);
    assert_eq!(second[0].location, RecoveryLocation::Drive);
    assert_eq!(
        catalog.path_of(&chosen).unwrap(),
        yesterday,
        "the chosen id still names yesterday's file"
    );
    for backup in &second[1..] {
        assert_eq!(
            first_ids.get(&backup.id),
            Some(&backup.file_name),
            "a file listed before keeps its id"
        );
    }
    let stick_id = second[0].id.clone();
    assert!(
        !first_ids.contains_key(&stick_id),
        "a new file gets a new id"
    );
    assert_eq!(catalog.path_of(&stick_id).unwrap(), on_stick);

    // The stick is pulled out: its id no longer resolves, the others still name their files.
    std::fs::remove_dir_all(&usb).unwrap();
    let third = catalog.list(recovery::collect(&paths, &[])).unwrap();
    assert_eq!(names_by_id(&third), first_ids);
    assert_eq!(
        catalog.path_of(&stick_id).unwrap_err().code(),
        "backup_not_found"
    );
    assert_eq!(catalog.path_of(&chosen).unwrap(), yesterday);

    // Another stick with another backup gets a fresh id, never the retired one.
    let other = dir.path().join("other");
    let elsewhere = write(
        &mirror::drive_folder(&other),
        now - 30_000,
        BackupKind::Manual,
    );
    let fourth = catalog
        .list(recovery::collect(&paths, std::slice::from_ref(&other)))
        .unwrap();
    let other_id = fourth
        .iter()
        .find(|b| b.location == RecoveryLocation::Drive)
        .unwrap()
        .id
        .clone();
    assert_ne!(other_id, stick_id);
    assert!(!first_ids.contains_key(&other_id));
    assert_eq!(catalog.path_of(&other_id).unwrap(), elsewhere);
    assert_eq!(
        catalog.path_of(&stick_id).unwrap_err().code(),
        "backup_not_found"
    );

    // The first stick back in: its file is named by its old id again.
    let back = write(
        &mirror::drive_folder(&usb),
        now - 60_000,
        BackupKind::Manual,
    );
    assert_eq!(back, on_stick);
    let fifth = catalog
        .list(recovery::collect(&paths, &[usb.clone(), other.clone()]))
        .unwrap();
    assert_eq!(fifth.len(), 4);
    assert!(fifth.iter().any(|b| b.id == stick_id));
    assert_eq!(catalog.path_of(&stick_id).unwrap(), on_stick);
    assert_eq!(catalog.path_of(&other_id).unwrap(), elsewhere);
    assert_eq!(catalog.path_of(&chosen).unwrap(), yesterday);
}

#[test]
fn an_id_whose_file_vanished_since_the_listing_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let usb = dir.path().join("usb");
    let on_stick = write(
        &mirror::drive_folder(&usb),
        now_ms() - 60_000,
        BackupKind::Manual,
    );
    let catalog = RecoveryCatalog::default();
    assert_eq!(
        catalog.path_of("r0").unwrap_err().code(),
        "backup_not_found",
        "nothing listed yet"
    );
    let listed = catalog
        .list(recovery::collect(&paths, std::slice::from_ref(&usb)))
        .unwrap();
    let path = catalog.path_of(&listed[0].id).unwrap();
    assert_eq!(recovery::still_there(path.clone()).unwrap(), on_stick);
    std::fs::remove_dir_all(&usb).unwrap(); // pulled out between two polls
    assert_eq!(
        recovery::still_there(path).unwrap_err().code(),
        "backup_not_found"
    );
    assert_eq!(
        catalog.path_of("r99").unwrap_err().code(),
        "backup_not_found"
    );
}

/// Whether this PC is a till comes from `local.json` (a damaged database cannot say).
/// A till is never restored behind its main PC: the fatal screen offers to join it again. The
/// demo shop is a database of its own, so it is never treated as the till's copy.
#[test]
fn a_till_joins_its_main_pc_again_instead_of_restoring_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    let corrupt = pos_core::error::AppError::conflict("database_corrupt");
    assert_eq!(paths.till_of(), None);
    assert_eq!(crate::state::failure_at(&corrupt, &paths).till_of, None);
    recovery::restore_allowed_here(&paths).unwrap();
    assert_eq!(
        recovery::rejoin_allowed(&paths).unwrap_err().code(),
        "lan_not_paired"
    );

    std::fs::write(
        &paths.local_config,
        r#"{ "lan": { "role": "till", "host": "Supérette El Amel" } }"#,
    )
    .unwrap();
    assert_eq!(paths.till_of().as_deref(), Some("Supérette El Amel"));
    let failure = crate::state::failure_at(&corrupt, &paths);
    assert_eq!(
        (
            failure.error.code.as_str(),
            failure.data_dir.as_str(),
            failure.till_of.as_deref()
        ),
        (
            "database_corrupt",
            paths.data_dir.as_str(),
            Some("Supérette El Amel")
        )
    );
    assert_eq!(
        recovery::restore_allowed_here(&paths).unwrap_err().code(),
        "lan_restore_blocked"
    );
    recovery::rejoin_allowed(&paths).unwrap();

    let demo = AppPaths::for_profile(dir.path(), true);
    assert_eq!(
        demo.local_config, paths.local_config,
        "local.json is shared"
    );
    assert_eq!(demo.till_of(), None);
    recovery::restore_allowed_here(&demo).unwrap();
}
