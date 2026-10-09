//! Task 11 review: a till sets its copy aside only when that copy cannot be used: its
//! database is damaged, missing, or a restore stopped half way. A locked file, a disk error, a
//! failed migration (`database_error`) or a database from a newer version keeps its files where
//! they are: a retry or the latest version opens them, and they may hold sales the main PC never
//! received. The code comes from the startup's own failure, never from the webview.

use super::recovery;
use crate::state::{StartupFailure, StartupStatus, startup_failure};
use pos_core::error::AppError;

fn failed_with(error: AppError) -> StartupFailure {
    startup_failure(&error, "C:\\data")
}

#[test]
fn a_till_sets_its_copy_aside_only_when_the_copy_cannot_be_used() {
    for code in [
        "database_corrupt",
        "database_missing",
        "restore_needs_attention",
    ] {
        recovery::rejoin_after(Some(&failed_with(AppError::conflict(code))))
            .unwrap_or_else(|e| panic!("{code} lets the till join again: {e:?}"));
    }

    let locked = failed_with(AppError::Db("database is locked".into()));
    assert_eq!(locked.error.code, "database_error");
    let migration = failed_with(AppError::Db("migration: disk I/O error".into()));
    assert_eq!(migration.error.code, "database_error");
    for failure in [
        locked,
        migration,
        failed_with(AppError::conflict("database_newer_than_app")),
        failed_with(AppError::Internal("the start was interrupted".into())),
    ] {
        assert_eq!(
            recovery::rejoin_after(Some(&failure)).unwrap_err().code(),
            "recovery_not_available",
            "{} keeps the till's files",
            failure.error.code
        );
    }

    assert_eq!(
        recovery::rejoin_after(None).unwrap_err().code(),
        "recovery_not_available",
        "the database opened"
    );
}

/// `recovery_rejoin` reads the code the opening thread reported, the same one the fatal screen
/// shows.
#[test]
fn the_startup_status_keeps_the_code_the_rejoin_checks() {
    let (reporter, status) = StartupStatus::channel();
    reporter.finish(Some(failed_with(AppError::Db("disk I/O error".into()))));
    let outcome = tauri::async_runtime::block_on(status.outcome());
    assert_eq!(
        recovery::rejoin_after(outcome.as_ref()).unwrap_err().code(),
        "recovery_not_available"
    );

    let (reporter, status) = StartupStatus::channel();
    reporter.finish(Some(failed_with(AppError::conflict("database_corrupt"))));
    let outcome = tauri::async_runtime::block_on(status.outcome());
    recovery::rejoin_after(outcome.as_ref()).unwrap();
}
