//! M6 review, findings 7, 10 and 33: a `pos.db` that disappears while the shop's backups are
//! there stops the start on the guided restore instead of making an empty shop; the owner's
//! "new shop anyway" and a till set aside to pair again make their empty start on purpose, and
//! both are journaled in the new database.

use super::recovery;
use super::tests::first_run;
use crate::state::{AppPaths, AppState};
use pos_core::db::recovery::{FRESH_MARKER, REJOIN_MARKER};
use std::path::Path;

fn open(paths: &AppPaths) -> Result<AppState, pos_core::error::AppError> {
    AppState::open(paths, crate::printing::silent())
}

fn count(st: &AppState, action: &'static str) -> i64 {
    st.db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = ?1",
                [action],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

/// A shop with an owner (PIN 2580) and one manual backup, closed; `pos.db` then deleted.
fn lost_shop(dir: &Path) -> (AppPaths, String, std::path::PathBuf) {
    let paths = AppPaths::new(dir);
    let st = open(&paths).unwrap();
    let owner = first_run(&st);
    let backup = st.db.backup(pos_core::db::BackupKind::Manual).unwrap();
    st.db.mark_clean_shutdown().unwrap();
    drop(st);
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", paths.db_path));
    }
    (paths, owner.user_id, std::path::PathBuf::from(backup.path))
}

#[test]
fn a_lost_database_with_backups_stops_on_the_guided_restore() {
    let dir = tempfile::tempdir().unwrap();
    let (paths, owner, backup) = lost_shop(dir.path());
    let refused = open(&paths).err().map(|e| e.code());
    assert_eq!(refused.as_deref(), Some("database_missing"));
    assert!(
        !Path::new(&paths.db_path).exists(),
        "no empty shop was made in its place"
    );
    let failure = crate::state::failure_at(
        &pos_core::error::AppError::conflict("database_missing"),
        &paths,
    );
    recovery::fresh_allowed(Some(&failure), &paths).unwrap();
    // A wrong PIN changes nothing; the owner's PIN of the backup lets the next start begin anew.
    assert_eq!(
        recovery::start_fresh(&paths, &backup, &owner, "0000", pos_core::time::now_ms())
            .unwrap_err()
            .code(),
        "invalid_pin"
    );
    assert!(!dir.path().join(FRESH_MARKER).exists());
    recovery::start_fresh(&paths, &backup, &owner, "2580", pos_core::time::now_ms()).unwrap();
    let fresh = open(&paths).unwrap();
    assert!(!fresh.db.read(pos_core::setup::status).unwrap().completed);
    assert_eq!(count(&fresh, "recovery.fresh_start"), 1);
    assert!(!dir.path().join(FRESH_MARKER).exists());
    assert!(backup.exists(), "the backups stay");
    // Any other failure never offers it.
    let corrupt = crate::state::failure_at(
        &pos_core::error::AppError::conflict("database_corrupt"),
        &paths,
    );
    assert_eq!(
        recovery::fresh_allowed(Some(&corrupt), &paths)
            .unwrap_err()
            .code(),
        "recovery_not_available"
    );
}

#[test]
fn a_new_install_and_a_till_set_aside_start_empty_on_purpose() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    // A new install: no backup anywhere.
    let st = open(&paths).unwrap();
    first_run(&st);
    st.db.backup(pos_core::db::BackupKind::Manual).unwrap();
    st.db.mark_clean_shutdown().unwrap();
    drop(st);
    // The damaged copy of a till set aside: the next start makes its empty database.
    let set_aside = pos_core::db::recovery::set_aside_for_pairing(
        Path::new(&paths.db_path),
        Path::new(&paths.backups_dir),
        pos_core::time::now_ms(),
    )
    .unwrap();
    assert!(!set_aside.damaged_folder.is_empty());
    let note = pos_core::db::recovery::rejoin_note(dir.path()).unwrap();
    assert_eq!(note.moved, set_aside.moved);
    let empty = open(&paths).unwrap();
    assert!(!empty.db.read(pos_core::setup::status).unwrap().completed);
    assert!(
        dir.path().join(REJOIN_MARKER).exists(),
        "kept until this PC is set up again"
    );
    first_run(&empty);
    empty.db.mark_clean_shutdown().unwrap();
    drop(empty);
    let again = open(&paths).unwrap();
    assert_eq!(
        count(&again, "recovery.set_aside"),
        1,
        "set up as a new shop"
    );
    assert!(!dir.path().join(REJOIN_MARKER).exists());
}
