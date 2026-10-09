//! The guided restore of the fatal screen (Task 3): answers only when the database did not open.
//! Backups are listed from the backups folder, this PC's second-copy folder and every USB stick,
//! and are addressed by ids (`r0`, `r1`…) that only Rust maps to paths. An id names one file for
//! the life of the process (see `RecoveryIds`). A till is never restored: when its copy
//! cannot be used (`REJOINABLE`), it sets that copy aside and joins its main PC again.

use super::blocking;
use crate::state::{AppPaths, StartupFailure, StartupStatus};
use pos_core::db::recovery::{self, BackupPreview, GuidedRestore, SetAside};
use pos_core::db::{BackupKind, list_backups_in, mirror};
use pos_core::error::AppError;
use pos_core::time::now_ms;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryLocation {
    Local,
    Mirror,
    Drive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryBackup {
    pub id: String,
    pub file_name: String,
    /// When the backup was made, from its file name (a copy keeps it, unlike the file's date).
    pub at: i64,
    pub size: i64,
    pub kind: BackupKind,
    pub location: RecoveryLocation,
    /// The folder or the drive shown to the owner (`E:\`, `\\BUREAU\sauvegardes`).
    pub place: Option<String>,
}

/// The ids handed to the fatal screen and the files they name. The screen polls the list every
/// 5 s while the owner chooses, previews and types the PIN, and a stick or a share can come or
/// go meanwhile. So an id names one file for the life of the process: a file listed again keeps
/// its id, a new file gets the next number, and an id is never given to another file. Only the
/// files of the latest listing resolve: a stick pulled out takes its ids with it.
#[derive(Debug, Default)]
struct RecoveryIds {
    /// Every id given so far and its file.
    paths: HashMap<String, PathBuf>,
    /// The way back, so a file listed again keeps its id.
    ids: HashMap<PathBuf, String>,
    /// The ids of the latest listing.
    listed: HashSet<String>,
    next: u64,
}

impl RecoveryIds {
    fn id_of(&mut self, path: PathBuf) -> String {
        if let Some(id) = self.ids.get(&path) {
            return id.clone();
        }
        let id = format!("r{}", self.next);
        self.next += 1;
        self.paths.insert(id.clone(), path.clone());
        self.ids.insert(path, id.clone());
        id
    }
}

#[derive(Default)]
pub struct RecoveryCatalog(Mutex<RecoveryIds>);

#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRestoreInput {
    pub id: String,
    pub owner_id: String,
    pub pin: String,
}

impl std::fmt::Debug for RecoveryRestoreInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryRestoreInput")
            .field("id", &self.id)
            .field("owner_id", &self.owner_id)
            .field("pin", &"<redacted>")
            .finish()
    }
}

/// The guided restore is the fatal screen's: once the database opened, a restore goes through
/// Settings › Backups (owner session, PIN, the live journal).
pub(crate) fn recovery_allowed(startup_failed: bool) -> Result<(), AppError> {
    if startup_failed {
        Ok(())
    } else {
        Err(AppError::conflict("recovery_not_available"))
    }
}

/// A till is never restored behind its main PC (`local.json` says it is a till; its
/// damaged database cannot): it joins the main PC again instead (`recovery_rejoin`).
pub(crate) fn restore_allowed_here(paths: &AppPaths) -> Result<(), AppError> {
    match paths.till_of() {
        Some(_) => Err(AppError::conflict("lan_restore_blocked")),
        None => Ok(()),
    }
}

/// The start failures a till repairs by setting its copy aside: the copy cannot be used
/// (damaged, missing, or a restore stopped half way). Any other failure keeps the files where
/// they are: a locked file, a disk error or a failed migration (`database_error`) can pass on a
/// retry, a database from a newer version opens in the latest version, and the copy may hold
/// sales its main PC never received. The fatal screen offers the move for these codes only.
pub(crate) const REJOINABLE: [&str; 3] = [
    "database_corrupt",
    "database_missing",
    "restore_needs_attention",
];

/// `recovery_rejoin` answers only after a start that failed with one of [`REJOINABLE`], read
/// from the startup's own failure (never from the webview); `recovery_not_available` otherwise.
pub(crate) fn rejoin_after(failure: Option<&StartupFailure>) -> Result<(), AppError> {
    match failure {
        Some(failure) if REJOINABLE.contains(&failure.error.code.as_str()) => Ok(()),
        _ => Err(AppError::conflict("recovery_not_available")),
    }
}

