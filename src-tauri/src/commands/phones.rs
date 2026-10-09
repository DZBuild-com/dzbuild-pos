//! Phones and flexy (Task 20; VRT-002, PHN): the serials in stock of a product for the till's
//! serial prompt, the top-up operators with their floats, the cart line of a top-up, the float
//! bought from the distributor and its ledger. The float is a balance, so buying it is a host
//! service: a till does it on the main PC, and offline it is refused (`host_required`).

use super::blocking;
use crate::lan::route::forward_money;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::flexy::{self, FlexyOperator, FloatEntry, FloatPurchase};
use pos_core::money::Money;
use pos_core::sales::CartLine;
use pos_core::stock::serials::{self, Serial, SerialQuery, SerialStatus};
use serde_json::json;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn serials_available(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Vec<Serial>, AppError> {
    let st = state.inner().clone();
    blocking(move || serials_available_in(&st, product_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn flexy_operators(
    state: State<'_, SharedState>,
) -> Result<Vec<FlexyOperator>, AppError> {
    let st = state.inner().clone();
    blocking(move || operators(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn flexy_line(
    state: State<'_, SharedState>,
    operator: String,
    phone: String,
    amount: Money,
) -> Result<CartLine, AppError> {
    let st = state.inner().clone();
    blocking(move || line(&st, operator, phone, amount)).await
}

#[tauri::command]
#[specta::specta]
pub async fn flexy_purchase(
    state: State<'_, SharedState>,
    input: FloatPurchase,
) -> Result<FloatEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || purchase(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn flexy_balances(
    state: State<'_, SharedState>,
) -> Result<Vec<(String, Money)>, AppError> {
    let st = state.inner().clone();
    blocking(move || balances(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn flexy_entries(
    state: State<'_, SharedState>,
    operator: Option<String>,
    from: String,
    to: String,
) -> Result<Vec<FloatEntry>, AppError> {
    let st = state.inner().clone();
    blocking(move || entries(&st, operator, from, to)).await
}

/// The serials of `product_id` still in stock, newest first: what the till's prompt offers.
pub(crate) fn serials_available_in(
    st: &AppState,
    product_id: String,
) -> Result<Vec<Serial>, AppError> {
    st.require(Permission::SalesCreate)?;
    let query = SerialQuery {
        product_id: Some(product_id),
        status: Some(SerialStatus::InStock),
        text: String::new(),
        limit: Some(500),
    };
    st.db.read(move |c| serials::list(c, &query))
}

pub(crate) fn operators(st: &AppState) -> Result<Vec<FlexyOperator>, AppError> {
    st.require(Permission::SalesCreate)?;
    let pack = st.datapack.clone();
    st.db.read(move |c| flexy::operators(c, &pack))
}

/// The cart line of a top-up of `amount` to `phone` (the till adds it like any line).
pub(crate) fn line(
    st: &AppState,
    operator: String,
    phone: String,
    amount: Money,
) -> Result<CartLine, AppError> {
    st.require(Permission::SalesCreate)?;
    let pack = st.datapack.clone();
    st.db
        .read(move |c| flexy::line(c, &pack, &operator, &phone, amount))
}

/// Float bought from the distributor, paid from this till's drawer or outside it. On a till it is
/// recorded on the main PC as the till's user (`flexy.purchase`), and the cash paid from the
/// drawer goes out of this till's drawer (`route::keep_cash`, Task 20 review).
pub(crate) fn purchase(st: &AppState, input: FloatPurchase) -> Result<FloatEntry, AppError> {
    if let Some(done) =
        forward_money::<FloatEntry>(st, "flexy.purchase", json!({ "input": input }), false)?
    {
        return Ok(done);
    }
    let ctx = st.till(Permission::CashPaidInOut)?;
    let pack = st.datapack.clone();
    st.db
        .write(move |tx| flexy::purchase(tx, &ctx, &pack, input))
}

pub(crate) fn balances(st: &AppState) -> Result<Vec<(String, Money)>, AppError> {
    st.require(Permission::SalesCreate)?;
    let pack = st.datapack.clone();
    st.db.read(move |c| flexy::balances(c, &pack))
}

/// The float ledger of the business days `from`..=`to` (the cash report's flexy tab).
pub(crate) fn entries(
    st: &AppState,
    operator: Option<String>,
    from: String,
    to: String,
) -> Result<Vec<FloatEntry>, AppError> {
    st.require(Permission::ReportsCash)?;
    st.db
        .read(move |c| flexy::entries(c, operator.as_deref(), &from, &to))
}
