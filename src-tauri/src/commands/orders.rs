//! Customer orders, layaway and appointments (Task 20; ORD-001…003, ORD-007, CRD-007/008): an
//! order recorded with its deposit (`CC…` and its `AC…` receipt), more deposits over time, made
//! ready, served at the till (a ticket and its receipt), delivered as a BL or an invoice, closed,
//! or its arrhes kept; the appointment book of a salon or a clinic.
//!
//! Orders are company-series documents holding deposits, and appointments a shared book, so every
//! write here is a host service: on a till it runs on the main PC as the
//! till's user, and offline it is refused (`host_required`). Receipts print on this PC, and the
//! cash the main PC booked for this till goes into this till's drawer (`route::keep_cash`,
//! Task 20 review).

use super::services::{TakenDeposit, print_deposit, queue_receipt, taken_deposit};
use super::{blocking, redact};
use crate::lan::route::{forward, forward_money};
use crate::state::{AppState, SharedState};
use pos_core::appointments::{self, Appointment, AppointmentInput, AppointmentStatus};
use pos_core::auth::Permission;
use pos_core::billing::kind_permission;
use pos_core::billing::validate::ValidatedDocument;
use pos_core::deposits::{self, Deposit, OwnerType};
use pos_core::error::AppError;
use pos_core::orders::{
    self, CustomerOrderFilter, CustomerOrderInput, DeliverInput, FulfilInput, Fulfilled, Order,
    OrderDeposit, OrderSummary,
};
use pos_core::sales::Cart;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::State;

/// An order recorded and, when it took one, its deposit with what the `AC…` receipt prints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreatedOrder {
    pub order: Order,
    pub deposit: Option<TakenDeposit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FulfilledDone {
    pub fulfilled: Fulfilled,
    /// The ticket's receipt (or the drawer for its cash) was queued.
    pub print_queued: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn orders_create(
    state: State<'_, SharedState>,
    input: CustomerOrderInput,
) -> Result<Order, AppError> {
    let st = state.inner().clone();
    blocking(move || create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_get(state: State<'_, SharedState>, id: String) -> Result<Order, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_list(
    state: State<'_, SharedState>,
    filter: CustomerOrderFilter,
) -> Result<Vec<OrderSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, filter)).await
}

