//! Data files (`settings.data`, spec §13): a `.dzpos` backup file to carry the shop to another PC
//! (a fresh manual backup with its manifest), the import of such a file into the backups folder
//! (restored afterwards with M0's owner-and-PIN restore, never by itself), and the whole data as
//! CSV files in a zip. Every path comes from a Rust dialog; each file is journaled
//! (`backup.exported`, `backup.imported`, `data.exported`).

use super::exports::ExportDone;
use super::{blocking, pick_dialog, save_dialog, today};
use crate::state::{AppState, SharedState};
use pos_core::audit;
use pos_core::auth::{Permission, Session};
use pos_core::company::{self, Company};
use pos_core::db::migrations::LATEST_VERSION;
use pos_core::db::{BackupInfo, BackupKind};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::time::now_ms;
use pos_docs::archive::{self, BACKUP_EXTENSION, BackupManifest, ExportInfo};
use serde_json::json;
use std::path::Path;
use tauri::{AppHandle, State, Window};

const MANIFEST_FORMAT: u32 = 1;

/// Packs a fresh manual backup as a `.dzpos` file where the user says; `None` when they cancel.
#[tauri::command]
#[specta::specta]
pub async fn backup_export_file(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<String>, AppError> {
    let st = state.inner().clone();
    may_manage_data(&st)?;
    blocking(move || {
        let name = format!("dzbuild-pos-{}.{BACKUP_EXTENSION}", today());
        let Some(path) = save_dialog(&app, &window, &name, &[BACKUP_EXTENSION])? else {
            return Ok(None);
        };
        export_backup_to(&st, &path).map(Some)
    })
    .await
}

/// Places a picked `.dzpos` file in the backups folder, where the restore screen lists it;
/// `None` when the user cancels the dialog.
#[tauri::command]
#[specta::specta]
pub async fn backup_import_file(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<BackupInfo>, AppError> {
    let st = state.inner().clone();
    may_manage_data(&st)?;
    blocking(move || {
        let Some(path) = pick_dialog(&app, &window, BACKUP_EXTENSION, &[BACKUP_EXTENSION])? else {
            return Ok(None);
        };
        import_backup_from(&st, &path).map(Some)
    })
    .await
}

/// The whole data as one CSV per table in a zip; `None` when the user cancels the dialog.
#[tauri::command]
#[specta::specta]
pub async fn data_export_all(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<ExportDone>, AppError> {
    let st = state.inner().clone();
    may_manage_data(&st)?;
    blocking(move || {
        let name = format!("dzbuild-pos-data-{}.zip", today());
        let Some(path) = save_dialog(&app, &window, &name, &["zip"])? else {
            return Ok(None);
        };
        export_all_to(&st, &path).map(Some)
    })
    .await
}

/// The guard of the three commands, checked before any dialog opens.
pub(crate) fn may_manage_data(st: &AppState) -> Result<Session, AppError> {
    st.require(Permission::SettingsData)
}

/// The shop's name in `lang` (another language when it is empty) and its NIF.
pub(crate) fn company_of(company: Option<Company>, lang: Lang) -> (String, Option<String>) {
    let Some(c) = company else {
        return (String::new(), None);
    };
    let order = match lang {
        Lang::Ar => [&c.name_ar, &c.name_fr, &c.name_en],
        Lang::Fr => [&c.name_fr, &c.name_ar, &c.name_en],
        Lang::En => [&c.name_en, &c.name_fr, &c.name_ar],
    };
    let name = order
        .into_iter()
        .find(|n| !n.is_empty())
        .cloned()
        .unwrap_or_default();
    (name, c.nif.clone())
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// Journals a data file (no service transaction writes it).
fn journal(st: &AppState, event: audit::AuditEvent) -> Result<(), AppError> {
    st.db.write(move |tx| {
        audit::append(tx, &event)?;
        Ok(())
    })
}

/// A manual backup (the copy and its integrity check, `Db::backup`) packed with its manifest.
pub(crate) fn export_backup_to(st: &AppState, out: &Path) -> Result<String, AppError> {
    let session = may_manage_data(st)?;
    let copy = st.db.backup(BackupKind::Manual)?;
    let (db_sha256, db_size) = archive::sha256_file(Path::new(&copy.path))?;
    let company = st.db.read(company::get)?;
    let demo = company.as_ref().is_some_and(|c| c.demo);
    let (company_name, nif) = company_of(company, session.lang);
    let manifest = BackupManifest {
        format: MANIFEST_FORMAT,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version: LATEST_VERSION,
        created_at: copy.at,
        company_name,
        nif,
        db_sha256,
        db_size,
        demo,
    };
    archive::export_backup(Path::new(&copy.path), &manifest, out)?;
    let event = session.actor().event(
        "backup.exported",
        "backup",
        Some(&copy.file_name),
        json!({ "file": file_name_of(out), "size": db_size }),
    );
    journal(st, event)?;
    Ok(out.display().to_string())
}

/// Checks the file (manifest, SHA-256, the database itself) and places it in the backups folder.
pub(crate) fn import_backup_from(st: &AppState, file: &Path) -> Result<BackupInfo, AppError> {
    let session = may_manage_data(st)?;
    let manifest = archive::read_manifest(file)?;
    let dir = st
        .db
        .backup_dir()
        .ok_or_else(|| AppError::validation("backupDir", "backup_dir_missing"))?
        .to_path_buf();
    std::fs::create_dir_all(&dir)?;
    let placed = file_name_of(&archive::import_backup(file, &dir, now_ms())?);
    let event = session.actor().event(
        "backup.imported",
        "backup",
        Some(&placed),
        json!({ "file": file_name_of(file), "companyName": manifest.company_name,
                "createdAt": manifest.created_at, "appVersion": manifest.app_version,
                "schemaVersion": manifest.schema_version }),
    );
    journal(st, event)?;
    st.db
        .list_backups()?
        .into_iter()
        .find(|b| b.file_name == placed)
        .ok_or_else(|| AppError::not_found("backup"))
}

/// Every table as a CSV file; the manifest names the user's language (the readme has all three).
pub(crate) fn export_all_to(st: &AppState, out: &Path) -> Result<ExportDone, AppError> {
    let session = may_manage_data(st)?;
    let (company_name, nif) = company_of(st.db.read(company::get)?, session.lang);
    let info = ExportInfo {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at: now_ms(),
        company_name,
        nif,
    };
    let (lang, path) = (session.lang, out.to_path_buf());
    let done = st
        .db
        .read(move |c| archive::full_export(c, lang, &info, &path))?;
    let event = session.actor().event(
        "data.exported",
        "export",
        None,
        json!({ "target": "all", "file": file_name_of(out), "tables": done.tables,
                "rows": done.rows }),
    );
    journal(st, event)?;
    Ok(ExportDone {
        path: out.display().to_string(),
        rows: u32::try_from(done.rows).unwrap_or(u32::MAX),
    })
}
