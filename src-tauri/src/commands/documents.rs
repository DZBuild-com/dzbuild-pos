//! Sales history (SAL-056), lookup for returns (RET-002), DUPLICATA reprints (DOC-018) and
//! chain verification. Without `sales.view_all` a user sees this terminal's documents only; a
//! commercial document (M3) is seen by its kind's permission only.

use super::{blocking, redact};
use crate::printing::{hw_error, load_receipt};
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission, Session};
use pos_core::billing::kind_permission;
use pos_core::documents::{
    self, DocChainReport, DocStatus, DocType, Document, DocumentQuery, DocumentSummary,
};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_hw::PrintOutcome;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn documents_list(
    state: State<'_, SharedState>,
    query: DocumentQuery,
) -> Result<Vec<DocumentSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Document, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_find(
    state: State<'_, SharedState>,
    number: String,
) -> Result<Option<Document>, AppError> {
    let st = state.inner().clone();
    blocking(move || find(&st, number)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_reprint(
    state: State<'_, SharedState>,
    id: String,
    lang: Option<Lang>,
) -> Result<PrintOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || reprint(&st, id, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_verify_chain(
    state: State<'_, SharedState>,
) -> Result<Vec<DocChainReport>, AppError> {
    let st = state.inner().clone();
    blocking(move || verify_chain(&st)).await
}

/// The till's and the sales history's permissions (M1b).
const SALES_READS: [Permission; 4] = [
    Permission::SalesViewAll,
    Permission::SalesReprint,
    Permission::SalesCreate,
    Permission::SalesReturn,
];

/// Whoever may look a document up: the sales readers, and the commercial documents' users (M3),
/// who see their own kinds only ([`shown`]).
const READERS: [Permission; 8] = [
    Permission::SalesViewAll,
    Permission::SalesReprint,
    Permission::SalesCreate,
    Permission::SalesReturn,
    Permission::DocumentsInvoice,
    Permission::DocumentsQuote,
    Permission::DocumentsDelivery,
    Permission::DocumentsCreditNote,
];

/// The purchase side: an order, a goods receipt, a supplier invoice or a return to a supplier
/// (a `return_note` naming its supplier) — purchase costs, for `purchases.view` only.
pub(crate) fn purchase_side(doc: &Document) -> bool {
    doc.supplier_id.is_some()
        || matches!(
            doc.doc_type,
            DocType::PurchaseOrder | DocType::GoodsReceipt | DocType::SupplierInvoice
        )
}

/// Whether `session` may see `doc`: a purchase-side document by `purchases.view` only; a
/// commercial document (M3) by its kind's permission, whatever the terminal, and a validated
/// quote or proforma by the till too (`sales.create`: the cashier types its number to sell it,
/// SAL-046); any other (a ticket, a return note to a customer, a transfer note valued at cost and
/// redacted) by a sales permission, on this terminal only without `sales.view_all`.
pub(crate) fn shown(doc: &Document, session: &Session) -> bool {
    if purchase_side(doc) {
        return session.has(Permission::PurchasesView);
    }
    let offer = matches!(doc.doc_type, DocType::Quote | DocType::Proforma)
        && doc.status == DocStatus::Validated;
    match kind_permission(doc.doc_type) {
        Some(permission) => {
            session.has(permission) || (offer && session.has(Permission::SalesCreate))
        }
        None => {
            SALES_READS.iter().any(|p| session.has(*p))
                && (session.has(Permission::SalesViewAll) || doc.terminal_id == session.terminal_id)
        }
    }
}

/// A document the session may not see is "not found" ([`shown`]); costs are redacted for
/// users without `reports.profit`.
fn visible(mut doc: Document, session: &Session) -> Result<Document, AppError> {
    if !shown(&doc, session) {
        return Err(AppError::not_found("document"));
    }
    redact::document(&mut doc, session);
    Ok(doc)
}

/// The kinds the sales history lists to `session` (a sales reader): every kind of the till
/// (tickets, return notes, transfer notes…) and each commercial kind (M3) only when its
/// permission allows it ([`shown`]'s rule; supplier documents are never in this list).
fn history_kinds(session: &Session) -> Vec<DocType> {
    DocType::ALL
        .into_iter()
        .filter(|k| kind_permission(*k).is_none_or(|p| session.has(p)))
        .collect()
}

/// The sales history (M1b): this terminal's documents without `sales.view_all`, of the kinds the
/// session may see ([`history_kinds`]).
pub(crate) fn list(
    st: &AppState,
    mut query: DocumentQuery,
) -> Result<Vec<DocumentSummary>, AppError> {
    let session = st.require_any(&SALES_READS)?;
    if !session.has(Permission::SalesViewAll) {
        query.terminal_id = Some(session.terminal_id.clone());
    }
    let kinds = history_kinds(&session);
    let mut rows = st.db.read(move |c| documents::list_of(c, &query, &kinds))?;
    for row in &mut rows {
        redact::summary(row, &session);
    }
    Ok(rows)
}

pub(crate) fn get(st: &AppState, id: String) -> Result<Document, AppError> {
    let session = st.require_any(&READERS)?;
    let doc = st.db.read(move |c| documents::get(c, &id))?;
    visible(doc, &session)
}

/// By the printed number, typed in any case or scanned from the receipt barcode.
pub(crate) fn find(st: &AppState, number: String) -> Result<Option<Document>, AppError> {
    let session = st.require_any(&READERS)?;
    let number = number.trim().to_uppercase();
    let doc = st.db.read(move |c| documents::find_by_number(c, &number))?;
    doc.map(|d| visible(d, &session)).transpose()
}

/// Journaled (`document.reprinted`) before printing, marked DUPLICATA, printed now: the cashier
/// asked for it and waits for the paper. Once a duplicate is on record, the original is no
/// longer pending a retry (`PrintQueue::forget`): `hardware_retry_print` answers
/// `print_not_pending`, so an unmarked original never follows a DUPLICATA. A transfer note is
/// printed from the stored document, prices at the CMUP included, so reprinting one also needs
/// `reports.profit`.
pub(crate) fn reprint(
    st: &AppState,
    id: String,
    lang: Option<Lang>,
) -> Result<PrintOutcome, AppError> {
    let session = st.require(Permission::SalesReprint)?;
    let doc = get(st, id)?;
    if doc.doc_type == DocType::Transfer {
        auth::require(&session, Permission::ReportsProfit)?;
    }
    let actor = session.actor();
    let doc_id = doc.id.clone();
    st.db
        .write(move |tx| documents::record_reprint(tx, &actor, &doc_id).map(|_| ()))?;
    st.printer.forget(&doc.id)?;
    let (receipt, logo) = load_receipt(&st.db, &doc.id, true, lang)?;
    st.hw
        .print_receipt(&receipt, logo.as_deref(), false)
        .map_err(hw_error)
}

pub(crate) fn verify_chain(st: &AppState) -> Result<Vec<DocChainReport>, AppError> {
    st.require(Permission::AuditView)?;
    st.db
        .read(|c| pos_core::fiscal::archive::in_snapshot(c, documents::verify_all))
}
