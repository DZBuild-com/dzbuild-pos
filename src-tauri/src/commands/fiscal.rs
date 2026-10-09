//! Tax commands: the G50 helper (RPT-048) and the fiscal archive (AUD-010, RPT-061) with its
//! progress. The archive's path comes from the Rust save dialog, the verified one from the Rust
//! open dialog: the webview never names a file.

use super::{blocking, pick_dialog, save_dialog};
use crate::print_host::{HostPage, PageSetup, PrintHost};
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission, Session};
use pos_core::error::AppError;
use pos_core::fiscal::archive::{self, ArchivePart, ArchiveScope, ChainSummary};
use pos_core::fiscal::g50::{G50, g50, month_range};
use pos_core::lang::Lang;
use pos_docs::fiscal_archive::{
    ArchiveCheck, ArchiveCompany, ArchiveWriter, ManifestBase, ManifestFile, build_summary, verify,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, State, Window};
use tauri_specta::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveStage {
    Parts,
    Summary,
    Manifest,
}

/// How far the archive being written is (`archive-progress`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveProgress {
    pub stage: ArchiveStage,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInput {
    pub from: String,
    pub to: String,
    /// The language of `summary.pdf`.
    pub lang: Lang,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveDone {
    pub path: String,
    pub manifest_sha256: String,
    pub files: Vec<ManifestFile>,
    pub chains: ChainSummary,
}

pub(crate) fn g50_of(st: &AppState, month: String) -> Result<G50, AppError> {
    st.require(Permission::ReportsTax)?;
    let (from, to) = month_range(&month)?;
    let pack = st.datapack.clone();
    st.db.read(move |c| g50(c, &pack, &from, &to))
}

/// The archive needs `reports.tax` and, as a copy of the whole books, `settings.data`.
fn may_archive(st: &AppState) -> Result<Session, AppError> {
    let session = st.require(Permission::ReportsTax)?;
    auth::require(&session, Permission::SettingsData)?;
    Ok(session)
}

/// Writes the archive beside `path` and puts it there once whole and anchored: an earlier
/// archive at `path` is never lost to an export that fails (`ArchiveWriter`). `progress` runs
/// on the database reader while the parts are read, so it is `Send`.
pub(crate) fn export_archive_to(
    st: &AppState,
    host: &PrintHost,
    input: ArchiveInput,
    path: &Path,
    progress: impl FnMut(ArchiveProgress) + Send + 'static,
) -> Result<ArchiveDone, AppError> {
    let session = may_archive(st)?;
    let scope = archive::check_scope(&input.from, &input.to)?;
    let company = st
        .db
        .read(pos_core::company::get)?
        .ok_or_else(|| AppError::conflict("setup_required"))?;
    let identity = ArchiveCompany {
        name: super::data_files::company_of(Some(company.clone()), Lang::Fr).0,
        nif: company.nif,
        nis: company.nis,
        rc: company.rc,
    };
    // Dropped on any failure below, the writer takes its partial file with it.
    let writer = ArchiveWriter::create(path)?;
    write_archive(
        st, host, &session, &input, scope, identity, writer, path, progress,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_archive(
    st: &AppState,
    host: &PrintHost,
    session: &Session,
    input: &ArchiveInput,
    scope: ArchiveScope,
    identity: ArchiveCompany,
    writer: ArchiveWriter,
    path: &Path,
    progress: impl FnMut(ArchiveProgress) + Send + 'static,
) -> Result<ArchiveDone, AppError> {
    let total = u32::try_from(ArchivePart::ALL.len()).unwrap_or(0) + 2;
    let parts_scope = scope.clone();
    // One read transaction on one reader for every part, the schema and the chain checks: the
    // archive is one state of the books, whatever the tills sell meanwhile (the writer goes on).
    // The writer travels into the reader and back: rows go from SQLite to the zip one by one,
    // so memory stays flat whatever the period.
    let (mut writer, counts, chains, mut progress) = st.db.read(move |c| {
        archive::in_snapshot(c, move |c| {
            let (mut writer, mut progress) = (writer, progress);
            let mut counts: Vec<(ArchivePart, i64)> = Vec::new();
            for (i, part) in ArchivePart::ALL.into_iter().enumerate() {
                let header = archive::header(c, part)?;
                let mut rows = 0_i64;
                writer.add_csv(part.file_name(), &header, |sink| {
                    rows = archive::dump(c, part, &parts_scope, &mut |r| sink.row(r))?.rows;
                    Ok(rows)
                })?;
                counts.push((part, rows));
                progress(ArchiveProgress {
                    stage: ArchiveStage::Parts,
                    done: u32::try_from(i + 1).unwrap_or(0),
                    total,
                });
            }
            writer.add_bytes("schema.sql", archive::schema_sql(c)?.as_bytes())?;
            let chains = archive::chains(c)?;
            writer.add_bytes("chains.json", &serde_json::to_vec_pretty(&chains)?)?;
            Ok((writer, counts, chains, progress))
        })
    })?;
    let now = pos_core::time::now_ms();
    let summary = build_summary(
        &chains,
        &counts,
        identity.clone(),
        &scope.from,
        &scope.to,
        input.lang,
        now,
    );
    // A folder of its own: WebView2 writes the PDF by its path, no handle of ours stays open.
    let temp = tempfile::tempdir().map_err(|e| AppError::Io(e.to_string()))?;
    let pdf = temp.path().join("summary.pdf");
    host.pdf(
        HostPage::ArchiveSummary(Box::new(summary)),
        |_| PageSetup::a4(12),
        &pdf,
    )?;
    writer.add_bytes(
        "summary.pdf",
        &std::fs::read(&pdf).map_err(|e| AppError::Io(e.to_string()))?,
    )?;
    progress(ArchiveProgress {
        stage: ArchiveStage::Summary,
        done: total - 1,
        total,
    });
    let (manifest, sha, finished) = writer.finish(ManifestBase {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at: pos_core::time::algiers_datetime(now)
            .format("%Y-%m-%dT%H:%M:%S+01:00")
            .to_string(),
        company: identity,
        from: scope.from.clone(),
        to: scope.to.clone(),
        chains: serde_json::to_value(&chains)?,
    })?;
    let files: Vec<(String, String)> = manifest
        .files
        .iter()
        .map(|f| (f.name.clone(), f.sha256.clone()))
        .collect();
    let (actor, anchor) = (session.actor(), sha.clone());
    st.db
        .write(move |tx| archive::record_export(tx, &actor, &scope, &anchor, &files))?;
    // Only now does the new archive take the chosen file's place.
    finished.persist()?;
    progress(ArchiveProgress {
        stage: ArchiveStage::Manifest,
        done: total,
        total,
    });
    Ok(ArchiveDone {
        path: path.to_string_lossy().into_owned(),
        manifest_sha256: sha,
        files: manifest.files,
        chains,
    })
}

/// Recomputes every hash of an archive chosen in the Rust open dialog.
pub(crate) fn verify_archive_at(st: &AppState, path: &Path) -> Result<ArchiveCheck, AppError> {
    st.require(Permission::ReportsTax)?;
    verify(path)
}

/// The G50 figures of a month (`YYYY-MM`).
#[tauri::command]
#[specta::specta]
pub async fn fiscal_g50(state: State<'_, SharedState>, month: String) -> Result<G50, AppError> {
    let st = state.inner().clone();
    blocking(move || g50_of(&st, month)).await
}

/// Opens the Rust save dialog on `archive-fiscale-<from>-<to>.zip` once the period is valid;
/// `None` when the user cancels it. The progress arrives as `archive-progress` events.
#[tauri::command]
#[specta::specta]
pub async fn fiscal_export_archive(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: ArchiveInput,
) -> Result<Option<ArchiveDone>, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    may_archive(&st)?;
    blocking(move || {
        let scope = archive::check_scope(&input.from, &input.to)?;
        let name = format!("archive-fiscale-{}-{}.zip", scope.from, scope.to);
        let Some(path) = save_dialog(&app, &window, &name, &["zip"])? else {
            return Ok(None);
        };
        let events = app.clone();
        export_archive_to(&st, &host, input, &path, move |p| {
            if let Err(error) = p.emit(&events) {
                tracing::warn!(%error, "could not tell the window how the archive goes");
            }
        })
        .map(Some)
    })
    .await
}

/// Opens the Rust open dialog on a `.zip` and checks it; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn fiscal_verify_archive(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<ArchiveCheck>, AppError> {
    let st = state.inner().clone();
    st.require(Permission::ReportsTax)?;
    blocking(move || {
        let Some(path) = pick_dialog(&app, &window, "zip", &["zip"])? else {
            return Ok(None);
        };
        verify_archive_at(&st, &path).map(Some)
    })
    .await
}
