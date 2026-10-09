//! Units, categories, VAT rates and price levels (M1a Task 5). Reads serve the till too.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::catalog::categories::{self, Category, CategoryInput};
use pos_core::catalog::price_levels::{self, PriceLevel, PriceLevelInput};
use pos_core::catalog::tax_rates::{self, TaxRate, TaxRateUpdate};
use pos_core::catalog::units::{self, Unit, UnitInput};
use pos_core::error::AppError;
use tauri::State;

const READ: &[Permission] = &[Permission::ProductsView, Permission::SalesCreate];

#[tauri::command]
#[specta::specta]
pub async fn catalog_units_list(state: State<'_, SharedState>) -> Result<Vec<Unit>, AppError> {
    let st = state.inner().clone();
    blocking(move || units_list(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_units_create(
    state: State<'_, SharedState>,
    input: UnitInput,
) -> Result<Unit, AppError> {
    let st = state.inner().clone();
    blocking(move || units_create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_units_update(
    state: State<'_, SharedState>,
    id: String,
    input: UnitInput,
) -> Result<Unit, AppError> {
    let st = state.inner().clone();
    blocking(move || units_update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_categories_list(
    state: State<'_, SharedState>,
    include_archived: bool,
) -> Result<Vec<Category>, AppError> {
    let st = state.inner().clone();
    blocking(move || categories_list(&st, include_archived)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_categories_create(
    state: State<'_, SharedState>,
    input: CategoryInput,
) -> Result<Category, AppError> {
    let st = state.inner().clone();
    blocking(move || categories_create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_categories_update(
    state: State<'_, SharedState>,
    id: String,
    input: CategoryInput,
) -> Result<Category, AppError> {
    let st = state.inner().clone();
    blocking(move || categories_update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_categories_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || categories_archive(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_tax_rates_list(
    state: State<'_, SharedState>,
) -> Result<Vec<TaxRate>, AppError> {
    let st = state.inner().clone();
    blocking(move || tax_rates_list(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_tax_rates_update(
    state: State<'_, SharedState>,
    id: String,
    input: TaxRateUpdate,
) -> Result<TaxRate, AppError> {
    let st = state.inner().clone();
    blocking(move || tax_rates_update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_price_levels_list(
    state: State<'_, SharedState>,
) -> Result<Vec<PriceLevel>, AppError> {
    let st = state.inner().clone();
    blocking(move || price_levels_list(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_price_levels_create(
    state: State<'_, SharedState>,
    input: PriceLevelInput,
) -> Result<PriceLevel, AppError> {
    let st = state.inner().clone();
    blocking(move || price_levels_create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_price_levels_update(
    state: State<'_, SharedState>,
    id: String,
    input: PriceLevelInput,
) -> Result<PriceLevel, AppError> {
    let st = state.inner().clone();
    blocking(move || price_levels_update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_price_levels_set_default(
    state: State<'_, SharedState>,
    id: String,
) -> Result<PriceLevel, AppError> {
    let st = state.inner().clone();
    blocking(move || price_levels_set_default(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn catalog_price_levels_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || price_levels_archive(&st, id)).await
}

pub(crate) fn units_list(st: &AppState) -> Result<Vec<Unit>, AppError> {
    st.require_any(READ)?;
    st.db.read(units::list)
}

pub(crate) fn units_create(st: &AppState, input: UnitInput) -> Result<Unit, AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| units::create(tx, &actor, input))
}

pub(crate) fn units_update(st: &AppState, id: String, input: UnitInput) -> Result<Unit, AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| units::update(tx, &actor, &id, input))
}

pub(crate) fn categories_list(
    st: &AppState,
    include_archived: bool,
) -> Result<Vec<Category>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| categories::list(c, include_archived))
}

pub(crate) fn categories_create(st: &AppState, input: CategoryInput) -> Result<Category, AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| categories::create(tx, &actor, input))
}

pub(crate) fn categories_update(
    st: &AppState,
    id: String,
    input: CategoryInput,
) -> Result<Category, AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db
        .write(move |tx| categories::update(tx, &actor, &id, input))
}

pub(crate) fn categories_archive(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| categories::archive(tx, &actor, &id))
}

pub(crate) fn tax_rates_list(st: &AppState) -> Result<Vec<TaxRate>, AppError> {
    st.require_any(&[
        Permission::ProductsView,
        Permission::SalesCreate,
        Permission::SettingsCompany,
    ])?;
    st.db.read(tax_rates::list)
}

pub(crate) fn tax_rates_update(
    st: &AppState,
    id: String,
    input: TaxRateUpdate,
) -> Result<TaxRate, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| tax_rates::update(tx, &actor, &id, input))
}

pub(crate) fn price_levels_list(st: &AppState) -> Result<Vec<PriceLevel>, AppError> {
    st.require_any(READ)?;
    st.db.read(price_levels::list)
}

pub(crate) fn price_levels_create(
    st: &AppState,
    input: PriceLevelInput,
) -> Result<PriceLevel, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| price_levels::create(tx, &actor, input))
}

pub(crate) fn price_levels_update(
    st: &AppState,
    id: String,
    input: PriceLevelInput,
) -> Result<PriceLevel, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| price_levels::update(tx, &actor, &id, input))
}

pub(crate) fn price_levels_set_default(st: &AppState, id: String) -> Result<PriceLevel, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| price_levels::set_default(tx, &actor, &id))
}

pub(crate) fn price_levels_archive(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| price_levels::archive(tx, &actor, &id))
}
