//! Wholesale and delivery rounds (Task 20; VRT-100…102, PRD-033, DOC-030): the customers'
//! agreed prices and the minimum orders (host services: the catalogue is the main PC's), the
//! packs of a unit product, and the rounds — planned with their stops, started, each stop
//! reported, closed with one M2 versement per collection and its `RV…` receipt, the van loaded
//! and unloaded by M2 transfers, the route sheet for the print host.
//!
//! Rounds and their `TR…` series are the main PC's (their tables never reach a
//! till): on a till every rounds command, its reads included, is a host service run on the main
//! PC as the till's user (offline: `host_required`); closing a round there books the driver's
//! collections on this till's drawer, which takes their cash (`route::keep_cash`), and the `RV…`
//! receipts print here (Task 20 review).

use super::print::{A4Printed, print_page, seller_block};
use super::{blocking, redact};
use crate::lan::route::{forward, forward_money, forward_spent};
use crate::print_host::{HostPage, PrintHost};
use crate::printing::PrintRequest;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::catalog::products::Product;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::stock::StockLocation;
use pos_core::stock::transfers::{TransferDone, TransferLine};
use pos_core::wholesale::rounds::{
    self, ClosePreview, ClosedRound, Round, RoundFilter, RoundInput, StopReport,
};
use pos_core::wholesale::{self, CustomerPrice, PackInput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClosedRoundDone {
    pub round: ClosedRound,
    /// Every versement receipt went to the print queue.
    pub print_queued: bool,
}

/// A van's name in the three languages (its stock location).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VanNames {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
}

#[tauri::command]
#[specta::specta]
pub async fn wholesale_prices(
    state: State<'_, SharedState>,
    customer_id: String,
) -> Result<Vec<CustomerPrice>, AppError> {
    let st = state.inner().clone();
    blocking(move || prices(&st, customer_id)).await
}

/// Sets (or, with `None`, removes) a customer's agreed price of a product.
#[tauri::command]
#[specta::specta]
pub async fn wholesale_price_set(
    state: State<'_, SharedState>,
    customer_id: String,
    product_id: String,
    price: Option<Money>,
) -> Result<Option<CustomerPrice>, AppError> {
    let st = state.inner().clone();
    blocking(move || price_set(&st, customer_id, product_id, price)).await
}

