//! A till's database is a copy of its main PC's. When it cannot open, the guided
//! restore must not rewind it behind the main PC: `local.json` says the PC is a till (the
//! damaged database cannot), and the fatal screen sets the damaged files aside, never deleted,
//! so the next start is a new PC that joins the main PC again.

use super::recovery::{MOVING_MARKER, refuse_empty_start, set_aside_for_pairing, till_of};
use super::{Db, DbOptions, RESTORE_FAILED_MARKER, RESTORE_MARKER};
use std::path::{Path, PathBuf};

/// 2026-09-28 09:15:00 in Algiers.
const NOW: i64 = 1_790_583_300_000;

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}

/// A data folder whose `pos.db` is 4 KiB of zeros ("not a database"), with its WAL.
fn damaged(dir: &Path) -> PathBuf {
    let db_path = dir.join("pos.db");
    write(&db_path, &[0u8; 4096]);
    write(&dir.join("pos.db-wal"), b"wal of the damaged copy");
    db_path
}

#[test]
fn local_json_names_the_main_pc_of_a_till_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("local.json");
    assert_eq!(till_of(&path), None, "no file: a single PC");
    for text in [
        "{}",
        r#"{ "lightMode": true }"#,
        r#"{ "lan": null }"#,
        r#"{ "lan": { "role": "host", "host": "X" } }"#,
        "not json",
    ] {
        write(&path, text.as_bytes());
        assert_eq!(till_of(&path), None, "{text}");
    }
    write(
        &path,
        r#"{ "backupMirror": null, "lan": { "role": "till", "host": "Supérette El Amel" } }"#
            .as_bytes(),
    );
    assert_eq!(till_of(&path).as_deref(), Some("Supérette El Amel"));
    write(&path, br#"{ "lan": { "role": "till" } }"#);
    assert_eq!(
        till_of(&path).as_deref(),
        Some(""),
        "a till whose main PC has no name is still a till"
    );
}

#[test]
fn a_till_s_damaged_files_are_set_aside_and_the_next_start_is_a_new_pc() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = damaged(dir.path());
    let backups = dir.path().join("backups");
    // An earlier restore that failed: kept with the damaged files, else a start without
    // `pos.db` would stop on the fatal screen again (`database_missing`).
    write(
        &dir.path().join(RESTORE_FAILED_MARKER),
        b"x.db\nio_error\nid\n",
    );

    let done = set_aside_for_pairing(&db_path, &backups, NOW).unwrap();

    let folder = PathBuf::from(&done.damaged_folder);
    assert_eq!(folder, backups.join("corrupt-20260928-091500"));
    assert_eq!(done.moved, ["pos.db", "pos.db-wal", RESTORE_FAILED_MARKER]);
    assert_eq!(std::fs::read(folder.join("pos.db")).unwrap(), [0u8; 4096]);
    assert_eq!(
        std::fs::read(folder.join("pos.db-wal")).unwrap(),
        b"wal of the damaged copy"
    );
    assert!(folder.join(RESTORE_FAILED_MARKER).exists());
    for gone in [
        "pos.db",
        "pos.db-wal",
        RESTORE_FAILED_MARKER,
        MOVING_MARKER,
        RESTORE_MARKER,
    ] {
        assert!(!dir.path().join(gone).exists(), "{gone} left behind");
    }
    // Nothing is scheduled: the next start makes a new, empty database.
    refuse_empty_start(&db_path).unwrap();
    let fresh = Db::open(&db_path, DbOptions::default()).unwrap();
    drop(fresh);
}

#[test]
fn a_scheduled_restore_is_never_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = damaged(dir.path());
    write(&dir.path().join(RESTORE_MARKER), b"scheduled");

    let error = set_aside_for_pairing(&db_path, &dir.path().join("backups"), NOW).unwrap_err();

    assert_eq!(error.code(), "restore_already_scheduled");
    assert!(db_path.exists(), "nothing moved");
    assert!(dir.path().join("pos.db-wal").exists());
    assert!(!dir.path().join(MOVING_MARKER).exists());
}

#[test]
fn a_set_aside_cut_short_is_finished_into_the_same_folder() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = damaged(dir.path());
    let backups = dir.path().join("backups");
    // A power cut after `pos.db` moved, before its WAL: `restore.moving` names the folder.
    let folder = backups.join("corrupt-20260928-090000");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::rename(&db_path, folder.join("pos.db")).unwrap();
    write(
        &dir.path().join(MOVING_MARKER),
        folder.to_string_lossy().as_bytes(),
    );
    assert_eq!(
        refuse_empty_start(&db_path).unwrap_err().code(),
        "database_missing",
        "the start stops on the fatal screen"
    );

    let done = set_aside_for_pairing(&db_path, &backups, NOW).unwrap();

    assert_eq!(PathBuf::from(&done.damaged_folder), folder);
    assert_eq!(done.moved, ["pos.db-wal"]);
    assert!(folder.join("pos.db-wal").exists());
    assert!(!dir.path().join(MOVING_MARKER).exists());
    refuse_empty_start(&db_path).unwrap();
}

#[test]
fn nothing_to_set_aside_still_lets_the_next_start_make_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("pos.db");
    // A stale mark naming no folder: the start would refuse to make a database.
    write(&dir.path().join(MOVING_MARKER), b"C:\\nowhere\\corrupt-1");

    let done = set_aside_for_pairing(&db_path, &dir.path().join("backups"), NOW).unwrap();

    assert_eq!(done.damaged_folder, "");
    assert!(done.moved.is_empty());
    refuse_empty_start(&db_path).unwrap();
}
