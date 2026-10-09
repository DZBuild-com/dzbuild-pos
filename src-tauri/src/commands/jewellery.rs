//! Jewellery (VRT-140…145): the pieces priced from the gold board, the board and its history,
//! the repricing of the catalogue, and the jeweller register's entries (CII art. 359, LF 2026
//! art. 62). The board, the pieces and the register are the main PC's: on a
//! till the writes are its host services and the register — never copied to a till — is read
//! from it.

use super::blocking;
use crate::lan::route::forward;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::jewellery::{
    self, Metal, MetalPrice, Piece, PieceInput, RegisterEntry, RepriceReport,
};
use pos_core::money::Money;
use serde_json::json;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn jewellery_piece_get(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Option<Piece>, AppError> {
    let st = state.inner().clone();
    blocking(move || piece_get(&st, product_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn jewellery_piece_set(
    state: State<'_, SharedState>,
    product_id: String,
    input: PieceInput,
) -> Result<Piece, AppError> {
    let st = state.inner().clone();
    blocking(move || piece_set(&st, product_id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn metal_board(state: State<'_, SharedState>) -> Result<Vec<MetalPrice>, AppError> {
    let st = state.inner().clone();
    blocking(move || board(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn metal_price_set(
    state: State<'_, SharedState>,
    metal: Metal,
    titre: u16,
    price_per_gram: Money,
) -> Result<MetalPrice, AppError> {
    let st = state.inner().clone();
    blocking(move || set_metal_price(&st, metal, titre, price_per_gram)).await
}

#[tauri::command]
#[specta::specta]
pub async fn metal_history(
    state: State<'_, SharedState>,
    metal: Metal,
    titre: u16,
) -> Result<Vec<MetalPrice>, AppError> {
    let st = state.inner().clone();
    blocking(move || history(&st, metal, titre)).await
}

#[tauri::command]
#[specta::specta]
pub async fn jewellery_reprice(state: State<'_, SharedState>) -> Result<RepriceReport, AppError> {
    let st = state.inner().clone();
    blocking(move || reprice(&st)).await
}

/// The register of `year` from entry `fromSeq`, in order (at most `limit`, ≤ 1000).
#[tauri::command]
#[specta::specta]
pub async fn jeweller_register_entries(
    state: State<'_, SharedState>,
    year: i32,
    from_seq: u32,
    limit: u32,
) -> Result<Vec<RegisterEntry>, AppError> {
    let st = state.inner().clone();
    blocking(move || register_entries(&st, year, from_seq, limit)).await
}

pub(crate) fn piece_get(st: &AppState, product_id: String) -> Result<Option<Piece>, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(move |c| jewellery::piece(c, &product_id))
}

pub(crate) fn piece_set(
    st: &AppState,
    product_id: String,
    input: PieceInput,
) -> Result<Piece, AppError> {
    if let Some(done) = forward(
        st,
        "jewellery.piece_set",
        json!({ "productId": product_id, "input": input }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db
        .write(move |tx| jewellery::set_piece(tx, &actor, &product_id, input))
}

/// The board the till prices the pieces from.
pub(crate) fn board(st: &AppState) -> Result<Vec<MetalPrice>, AppError> {
    st.require_any(&[Permission::ProductsView, Permission::SalesCreate])?;
    st.db.read(jewellery::board)
}

pub(crate) fn set_metal_price(
    st: &AppState,
    metal: Metal,
    titre: u16,
    price_per_gram: Money,
) -> Result<MetalPrice, AppError> {
    if let Some(done) = forward(
        st,
        "jewellery.metal_price_set",
        json!({ "metal": metal, "titre": titre, "pricePerGram": price_per_gram }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::ProductsPrice)?;
    st.db
        .write(move |tx| jewellery::set_metal_price(tx, &ctx, metal, titre, price_per_gram))
}

pub(crate) fn history(
    st: &AppState,
    metal: Metal,
    titre: u16,
) -> Result<Vec<MetalPrice>, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(move |c| jewellery::history(c, metal, titre))
}

/// The live prices of the pieces priced by weight written into the catalogue.
pub(crate) fn reprice(st: &AppState) -> Result<RepriceReport, AppError> {
    if let Some(done) = forward(st, "jewellery.reprice", json!({}))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::ProductsPrice)?;
    st.db.write(move |tx| jewellery::reprice(tx, &ctx))
}

/// The register holds the buyers' identities and lives on the main PC only: a till asks it. It
/// is a manager's read (`customers.manage`): a cashier never reads buyers' identity numbers.
pub(crate) fn register_entries(
    st: &AppState,
    year: i32,
    from_seq: u32,
    limit: u32,
) -> Result<Vec<RegisterEntry>, AppError> {
    if let Some(done) = forward(
        st,
        "jewellery.register_entries",
        json!({ "year": year, "fromSeq": from_seq, "limit": limit }),
    )? {
        return Ok(done);
    }
    st.require(Permission::CustomersManage)?;
    st.db.read(move |c| {
        pos_core::packs::require(c, pos_core::packs::Module::Jewellery)?;
        jewellery::entries(c, year, from_seq, limit)
    })
}
