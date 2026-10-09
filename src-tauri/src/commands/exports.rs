//! Exports of every table (spec §12, UX-043): the rows a screen shows, written by Rust as xlsx or
//! csv in the language chosen in the toolbar (Arabic sheets read right to left) after a Rust save
//! dialog. Each target checks its own permission before the dialog opens; costs leave without
//! `reports.profit`, kridi balances without `credit.view`, and the customer and supplier files are
//! journaled (`data.exported`, loi 18-07).

use super::{billing, blocking, redact, save_dialog, today};
use crate::state::{AppState, SharedState};
use pos_core::audit;
use pos_core::auth::{Permission, Session};
use pos_core::billing::archive::{self, ArchiveQuery};
use pos_core::catalog::products;
use pos_core::customers::{self, CustomerQuery};
use pos_core::error::AppError;
use pos_core::expenses::{self, ExpenseQuery};
use pos_core::lang::Lang;
use pos_core::reports::ReportRequest;
use pos_core::stock::counts;
use pos_core::suppliers::{self, SupplierQuery};
use pos_core::table::Table;
use pos_docs::sheet::{self, SheetFormat, SheetMeta};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;
use std::path::Path;
use tauri::{AppHandle, State, Window};

/// What to export: a report, a directory, the expenses, the catalogue, a count sheet or the
/// document archive.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExportTarget {
    Report {
        request: ReportRequest,
    },
    Customers {
        query: CustomerQuery,
    },
    Suppliers {
        query: SupplierQuery,
    },
    Expenses {
        query: ExpenseQuery,
    },
    Products,
    #[serde(rename_all = "camelCase")]
    CountSheet {
        count_id: String,
    },
    /// The archive's rows for these filters (every page), of the kinds the user may see.
    Documents {
        query: ArchiveQuery,
    },
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub target: ExportTarget,
    pub format: SheetFormat,
    /// The language of the file (titles, headers, closed lists, names).
    pub lang: Lang,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportDone {
    pub path: String,
    pub rows: u32,
}

/// Opens the Rust save dialog on `<target>-<day>.<format>`; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn exports_save(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    request: ExportRequest,
) -> Result<Option<ExportDone>, AppError> {
    let st = state.inner().clone();
    may_export(&st, &request.target)?;
    blocking(move || {
        let name = sheet::file_name(stem(&request.target), &today(), request.format);
        let Some(path) = save_dialog(&app, &window, &name, &[request.format.extension()])? else {
            return Ok(None);
        };
        export_to(&st, request, &path).map(Some)
    })
    .await
}

/// The permission of each target, checked before the dialog opens; the archive has its own
/// (any of `billing::ARCHIVE_PERMISSIONS`, the kinds then filtered by `visible_kinds`).
pub(crate) fn may_export(st: &AppState, target: &ExportTarget) -> Result<Session, AppError> {
    let permission = match target {
        ExportTarget::Report { request } => request.kind.permission(),
        ExportTarget::Customers { .. } => Permission::CustomersManage,
        ExportTarget::Suppliers { .. } => Permission::SuppliersManage,
        ExportTarget::Expenses { .. } => Permission::ReportsCash,
        ExportTarget::Products => Permission::ProductsView,
        ExportTarget::CountSheet { .. } => Permission::StockCount,
        ExportTarget::Documents { .. } => return st.require_any(&billing::ARCHIVE_PERMISSIONS),
    };
    st.require(permission)
}

pub(crate) fn stem(target: &ExportTarget) -> &'static str {
    match target {
        ExportTarget::Report { request } => request.kind.as_str(),
        ExportTarget::Customers { .. } => "customers",
        ExportTarget::Suppliers { .. } => "suppliers",
        ExportTarget::Expenses { .. } => "expenses",
        ExportTarget::Products => "products",
        ExportTarget::CountSheet { .. } => "count-sheet",
        ExportTarget::Documents { .. } => "documents",
    }
}

/// A sheet titled with the `pos-docs` word `key` in `lang`.
fn titled(lang: Lang, key: &str) -> SheetMeta {
    SheetMeta {
        title: pos_docs::locales::text(lang.as_str(), key).unwrap_or_default(),
        subtitle: None,
        lang,
        extra_labels: BTreeMap::new(),
    }
}

/// `table` without the columns `keys` (their cells and totals too).
fn drop_columns(mut table: Table, keys: &[&str]) -> Table {
    let keep: Vec<bool> = table
        .columns
        .iter()
        .map(|c| !keys.contains(&c.key.as_str()))
        .collect();
    let strip = |row: Vec<pos_core::table::Cell>| {
        row.into_iter()
            .zip(&keep)
            .filter_map(|(cell, kept)| kept.then_some(cell))
            .collect()
    };
    table.rows = std::mem::take(&mut table.rows)
        .into_iter()
        .map(strip)
        .collect();
    table.totals = table.totals.take().map(strip);
    table.columns.retain(|c| !keys.contains(&c.key.as_str()));
    table
}

