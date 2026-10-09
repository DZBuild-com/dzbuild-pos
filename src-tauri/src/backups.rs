//! What follows every backup this PC makes, whoever made it (the daily task, the Backups screen,
//! a Z close, an update, closing the app): the verified second copy when a folder is chosen,
//! journaled either way (DAT-008/010), and the outcome the banners and the tray read. Also the
//! backup when the app closes (DAT-024) and what an integrity check that fails, or cannot run,
//! raises.

use crate::state::AppState;
use pos_core::audit::{self, Actor};
use pos_core::db::{BackupInfo, BackupKind, Retention, mirror};
use pos_core::error::AppError;
use pos_core::settings;
use pos_core::time::{business_date, now_ms};
use serde_json::json;
use std::path::{Path, PathBuf};

/// DAT-024: closing backs up unless a backup is younger than this (a Z close just before).
pub const CLOSE_BACKUP_AFTER_MS: i64 = 30 * 60 * 1000;

/// The second copy of `info`, when this PC chose a folder; `None` when it did not. A good copy
/// rotates the automatic copies this database logged making in that folder (read before this
/// copy is logged, so the copy just made is never among them); another PC's copies in a shared
/// folder are never touched. The outcome is journaled (`backup.mirrored` or
/// `backup.mirror_failed`) and kept for the banner. The demo shop never makes one: its
/// copies stay in `demo\backups`, away from the real shop's stick or folder.
pub fn after_backup(
    st: &AppState,
    info: &BackupInfo,
    actor: Actor,
) -> Option<Result<BackupInfo, AppError>> {
    if st.paths.demo {
        return None;
    }
    let folder = st.local.get().backup_mirror?;
    let dir = PathBuf::from(&folder.path);
    let outcome = mirror::copy_verified(Path::new(&info.path), &dir);
    match &outcome {
        Ok(_) => rotate(st, &dir, &info.file_name),
        Err(error) => tracing::warn!(%error, file = %info.file_name, "second copy failed"),
    }
    if let Err(error) = st
        .db
        .record_mirror(actor, &folder.path, &info.file_name, &outcome)
    {
        tracing::error!(%error, "could not journal the second copy");
    }
    st.backups
        .record_mirror(outcome.as_ref().err().map(AppError::code));
    Some(outcome)
}

/// Rotation of the mirror folder `dir` after a good copy of `file`: only the copies this
/// database logged there are candidates; when that log cannot be read, nothing is removed.
fn rotate(st: &AppState, dir: &Path, file: &str) {
    let retention = st.db.read(Retention::from_settings).unwrap_or_default();
    let logged = dir.to_path_buf();
    let removed = match st.db.read(move |c| mirror::logged_copies_in(c, &logged)) {
        Ok(owned) => mirror::prune(dir, retention, business_date(now_ms()), &owned),
        Err(error) => {
            tracing::warn!(%error, "could not read which copies this PC made; nothing rotated");
            0
        }
    };
    tracing::info!(file, removed, "second copy done");
}

/// DAT-024: back up on close when it is enabled and no backup is younger than
/// [`CLOSE_BACKUP_AFTER_MS`]; a backup dated in the future (a clock set back) is not recent.
pub fn should_backup_on_close(enabled: bool, newest_at: Option<i64>, now: i64) -> bool {
    enabled && newest_at.is_none_or(|at| at > now || now - at >= CLOSE_BACKUP_AFTER_MS)
}

/// `backup.on_close` is on and the newest backup is 30 minutes old or more (or none exists).
pub fn close_backup_due(st: &AppState) -> bool {
    let enabled = st
        .db
        .read(|c| settings::get::<bool>(c, "backup.on_close"))
        .unwrap_or(true);
    let newest = st
        .db
        .list_backups()
        .ok()
        .and_then(|list| list.iter().map(|b| b.at).max());
    should_backup_on_close(enabled, newest, now_ms())
}

/// The backup when the app closes, journaled `backup.created { kind: "auto", reason: "on_close" }`
/// and mirrored. A failure is logged: closing never waits on a broken disk.
pub fn backup_on_close(st: &AppState) {
    let actor = st.actor(st.current_session().ok().flatten().as_ref());
    match st.db.backup(BackupKind::Auto) {
        Ok(info) => {
            let logged = info.clone();
            let journal = actor.clone();
            if let Err(error) = st.db.write(move |tx| {
                audit::append(
                    tx,
                    &journal.event(
                        "backup.created",
                        "backup",
                        Some(&logged.file_name),
                        json!({ "size": logged.size, "kind": logged.kind, "reason": "on_close" }),
                    ),
                )
                .map(drop)
            }) {
                tracing::warn!(%error, "could not journal the backup on close");
            }
            after_backup(st, &info, actor);
        }
        Err(error) => tracing::error!(%error, "the backup on close failed"),
    }
}

