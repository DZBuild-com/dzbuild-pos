//! Expenses (FIN-001, CSH-002): editable categories, expenses paid from the drawer or in the
//! back office, reversals (never a deletion). The cashier records; managers read the lists.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::expenses::{
    self, CategoryTotal, Expense, ExpenseCategoryInfo, ExpenseCategoryInput, ExpenseQuery,
    NewExpense,
};
use tauri::State;

/// The categories are picked by whoever records an expense, and read in the reports.
const CATEGORIES_READ: &[Permission] = &[
    Permission::CashPaidInOut,
    Permission::ReportsCash,
    Permission::SettingsCompany,
];
const READ: &[Permission] = &[Permission::ReportsCash, Permission::CashViewHistory];

#[tauri::command]
#[specta::specta]
pub async fn expenses_categories(
    state: State<'_, SharedState>,
    include_archived: bool,
) -> Result<Vec<ExpenseCategoryInfo>, AppError> {
    let st = state.inner().clone();
    blocking(move || categories(&st, include_archived)).await
}

#[tauri::command]
#[specta::specta]
pub async fn expenses_category_create(
    state: State<'_, SharedState>,
    input: ExpenseCategoryInput,
) -> Result<ExpenseCategoryInfo, AppError> {
    let st = state.inner().clone();
    blocking(move || create_category(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn expenses_category_update(
    state: State<'_, SharedState>,
    id: String,
    input: ExpenseCategoryInput,
) -> Result<ExpenseCategoryInfo, AppError> {
    let st = state.inner().clone();
    blocking(move || update_category(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn expenses_category_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || archive_category(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn expenses_record(
    state: State<'_, SharedState>,
    input: NewExpense,
) -> Result<Expense, AppError> {
    let st = state.inner().clone();
    blocking(move || record(&st, input)).await
}

/// A negative expense cancelling `id`; `note` says why.
#[tauri::command]
#[specta::specta]
pub async fn expenses_reverse(
    state: State<'_, SharedState>,
    id: String,
    note: String,
) -> Result<Expense, AppError> {
    let st = state.inner().clone();
    blocking(move || reverse(&st, id, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn expenses_list(
    state: State<'_, SharedState>,
    query: ExpenseQuery,
) -> Result<Vec<Expense>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, query)).await
}

/// Net expenses per category between two business days (`YYYY-MM-DD`, both included).
#[tauri::command]
#[specta::specta]
pub async fn expenses_totals(
    state: State<'_, SharedState>,
    from: String,
    to: String,
) -> Result<Vec<CategoryTotal>, AppError> {
    let st = state.inner().clone();
    blocking(move || totals(&st, from, to)).await
}

pub(crate) fn categories(
    st: &AppState,
    include_archived: bool,
) -> Result<Vec<ExpenseCategoryInfo>, AppError> {
    st.require_any(CATEGORIES_READ)?;
    st.db
        .read(move |c| expenses::categories(c, include_archived))
}

pub(crate) fn create_category(
    st: &AppState,
    input: ExpenseCategoryInput,
) -> Result<ExpenseCategoryInfo, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| expenses::create_category(tx, &actor, input))
}

pub(crate) fn update_category(
    st: &AppState,
    id: String,
    input: ExpenseCategoryInput,
) -> Result<ExpenseCategoryInfo, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| expenses::update_category(tx, &actor, &id, input))
}

pub(crate) fn archive_category(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| expenses::archive_category(tx, &actor, &id))
}

pub(crate) fn record(st: &AppState, input: NewExpense) -> Result<Expense, AppError> {
    let ctx = st.till(Permission::CashPaidInOut)?;
    let write = ctx.clone();
    let expense = st.db.write(move |tx| expenses::record(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    Ok(expense)
}

pub(crate) fn reverse(st: &AppState, id: String, note: String) -> Result<Expense, AppError> {
    let ctx = st.till(Permission::CashPaidInOut)?;
    let write = ctx.clone();
    let reversal = st
        .db
        .write(move |tx| expenses::reverse(tx, &write, &id, &note))?;
    st.spend_approvals(&ctx)?;
    Ok(reversal)
}

pub(crate) fn list(st: &AppState, query: ExpenseQuery) -> Result<Vec<Expense>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| expenses::list(c, &query))
}

pub(crate) fn totals(
    st: &AppState,
    from: String,
    to: String,
) -> Result<Vec<CategoryTotal>, AppError> {
    st.require_any(READ)?;
    st.db
        .read(move |c| expenses::totals_by_category(c, &from, &to))
}
