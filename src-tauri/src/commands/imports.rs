//! The import wizard (IMX-001…012, spec §12): a file picked by a Rust dialog is read and held in
//! memory for the user who opened it (`crate::staging`; the webview holds a token, never a path),
//! previewed with the suggested mapping, checked by a dry run that records the exact options it
//! checked, then applied only with those options, in batches whose progress reaches the window
//! as `import-progress`. The template, the error report, the history and the undo of the latest
//! import complete it. Every command checks the entity's permission (`ImportEntity::permission`).

use super::{blocking, pick_dialog, save_dialog, today};
use crate::staging::Staged;
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::catalog::price_levels::{self, PriceLevel};
use pos_core::error::AppError;
use pos_core::ids::new_id;
use pos_core::imports::engine::{self, DryRun, ImportJob, ImportOptions, ImportProgress};
use pos_core::imports::spec::{self, FieldKind, ImportEntity, Problem};
use pos_core::lang::Lang;
use pos_core::table::TagDomain;
use pos_docs::import::{FileFormat, RawFile, RawSheet};
use pos_docs::sheet::{self, SheetFormat};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::Path;
use tauri::{AppHandle, State, Window};
use tauri_specta::Event;

/// What an import may read; the filter's name is its extensions, the same in every language.
const IMPORT_FILTER: &str = "xlsx, xls, ods, csv";
const IMPORT_EXTENSIONS: [&str; 5] = ["xlsx", "xls", "ods", "csv", "txt"];
/// Rows of the file shown under the header in the mapping step.
const SAMPLE_ROWS: usize = 20;
/// Jobs in the history.
const JOBS_LIMIT: u32 = 100;
/// Whoever may import something (products, customers, suppliers or count lines).
const ANY_IMPORT: &[Permission] = &[
    Permission::ProductsImport,
    Permission::CustomersManage,
    Permission::SuppliersManage,
    Permission::StockCount,
];

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SheetInfo {
    pub name: String,
    pub rows: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportFile {
    pub token: String,
    pub file_name: String,
    pub format: FileFormat,
    pub encoding: Option<String>,
    pub delimiter: Option<String>,
    pub sheets: Vec<SheetInfo>,
}

/// A field of the entity with its label in the user's language.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    pub key: String,
    pub kind: FieldKind,
    pub required: bool,
    pub identifies: bool,
    pub choice: Option<TagDomain>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// 0-based index of the row taken as the header.
    pub header_row: u32,
    pub headers: Vec<String>,
    /// The rows under the header (20 at most).
    pub sample: Vec<Vec<String>>,
    pub fields: Vec<FieldInfo>,
    /// The field each header most likely means (`None`: left out).
    pub suggested: Vec<Option<String>>,
}

/// One batch of an import applied (`import-progress`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "import-progress")]
pub struct ImportProgressEvent(pub ImportProgress);

/// Opens the Rust file dialog; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn imports_open_file(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<ImportFile>, AppError> {
    let st = state.inner().clone();
    may_import(&st)?;
    blocking(move || {
        let Some(path) = pick_dialog(&app, &window, IMPORT_FILTER, &IMPORT_EXTENSIONS)? else {
            return Ok(None);
        };
        stage_path(&st, &path).map(Some)
    })
    .await
}

/// The header, sample rows and suggested mapping of a sheet, under the row the reader found
/// (`header_row: None`) or the one the user chose.
#[tauri::command]
#[specta::specta]
pub async fn imports_preview(
    state: State<'_, SharedState>,
    token: String,
    sheet: u32,
    entity: ImportEntity,
    header_row: Option<u32>,
) -> Result<ImportPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || preview(&st, token, sheet, entity, header_row)).await
}

/// Runs the import without keeping anything and records these options as checked.
#[tauri::command]
#[specta::specta]
pub async fn imports_dry_run(
    state: State<'_, SharedState>,
    token: String,
    sheet: u32,
    options: ImportOptions,
) -> Result<DryRun, AppError> {
    let st = state.inner().clone();
    blocking(move || dry_run(&st, token, sheet, options)).await
}

