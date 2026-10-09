//! Purchasing (PUR-002…008): purchase orders (drafts, then `BC…`), goods receipts, supplier
//! invoices and returns, each validated at once in one transaction by the core. Only supplier
//! documents are read here; average costs are redacted without `reports.profit`.

use super::{blocking, redact};
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::documents::{self, Document};
use pos_core::error::AppError;
use pos_core::purchases::invoices::{self, InvoiceDone, InvoiceInput};
use pos_core::purchases::orders::{self, OrderInput, RemainingLine};
use pos_core::purchases::receipts::{self, ReceiptDone, ReceiptInput};
use pos_core::purchases::returns::{self, SupplierReturnDone, SupplierReturnInput};
use pos_core::purchases::{self, CostChange, PurchaseQuery, PurchaseSummary};
use tauri::State;

/// The CMUP changes shown for a product, newest first.
const COST_HISTORY_LIMIT: u32 = 100;

#[tauri::command]
#[specta::specta]
pub async fn purchases_list(
    state: State<'_, SharedState>,
    query: PurchaseQuery,
) -> Result<Vec<PurchaseSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Document, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_remaining(
    state: State<'_, SharedState>,
    order_id: String,
) -> Result<Vec<RemainingLine>, AppError> {
    let st = state.inner().clone();
    blocking(move || remaining(&st, order_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_cost_history(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Vec<CostChange>, AppError> {
    let st = state.inner().clone();
    blocking(move || cost_history(&st, product_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_save_order(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: OrderInput,
) -> Result<Document, AppError> {
    let st = state.inner().clone();
    blocking(move || save_order(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_validate_order(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Document, AppError> {
    let st = state.inner().clone();
    blocking(move || validate_order(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_cancel_order(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || cancel_order(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_receive(
    state: State<'_, SharedState>,
    input: ReceiptInput,
) -> Result<ReceiptDone, AppError> {
    let st = state.inner().clone();
    blocking(move || receive(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_record_invoice(
    state: State<'_, SharedState>,
    input: InvoiceInput,
) -> Result<InvoiceDone, AppError> {
    let st = state.inner().clone();
    blocking(move || record_invoice(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn purchases_return(
    state: State<'_, SharedState>,
    input: SupplierReturnInput,
) -> Result<SupplierReturnDone, AppError> {
    let st = state.inner().clone();
    blocking(move || return_to_supplier(&st, input)).await
}

fn shown(mut doc: Document, session: &Session) -> Document {
    redact::document(&mut doc, session);
    doc
}

pub(crate) fn list(st: &AppState, query: PurchaseQuery) -> Result<Vec<PurchaseSummary>, AppError> {
    st.require(Permission::PurchasesView)?;
    st.db.read(move |c| purchases::list(c, &query))
}

/// A supplier document; a ticket or an invoice to a customer is "not found" here (the sales
/// history has its own rules, `documents_get`).
pub(crate) fn get(st: &AppState, id: String) -> Result<Document, AppError> {
    let session = st.require(Permission::PurchasesView)?;
    let doc = st.db.read(move |c| documents::get(c, &id))?;
    if doc.supplier_id.is_none() {
        return Err(AppError::not_found("document"));
    }
    Ok(shown(doc, &session))
}

pub(crate) fn remaining(st: &AppState, order_id: String) -> Result<Vec<RemainingLine>, AppError> {
    let session = st.require(Permission::PurchasesView)?;
    let mut lines = st.db.read(move |c| orders::remaining(c, &order_id))?;
    if !redact::sees_costs(&session) {
        for l in &mut lines {
            l.line.cost_price = pos_core::money::Money::ZERO;
        }
    }
    Ok(lines)
}

pub(crate) fn cost_history(st: &AppState, product_id: String) -> Result<Vec<CostChange>, AppError> {
    let session = st.require(Permission::PurchasesView)?;
    let mut changes = st
        .db
        .read(move |c| purchases::cost_history(c, &product_id, COST_HISTORY_LIMIT))?;
    for change in &mut changes {
        redact::cost_change(change, &session);
    }
    Ok(changes)
}

pub(crate) fn save_order(
    st: &AppState,
    id: Option<String>,
    input: OrderInput,
) -> Result<Document, AppError> {
    let ctx = st.till(Permission::PurchasesManage)?;
    let write = ctx.clone();
    let saved = st
        .db
        .write(move |tx| orders::save_order(tx, &write, id.as_deref(), input))?;
    st.spend_approvals(&ctx)?;
    Ok(shown(saved, &ctx.session))
}

pub(crate) fn validate_order(st: &AppState, id: String) -> Result<Document, AppError> {
    let ctx = st.till(Permission::PurchasesManage)?;
    let write = ctx.clone();
    let validated = st
        .db
        .write(move |tx| orders::validate_order(tx, &write, &id))?;
    st.spend_approvals(&ctx)?;
    Ok(shown(validated, &ctx.session))
}

pub(crate) fn cancel_order(st: &AppState, id: String) -> Result<(), AppError> {
    let ctx = st.till(Permission::PurchasesManage)?;
    let write = ctx.clone();
    st.db
        .write(move |tx| orders::cancel_order(tx, &write, &id))?;
    st.spend_approvals(&ctx)
}

/// A payment "now" from the drawer is refused by the core without `cash.paid_in_out`.
pub(crate) fn receive(st: &AppState, input: ReceiptInput) -> Result<ReceiptDone, AppError> {
    let ctx = st.till(Permission::PurchasesManage)?;
    let write = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| receipts::receive(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    redact::document(&mut done.receipt, &ctx.session);
    if let Some(invoice) = done.invoice.as_mut() {
        redact::document(invoice, &ctx.session);
    }
    for change in &mut done.cost_changes {
        redact::cost_move(change, &ctx.session);
    }
    Ok(done)
}

pub(crate) fn record_invoice(st: &AppState, input: InvoiceInput) -> Result<InvoiceDone, AppError> {
    let ctx = st.till(Permission::PurchasesManage)?;
    let write = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| invoices::record_invoice(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    redact::document(&mut done.invoice, &ctx.session);
    Ok(done)
}

pub(crate) fn return_to_supplier(
    st: &AppState,
    input: SupplierReturnInput,
) -> Result<SupplierReturnDone, AppError> {
    let ctx = st.till(Permission::PurchasesManage)?;
    let write = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| returns::return_to_supplier(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    redact::document(&mut done.document, &ctx.session);
    Ok(done)
}
