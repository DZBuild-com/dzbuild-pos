//! Stock (M1a Task 11): levels, movements, low stock, adjustments, locations, rebuild.

use super::{blocking, redact};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::stock::{
    self, AdjustInput, LocationInput, LowStock, RebuildReport, StockLevel, StockLocation,
    StockMovement,
};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn stock_locations(
    state: State<'_, SharedState>,
) -> Result<Vec<StockLocation>, AppError> {
    let st = state.inner().clone();
    blocking(move || locations(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_create_location(
    state: State<'_, SharedState>,
    input: LocationInput,
) -> Result<StockLocation, AppError> {
    let st = state.inner().clone();
    blocking(move || create_location(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_levels(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Vec<StockLevel>, AppError> {
    let st = state.inner().clone();
    blocking(move || levels(&st, product_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_movements(
    state: State<'_, SharedState>,
    product_id: String,
    limit: u32,
) -> Result<Vec<StockMovement>, AppError> {
    let st = state.inner().clone();
    blocking(move || movements(&st, product_id, limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_low(
    state: State<'_, SharedState>,
    limit: u32,
) -> Result<Vec<LowStock>, AppError> {
    let st = state.inner().clone();
    blocking(move || low(&st, limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_adjust(
    state: State<'_, SharedState>,
    input: AdjustInput,
) -> Result<StockMovement, AppError> {
    let st = state.inner().clone();
    blocking(move || adjust(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_rebuild_levels(
    state: State<'_, SharedState>,
) -> Result<RebuildReport, AppError> {
    let st = state.inner().clone();
    blocking(move || rebuild_levels(&st)).await
}

pub(crate) fn locations(st: &AppState) -> Result<Vec<StockLocation>, AppError> {
    st.require_any(&[Permission::StockView, Permission::StockAdjust])?;
    st.db.read(stock::locations)
}

pub(crate) fn create_location(
    st: &AppState,
    input: LocationInput,
) -> Result<StockLocation, AppError> {
    let actor = st.require(Permission::StockAdjust)?.actor();
    st.db
        .write(move |tx| stock::create_location(tx, &actor, input))
}

pub(crate) fn levels(st: &AppState, product_id: String) -> Result<Vec<StockLevel>, AppError> {
    st.require_any(&[Permission::StockView, Permission::SalesCreate])?;
    st.db.read(move |c| stock::levels(c, &product_id))
}

pub(crate) fn movements(
    st: &AppState,
    product_id: String,
    limit: u32,
) -> Result<Vec<StockMovement>, AppError> {
    let session = st.require(Permission::StockView)?;
    let mut rows = st
        .db
        .read(move |c| stock::movements(c, &product_id, limit.clamp(1, 500)))?;
    for row in &mut rows {
        redact::movement(row, &session);
    }
    Ok(rows)
}

pub(crate) fn low(st: &AppState, limit: u32) -> Result<Vec<LowStock>, AppError> {
    st.require(Permission::StockView)?;
    st.db
        .read(move |c| stock::below_minimum(c, limit.clamp(1, 500)))
}

pub(crate) fn adjust(st: &AppState, input: AdjustInput) -> Result<StockMovement, AppError> {
    let session = st.require(Permission::StockAdjust)?;
    let (actor, now) = (session.actor(), st.now());
    let mut movement = st
        .db
        .write(move |tx| stock::adjust(tx, &actor, input, now))?;
    redact::movement(&mut movement, &session);
    Ok(movement)
}

pub(crate) fn rebuild_levels(st: &AppState) -> Result<RebuildReport, AppError> {
    let actor = st.require(Permission::SettingsData)?.actor();
    st.db.write(move |tx| stock::rebuild_levels(tx, &actor))
}