/// Only a till sets its damaged copy aside to join its main PC again; any other PC restores a
/// backup.
pub(crate) fn rejoin_allowed(paths: &AppPaths) -> Result<(), AppError> {
    match paths.till_of() {
        Some(_) => Ok(()),
        None => Err(AppError::conflict("lan_not_paired")),
    }
}

/// Every backup found, newest first, each place's copy listed with its own id and place: a copy
/// of the same name on the second copy or a stick is never hidden behind the local one, which
/// may be the damaged one when the local disk is failing (the preview tells them apart).
pub(crate) fn collect(
    paths: &AppPaths,
    drive_mounts: &[PathBuf],
) -> Vec<(RecoveryBackup, PathBuf)> {
    let mut places: Vec<(RecoveryLocation, PathBuf, Option<String>)> = vec![(
        RecoveryLocation::Local,
        PathBuf::from(&paths.backups_dir),
        None,
    )];
    if let Some(folder) =
        pos_core::local::LocalConfig::load(Path::new(&paths.local_config)).backup_mirror
    {
        places.push((
            RecoveryLocation::Mirror,
            PathBuf::from(&folder.path),
            Some(folder.path.clone()),
        ));
    }
    for mount in drive_mounts {
        places.push((
            RecoveryLocation::Drive,
            mirror::drive_folder(mount),
            Some(mount.to_string_lossy().into_owned()),
        ));
    }
    let mut found = Vec::new();
    for (location, dir, place) in places {
        let listed = list_backups_in(&dir).unwrap_or_else(|error| {
            tracing::warn!(%error, ?location, "could not list a backups folder for the restore");
            Vec::new()
        });
        for info in listed {
            let path = PathBuf::from(&info.path);
            found.push((
                RecoveryBackup {
                    id: String::new(),
                    at: pos_core::db::time_of(&info.file_name).unwrap_or(info.at),
                    file_name: info.file_name,
                    size: info.size,
                    kind: info.kind,
                    location,
                    place: place.clone(),
                },
                path,
            ));
        }
    }
    found.sort_by(|a, b| {
        b.0.at
            .cmp(&a.0.at)
            .then_with(|| (a.0.location as u8).cmp(&(b.0.location as u8)))
    });
    found
}

impl RecoveryCatalog {
    fn ids(&self) -> Result<MutexGuard<'_, RecoveryIds>, AppError> {
        self.0
            .lock()
            .map_err(|_| AppError::Internal("recovery catalog poisoned".into()))
    }

    /// Gives every backup of a listing its id (a file listed before keeps its own) and makes this
    /// listing the one ids resolve against.
    pub(crate) fn list(
        &self,
        found: Vec<(RecoveryBackup, PathBuf)>,
    ) -> Result<Vec<RecoveryBackup>, AppError> {
        let mut guard = self.ids()?;
        let ids = &mut *guard;
        ids.listed.clear();
        Ok(found
            .into_iter()
            .map(|(mut backup, path)| {
                backup.id = ids.id_of(path);
                ids.listed.insert(backup.id.clone());
                backup
            })
            .collect())
    }

    /// The file `id` names, when the latest listing showed it; `backup_not_found` otherwise
    /// (never an id that was never given, nor one whose stick was pulled out).
    pub(crate) fn path_of(&self, id: &str) -> Result<PathBuf, AppError> {
        let ids = self.ids()?;
        ids.listed
            .contains(id)
            .then(|| ids.paths.get(id).cloned())
            .flatten()
            .ok_or_else(|| AppError::not_found("backup"))
    }
}

/// The file behind an id, still on disk: a stick pulled out since the latest listing is
/// `backup_not_found` (refresh the list), not a damaged backup.
pub(crate) fn still_there(path: PathBuf) -> Result<PathBuf, AppError> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(AppError::not_found("backup"))
    }
}

#[tauri::command]
#[specta::specta]
pub async fn recovery_list(
    paths: State<'_, AppPaths>,
    status: State<'_, StartupStatus>,
    catalog: State<'_, RecoveryCatalog>,
) -> Result<Vec<RecoveryBackup>, AppError> {
    recovery_allowed(status.outcome().await.is_some())?;
    let paths = paths.inner().clone();
    let found = blocking(move || {
        let mounts: Vec<PathBuf> = crate::disks::removable_drives()
            .into_iter()
            .map(|d| PathBuf::from(d.mount))
            .collect();
        Ok(collect(&paths, &mounts))
    })
    .await?;
    catalog.list(found)
}

#[tauri::command]
#[specta::specta]
pub async fn recovery_inspect(
    status: State<'_, StartupStatus>,
    catalog: State<'_, RecoveryCatalog>,
    id: String,
) -> Result<BackupPreview, AppError> {
    recovery_allowed(status.outcome().await.is_some())?;
    let path = catalog.path_of(&id)?;
    blocking(move || Ok(recovery::inspect_backup(&still_there(path)?))).await
}