/// Applies the options the last dry run checked, once (`import_mapping_invalid` for any other,
/// or when that check was already spent).
#[tauri::command]
#[specta::specta]
pub async fn imports_apply(
    app: AppHandle,
    state: State<'_, SharedState>,
    token: String,
    sheet: u32,
    options: ImportOptions,
) -> Result<ImportJob, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        apply(&st, token, sheet, options, &mut |p| {
            if let Err(error) = ImportProgressEvent(p.clone()).emit(&app) {
                tracing::warn!(%error, "could not tell the window how the import goes");
            }
        })
    })
    .await
}

/// The problems as an xlsx sheet, saved where the user says; `None` when they cancel.
#[tauri::command]
#[specta::specta]
pub async fn imports_error_report(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    entity: ImportEntity,
    problems: Vec<Problem>,
    lang: Lang,
) -> Result<Option<String>, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        let bytes = error_report_bytes(&st, entity, &problems, lang)?;
        let name = sheet::file_name(
            &format!("import-problems-{}", entity.as_str()),
            &today(),
            SheetFormat::Xlsx,
        );
        save_bytes(&app, &window, &name, &bytes)
    })
    .await
}

/// The template of an import in `lang`, saved where the user says; `None` when they cancel.
#[tauri::command]
#[specta::specta]
pub async fn imports_template(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    entity: ImportEntity,
    lang: Lang,
) -> Result<Option<String>, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        let bytes = template_bytes(&st, entity, lang)?;
        let name = sheet::file_name(
            &format!("template-{}", entity.as_str()),
            &today(),
            SheetFormat::Xlsx,
        );
        save_bytes(&app, &window, &name, &bytes)
    })
    .await
}

/// The latest imports of the kinds the user may import, newest first.
#[tauri::command]
#[specta::specta]
pub async fn imports_jobs(state: State<'_, SharedState>) -> Result<Vec<ImportJob>, AppError> {
    let st = state.inner().clone();
    blocking(move || jobs(&st)).await
}

/// Undoes the latest import of the day (`import_undo_not_allowed` for any other).
#[tauri::command]
#[specta::specta]
pub async fn imports_undo(
    state: State<'_, SharedState>,
    job_id: String,
) -> Result<ImportJob, AppError> {
    let st = state.inner().clone();
    blocking(move || undo(&st, job_id)).await
}

/// Drops a file the wizard no longer needs.
#[tauri::command]
#[specta::specta]
pub async fn imports_discard(state: State<'_, SharedState>, token: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || discard(&st, token)).await
}

/// The guard of the file dialog and the history: any import permission.
pub(crate) fn may_import(st: &AppState) -> Result<Session, AppError> {
    st.require_any(ANY_IMPORT)
}

fn stage(
    st: &AppState,
    session: &Session,
    file: RawFile,
    file_name: &str,
) -> Result<ImportFile, AppError> {
    let token = new_id();
    let info = ImportFile {
        token: token.clone(),
        file_name: file_name.to_owned(),
        format: file.format,
        encoding: file.encoding.clone(),
        delimiter: file.delimiter.map(String::from),
        sheets: file
            .sheets
            .iter()
            .map(|s| SheetInfo {
                name: s.name.clone(),
                rows: u32::try_from(s.rows.len()).unwrap_or(u32::MAX),
            })
            .collect(),
    };
    let now = st.now();
    let staged = Staged {
        user_id: session.user_id.clone(),
        file_name: file_name.to_owned(),
        file,
        opened_at: now,
        checked: None,
    };
    st.staging.put(token, staged, now)?;
    Ok(info)
}

/// Reads bytes into the staging area, like `stage_path` does with the file the dialog picked.
#[cfg(test)]
pub(crate) fn stage_bytes(
    st: &AppState,
    bytes: &[u8],
    file_name: &str,
) -> Result<ImportFile, AppError> {
    let session = may_import(st)?;
    let file = pos_docs::import::read_bytes(bytes, file_name)?;
    stage(st, &session, file, file_name)
}