/// The product's minimum order, `None` when it has none (the product form shows it).
#[tauri::command]
#[specta::specta]
pub async fn wholesale_min_order(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Option<Qty>, AppError> {
    let st = state.inner().clone();
    blocking(move || min_order(&st, product_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn wholesale_min_order_set(
    state: State<'_, SharedState>,
    product_id: String,
    min_order_qty: Option<Qty>,
) -> Result<Option<Qty>, AppError> {
    let st = state.inner().clone();
    blocking(move || min_order_set(&st, product_id, min_order_qty)).await
}

#[tauri::command]
#[specta::specta]
pub async fn wholesale_pack_create(
    state: State<'_, SharedState>,
    input: PackInput,
) -> Result<Product, AppError> {
    let st = state.inner().clone();
    blocking(move || pack_create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_list(
    state: State<'_, SharedState>,
    filter: RoundFilter,
) -> Result<Vec<Round>, AppError> {
    let st = state.inner().clone();
    blocking(move || rounds(&st, filter)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_get(state: State<'_, SharedState>, id: String) -> Result<Round, AppError> {
    let st = state.inner().clone();
    blocking(move || round_get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_create(
    state: State<'_, SharedState>,
    input: RoundInput,
) -> Result<Round, AppError> {
    let st = state.inner().clone();
    blocking(move || round_create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_set_stops(
    state: State<'_, SharedState>,
    id: String,
    document_ids: Vec<String>,
) -> Result<Round, AppError> {
    let st = state.inner().clone();
    blocking(move || round_set_stops(&st, id, document_ids)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_start(state: State<'_, SharedState>, id: String) -> Result<Round, AppError> {
    let st = state.inner().clone();
    blocking(move || round_start(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_report_stop(
    state: State<'_, SharedState>,
    id: String,
    report: StopReport,
) -> Result<Round, AppError> {
    let st = state.inner().clone();
    blocking(move || round_report_stop(&st, id, report)).await
}

/// What closing the round will book (collected by method, the cash to hand in with its droit de
/// timbre, the stops not delivered); nothing is written.
#[tauri::command]
#[specta::specta]
pub async fn rounds_close_preview(
    state: State<'_, SharedState>,
    id: String,
) -> Result<ClosePreview, AppError> {
    let st = state.inner().clone();
    blocking(move || round_close_preview(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_close(
    state: State<'_, SharedState>,
    id: String,
) -> Result<ClosedRoundDone, AppError> {
    let st = state.inner().clone();
    blocking(move || round_close(&st, id)).await
}

/// The route sheet's page, for the in-app preview (the print host renders the same page).
#[tauri::command]
#[specta::specta]
pub async fn rounds_route_sheet(
    state: State<'_, SharedState>,
    id: String,
    lang: Lang,
) -> Result<HostPage, AppError> {
    let st = state.inner().clone();
    blocking(move || route_sheet(&st, id, lang)).await
}

/// Prints the route sheet on this PC's A4 printer through the print host.
#[tauri::command]
#[specta::specta]
pub async fn rounds_route_sheet_print(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    id: String,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_route_sheet(&st, &host, id, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_create_van(
    state: State<'_, SharedState>,
    code: String,
    names: VanNames,
) -> Result<StockLocation, AppError> {
    let st = state.inner().clone();
    blocking(move || van_create(&st, code, names)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rounds_load(
    state: State<'_, SharedState>,
    id: String,
    lines: Vec<TransferLine>,
) -> Result<TransferDone, AppError> {
    let st = state.inner().clone();
    blocking(move || round_load(&st, id, lines)).await
}

/// Brings what is left on the round's van back; `None` when the van is empty.
#[tauri::command]
#[specta::specta]
pub async fn rounds_unload(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Option<TransferDone>, AppError> {
    let st = state.inner().clone();
    blocking(move || round_unload(&st, id)).await
}

pub(crate) fn prices(st: &AppState, customer_id: String) -> Result<Vec<CustomerPrice>, AppError> {
    let lang = st.require(Permission::CustomersView)?.lang;
    st.db
        .read(move |c| wholesale::prices_of(c, &customer_id, lang))
}

pub(crate) fn price_set(
    st: &AppState,
    customer_id: String,
    product_id: String,
    price: Option<Money>,
) -> Result<Option<CustomerPrice>, AppError> {
    if let Some(done) = forward(
        st,
        "wholesale.price_set",
        json!({ "customerId": customer_id, "productId": product_id, "price": price }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsPrice)?.actor();
    st.db
        .write(move |tx| wholesale::set_price(tx, &actor, &customer_id, &product_id, price))
}

/// A product's minimum order, read here: the catalogue reaches every till.
pub(crate) fn min_order(st: &AppState, product_id: String) -> Result<Option<Qty>, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(move |c| wholesale::min_order(c, &product_id))
}

pub(crate) fn min_order_set(
    st: &AppState,
    product_id: String,
    min_order_qty: Option<Qty>,
) -> Result<Option<Qty>, AppError> {
    if let Some(done) = forward(
        st,
        "wholesale.min_order_set",
        json!({ "productId": product_id, "minOrderQty": min_order_qty }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsPrice)?.actor();
    st.db
        .write(move |tx| wholesale::set_min_order(tx, &actor, &product_id, min_order_qty))
}

/// A pack (carton, bag, pallet…) of a unit product (PRD-033): a product of the catalogue, so made
/// on the main PC (`wholesale.pack_create` on a till).
pub(crate) fn pack_create(st: &AppState, input: PackInput) -> Result<Product, AppError> {
    if let Some(done) = forward(st, "wholesale.pack_create", json!({ "input": input }))? {
        return Ok(done);
    }
    let session = st.require(Permission::ProductsEdit)?;
    let actor = session.actor();
    let mut product = st
        .db
        .write(move |tx| wholesale::create_pack(tx, &actor, input))?;
    redact::product(&mut product, &session);
    Ok(product)
}

/// The rounds of `filter`, read where they live (`rounds.list` on a till).
pub(crate) fn rounds(st: &AppState, filter: RoundFilter) -> Result<Vec<Round>, AppError> {
    if let Some(done) = forward(st, "rounds.list", json!({ "filter": filter }))? {
        return Ok(done);
    }
    st.require(Permission::DocumentsDelivery)?;
    st.db.read(move |c| rounds::list(c, &filter))
}

pub(crate) fn round_get(st: &AppState, id: String) -> Result<Round, AppError> {
    if let Some(done) = forward(st, "rounds.get", json!({ "id": id }))? {
        return Ok(done);
    }
    st.require(Permission::DocumentsDelivery)?;
    st.db.read(move |c| rounds::get(c, &id))
}

pub(crate) fn round_create(st: &AppState, input: RoundInput) -> Result<Round, AppError> {
    if let Some(done) = forward(st, "rounds.create", json!({ "input": input }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::DocumentsDelivery)?;
    st.db.write(move |tx| rounds::create(tx, &ctx, input))
}

pub(crate) fn round_set_stops(
    st: &AppState,
    id: String,
    document_ids: Vec<String>,
) -> Result<Round, AppError> {
    if let Some(done) = forward(
        st,
        "rounds.set_stops",
        json!({ "id": id, "documentIds": document_ids }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::DocumentsDelivery)?;
    st.db
        .write(move |tx| rounds::set_stops(tx, &ctx, &id, document_ids))
}

pub(crate) fn round_start(st: &AppState, id: String) -> Result<Round, AppError> {
    if let Some(done) = forward(st, "rounds.start", json!({ "id": id }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::DocumentsDelivery)?;
    st.db.write(move |tx| rounds::start(tx, &ctx, &id))
}

pub(crate) fn round_report_stop(
    st: &AppState,
    id: String,
    report: StopReport,
) -> Result<Round, AppError> {
    if let Some(done) = forward(
        st,
        "rounds.report_stop",
        json!({ "id": id, "report": report }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::DocumentsDelivery)?;
    st.db
        .write(move |tx| rounds::report_stop(tx, &ctx, &id, report))
}

/// The close previewed where the round lives (`rounds.close_preview` on a till): the same quotes
/// and checks as [`round_close_here`], with the closing user's permission, nothing written.
pub(crate) fn round_close_preview(st: &AppState, id: String) -> Result<ClosePreview, AppError> {
    if let Some(done) = forward(st, "rounds.close_preview", json!({ "id": id }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::CreditCollect)?;
    st.db.read(move |c| rounds::close_preview(c, &ctx, &id))
}

/// Closes the round where it lives (`rounds.close` on a till): one M2 versement per collection
/// in this till's session, whose drawer takes the cash; then every `RV…` receipt (and the drawer
/// for the cash) through this PC's print queue.
pub(crate) fn round_close(st: &AppState, id: String) -> Result<ClosedRoundDone, AppError> {
    let round = match forward_money::<ClosedRound>(st, "rounds.close", json!({ "id": id }), true)? {
        Some(done) => done,
        None => round_close_here(st, id)?,
    };
    let mut print_queued = true;
    for v in &round.versements {
        let cash = v.entry.method.is_some_and(|m| m.is_cash());
        print_queued &= st
            .printer
            .enqueue(PrintRequest::versement(&v.entry.id, true, cash));
    }
    Ok(ClosedRoundDone {
        round,
        print_queued,
    })
}

/// The host service `rounds.close`: the write, nothing printed.
pub(crate) fn round_close_here(st: &AppState, id: String) -> Result<ClosedRound, AppError> {
    let ctx = st.till(Permission::CreditCollect)?;
    let write = ctx.clone();
    let round = st.db.write(move |tx| rounds::close(tx, &write, &id))?;
    st.spend_approvals(&ctx)?;
    Ok(round)
}

/// The route sheet of a round in `lang` (VRT-100): the shop, the driver and every stop. On a
/// till the round is read on the main PC ([`round_get`]) and the sheet laid out here.
pub(crate) fn route_sheet(st: &AppState, id: String, lang: Lang) -> Result<HostPage, AppError> {
    let round = round_get(st, id)?;
    let seller = seller_block(st, lang)?;
    Ok(HostPage::RouteSheet(Box::new(
        pos_docs::a4::packs::build_route_sheet(&round, seller, lang),
    )))
}

pub(crate) fn print_route_sheet(
    st: &AppState,
    host: &PrintHost,
    id: String,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let page = route_sheet(st, id, lang)?;
    print_page(st, host, page)
}

/// A delivery van: a stock location of its own the rounds load and the till sells from, made
/// on the main PC (`rounds.create_van` on a till).
pub(crate) fn van_create(
    st: &AppState,
    code: String,
    names: VanNames,
) -> Result<StockLocation, AppError> {
    if let Some(done) = forward(
        st,
        "rounds.create_van",
        json!({ "code": code, "names": names }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::StockTransfer)?.actor();
    st.db.write(move |tx| {
        rounds::create_van(
            tx,
            &actor,
            &code,
            (&names.name_ar, &names.name_fr, &names.name_en),
        )
    })
}

pub(crate) fn round_load(
    st: &AppState,
    id: String,
    lines: Vec<TransferLine>,
) -> Result<TransferDone, AppError> {
    if let Some(done) = forward_spent(st, "rounds.load", json!({ "id": id, "lines": lines }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::StockTransfer)?;
    let write = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| rounds::load(tx, &write, &id, lines))?;
    st.spend_approvals(&ctx)?;
    redact::valued_at_cost(&mut done.document, &ctx.session);
    Ok(done)
}

pub(crate) fn round_unload(st: &AppState, id: String) -> Result<Option<TransferDone>, AppError> {
    if let Some(done) = forward_spent(st, "rounds.unload", json!({ "id": id }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::StockTransfer)?;
    let write = ctx.clone();
    let mut done = st.db.write(move |tx| rounds::unload(tx, &write, &id))?;
    st.spend_approvals(&ctx)?;
    if let Some(done) = &mut done {
        redact::valued_at_cost(&mut done.document, &ctx.session);
    }
    Ok(done)
}
