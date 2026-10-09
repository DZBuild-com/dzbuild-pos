//! The demo shop (ONB-002) and the first-run import of a shop from another PC (DAT-013). The
//! demo is a separate database under `demo\`; entering or leaving it restarts the app on the
//! other profile. Nothing here touches the real `pos.db`.

use super::{blocking, pick_dialog};
use crate::state::{AppPaths, AppState, SharedState};
use pos_core::audit::{self, Actor};
use pos_core::auth::Permission;
use pos_core::db::{Db, DbOptions};
use pos_core::demo::{self as core_demo, DemoSummary};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::time::now_ms;
use pos_docs::archive::{self, BACKUP_EXTENSION};
use serde::Serialize;
use serde_json::json;
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, Window};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DemoStatus {
    /// This run is the demo shop.
    pub active: bool,
    /// A demo shop exists on this PC (it can be erased).
    pub exists: bool,
    /// The demo's PINs, for the login screen's hint: only while the demo is open.
    pub owner_pin: Option<String>,
    pub cashier_pin: Option<String>,
}

fn setup_done(st: &AppState) -> Result<bool, AppError> {
    Ok(st.db.read(pos_core::setup::status)?.completed)
}

/// Before the first setup anyone may try the demo; afterwards only an owner holding
/// `settings.data` (a cashier cannot leave the till for a training shop).
pub(crate) fn may_enter(st: Option<&AppState>) -> Result<(), AppError> {
    match st {
        None => Ok(()),
        Some(st) if !setup_done(st)? => Ok(()),
        Some(st) => st.require_owner(Permission::SettingsData).map(drop),
    }
}

/// Importing a whole shop is for a new install only.
pub(crate) fn may_import(st: Option<&AppState>) -> Result<(), AppError> {
    match st {
        Some(st) if setup_done(st)? => Err(AppError::conflict("setup_already_done")),
        _ => Ok(()),
    }
}

/// Seeds the demo database once (in `lang`) and writes the marker; the caller restarts.
pub(crate) fn enter_now(paths: &AppPaths, lang: Lang, now: i64) -> Result<DemoSummary, AppError> {
    let data_dir = PathBuf::from(&paths.data_dir);
    let demo_dir = core_demo::data_dir_of(&data_dir);
    let db = Db::open(
        &demo_dir.join("pos.db"),
        DbOptions {
            readers: 1,
            backup_dir: Some(demo_dir.join("backups")),
        },
    )?;
    let terminal = pos_core::setup::bootstrap(&db)?;
    let pack = Arc::new(pos_core::datapack::bundled()?);
    let summary = core_demo::seed(&db, pack, &terminal, lang, now)?;
    db.close()?;
    core_demo::activate(&data_dir)?;
    Ok(summary)
}

fn state_of(app: &AppHandle) -> Option<SharedState> {
    app.try_state::<SharedState>().map(|s| s.inner().clone())
}

/// Journals a demo action in `st`'s database; a failure is logged, never in the way.
fn journal(st: &AppState, action: &'static str, payload: serde_json::Value) {
    let actor = st.actor(st.current_session().ok().flatten().as_ref());
    if let Err(error) = st
        .db
        .write(move |tx| audit::append(tx, &actor.event(action, "demo", None, payload)).map(drop))
    {
        tracing::warn!(%error, action, "could not journal the demo action");
    }
}