/// The table a target names, its sheet, the session that may export it and, for the files
/// journaled, the target's name.
pub(crate) struct Built {
    pub(crate) table: Table,
    pub(crate) meta: SheetMeta,
    pub(crate) session: Session,
    pub(crate) journaled: Option<&'static str>,
}

/// The table `target` names in `lang` (the xlsx/csv exports and the PDF of any table, IMX-013).
pub(crate) fn table_of(st: &AppState, target: ExportTarget, lang: Lang) -> Result<Built, AppError> {
    let session = may_export(st, &target)?;
    let built = |table, meta, journaled| Built {
        table,
        meta,
        session: session.clone(),
        journaled,
    };
    match target {
        ExportTarget::Report { mut request } => {
            request.lang = Some(lang);
            // Every row of the period, never the screen's cut (a legal register is whole).
            let view = super::reports::run_for_export(st, request)?;
            let meta = SheetMeta {
                title: view.title,
                subtitle: Some(view.subtitle),
                lang,
                extra_labels: BTreeMap::new(),
            };
            Ok(built(view.report.table, meta, None))
        }
        ExportTarget::Customers { mut query } => {
            // The list of debtors is kridi data like the balances: no "with debt" filter either.
            if !session.has(Permission::CreditView) {
                query.with_debt = false;
            }
            let mut table = st.db.read(move |c| customers::export_table(c, &query))?;
            if !session.has(Permission::CreditView) {
                table = drop_columns(table, &["balance", "creditLimit"]);
            }
            Ok(built(
                table,
                titled(lang, "exports.customers"),
                Some("customers"),
            ))
        }
        ExportTarget::Suppliers { query } => {
            let table = st.db.read(move |c| suppliers::export_table(c, &query))?;
            Ok(built(
                table,
                titled(lang, "exports.suppliers"),
                Some("suppliers"),
            ))
        }
        ExportTarget::Expenses { query } => {
            let table = st
                .db
                .read(move |c| expenses::export_table(c, &query, lang))?;
            Ok(built(table, titled(lang, "exports.expenses"), None))
        }
        ExportTarget::Products => {
            let (mut table, labels) = st.db.read(move |c| products::export_table(c, lang))?;
            if !redact::sees_costs(&session) {
                table = drop_columns(table, &["costPrice"]);
            }
            let mut meta = titled(lang, "exports.products");
            meta.extra_labels = labels;
            Ok(built(table, meta, None))
        }
        ExportTarget::CountSheet { count_id } => {
            let reveal = session.has(Permission::StockAdjust);
            let table = st
                .db
                .read(move |c| counts::sheet(c, &count_id, lang, reveal))?;
            Ok(built(table, titled(lang, "exports.countSheet"), None))
        }
        ExportTarget::Documents { query } => {
            let table = archive::table(&billing::search_all(st, query)?)?;
            Ok(built(table, titled(lang, "exports.documents"), None))
        }
    }
}

/// Journals a written customer or supplier file (`data.exported`, loi 18-07); `rows` of it.
pub(crate) fn journal(
    st: &AppState,
    session: &Session,
    journaled: Option<&'static str>,
    rows: u32,
    path: &Path,
) -> Result<(), AppError> {
    let Some(target) = journaled else {
        return Ok(());
    };
    let actor = session.actor();
    let file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    st.db.write(move |tx| {
        let event = actor.event(
            "data.exported",
            "export",
            None,
            json!({ "target": target, "rows": rows, "file": file }),
        );
        audit::append(tx, &event)?;
        Ok(())
    })
}

/// Writes the export to `path` (the dialog command asks where first).
pub(crate) fn export_to(
    st: &AppState,
    request: ExportRequest,
    path: &Path,
) -> Result<ExportDone, AppError> {
    let Built {
        table,
        meta,
        session,
        journaled,
    } = table_of(st, request.target, request.lang)?;
    let bytes = sheet::render(&table, &meta, request.format)?;
    // Beside the chosen file first: a failed write never leaves it half replaced.
    pos_docs::archive::write_file(path, &bytes)?;
    let rows = u32::try_from(table.rows.len()).unwrap_or(u32::MAX);
    journal(st, &session, journaled, rows, path)?;
    Ok(ExportDone {
        path: path.display().to_string(),
        rows,
    })
}
