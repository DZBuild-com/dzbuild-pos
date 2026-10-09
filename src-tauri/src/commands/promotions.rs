//! Promotions (PRM-001…024): automatic promotions and named discounts (presets) the cashier
//! applies by hand, legal soldes and their declaration. Writes need `products.price`.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::promotions::{self, Promotion, PromotionInput};
use pos_core::table::Table;
use pos_core::time::business_date;
use tauri::State;

const READ: &[Permission] = &[Permission::ProductsView];
/// The till's named discounts (PRM-003) are read at the sale too.
const PRESETS: &[Permission] = &[Permission::ProductsView, Permission::SalesCreate];

#[tauri::command]
#[specta::specta]
pub async fn promotions_list(
    state: State<'_, SharedState>,
    include_archived: bool,
) -> Result<Vec<Promotion>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, include_archived)).await
}

#[tauri::command]
#[specta::specta]
pub async fn promotions_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Promotion, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

/// The named discounts running today.
#[tauri::command]
#[specta::specta]
pub async fn promotions_presets(state: State<'_, SharedState>) -> Result<Vec<Promotion>, AppError> {
    let st = state.inner().clone();
    blocking(move || presets(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn promotions_create(
    state: State<'_, SharedState>,
    input: PromotionInput,
) -> Result<Promotion, AppError> {
    let st = state.inner().clone();
    blocking(move || create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn promotions_update(
    state: State<'_, SharedState>,
    id: String,
    input: PromotionInput,
) -> Result<Promotion, AppError> {
    let st = state.inner().clone();
    blocking(move || update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn promotions_archive(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || archive(&st, id)).await
}

/// The soldes declaration (PRM-022, RPT-054) in `lang`.
#[tauri::command]
#[specta::specta]
pub async fn promotions_declaration(
    state: State<'_, SharedState>,
    id: String,
    lang: Lang,
) -> Result<Table, AppError> {
    let st = state.inner().clone();
    blocking(move || declaration(&st, id, lang)).await
}

pub(crate) fn list(st: &AppState, include_archived: bool) -> Result<Vec<Promotion>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| promotions::list(c, include_archived))
}

pub(crate) fn get(st: &AppState, id: String) -> Result<Promotion, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| promotions::get(c, &id))
}

pub(crate) fn presets(st: &AppState) -> Result<Vec<Promotion>, AppError> {
    st.require_any(PRESETS)?;
    let today = business_date(st.now()).format("%Y-%m-%d").to_string();
    st.db.read(move |c| promotions::presets(c, &today))
}

pub(crate) fn create(st: &AppState, input: PromotionInput) -> Result<Promotion, AppError> {
    let actor = st.require(Permission::ProductsPrice)?.actor();
    st.db.write(move |tx| promotions::create(tx, &actor, input))
}

pub(crate) fn update(
    st: &AppState,
    id: String,
    input: PromotionInput,
) -> Result<Promotion, AppError> {
    let actor = st.require(Permission::ProductsPrice)?.actor();
    st.db
        .write(move |tx| promotions::update(tx, &actor, &id, input))
}

pub(crate) fn archive(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::ProductsPrice)?.actor();
    st.db.write(move |tx| promotions::archive(tx, &actor, &id))
}

pub(crate) fn declaration(st: &AppState, id: String, lang: Lang) -> Result<Table, AppError> {
    st.require(Permission::ReportsSales)?;
    st.db.read(move |c| promotions::declaration(c, &id, lang))
}
