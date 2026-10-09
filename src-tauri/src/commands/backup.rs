//! Backup and restore commands (spec §13). Restore = owner + PIN + restart. M6: the second copy
//! (a folder this PC chose), a one-off copy to a USB stick, the preview of a backup and the
//! integrity check of the live database.

use super::blocking;
use crate::state::{AppState, BackupStatus, SharedState};
use pos_core::audit;
use pos_core::auth::{self, Permission, Session};
use pos_core::db::recovery::BackupPreview;
use pos_core::db::{BackupInfo, BackupKind, mirror};
use pos_core::error::AppError;
use pos_core::local::MirrorFolder;
use pos_core::time::now_ms;
use serde::Serialize;
use serde_json::json;
use specta::Type;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, State, Window};

#[tauri::command]
#[specta::specta]
pub async fn backup_now(state: State<'_, SharedState>) -> Result<BackupInfo, AppError> {
    let st = state.inner().clone();
    blocking(move || backup_now_impl(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn backup_list(state: State<'_, SharedState>) -> Result<Vec<BackupInfo>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st)).await
}

/// The automatic backup's outcome and the newest backup's age, for the banner every signed-in
/// user sees (a cashier tells the owner). Reads the backups folder.
#[tauri::command]
#[specta::specta]
pub async fn backup_status(state: State<'_, SharedState>) -> Result<BackupStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || status(&st)).await
}

/// On success the database is already closed to writes and the app restarts 600 ms later.
#[tauri::command]
#[specta::specta]
pub async fn backup_restore(
    app: AppHandle,
    state: State<'_, SharedState>,
    file_name: String,
    pin: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || restore(&st, &file_name, &pin)).await?;
    restart_soon(app);
    Ok(())
}

/// "Sauvegarder maintenant": a manual backup, then its second copy when a folder is chosen (a
/// failed second copy is journaled and shown on the banner; the local backup stands).
pub(crate) fn backup_now_impl(st: &AppState) -> Result<BackupInfo, AppError> {
    let session = st.require(Permission::SettingsData)?;
    let info = backup_now_impl_without_mirror(st, &session)?;
    crate::backups::after_backup(st, &info, session.actor());
    Ok(info)
}

/// The manual backup and its `backup.created` row, without the second copy (a backup to a USB
/// stick copies it there itself).
fn backup_now_impl_without_mirror(
    st: &AppState,
    session: &Session,
) -> Result<BackupInfo, AppError> {
    let info = st.db.backup(BackupKind::Manual)?;
    let actor = session.actor();
    let logged = info.clone();
    st.db.write(move |tx| {
        let event = actor.event(
            "backup.created",
            "backup",
            Some(&logged.file_name),
            json!({ "size": logged.size, "kind": logged.kind }),
        );
        audit::append(tx, &event)?;
        Ok(())
    })?;
    Ok(info)
}

pub(crate) fn list(st: &AppState) -> Result<Vec<BackupInfo>, AppError> {
    st.require(Permission::SettingsData)?;
    st.db.list_backups()
}

pub(crate) fn status(st: &AppState) -> Result<BackupStatus, AppError> {
    st.session()?;
    Ok(st.backup_status(now_ms()))
}

/// Checks owner + PIN, journals the request (kept in the pre-restore copy), schedules the swap
/// with the owner as requester (the restored database journals it, anchored to the journal it
/// replaces), then closes the database to writes at once: from here the next start replaces
/// it, so nothing may be written before the restart.
pub(crate) fn restore(st: &AppState, file_name: &str, pin: &str) -> Result<BackupInfo, AppError> {
    let session = st.require_owner(Permission::SettingsData)?;
    // A till's copy comes from the main PC, and a main PC with tills would go back under them
    // (`lan_restore_blocked`): the till is paired again, the tills leave first.
    st.db.read(pos_core::sync::role::restore_allowed)?;
    auth::confirm_pin(&st.db, &st.terminal, &session, pin, now_ms())?;
    let found = st
        .db
        .list_backups()?
        .into_iter()
        .find(|b| b.file_name == file_name)
        .ok_or_else(|| AppError::not_found("backup"))?;
    // Read from the copy itself: never the demo's copy in the real shop's place (nor the
    // reverse), never a till's copy, never a main PC's copy that has tills.
    pos_core::db::recovery::restore_guard(Path::new(&found.path), st.paths.demo)?;
    let actor = session.actor();
    let name = file_name.to_owned();
    st.db.write(move |tx| {
        let event = actor.event("backup.restore_scheduled", "backup", Some(&name), json!({}));
        audit::append(tx, &event)?;
        Ok(())
    })?;
    let info = st.db.schedule_restore(file_name, Some(&session.user_id))?;
    // The marker is written, so the restart must happen whatever this returns. The writer shuts
    // writes out first thing in this job, even when writing the mark then fails.
    if let Err(error) = st.db.mark_clean_shutdown() {
        tracing::error!(%error, "could not mark the database closed before the restore restart");
    }
    Ok(info)
}

/// This process may still hold `pos.db`/`-wal`/`-shm` while the new one starts: the new process's
/// `apply_pending_restore` retries the swap with backoff for about 10 s (Task 5), and a swap that
/// never succeeds keeps the current data and shows `StartupNotice::RestoreFailed` (Tasks 14, 20).
/// The LAN restarts the same way once a main PC's copy is staged or a till leaves the network.
pub(crate) fn restart_soon(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("pos-restart".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            app.restart();
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "could not schedule the restart");
    }
}

/// This PC's second copy, for the Backups screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MirrorStatus {
    pub folder: Option<String>,
    pub label: Option<String>,
    /// The folder answers now (the stick is plugged in, the share is reachable).
    pub available: bool,
    pub free_bytes: Option<u64>,
    /// The newest good copy logged in that folder, even when it is out now.
    pub last_copy_at: Option<i64>,
    /// This run's last failed copy (`mirror_unavailable`, `backup_invalid`, `io_error`).
    pub last_error: Option<String>,
}

