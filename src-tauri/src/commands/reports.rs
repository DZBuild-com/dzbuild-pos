//! Reports and the dashboard (spec §5.1 M2, RPT-001…060): every kind runs with its group's
//! permission (`ReportKind::permission`); costs and margins leave the rows for viewers without
//! `reports.profit`, except in the purchases group whose costs are its subject (and whose
//! permission is `purchases.view`). The titles, labels and tag words come from `pos-docs`, the
//! same as the exported files.

use super::{blocking, redact};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::reports::{
    self, Dashboard, DashboardView, Report, ReportFilter, ReportGroup, ReportKind, ReportRequest,
};
use serde::Serialize;
use specta::Type;
use std::collections::BTreeMap;
use tauri::State;

/// A report kind as the catalogue screen lists it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReportInfo {
    pub kind: ReportKind,
    pub group: ReportGroup,
    pub title: String,
    pub group_title: String,
    pub filters: Vec<ReportFilter>,
    /// `false` for "as of" reports (one day).
    pub dated: bool,
    pub required_filter: Option<ReportFilter>,
    /// The signed-in user may run it.
    pub allowed: bool,
}

/// A report with the words to show it: the title, the period, one label per column and the words
/// of the tag values it holds (`"<domain>.<value>"`).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub report: Report,
    pub title: String,
    pub subtitle: String,
    pub labels: Vec<String>,
    pub tag_labels: BTreeMap<String, String>,
}

#[tauri::command]
#[specta::specta]
pub async fn reports_catalog(
    state: State<'_, SharedState>,
    lang: Lang,
) -> Result<Vec<ReportInfo>, AppError> {
    let st = state.inner().clone();
    blocking(move || catalog(&st, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn reports_run(
    state: State<'_, SharedState>,
    request: ReportRequest,
) -> Result<ReportView, AppError> {
    let st = state.inner().clone();
    blocking(move || run(&st, request)).await
}

#[tauri::command]
#[specta::specta]
pub async fn reports_dashboard(state: State<'_, SharedState>) -> Result<Dashboard, AppError> {
    let st = state.inner().clone();
    blocking(move || dashboard(&st)).await
}

/// A pack's reports are listed only while its module is on (they refuse to run
/// otherwise).
pub(crate) fn catalog(st: &AppState, lang: Lang) -> Result<Vec<ReportInfo>, AppError> {
    let session = st.session()?;
    let modules = st.db.read(pos_core::packs::state)?.modules;
    Ok(ReportKind::ALL
        .into_iter()
        .filter(|kind| kind.module().is_none_or(|m| modules.contains(&m)))
        .map(|kind| ReportInfo {
            kind,
            group: kind.group(),
            title: pos_docs::sheet::report_title(lang, kind.as_str()).unwrap_or_default(),
            group_title: pos_docs::sheet::group_title(lang, kind.group().as_str())
                .unwrap_or_default(),
            filters: kind.filters().to_vec(),
            dated: kind.dated(),
            required_filter: kind.required_filter(),
            allowed: session.has(kind.permission()),
        })
        .collect())
}

/// Runs `request` in its language (the user's when it has none).
pub(crate) fn run(st: &AppState, request: ReportRequest) -> Result<ReportView, AppError> {
    view_of(st, request, false)
}

/// The report as an export writes it: every row of the period, refused (`export_too_large`)
/// rather than cut (`reports::run_for_export`).
pub(crate) fn run_for_export(
    st: &AppState,
    request: ReportRequest,
) -> Result<ReportView, AppError> {
    view_of(st, request, true)
}

fn view_of(
    st: &AppState,
    mut request: ReportRequest,
    export: bool,
) -> Result<ReportView, AppError> {
    let session = st.require(request.kind.permission())?;
    let lang = *request.lang.get_or_insert(session.lang);
    // The legal registers and the delivery rounds are kept on the
    // main PC only (`HostOnly`): a till's copy has none, so a till runs those reports
    // on the main PC (`reports.run`, `host_required` offline), as its user.
    let forwarded = if request.kind.reads_host_only() {
        crate::lan::route::forward::<reports::Report>(
            st,
            "reports.run",
            serde_json::json!({ "request": request, "export": export }),
        )?
    } else {
        None
    };
    let mut report = match forwarded {
        Some(report) => report,
        None => host_report(st, &request, export)?,
    };
    if request.kind.group() != ReportGroup::Purchases && !redact::sees_costs(&session) {
        report = report.hide_costs();
    }
    let title = pos_docs::sheet::report_title(lang, request.kind.as_str()).unwrap_or_default();
    let subtitle = if request.kind.dated() {
        pos_docs::sheet::range_subtitle(lang, &report.from, &report.to)
    } else {
        pos_docs::sheet::as_of_subtitle(lang, &report.to)
    };
    let labels = pos_docs::sheet::labels(&report.table, lang, &BTreeMap::new());
    let tag_labels = pos_docs::sheet::tag_labels(&report.table, lang);
    Ok(ReportView {
        report,
        title,
        subtitle,
        labels,
        tag_labels,
    })
}

/// The host service `reports.run` (and this PC's own run): `request` on this PC's books, as the
/// signed-in user (the till's on the main PC), costs hidden from whoever may not see them.
pub(crate) fn host_report(
    st: &AppState,
    request: &ReportRequest,
    export: bool,
) -> Result<Report, AppError> {
    let session = st.require(request.kind.permission())?;
    let (request, pack) = (request.clone(), st.datapack.clone());
    let report = st.db.read(move |c| {
        if export {
            reports::run_for_export(c, &request, &pack)
        } else {
            reports::run_with(c, &request, &pack)
        }
    })?;
    Ok(
        if report.kind.group() != ReportGroup::Purchases && !redact::sees_costs(&session) {
            report.hide_costs()
        } else {
            report
        },
    )
}

/// Today's figures; what the viewer may not see comes back `None` (`DashboardView`).
pub(crate) fn dashboard(st: &AppState) -> Result<Dashboard, AppError> {
    let session = st.require(Permission::ReportsSales)?;
    let view = DashboardView {
        costs: redact::sees_costs(&session),
        credit: session.has(Permission::CreditView),
        purchases: session.has(Permission::PurchasesView),
        stock: session.has(Permission::StockView),
    };
    let today = super::today();
    st.db
        .read(move |c| reports::dashboard(c, &today, session.lang, view))
}
