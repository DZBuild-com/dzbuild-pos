//! Products (M1a Tasks 6–7): edits, level prices, search, scan lookup, grid and variants.
//! Every product answered goes through `redact::product`.

use super::{blocking, redact};
use crate::lan::route::forward;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::catalog::products::{self, Product, ProductInput};
use pos_core::catalog::search::{self, CatalogQuery, ProductHit, ScanHit, SearchQuery};
use pos_core::error::AppError;
use pos_core::money::Money;
use serde_json::json;
use tauri::State;

const READ: &[Permission] = &[Permission::ProductsView, Permission::SalesCreate];
const GRID_DEFAULT: u32 = 200;
const GRID_MAX: u32 = 500;

#[tauri::command]
#[specta::specta]
pub async fn products_get(state: State<'_, SharedState>, id: String) -> Result<Product, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_create(
    state: State<'_, SharedState>,
    input: ProductInput,
) -> Result<Product, AppError> {
    let st = state.inner().clone();
    blocking(move || create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_update(
    state: State<'_, SharedState>,
    id: String,
    input: ProductInput,
) -> Result<Product, AppError> {
    let st = state.inner().clone();
    blocking(move || update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_archive(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || archive(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_restore(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || restore(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_duplicate(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Product, AppError> {
    let st = state.inner().clone();
    blocking(move || duplicate(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_set_price(
    state: State<'_, SharedState>,
    id: String,
    price_level_id: String,
    price: Money,
) -> Result<Product, AppError> {
    let st = state.inner().clone();
    blocking(move || set_price(&st, id, price_level_id, price)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_search(
    state: State<'_, SharedState>,
    query: SearchQuery,
) -> Result<Vec<ProductHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || search(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_find_by_code(
    state: State<'_, SharedState>,
    code: String,
) -> Result<Option<ScanHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || find_by_code(&st, code)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_grid(
    state: State<'_, SharedState>,
    category_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<ProductHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || grid(&st, category_id, limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_catalog(
    state: State<'_, SharedState>,
    query: CatalogQuery,
) -> Result<Vec<ProductHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || catalog(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn products_variants(
    state: State<'_, SharedState>,
    parent_id: String,
) -> Result<Vec<ProductHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || variants(&st, parent_id)).await
}

pub(crate) fn get(st: &AppState, id: String) -> Result<Product, AppError> {
    let session = st.require_any(READ)?;
    let mut product = st.db.read(move |c| products::get(c, &id))?;
    redact::product(&mut product, &session);
    Ok(product)
}

/// `products.edit` alone may set a new product's first prices: `products.price` guards changing
/// an existing price (`set_price`, `update`), and a storekeeper must be able to add the goods
/// they receive. The first prices are in the `product.created` journal entry. On a till the
/// catalogue is the main PC's: this runs there (`lan::route::forward`).
pub(crate) fn create(st: &AppState, input: ProductInput) -> Result<Product, AppError> {
    if let Some(done) = forward(st, "products.create", json!({ "input": input }))? {
        return Ok(done);
    }
    let session = st.require(Permission::ProductsEdit)?;
    let actor = session.actor();
    let mut product = st.db.write(move |tx| products::create(tx, &actor, input))?;
    redact::product(&mut product, &session);
    Ok(product)
}

/// An editor who cannot see costs (no `reports.profit`) sent back the zero we showed: the stored
/// cost is kept, never overwritten by the redaction. An editor without `products.price` cannot
/// reprice through an update either: the stored `sell_price` and level prices are kept, like
/// `set_price` refuses them (spec §4.5).
pub(crate) fn update(
    st: &AppState,
    id: String,
    mut input: ProductInput,
) -> Result<Product, AppError> {
    if let Some(done) = forward(st, "products.update", json!({ "id": id, "input": input }))? {
        return Ok(done);
    }
    let session = st.require(Permission::ProductsEdit)?;
    let actor = session.actor();
    let keep_cost = !redact::sees_costs(&session);
    let keep_prices = !session.has(Permission::ProductsPrice);
    let mut product = st.db.write(move |tx| {
        if keep_cost || keep_prices {
            let stored = products::get(tx, &id)?;
            if keep_cost {
                input.cost_price = stored.cost_price;
            }
            if keep_prices {
                input.sell_price = stored.sell_price;
                input.prices = stored.prices;
            }
        }
        products::update(tx, &actor, &id, input)
    })?;
    redact::product(&mut product, &session);
    Ok(product)
}

/// The copy carries the source's prices unchanged, so `products.edit` is enough.
pub(crate) fn duplicate(st: &AppState, id: String) -> Result<Product, AppError> {
    let session = st.require(Permission::ProductsEdit)?;
    let actor = session.actor();
    let mut product = st
        .db
        .write(move |tx| products::duplicate(tx, &actor, &id))?;
    redact::product(&mut product, &session);
    Ok(product)
}

pub(crate) fn set_price(
    st: &AppState,
    id: String,
    price_level_id: String,
    price: Money,
) -> Result<Product, AppError> {
    if let Some(done) = forward(
        st,
        "products.set_price",
        json!({ "id": id, "priceLevelId": price_level_id, "price": price }),
    )? {
        return Ok(done);
    }
    let session = st.require(Permission::ProductsPrice)?;
    let actor = session.actor();
    let mut product = st
        .db
        .write(move |tx| products::set_price(tx, &actor, &id, &price_level_id, price))?;
    redact::product(&mut product, &session);
    Ok(product)
}

pub(crate) fn archive(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| products::archive(tx, &actor, &id))
}

pub(crate) fn restore(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| products::restore(tx, &actor, &id))
}

pub(crate) fn search(st: &AppState, query: SearchQuery) -> Result<Vec<ProductHit>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| search::search(c, &query))
}

/// Barcode (with its pack quantity), else reference, else PLU (SAL-012).
pub(crate) fn find_by_code(st: &AppState, code: String) -> Result<Option<ScanHit>, AppError> {
    let session = st.require_any(READ)?;
    let mut hit = st.db.read(move |c| search::find_by_code(c, code.trim()))?;
    if let Some(hit) = hit.as_mut() {
        redact::product(&mut hit.product, &session);
    }
    Ok(hit)
}

pub(crate) fn grid(
    st: &AppState,
    category_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<ProductHit>, AppError> {
    st.require_any(READ)?;
    let limit = i64::from(limit.unwrap_or(GRID_DEFAULT).clamp(1, GRID_MAX));
    st.db
        .read(move |c| search::list_by_category(c, category_id.as_deref(), limit))
}

/// The back-office list (`products.view`): every product, sold at the till or not, archived on
/// demand, a page at a time.
pub(crate) fn catalog(st: &AppState, query: CatalogQuery) -> Result<Vec<ProductHit>, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(move |c| search::catalog(c, &query))
}

pub(crate) fn variants(st: &AppState, parent_id: String) -> Result<Vec<ProductHit>, AppError> {
    st.require_any(READ)?;
    st.db.read(move |c| search::variants_of(c, &parent_id))
}
