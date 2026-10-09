//! Suppliers (PUR-001, PUR-006): directory, opening balance, adjustments, payments from the safe
//! or the drawer, statements and ageing of what the shop owes.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission};
use pos_core::error::AppError;
use pos_core::ledger::{AgingRow, EntryPage, EntryQuery, LedgerEntry, Statement};
use pos_core::money::Money;
use pos_core::suppliers::{
    self, Supplier, SupplierHit, SupplierInput, SupplierPaymentInput, SupplierQuery,
};
use tauri::State;

/// The directory and the accounts are read by whoever buys (`purchases.view`) or keeps them.
const READ: &[Permission] = &[Permission::SuppliersManage, Permission::PurchasesView];

#[tauri::command]
#[specta::specta]
pub async fn suppliers_search(
    state: State<'_, SharedState>,
    query: SupplierQuery,
) -> Result<Vec<SupplierHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || search(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Supplier, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_create(
    state: State<'_, SharedState>,
    input: SupplierInput,
) -> Result<Supplier, AppError> {
    let st = state.inner().clone();
    blocking(move || create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_update(
    state: State<'_, SharedState>,
    id: String,
    input: SupplierInput,
) -> Result<Supplier, AppError> {
    let st = state.inner().clone();
    blocking(move || update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_archive(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || archive(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_restore(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || restore(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_set_opening(
    state: State<'_, SharedState>,
    supplier_id: String,
    amount: Money,
    note: Option<String>,
) -> Result<LedgerEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || set_opening(&st, supplier_id, amount, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_adjust(
    state: State<'_, SharedState>,
    supplier_id: String,
    amount: Money,
    note: String,
) -> Result<LedgerEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || adjust(&st, supplier_id, amount, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_pay(
    state: State<'_, SharedState>,
    input: SupplierPaymentInput,
) -> Result<LedgerEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || pay(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_entries(
    state: State<'_, SharedState>,
    supplier_id: String,
    query: EntryQuery,
) -> Result<EntryPage, AppError> {
    let st = state.inner().clone();
    blocking(move || entries(&st, supplier_id, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_open_items(
    state: State<'_, SharedState>,
    supplier_id: String,
) -> Result<Vec<LedgerEntry>, AppError> {
    let st = state.inner().clone();
    blocking(move || open_items(&st, supplier_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_statement(
    state: State<'_, SharedState>,
    supplier_id: String,
    from: String,
    to: String,
) -> Result<Statement, AppError> {
    let st = state.inner().clone();
    blocking(move || statement(&st, supplier_id, from, to)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_aging(
    state: State<'_, SharedState>,
    as_of: String,
) -> Result<Vec<AgingRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || aging(&st, as_of)).await
}

#[tauri::command]
#[specta::specta]
pub async fn suppliers_payables(state: State<'_, SharedState>) -> Result<Money, AppError> {
    let st = state.inner().clone();
    blocking(move || payables(&st)).await
}

pub(crate) fn search(st: &AppState, query: SupplierQuery) -> Result<Vec<SupplierHit>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| suppliers::search(c, &query))
}

pub(crate) fn get(st: &AppState, id: String) -> Result<Supplier, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| suppliers::get(c, &id))
}

pub(crate) fn create(st: &AppState, input: SupplierInput) -> Result<Supplier, AppError> {
    let actor = st.require(Permission::SuppliersManage)?.actor();
    let pack = st.datapack.clone();
    st.db
        .write(move |tx| suppliers::create(tx, &actor, &pack, input))
}

pub(crate) fn update(
    st: &AppState,
    id: String,
    input: SupplierInput,
) -> Result<Supplier, AppError> {
    let actor = st.require(Permission::SuppliersManage)?.actor();
    let pack = st.datapack.clone();
    st.db
        .write(move |tx| suppliers::update(tx, &actor, &pack, &id, input))
}

pub(crate) fn archive(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::SuppliersManage)?.actor();
    st.db.write(move |tx| suppliers::archive(tx, &actor, &id))
}

pub(crate) fn restore(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::SuppliersManage)?.actor();
    st.db.write(move |tx| suppliers::restore(tx, &actor, &id))
}

pub(crate) fn set_opening(
    st: &AppState,
    supplier_id: String,
    amount: Money,
    note: Option<String>,
) -> Result<LedgerEntry, AppError> {
    let ctx = st.till(Permission::SuppliersManage)?;
    let write = ctx.clone();
    let entry = st.db.write(move |tx| {
        suppliers::set_opening(tx, &write, &supplier_id, amount, note.as_deref())
    })?;
    st.spend_approvals(&ctx)?;
    Ok(entry)
}

pub(crate) fn adjust(
    st: &AppState,
    supplier_id: String,
    amount: Money,
    note: String,
) -> Result<LedgerEntry, AppError> {
    let ctx = st.till(Permission::SuppliersManage)?;
    let write = ctx.clone();
    let entry = st
        .db
        .write(move |tx| suppliers::adjust(tx, &write, &supplier_id, amount, &note))?;
    st.spend_approvals(&ctx)?;
    Ok(entry)
}

/// Cash from the drawer also needs `cash.paid_in_out` (or its approval), checked before the
/// session is looked at: a storekeeper pays from the safe.
pub(crate) fn pay(st: &AppState, input: SupplierPaymentInput) -> Result<LedgerEntry, AppError> {
    let ctx = st.till(Permission::SuppliersManage)?;
    if input.from_till && !ctx.allows(Permission::CashPaidInOut) {
        auth::require(&ctx.session, Permission::CashPaidInOut)?;
    }
    let write = ctx.clone();
    let entry = st.db.write(move |tx| suppliers::pay(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    Ok(entry)
}

pub(crate) fn entries(
    st: &AppState,
    supplier_id: String,
    query: EntryQuery,
) -> Result<EntryPage, AppError> {
    st.require_any(READ)?;
    st.db
        .read(move |c| suppliers::entries(c, &supplier_id, &query))
}

pub(crate) fn open_items(st: &AppState, supplier_id: String) -> Result<Vec<LedgerEntry>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| suppliers::open_items(c, &supplier_id))
}

pub(crate) fn statement(
    st: &AppState,
    supplier_id: String,
    from: String,
    to: String,
) -> Result<Statement, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| {
        suppliers::get(c, &supplier_id)?;
        suppliers::statement(c, &supplier_id, &from, &to)
    })
}

pub(crate) fn aging(st: &AppState, as_of: String) -> Result<Vec<AgingRow>, AppError> {
    st.require(Permission::PurchasesView)?;
    st.db.read(move |c| suppliers::aging(c, &as_of))
}

pub(crate) fn payables(st: &AppState) -> Result<Money, AppError> {
    st.require(Permission::PurchasesView)?;
    st.db.read(suppliers::payables)
}