/// The order's lines as a till cart at the ordered prices.
#[tauri::command]
#[specta::specta]
pub async fn orders_fulfil_cart(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Cart, AppError> {
    let st = state.inner().clone();
    blocking(move || fulfil_cart(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_pay_in(
    state: State<'_, SharedState>,
    id: String,
    deposit: OrderDeposit,
) -> Result<Deposit, AppError> {
    let st = state.inner().clone();
    blocking(move || pay_in(&st, id, deposit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_set_ready(
    state: State<'_, SharedState>,
    id: String,
    note: Option<String>,
) -> Result<Order, AppError> {
    let st = state.inner().clone();
    blocking(move || set_ready(&st, id, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_fulfil(
    state: State<'_, SharedState>,
    input: FulfilInput,
    print_receipt: bool,
) -> Result<FulfilledDone, AppError> {
    let st = state.inner().clone();
    blocking(move || fulfil(&st, input, print_receipt)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_deliver(
    state: State<'_, SharedState>,
    input: DeliverInput,
) -> Result<ValidatedDocument, AppError> {
    let st = state.inner().clone();
    blocking(move || deliver(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_close(
    state: State<'_, SharedState>,
    id: String,
    note: Option<String>,
) -> Result<Order, AppError> {
    let st = state.inner().clone();
    blocking(move || close(&st, id, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn orders_keep_deposit(
    state: State<'_, SharedState>,
    id: String,
    note: Option<String>,
) -> Result<FulfilledDone, AppError> {
    let st = state.inner().clone();
    blocking(move || keep_deposit(&st, id, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn appointments_day(
    state: State<'_, SharedState>,
    date: String,
) -> Result<Vec<Appointment>, AppError> {
    let st = state.inner().clone();
    blocking(move || appointment_day(&st, date)).await
}

/// The appointments starting in `[from_ms, to_ms)`, of one staff member or everyone.
#[tauri::command]
#[specta::specta]
pub async fn appointments_range(
    state: State<'_, SharedState>,
    from_ms: i64,
    to_ms: i64,
    staff_user_id: Option<String>,
) -> Result<Vec<Appointment>, AppError> {
    let st = state.inner().clone();
    blocking(move || appointment_range(&st, from_ms, to_ms, staff_user_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn appointments_book(
    state: State<'_, SharedState>,
    input: AppointmentInput,
) -> Result<Appointment, AppError> {
    let st = state.inner().clone();
    blocking(move || appointment_book(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn appointments_update(
    state: State<'_, SharedState>,
    id: String,
    input: AppointmentInput,
) -> Result<Appointment, AppError> {
    let st = state.inner().clone();
    blocking(move || appointment_update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn appointments_set_status(
    state: State<'_, SharedState>,
    id: String,
    status: AppointmentStatus,
) -> Result<Appointment, AppError> {
    let st = state.inner().clone();
    blocking(move || appointment_set_status(&st, id, status)).await
}

/// Records the order where the `CC…` series and its deposits live (`orders.create` on a till),
/// then prints the deposit's `AC…` receipt here when it took one.
pub(crate) fn create(st: &AppState, input: CustomerOrderInput) -> Result<Order, AppError> {
    let done = match forward_money::<CreatedOrder>(
        st,
        "orders.create",
        json!({ "input": input }),
        true,
    )? {
        Some(done) => done,
        None => create_here(st, input)?,
    };
    if let Some(deposit) = &done.deposit {
        print_deposit(st, deposit, st.session()?.lang);
    }
    Ok(done.order)
}

/// The host service `orders.create`.
pub(crate) fn create_here(
    st: &AppState,
    input: CustomerOrderInput,
) -> Result<CreatedOrder, AppError> {
    let ctx = st.till(Permission::DocumentsQuote)?;
    let write = ctx.clone();
    let (order, deposit) = st.db.write(move |tx| {
        let order = orders::create(tx, &write, input)?;
        // The deposit taken with the order is its only one.
        let deposit = deposits::list(tx, OwnerType::CustomerOrder, &order.id)?
            .into_iter()
            .next_back();
        Ok((order, deposit))
    })?;
    st.spend_approvals(&ctx)?;
    let deposit = deposit.map(|d| taken_deposit(st, d)).transpose()?;
    Ok(CreatedOrder { order, deposit })
}

pub(crate) fn get(st: &AppState, id: String) -> Result<Order, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |c| orders::get(c, &id))
}

pub(crate) fn list(
    st: &AppState,
    filter: CustomerOrderFilter,
) -> Result<Vec<OrderSummary>, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |c| orders::list(c, &filter))
}

pub(crate) fn fulfil_cart(st: &AppState, id: String) -> Result<Cart, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |c| orders::fulfil_cart(c, &id))
}

/// A further deposit (layaway payments over time), taken where the balance lives
/// (`orders.pay_in` on a till), its `AC…` receipt printed here.
pub(crate) fn pay_in(
    st: &AppState,
    id: String,
    deposit: OrderDeposit,
) -> Result<Deposit, AppError> {
    let done: TakenDeposit = match forward_money::<TakenDeposit>(
        st,
        "orders.pay_in",
        json!({ "id": id, "deposit": deposit }),
        true,
    )? {
        Some(done) => done,
        None => pay_in_here(st, id, deposit)?,
    };
    print_deposit(st, &done, st.session()?.lang);
    Ok(done.deposit)
}

/// The host service `orders.pay_in`.
pub(crate) fn pay_in_here(
    st: &AppState,
    id: String,
    deposit: OrderDeposit,
) -> Result<TakenDeposit, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let write = ctx.clone();
    let paid = st
        .db
        .write(move |tx| orders::pay_in(tx, &write, &id, deposit))?;
    st.spend_approvals(&ctx)?;
    taken_deposit(st, paid)
}

pub(crate) fn set_ready(
    st: &AppState,
    id: String,
    note: Option<String>,
) -> Result<Order, AppError> {
    if let Some(done) = forward(st, "orders.set_ready", json!({ "id": id, "note": note }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| orders::set_ready(tx, &ctx, &id, note))
}

/// Serves the order at the till: the sale is made where the deposit is held (`orders.fulfil` on
/// a till), then its receipt prints here like any sale's.
pub(crate) fn fulfil(
    st: &AppState,
    input: FulfilInput,
    print_receipt: bool,
) -> Result<FulfilledDone, AppError> {
    let fulfilled =
        match forward_money::<Fulfilled>(st, "orders.fulfil", json!({ "input": input }), true)? {
            Some(done) => done,
            None => fulfil_here(st, input)?,
        };
    done(st, fulfilled, print_receipt)
}

/// The host service `orders.fulfil`.
pub(crate) fn fulfil_here(st: &AppState, input: FulfilInput) -> Result<Fulfilled, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let write = ctx.clone();
    let fulfilled = st.db.write(move |tx| orders::fulfil(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    Ok(fulfilled)
}

/// A served order's ticket: its receipt queued, its costs hidden from whoever may not see them.
fn done(
    st: &AppState,
    mut fulfilled: Fulfilled,
    print_receipt: bool,
) -> Result<FulfilledDone, AppError> {
    let print_queued = queue_receipt(st, &fulfilled.sale, print_receipt);
    redact::document(&mut fulfilled.sale.document, &st.session()?);
    Ok(FulfilledDone {
        fulfilled,
        print_queued,
    })
}

/// Serves the order as M3's validated delivery note or invoice (its printing is M3's "Imprimer"
/// on the result screen); the kind's own permission applies.
pub(crate) fn deliver(st: &AppState, input: DeliverInput) -> Result<ValidatedDocument, AppError> {
    if let Some(done) =
        forward_money::<ValidatedDocument>(st, "orders.deliver", json!({ "input": input }), true)?
    {
        return Ok(done);
    }
    let needed = kind_permission(input.target).unwrap_or(Permission::DocumentsDelivery);
    let ctx = st.till(needed)?;
    let session = ctx.session.clone();
    let write = ctx.clone();
    let mut done = st.db.write(move |tx| orders::deliver(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    redact::validated(&mut done, &session);
    Ok(done)
}

pub(crate) fn close(st: &AppState, id: String, note: Option<String>) -> Result<Order, AppError> {
    if let Some(done) = forward(st, "orders.close", json!({ "id": id, "note": note }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::DocumentsQuote)?;
    st.db.write(move |tx| orders::close(tx, &ctx, &id, note))
}

/// The shop keeps the arrhes (`sales.return`, or a manager's approval of it): they are sold on
/// an "Arrhes conservées" ticket, whose receipt prints here.
pub(crate) fn keep_deposit(
    st: &AppState,
    id: String,
    note: Option<String>,
) -> Result<FulfilledDone, AppError> {
    let kept = match forward_money::<Fulfilled>(
        st,
        "orders.keep_deposit",
        json!({ "id": id, "note": note }),
        true,
    )? {
        Some(done) => done,
        None => keep_here(st, id, note)?,
    };
    done(st, kept, true)
}

/// The host service `orders.keep_deposit`.
pub(crate) fn keep_here(
    st: &AppState,
    id: String,
    note: Option<String>,
) -> Result<Fulfilled, AppError> {
    let ctx = st.till(Permission::SalesReturn)?;
    let write = ctx.clone();
    let kept = st
        .db
        .write(move |tx| orders::keep_deposit(tx, &write, &id, note))?;
    st.spend_approvals(&ctx)?;
    Ok(kept)
}

pub(crate) fn appointment_day(st: &AppState, date: String) -> Result<Vec<Appointment>, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |c| appointments::day(c, &date))
}

pub(crate) fn appointment_range(
    st: &AppState,
    from_ms: i64,
    to_ms: i64,
    staff_user_id: Option<String>,
) -> Result<Vec<Appointment>, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db
        .read(move |c| appointments::range(c, from_ms, to_ms, staff_user_id.as_deref()))
}

pub(crate) fn appointment_book(
    st: &AppState,
    input: AppointmentInput,
) -> Result<Appointment, AppError> {
    if let Some(done) = forward(st, "appointments.book", json!({ "input": input }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| appointments::book(tx, &ctx, input))
}

pub(crate) fn appointment_update(
    st: &AppState,
    id: String,
    input: AppointmentInput,
) -> Result<Appointment, AppError> {
    if let Some(done) = forward(
        st,
        "appointments.update",
        json!({ "id": id, "input": input }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| appointments::update(tx, &ctx, &id, input))
}

pub(crate) fn appointment_set_status(
    st: &AppState,
    id: String,
    status: AppointmentStatus,
) -> Result<Appointment, AppError> {
    if let Some(done) = forward(
        st,
        "appointments.set_status",
        json!({ "id": id, "status": status }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| appointments::set_status(tx, &ctx, &id, status))
}