/// Closes `st`'s database to writes before the restart onto the other profile.
fn close_for_restart(st: &AppState) {
    if let Err(error) = st.db.mark_clean_shutdown() {
        tracing::error!(%error, "could not close the shop before the restart");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn demo_status(paths: State<'_, AppPaths>) -> Result<DemoStatus, AppError> {
    let data = PathBuf::from(&paths.data_dir);
    let active = paths.demo;
    Ok(DemoStatus {
        active,
        exists: core_demo::exists(&data),
        owner_pin: active.then(|| core_demo::OWNER_PIN.to_owned()),
        cashier_pin: active.then(|| core_demo::CASHIER_PIN.to_owned()),
    })
}

/// Seeds the demo when it does not exist yet, then restarts on it (600 ms later).
#[tauri::command]
#[specta::specta]
pub async fn demo_enter(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    lang: Lang,
) -> Result<(), AppError> {
    if paths.demo {
        return Ok(());
    }
    let st = state_of(&app);
    let paths = paths.inner().clone();
    blocking(move || {
        may_enter(st.as_deref())?;
        let summary = enter_now(&paths, lang, now_ms())?;
        if let Some(st) = &st {
            journal(st, "demo.entered", json!({ "tickets": summary.tickets }));
            close_for_restart(st);
        }
        Ok(())
    })
    .await?;
    super::backup::restart_soon(app);
    Ok(())
}

/// Back to the real shop (the demo stays until erased), restarting 600 ms later.
#[tauri::command]
#[specta::specta]
pub async fn demo_leave(app: AppHandle, paths: State<'_, AppPaths>) -> Result<(), AppError> {
    if !paths.demo {
        return Err(AppError::conflict("demo_not_active"));
    }
    let st = state_of(&app);
    let data = PathBuf::from(&paths.data_dir);
    blocking(move || leave_now(st.as_deref(), &data, now_ms())).await?;
    super::backup::restart_soon(app);
    Ok(())
}

/// Leaves the demo: `demo.left` is noted for the real shop's journal (its next start writes the
/// row in the real `pos.db`: the database open now is the demo's), then the marker goes; when
/// it cannot, the demo stays open and writable.
pub(crate) fn leave_now(st: Option<&AppState>, data: &Path, now: i64) -> Result<(), AppError> {
    let user = st
        .and_then(|st| st.current_session().ok().flatten())
        .map(|s| s.display_name);
    core_demo::note_left(data, now, user.as_deref())?;
    core_demo::deactivate(data)?;
    if let Some(st) = st {
        close_for_restart(st);
    }
    Ok(())
}

/// Deletes the demo folder (refused with `demo_active` while it is the shop in use).
#[tauri::command]
#[specta::specta]
pub async fn demo_erase(app: AppHandle, paths: State<'_, AppPaths>) -> Result<(), AppError> {
    let st = state_of(&app).ok_or_else(|| AppError::auth("session_required"))?;
    let data = PathBuf::from(&paths.data_dir);
    blocking(move || {
        st.require_owner(Permission::SettingsData)?;
        if core_demo::erase(&data)? {
            journal(&st, "demo.erased", json!({}));
        }
        Ok(())
    })
    .await
}

/// DAT-013: at the first screen of a new install, a `.dzpos` file exported on the old PC is
/// placed in the backups folder and restored at once (the restart swaps it in); `false` when the
/// dialog is cancelled.
#[tauri::command]
#[specta::specta]
pub async fn setup_import_backup(app: AppHandle, window: Window) -> Result<bool, AppError> {
    let st = state_of(&app).ok_or_else(|| AppError::conflict("setup_required"))?;
    let picker = app.clone();
    let restoring = blocking(move || {
        may_import(Some(&*st))?;
        let Some(file) = pick_dialog(&picker, &window, BACKUP_EXTENSION, &[BACKUP_EXTENSION])?
        else {
            return Ok(false);
        };
        import_first_run(&st, &file)?;
        Ok(true)
    })
    .await?;
    if restoring {
        super::backup::restart_soon(app);
    }
    Ok(restoring)
}

/// Places `file` in the backups folder, journals it and schedules its restore; the database is
/// closed to writes on success, so the caller must restart.
fn import_first_run(st: &AppState, file: &std::path::Path) -> Result<(), AppError> {
    let dir = st
        .db
        .backup_dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| AppError::validation("backupDir", "backup_dir_missing"))?;
    std::fs::create_dir_all(&dir)?;
    let placed = archive::import_backup(file, &dir, now_ms())?;
    // The demo's copy, a till's copy or a main PC's copy with tills never becomes this shop.
    if let Err(error) = pos_core::db::recovery::restore_guard(&placed, st.paths.demo) {
        if let Err(removed) = std::fs::remove_file(&placed) {
            tracing::warn!(%removed, "could not remove a refused imported backup");
        }
        return Err(error);
    }
    let name = placed
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let actor = Actor::system(Some(st.terminal.id.clone()));
    let logged = name.clone();
    st.db.write(move |tx| {
        let event = actor.event(
            "backup.imported",
            "backup",
            Some(&logged),
            json!({ "firstRun": true }),
        );
        audit::append(tx, &event).map(drop)
    })?;
    st.db.schedule_restore(&name, None)?;
    // The marker is written, so the restart must happen whatever this returns.
    close_for_restart(st);
    Ok(())
}