pub(crate) fn mirror_status(st: &AppState) -> Result<MirrorStatus, AppError> {
    st.require(Permission::SettingsData)?;
    // The demo shop has no second copy: the folder in `local.json` is the real shop's.
    let mirror = st.local.get().backup_mirror.filter(|_| !st.paths.demo);
    let Some(folder) = mirror else {
        return Ok(MirrorStatus {
            folder: None,
            label: None,
            available: false,
            free_bytes: None,
            last_copy_at: None,
            last_error: None,
        });
    };
    let dir = PathBuf::from(&folder.path);
    let available = dir.is_dir();
    let path = folder.path.clone();
    Ok(MirrorStatus {
        available,
        free_bytes: if available {
            crate::disks::free_space(&dir)
        } else {
            None
        },
        last_copy_at: st.db.read(move |c| mirror::last_copy_in(c, &path))?,
        last_error: st.backup_status(now_ms()).mirror_error,
        folder: Some(folder.path),
        label: folder.label,
    })
}

/// "Ne plus faire de copie de secours": the folder is forgotten (journaled `local.updated`) and
/// the banner of a failed copy goes with it.
pub(crate) fn mirror_clear(st: &AppState) -> Result<MirrorStatus, AppError> {
    st.demo_guard()?;
    let session = st.require(Permission::SettingsData)?;
    st.local
        .update(&st.db, session.actor(), |c| c.backup_mirror = None)?;
    st.backups.record_mirror(None);
    mirror_status(st)
}

/// Keeps `folder` as this PC's second copy and copies the newest local backup there at once, so
/// the owner sees it works before leaving the screen.
pub(crate) fn mirror_set(st: &AppState, folder: PathBuf) -> Result<MirrorStatus, AppError> {
    st.demo_guard()?;
    let session = st.require(Permission::SettingsData)?;
    if let Some(code) = crate::backups::mirror_refusal(
        &folder,
        Path::new(&st.paths.data_dir),
        &crate::backups::synced_roots(),
    ) {
        return Err(AppError::conflict(code));
    }
    let label = crate::disks::removable_drives()
        .into_iter()
        .find(|d| folder.starts_with(&d.mount))
        .map(|d| d.label)
        .filter(|l| !l.is_empty());
    let path = folder.to_string_lossy().into_owned();
    st.local.update(&st.db, session.actor(), |c| {
        c.backup_mirror = Some(MirrorFolder { path, label })
    })?;
    if let Some(newest) = st.db.list_backups()?.into_iter().next() {
        crate::backups::after_backup(st, &newest, session.actor());
    }
    mirror_status(st)
}