/// Reads the picked file (`read_path` refuses it before reading when it is too large).
pub(crate) fn stage_path(st: &AppState, path: &Path) -> Result<ImportFile, AppError> {
    let session = may_import(st)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let file = pos_docs::import::read_path(path)?;
    stage(st, &session, file, &file_name)
}

fn sheet_of(file: &RawFile, sheet: u32) -> Result<&RawSheet, AppError> {
    file.sheets
        .get(usize::try_from(sheet).unwrap_or(usize::MAX))
        .ok_or_else(|| AppError::validation("sheet", "import_sheet_not_found"))
}

/// `price.<CODE>` → the price level's name in `lang` (its code when the name is empty), for the
/// product fields the catalogue does not name.
fn price_labels(levels: Vec<PriceLevel>, lang: Lang) -> BTreeMap<String, String> {
    levels
        .into_iter()
        .filter(|l| !l.is_default && !l.archived)
        .map(|l| {
            let name = match lang {
                Lang::Ar => l.name_ar,
                Lang::Fr => l.name_fr,
                Lang::En => l.name_en,
            };
            let label = if name.is_empty() {
                l.code.clone()
            } else {
                name
            };
            (format!("price.{}", l.code), label)
        })
        .collect()
}

fn fields_of(
    st: &AppState,
    entity: ImportEntity,
    lang: Lang,
) -> Result<(Vec<spec::FieldSpec>, BTreeMap<String, String>), AppError> {
    let (fields, levels) = st
        .db
        .read(move |c| Ok((spec::fields(c, entity)?, price_levels::list(c)?)))?;
    Ok((fields, price_labels(levels, lang)))
}

pub(crate) fn preview(
    st: &AppState,
    token: String,
    sheet: u32,
    entity: ImportEntity,
    header_row: Option<u32>,
) -> Result<ImportPreview, AppError> {
    let session = st.require(entity.permission())?;
    let (header_row, headers, sample) =
        st.staging.with(&token, &session.user_id, st.now(), |s| {
            let raw = sheet_of(&s.file, sheet)?;
            let at = match header_row {
                Some(row) => usize::try_from(row)
                    .ok()
                    .filter(|r| *r < raw.rows.len())
                    .ok_or_else(|| AppError::validation("headerRow", "value_invalid"))?,
                None => pos_docs::import::header_row(raw),
            };
            let headers = raw.rows.get(at).cloned().unwrap_or_default();
            let sample = raw
                .rows
                .iter()
                .skip(at + 1)
                .take(SAMPLE_ROWS)
                .cloned()
                .collect();
            Ok((at, headers, sample))
        })?;
    let lang = session.lang;
    let (fields, extra) = fields_of(st, entity, lang)?;
    let suggested = pos_docs::import::suggest(&headers, &fields);
    let fields = fields
        .iter()
        .map(|f| FieldInfo {
            key: f.key.clone(),
            kind: f.kind,
            required: f.required,
            identifies: f.identifies,
            choice: f.choice,
            label: pos_docs::import::field_label(lang, f, &extra),
        })
        .collect();
    Ok(ImportPreview {
        header_row: u32::try_from(header_row).unwrap_or(u32::MAX),
        headers,
        sample,
        fields,
        suggested,
    })
}

/// The rows the options map, the options carrying the name of the file read (the job and the
/// journal name the file, whatever the webview says).
fn rows_and_options(
    s: &Staged,
    sheet: u32,
    mut options: ImportOptions,
) -> Result<(Vec<engine::ImportRow>, ImportOptions), AppError> {
    options.file_name.clone_from(&s.file_name);
    let rows = engine::rows_of(&sheet_of(&s.file, sheet)?.rows, &options);
    Ok((rows, options))
}