/// The weekly integrity check's outcome (`Db::integrity_check_if_due`). A failed check, or one
/// that could not run, means the live database cannot be trusted: both raise the "stop selling"
/// alert. pos-core journals a check that ran; one that could not run is journaled here.
pub fn watch_integrity(st: &AppState, actor: Actor, checked: Result<Option<bool>, AppError>) {
    match checked {
        Ok(Some(false)) => {
            tracing::error!("the weekly integrity check failed: stop selling and restore");
            st.backups.mark_damaged();
        }
        Ok(_) => {}
        Err(error) => integrity_check_failed_to_run(st, actor, true, &error),
    }
}

/// An integrity check that could not run (a disk that no longer reads, a database the check
/// cannot open): journaled `db.integrity_checked { ok: false, scheduled, error }`, and the
/// database counts as damaged.
pub fn integrity_check_failed_to_run(
    st: &AppState,
    actor: Actor,
    scheduled: bool,
    error: &AppError,
) {
    tracing::error!(%error, scheduled, "the integrity check could not run: stop selling and restore");
    let code = error.code();
    let journaled = st.db.write(move |tx| {
        audit::append(
            tx,
            &actor.event(
                "db.integrity_checked",
                "database",
                None,
                json!({ "ok": false, "scheduled": scheduled, "error": code }),
            ),
        )
        .map(drop)
    });
    if let Err(error) = journaled {
        tracing::error!(%error, "could not journal the integrity check that did not run");
    }
    st.backups.mark_damaged();
}

/// The canonical form of `path` when it exists (`\?\C:\…` on Windows), else `path` itself.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Folder names of the sync clients that copy a folder to the Internet.
const SYNCED_NAMES: [&str; 4] = ["onedrive", "google drive", "my drive", "dropbox"];

/// Windows marks the files and folders of a cloud provider (OneDrive's Files On-Demand and the
/// cloud-files API others use): pinned, unpinned, recalled on open or on access.
#[cfg(windows)]
fn cloud_attributes(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const RECALL_ON_OPEN: u32 = 0x0004_0000;
    const PINNED: u32 = 0x0008_0000;
    const UNPINNED: u32 = 0x0010_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    std::fs::metadata(path).is_ok_and(|meta| {
        meta.file_attributes() & (RECALL_ON_OPEN | PINNED | UNPINNED | RECALL_ON_DATA_ACCESS) != 0
    })
}

#[cfg(not(windows))]
fn cloud_attributes(_path: &Path) -> bool {
    false
}

/// The folders OneDrive syncs on this PC (`%OneDrive%`, `%OneDriveCommercial%`,
/// `%OneDriveConsumer%`).
pub fn synced_roots() -> Vec<PathBuf> {
    ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"]
        .into_iter()
        .filter_map(std::env::var_os)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Why `folder` cannot be this PC's second copy, as an error code; `None` when it can.
/// - `mirror_invalid`: the data folder itself or any folder inside it (the backups, the demo,
///   the `corrupt-*` folders): the copy would sit on the same disk, in the same folder, as what
///   it must survive.
/// - `mirror_cloud_folder`: a folder a sync client copies to the Internet (under `synced_roots`,
///   named like OneDrive, Google Drive or Dropbox, or carrying Windows' cloud-file attributes):
///   unencrypted copies of the fiscal books, the customers and the PIN hashes would leave the PC
///   without the owner linking anything.
pub fn mirror_refusal(folder: &Path, data_dir: &Path, synced: &[PathBuf]) -> Option<&'static str> {
    let folder = canonical(folder);
    let data = canonical(data_dir);
    if folder.starts_with(&data) {
        return Some("mirror_invalid");
    }
    if synced
        .iter()
        .map(|root| canonical(root))
        .any(|root| folder.starts_with(root))
    {
        return Some("mirror_cloud_folder");
    }
    let named = folder.components().any(|part| {
        let part = part.as_os_str().to_string_lossy().to_lowercase();
        SYNCED_NAMES.iter().any(|name| part.contains(name))
    });
    if named || folder.ancestors().any(cloud_attributes) {
        return Some("mirror_cloud_folder");
    }
    None
}