/// Owner PIN from the backup, damaged files moved aside, restore scheduled, restart.
#[tauri::command]
#[specta::specta]
pub async fn recovery_restore(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    status: State<'_, StartupStatus>,
    catalog: State<'_, RecoveryCatalog>,
    input: RecoveryRestoreInput,
) -> Result<GuidedRestore, AppError> {
    recovery_allowed(status.outcome().await.is_some())?;
    let source = catalog.path_of(&input.id)?;
    let paths = paths.inner().clone();
    let db_path = PathBuf::from(&paths.db_path);
    let backups_dir = PathBuf::from(&paths.backups_dir);
    let done = blocking(move || {
        restore_allowed_here(&paths)?;
        let source = still_there(source)?;
        recovery::restore_guard(&source, paths.demo)?;
        let data_dir = db_path.parent().map(Path::to_path_buf).unwrap_or_default();
        let now = now_ms();
        recovery::check_owner_pin(&data_dir, &source, &input.owner_id, &input.pin, now)?;
        recovery::schedule_guided_restore(&db_path, &backups_dir, &source, &input.owner_id, now)
    })
    .await?;
    tracing::warn!(damaged = %done.damaged_folder, from = %done.restored_from, "guided restore scheduled");
    super::backup::restart_soon(app);
    Ok(done)
}

/// Finding 10: the owner's "create a new shop anyway" answers only after a start that stopped on
/// `database_missing` (the database is gone, backups are there), never on a till (it joins its
/// main PC again).
pub(crate) fn fresh_allowed(
    failure: Option<&StartupFailure>,
    paths: &AppPaths,
) -> Result<(), AppError> {
    match failure {
        Some(failure) if failure.error.code == "database_missing" => restore_allowed_here(paths),
        _ => Err(AppError::conflict("recovery_not_available")),
    }
}

/// The owner's PIN, checked against the chosen backup's users (the shop's last known owners),
/// then `fresh.start` lets the next start make an empty shop although backups exist; that shop
/// journals `recovery.fresh_start`. The backups stay where they are.
pub(crate) fn start_fresh(
    paths: &AppPaths,
    source: &Path,
    owner_id: &str,
    pin: &str,
    now: i64,
) -> Result<(), AppError> {
    restore_allowed_here(paths)?;
    let db_path = PathBuf::from(&paths.db_path);
    let data_dir = db_path.parent().map(Path::to_path_buf).unwrap_or_default();
    recovery::check_owner_pin(&data_dir, source, owner_id, pin, now)?;
    std::fs::write(data_dir.join(recovery::FRESH_MARKER), now.to_string())?;
    Ok(())
}

/// "Créer une nouvelle boutique quand même": owner PIN from a backup, then the app restarts on
/// an empty shop (finding 10). Only after a start that stopped on `database_missing`.
#[tauri::command]
#[specta::specta]
pub async fn recovery_start_fresh(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    status: State<'_, StartupStatus>,
    catalog: State<'_, RecoveryCatalog>,
    input: RecoveryRestoreInput,
) -> Result<(), AppError> {
    let paths = paths.inner().clone();
    fresh_allowed(status.outcome().await.as_ref(), &paths)?;
    let source = catalog.path_of(&input.id)?;
    blocking(move || {
        let source = still_there(source)?;
        start_fresh(&paths, &source, &input.owner_id, &input.pin, now_ms())
    })
    .await?;
    tracing::warn!("the owner chose a new, empty shop although backups exist");
    super::backup::restart_soon(app);
    Ok(())
}

/// A till's damaged files are moved aside (never deleted) and the app restarts as a new
/// PC, which joins its main PC again to get its copy back. Only after a start that failed with
/// one of [`REJOINABLE`].
#[tauri::command]
#[specta::specta]
pub async fn recovery_rejoin(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    status: State<'_, StartupStatus>,
) -> Result<SetAside, AppError> {
    rejoin_after(status.outcome().await.as_ref())?;
    let paths = paths.inner().clone();
    let done = blocking(move || {
        rejoin_allowed(&paths)?;
        recovery::set_aside_for_pairing(
            Path::new(&paths.db_path),
            Path::new(&paths.backups_dir),
            now_ms(),
        )
    })
    .await?;
    tracing::warn!(damaged = %done.damaged_folder, "a till's damaged copy set aside to join its main PC again");
    super::backup::restart_soon(app);
    Ok(done)
}