#[tauri::command]
#[specta::specta]
pub async fn backup_mirror_status(state: State<'_, SharedState>) -> Result<MirrorStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || mirror_status(&st)).await
}

/// The folder comes from a Rust dialog (`None` when the owner cancels); the webview never names it.
#[tauri::command]
#[specta::specta]
pub async fn backup_mirror_choose(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<MirrorStatus>, AppError> {
    let st = state.inner().clone();
    st.demo_guard()?;
    st.require(Permission::SettingsData)?;
    blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let Some(picked) = app
            .dialog()
            .file()
            .set_parent(&window)
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let folder = picked
            .into_path()
            .map_err(|_| AppError::Io("the chosen folder cannot be used".into()))?;
        mirror_set(&st, folder).map(Some)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn backup_mirror_clear(state: State<'_, SharedState>) -> Result<MirrorStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || mirror_clear(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn backup_drives(
    state: State<'_, SharedState>,
) -> Result<Vec<crate::disks::RemovableDrive>, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        st.require(Permission::SettingsData)?;
        Ok(crate::disks::removable_drives())
    })
    .await
}

/// DAT-008 "Sauvegarder sur clé USB": a fresh manual backup copied to a stick from the current
/// list of removable drives (`<drive>\DZBuild POS\sauvegardes`), verified and journaled.
pub(crate) fn to_drive(st: &AppState, mount: &str) -> Result<BackupInfo, AppError> {
    st.demo_guard()?;
    let session = st.require(Permission::SettingsData)?;
    if !crate::disks::removable_drives()
        .iter()
        .any(|d| d.mount == mount)
    {
        return Err(AppError::conflict("mirror_unavailable"));
    }
    let folder = mirror::prepare_drive_folder(Path::new(mount))?;
    let info = backup_now_impl_without_mirror(st, &session)?;
    let copy = mirror::copy_verified(Path::new(&info.path), &folder);
    st.db.record_mirror(
        session.actor(),
        &folder.to_string_lossy(),
        &info.file_name,
        &copy,
    )?;
    copy
}

#[tauri::command]
#[specta::specta]
pub async fn backup_to_drive(
    state: State<'_, SharedState>,
    mount: String,
) -> Result<BackupInfo, AppError> {
    let st = state.inner().clone();
    blocking(move || to_drive(&st, &mount)).await
}

/// What a backup of the backups folder holds, read without changing it (before a restore).
pub(crate) fn inspect(st: &AppState, file_name: &str) -> Result<BackupPreview, AppError> {
    st.require(Permission::SettingsData)?;
    let found = st
        .db
        .list_backups()?
        .into_iter()
        .find(|b| b.file_name == file_name)
        .ok_or_else(|| AppError::not_found("backup"))?;
    Ok(pos_core::db::recovery::inspect_backup(Path::new(
        &found.path,
    )))
}

#[tauri::command]
#[specta::specta]
pub async fn backup_inspect(
    state: State<'_, SharedState>,
    file_name: String,
) -> Result<BackupPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || inspect(&st, &file_name)).await
}

/// `PRAGMA integrity_check` of the live database, journaled `db.integrity_checked`. A failure,
/// or a check that cannot run, raises the "database damaged" alert.
pub(crate) fn check_database(st: &AppState) -> Result<bool, AppError> {
    let session = st.require(Permission::SettingsData)?;
    match st.db.check_integrity(session.actor(), false) {
        Ok(ok) => {
            if !ok {
                st.backups.mark_damaged();
            }
            Ok(ok)
        }
        Err(error) => {
            crate::backups::integrity_check_failed_to_run(st, session.actor(), false, &error);
            Err(error)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn backup_check_database(state: State<'_, SharedState>) -> Result<bool, AppError> {
    let st = state.inner().clone();
    blocking(move || check_database(&st)).await
}
