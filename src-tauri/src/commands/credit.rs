//! Kridi (CRD-001…008): versements with their numbered receipt and the droit de timbre from the
//! backend, the notebook's opening balance, adjustments, statements, ageing. A versement is a till
//! write (open session for cash, pending approvals spent); its receipt prints through the queue.

use super::blocking;
use crate::printing::{PrintRequest, hw_error, load_settlement, print_settlement};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::company;
use pos_core::customers::{
    self,
    credit::{self, CollectInput, CollectQuote, Collected},
    receipts::{self, ReceiptChainReport},
};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::ledger::{AgingRow, EntryPage, EntryQuery, LedgerEntry, LedgerRebuild, Statement};
use pos_core::money::Money;
use pos_core::payments::PaymentMethod;
use pos_docs::settlement::build_statement_print;
use pos_hw::PrintOutcome;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectRequest {
    pub input: CollectInput,
    pub print_receipt: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectDone {
    pub collected: Collected,
    /// The receipt (or the drawer for its cash) went to the print queue.
    pub print_queued: bool,
}

/// Who reads an account: the back office (`credit.view`) and the cashier taking a versement.
const ACCOUNT: &[Permission] = &[Permission::CreditView, Permission::CreditCollect];

#[tauri::command]
#[specta::specta]
pub async fn credit_quote(
    state: State<'_, SharedState>,
    input: CollectInput,
) -> Result<CollectQuote, AppError> {
    let st = state.inner().clone();
    blocking(move || quote(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_collect(
    state: State<'_, SharedState>,
    request: CollectRequest,
) -> Result<CollectDone, AppError> {
    let st = state.inner().clone();
    blocking(move || collect(&st, request)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_reprint_receipt(
    state: State<'_, SharedState>,
    entry_id: String,
    lang: Option<Lang>,
) -> Result<PrintOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || reprint_receipt(&st, entry_id, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_set_opening(
    state: State<'_, SharedState>,
    customer_id: String,
    amount: Money,
    note: Option<String>,
) -> Result<LedgerEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || set_opening(&st, customer_id, amount, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_adjust(
    state: State<'_, SharedState>,
    customer_id: String,
    amount: Money,
    note: String,
) -> Result<LedgerEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || adjust(&st, customer_id, amount, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_entries(
    state: State<'_, SharedState>,
    customer_id: String,
    query: EntryQuery,
) -> Result<EntryPage, AppError> {
    let st = state.inner().clone();
    blocking(move || entries(&st, customer_id, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_open_items(
    state: State<'_, SharedState>,
    customer_id: String,
) -> Result<Vec<LedgerEntry>, AppError> {
    let st = state.inner().clone();
    blocking(move || open_items(&st, customer_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_statement(
    state: State<'_, SharedState>,
    customer_id: String,
    from: String,
    to: String,
) -> Result<Statement, AppError> {
    let st = state.inner().clone();
    blocking(move || statement(&st, customer_id, from, to)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_print_statement(
    state: State<'_, SharedState>,
    customer_id: String,
    from: String,
    to: String,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || print_statement(&st, customer_id, from, to, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_aging(
    state: State<'_, SharedState>,
    as_of: String,
) -> Result<Vec<AgingRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || aging(&st, as_of)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_receivables(state: State<'_, SharedState>) -> Result<Money, AppError> {
    let st = state.inner().clone();
    blocking(move || receivables(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn credit_rebuild_balances(
    state: State<'_, SharedState>,
) -> Result<LedgerRebuild, AppError> {
    let st = state.inner().clone();
    blocking(move || rebuild_balances(&st)).await
}

/// The versement receipts in the integrity check (AUD-004), next to the document chains.
#[tauri::command]
#[specta::specta]
pub async fn credit_verify_receipts(
    state: State<'_, SharedState>,
) -> Result<ReceiptChainReport, AppError> {
    let st = state.inner().clone();
    blocking(move || verify_receipts(&st)).await
}

/// The figures of the versement dialog (the duty on the cash part comes from the core).
pub(crate) fn quote(st: &AppState, input: CollectInput) -> Result<CollectQuote, AppError> {
    let ctx = st.till(Permission::CreditCollect)?;
    st.db.read(move |c| credit::quote(c, &ctx, &input))
}

/// The versement commits, then its receipt (and the drawer for cash) is queued: a printer
/// failure never undoes it and reaches the window as `print-failed` with the `RV…` number. On a
/// till it needs the main PC to answer, where the balance lives (`kridi_needs_host`).
pub(crate) fn collect(st: &AppState, request: CollectRequest) -> Result<CollectDone, AppError> {
    let ctx = st.till(Permission::CreditCollect)?;
    crate::lan::route::require_live_for_kridi(st, true)?;
    let write = ctx.clone();
    let cash_moved = request.input.method == PaymentMethod::Cash;
    let collected = st
        .db
        .write(move |tx| credit::collect(tx, &write, request.input))?;
    st.spend_approvals(&ctx)?;
    let print_queued = (request.print_receipt || cash_moved)
        && st.printer.enqueue(PrintRequest::versement(
            &collected.entry.id,
            request.print_receipt,
            cash_moved,
        ));
    Ok(CollectDone {
        collected,
        print_queued,
    })
}

/// A DUPLICATA of a versement receipt, journaled (`credit.receipt_reprinted`) then printed now
/// (the customer waits for it); the original is then no longer pending a retry, like a ticket's.
pub(crate) fn reprint_receipt(
    st: &AppState,
    entry_id: String,
    lang: Option<Lang>,
) -> Result<PrintOutcome, AppError> {
    let actor = st
        .require_any(&[Permission::CreditCollect, Permission::SalesReprint])?
        .actor();
    let id = entry_id.clone();
    st.db
        .write(move |tx| credit::record_reprint(tx, &actor, &id).map(|_| ()))?;
    st.printer.forget(&entry_id)?;
    let receipt = load_settlement(&st.db, &entry_id, true, lang)?;
    print_settlement(&st.hw, &receipt)
}

pub(crate) fn set_opening(
    st: &AppState,
    customer_id: String,
    amount: Money,
    note: Option<String>,
) -> Result<LedgerEntry, AppError> {
    let ctx = st.till(Permission::CustomersManage)?;
    let write = ctx.clone();
    let entry = st
        .db
        .write(move |tx| credit::set_opening(tx, &write, &customer_id, amount, note.as_deref()))?;
    st.spend_approvals(&ctx)?;
    Ok(entry)
}

pub(crate) fn adjust(
    st: &AppState,
    customer_id: String,
    amount: Money,
    note: String,
) -> Result<LedgerEntry, AppError> {
    let ctx = st.till(Permission::CustomersManage)?;
    let write = ctx.clone();
    let entry = st
        .db
        .write(move |tx| credit::adjust(tx, &write, &customer_id, amount, &note))?;
    st.spend_approvals(&ctx)?;
    Ok(entry)
}

pub(crate) fn entries(
    st: &AppState,
    customer_id: String,
    query: EntryQuery,
) -> Result<EntryPage, AppError> {
    st.require_any(ACCOUNT)?;
    st.db
        .read(move |c| credit::entries(c, &customer_id, &query))
}

pub(crate) fn open_items(st: &AppState, customer_id: String) -> Result<Vec<LedgerEntry>, AppError> {
    st.require_any(ACCOUNT)?;
    st.db.read(move |c| credit::open_items(c, &customer_id))
}

pub(crate) fn statement(
    st: &AppState,
    customer_id: String,
    from: String,
    to: String,
) -> Result<Statement, AppError> {
    st.require(Permission::CreditView)?;
    st.db.read(move |c| {
        customers::get(c, &customer_id)?;
        credit::statement(c, &customer_id, &from, &to)
    })
}

/// RPT-037 on the receipt printer, in `lang`; A4 and PDF come with M3's print host.
pub(crate) fn print_statement(
    st: &AppState,
    customer_id: String,
    from: String,
    to: String,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    let session = st.require(Permission::CreditView)?;
    let now = st.now();
    let journaled = customer_id.clone();
    let printed = st.db.read(move |c| {
        let company = company::get(c)?.ok_or_else(|| AppError::conflict("setup_required"))?;
        let customer = customers::get(c, &customer_id)?;
        let statement = credit::statement(c, &customer_id, &from, &to)?;
        Ok(build_statement_print(
            &company,
            &customer.name,
            &customer.code,
            &statement,
            lang,
            now,
        ))
    })?;
    let outcome = st
        .hw
        .print_layout(&crate::slips::statement_layout(&printed))
        .map_err(hw_error)?;
    // PDP-003: a customer's statement printed is journaled (`privacy.accessed`, `print`).
    let actor = session.actor();
    st.db.write(move |tx| {
        pos_core::privacy::log_access(
            tx,
            &actor,
            &journaled,
            pos_core::privacy::AccessPurpose::Print,
        )
    })?;
    Ok(outcome)
}

pub(crate) fn aging(st: &AppState, as_of: String) -> Result<Vec<AgingRow>, AppError> {
    st.require(Permission::CreditView)?;
    st.db.read(move |c| credit::aging(c, &as_of))
}

pub(crate) fn receivables(st: &AppState) -> Result<Money, AppError> {
    st.require(Permission::CreditView)?;
    st.db.read(credit::receivables)
}

pub(crate) fn rebuild_balances(st: &AppState) -> Result<LedgerRebuild, AppError> {
    let actor = st.require(Permission::SettingsData)?.actor();
    st.db.write(move |tx| credit::rebuild_balances(tx, &actor))
}

/// Every `RV…` number handed out is on its versement, as the journal recorded it (`audit.view`,
/// like the document chains).
pub(crate) fn verify_receipts(st: &AppState) -> Result<ReceiptChainReport, AppError> {
    st.require(Permission::AuditView)?;
    st.db
        .read(|c| pos_core::fiscal::archive::in_snapshot(c, receipts::verify))
}