pub(crate) fn dry_run(
    st: &AppState,
    token: String,
    sheet: u32,
    options: ImportOptions,
) -> Result<DryRun, AppError> {
    let ctx = st.till(options.entity.permission())?;
    let user = ctx.session.user_id.clone();
    let (rows, options) = st.staging.with(&token, &user, ctx.now, |s| {
        rows_and_options(s, sheet, options)
    })?;
    let dry = engine::dry_run(
        &st.db,
        &ctx,
        &options,
        &rows,
        &pos_docs::import::choice_labels(),
    )?;
    let fingerprint = dry.fingerprint.clone();
    st.staging.with(&token, &user, ctx.now, move |s| {
        s.checked = Some((options, fingerprint));
        Ok(())
    })?;
    Ok(dry)
}

pub(crate) fn apply(
    st: &AppState,
    token: String,
    sheet: u32,
    options: ImportOptions,
    progress: &mut dyn FnMut(&ImportProgress),
) -> Result<ImportJob, AppError> {
    let ctx = st.till(options.entity.permission())?;
    let user = ctx.session.user_id.clone();
    // The check is spent under the staging lock: a second apply of the same file (a double
    // click, a retry, a failed apply whose first batches went through) needs a new dry run,
    // which reports the rows already there.
    let (rows, options, checked) = st.staging.with(&token, &user, ctx.now, |s| {
        let (rows, options) = rows_and_options(s, sheet, options)?;
        let checked = match s.checked.take() {
            Some((opts, fingerprint)) if opts == options => fingerprint,
            other => {
                s.checked = other;
                return Err(AppError::conflict("import_mapping_invalid"));
            }
        };
        Ok((rows, options, checked))
    })?;
    let job = engine::apply(
        &st.db,
        &ctx,
        &options,
        &rows,
        &pos_docs::import::choice_labels(),
        &checked,
        progress,
    )?;
    st.spend_approvals(&ctx)?;
    st.staging.remove(&token)?;
    Ok(job)
}

pub(crate) fn error_report_bytes(
    st: &AppState,
    entity: ImportEntity,
    problems: &[Problem],
    lang: Lang,
) -> Result<Vec<u8>, AppError> {
    st.require(entity.permission())?;
    let (fields, extra) = fields_of(st, entity, lang)?;
    pos_docs::import::error_report(problems, &fields, lang, &extra)
}

pub(crate) fn template_bytes(
    st: &AppState,
    entity: ImportEntity,
    lang: Lang,
) -> Result<Vec<u8>, AppError> {
    st.require(entity.permission())?;
    let (fields, extra) = fields_of(st, entity, lang)?;
    pos_docs::import::template(entity, &fields, lang, &extra)
}

fn save_bytes(
    app: &AppHandle,
    window: &Window,
    name: &str,
    bytes: &[u8],
) -> Result<Option<String>, AppError> {
    let Some(path) = save_dialog(app, window, name, &["xlsx"])? else {
        return Ok(None);
    };
    // beside the chosen file first: a failed write never leaves it half replaced
    pos_docs::archive::write_file(&path, bytes)?;
    Ok(Some(path.display().to_string()))
}

/// A job's rows name customers and suppliers (the problems keep the values read): each user
/// sees the kinds they may import only.
pub(crate) fn jobs(st: &AppState) -> Result<Vec<ImportJob>, AppError> {
    let session = may_import(st)?;
    let day = today();
    let all = st.db.read(move |c| engine::jobs(c, &day, JOBS_LIMIT))?;
    Ok(all
        .into_iter()
        .filter(|j| session.has(j.kind.permission()))
        .collect())
}

pub(crate) fn undo(st: &AppState, job_id: String) -> Result<ImportJob, AppError> {
    may_import(st)?;
    let (id, day) = (job_id.clone(), today());
    let kind = st.db.read(move |c| engine::job(c, &id, &day))?.kind;
    let ctx = st.till(kind.permission())?;
    let write = ctx.clone();
    let job = st.db.write(move |tx| engine::undo(tx, &write, &job_id))?;
    st.spend_approvals(&ctx)?;
    Ok(job)
}

pub(crate) fn discard(st: &AppState, token: String) -> Result<(), AppError> {
    let session = may_import(st)?;
    st.staging.discard(&token, &session.user_id)
}
