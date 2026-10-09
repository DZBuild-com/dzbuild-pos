//! Online orders at the till (DZB-014, OMN): the list, claim (DZBuild first, else queued),
//! prepare, sell as a ticket or ship with a BL, refuse; the blacklist lookup. The sale path is
//! M1a's: prices, approvals, duty, drawer and receipt are the till's. The orders live on the main
//! PC with the link: on a till the list is empty and every step answers `cloud_not_linked`.

use super::sales::SaleDone;
use super::{blocking, redact};
use crate::printing::PrintRequest;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::cloud::http::{Call, CallError, Endpoint, Lane};
use pos_core::cloud::orders::{
    self, OnlineOrder, OnlineOrderSummary, OrderQuery, OrderState, RefuseReason,
};
use pos_core::cloud::pull::{self, CustomerFlag};
use pos_core::cloud::store::{self, CloudLink, LinkStatus};
use pos_core::documents::Document;
use pos_core::error::AppError;
use pos_core::payments::PaymentInput;
use pos_core::sales::{PaymentPreview, SaleExtras};
use serde::Deserialize;
use serde_json::json;
use specta::Type;
use std::time::Duration;
use tauri::State;

/// How long the till waits for DZBuild to accept a claim before queueing it.
const CLAIM_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderSaleRequest {
    pub id: String,
    /// The delivery fee's line label, in the till's language ("Livraison", "التوصيل").
    pub shipping_label: String,
    pub payments: Vec<PaymentInput>,
    pub print_receipt: bool,
    /// The packs' part of the sale (M8: prescriptions, serials, buyer, payment types), empty
    /// by default.
    #[serde(default)]
    #[specta(optional)]
    pub extras: SaleExtras,
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_list(
    state: State<'_, SharedState>,
    query: OrderQuery,
) -> Result<Vec<OnlineOrderSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<OnlineOrder, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_seen(
    state: State<'_, SharedState>,
    ids: Vec<String>,
) -> Result<u32, AppError> {
    let st = state.inner().clone();
    blocking(move || seen(&st, ids)).await
}

/// Asks DZBuild first (3 s): another device's claim is refused, an unreachable DZBuild queues it.
#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_claim(
    state: State<'_, SharedState>,
    id: String,
) -> Result<OnlineOrder, AppError> {
    let st = state.inner().clone();
    blocking(move || claim(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_release(
    state: State<'_, SharedState>,
    id: String,
) -> Result<OnlineOrder, AppError> {
    let st = state.inner().clone();
    blocking(move || release(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_prepare(
    state: State<'_, SharedState>,
    id: String,
) -> Result<OnlineOrder, AppError> {
    let st = state.inner().clone();
    blocking(move || prepare(&st, id)).await
}

/// The order as the till prices it (online prices, discount, delivery fee) with `tenders`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_quote(
    state: State<'_, SharedState>,
    id: String,
    shipping_label: String,
    tenders: Vec<PaymentInput>,
) -> Result<PaymentPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || quote(&st, id, shipping_label, tenders)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_sell(
    state: State<'_, SharedState>,
    request: OrderSaleRequest,
) -> Result<SaleDone, AppError> {
    let st = state.inner().clone();
    blocking(move || sell(&st, request)).await
}

/// An order paid online, handed over at the shop or to a courier (`courier`): a ticket settled by
/// the online payment, never cash.
#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_hand_over(
    state: State<'_, SharedState>,
    id: String,
    shipping_label: String,
    courier: bool,
    print_receipt: bool,
) -> Result<SaleDone, AppError> {
    let st = state.inner().clone();
    blocking(move || hand_over(&st, id, shipping_label, courier, print_receipt)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_ship(
    state: State<'_, SharedState>,
    id: String,
    shipping_label: String,
    customer_id: Option<String>,
) -> Result<Document, AppError> {
    let st = state.inner().clone();
    blocking(move || ship(&st, id, shipping_label, customer_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_orders_refuse(
    state: State<'_, SharedState>,
    id: String,
    reason: RefuseReason,
    note: Option<String>,
) -> Result<OnlineOrder, AppError> {
    let st = state.inner().clone();
    blocking(move || refuse(&st, id, reason, note)).await
}

/// A flag DZBuild shares for this phone (a warning, never a refusal).
#[tauri::command]
#[specta::specta]
pub async fn cloud_customer_flag(
    state: State<'_, SharedState>,
    phone: String,
) -> Result<Option<CustomerFlag>, AppError> {
    let st = state.inner().clone();
    blocking(move || customer_flag(&st, phone)).await
}

fn linked(st: &AppState) -> Result<CloudLink, AppError> {
    match st.db.read(store::active_link)? {
        Some(link) if link.status == LinkStatus::Linked => Ok(link),
        Some(link) if link.status == LinkStatus::RelinkRequired => {
            Err(AppError::sync("cloud_relink_required"))
        }
        _ => Err(AppError::sync("cloud_not_linked")),
    }
}

/// A step the engine sends at once rather than at its next cycle (nothing without a runtime:
/// the step waits in the outbox).
fn wake(st: &AppState) {
    if let Ok(cloud) = st.cloud() {
        cloud.wake();
    }
}

pub(crate) fn list(st: &AppState, query: OrderQuery) -> Result<Vec<OnlineOrderSummary>, AppError> {
    let terminal = st.require(Permission::SalesCreate)?.terminal_id;
    st.db.read(move |c| orders::list(c, &query, &terminal))
}

pub(crate) fn get(st: &AppState, id: String) -> Result<OnlineOrder, AppError> {
    let terminal = st.require(Permission::SalesCreate)?.terminal_id;
    st.db.read(move |c| orders::get(c, &id, &terminal))
}

pub(crate) fn seen(st: &AppState, ids: Vec<String>) -> Result<u32, AppError> {
    st.require(Permission::SalesCreate)?;
    let now = st.now();
    let n = st.db.write(move |tx| orders::mark_seen(tx, &ids, now))?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// DZBuild first (3 s): accepted → confirmed; 409 → another device has it (nothing changes);
/// offline, not shipped, anything else → the claim is queued with the same key. An order this
/// till cannot take is refused before DZBuild hears of it.
pub(crate) fn claim(st: &AppState, id: String) -> Result<OnlineOrder, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let link = linked(st)?;
    let order = {
        let (id, terminal) = (id.clone(), ctx.session.terminal_id.clone());
        st.db.read(move |c| orders::get(c, &id, &terminal))?
    };
    if order.claimed_elsewhere {
        return Err(AppError::sync("cloud_order_claimed_elsewhere"));
    }
    if order.local_state != OrderState::New {
        return Err(AppError::sync("cloud_order_state_invalid"));
    }
    let confirmed = match st.cloud().ok().and_then(|c| c.remote_now()) {
        None => false,
        Some(remote) => {
            let body =
                json!({ "device_id": link.device_id, "terminal": ctx.session.terminal_code });
            let call = Call::post(
                Endpoint::OrderClaim,
                &format!("/orders/{}/claim", order.remote_id),
            )
            .json_body(body.to_string().into_bytes())
            .key(&pos_core::cloud::outbox_key(
                "order",
                &format!("{id}:claim"),
            ))
            .lane(Lane::Interactive)
            .timeout(CLAIM_TIMEOUT);
            match remote.send(call) {
                Ok(_) => true,
                Err(CallError::Rejected { status: 409, .. }) => {
                    return Err(AppError::sync("cloud_order_claimed_elsewhere"));
                }
                Err(error) => {
                    tracing::info!(target: "cloud", code = error.code(), "the claim waits in the outbox");
                    false
                }
            }
        }
    };
    let write = ctx.clone();
    let claimed = st
        .db
        .write(move |tx| orders::claim(tx, &write, &link, &id, confirmed))?;
    if !confirmed {
        wake(st);
    }
    Ok(claimed)
}

pub(crate) fn release(st: &AppState, id: String) -> Result<OnlineOrder, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| orders::release(tx, &ctx, &id))
}

pub(crate) fn prepare(st: &AppState, id: String) -> Result<OnlineOrder, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let link = linked(st)?;
    let out = st
        .db
        .write(move |tx| orders::prepare(tx, &ctx, &link, &id))?;
    wake(st);
    Ok(out)
}

pub(crate) fn quote(
    st: &AppState,
    id: String,
    shipping_label: String,
    tenders: Vec<PaymentInput>,
) -> Result<PaymentPreview, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let session = ctx.session.clone();
    let mut out = st
        .db
        .read(move |c| orders::quote(c, &ctx, &id, &shipping_label, &tenders))?;
    redact::priced(&mut out.cart, &session);
    Ok(out)
}

/// A ticket at the till (M1a), then DZBuild is told `delivered`; the receipt and the drawer as
/// for any sale.
pub(crate) fn sell(st: &AppState, request: OrderSaleRequest) -> Result<SaleDone, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let link = linked(st)?;
    let write = ctx.clone();
    let OrderSaleRequest {
        id,
        shipping_label,
        payments,
        print_receipt,
        extras,
    } = request;
    let mut sale = st.db.write(move |tx| {
        orders::sell_with(tx, &write, &link, &id, shipping_label, payments, &extras)
    })?;
    st.spend_approvals(&ctx)?;
    let cash_moved = sale.settlement.cash_tendered.is_positive();
    let print_queued = (print_receipt || cash_moved)
        && st.printer.enqueue(PrintRequest {
            document_id: sale.document.id.clone(),
            receipt: print_receipt,
            cash_moved,
            versement: false,
            slip: None,
            layout: None,
        });
    wake(st);
    redact::document(&mut sale.document, &ctx.session);
    Ok(SaleDone { sale, print_queued })
}

/// An order the buyer paid online: a ticket settled by that payment (no drawer, no duty), then
/// DZBuild is told `delivered` or `shipped`.
pub(crate) fn hand_over(
    st: &AppState,
    id: String,
    shipping_label: String,
    courier: bool,
    print_receipt: bool,
) -> Result<SaleDone, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let link = linked(st)?;
    let write = ctx.clone();
    let mut sale = st
        .db
        .write(move |tx| orders::hand_over(tx, &write, &link, &id, shipping_label, courier))?;
    st.spend_approvals(&ctx)?;
    let print_queued = print_receipt
        && st.printer.enqueue(PrintRequest {
            document_id: sale.document.id.clone(),
            receipt: true,
            cash_moved: false,
            versement: false,
            slip: None,
            layout: None,
        });
    wake(st);
    redact::document(&mut sale.document, &ctx.session);
    Ok(SaleDone { sale, print_queued })
}

/// A validated delivery note (M3) to the consignee, then DZBuild is told `shipped`. An online
/// order is a sale (`sales.create`); the BL also needs `documents.delivery`, held or approved.
pub(crate) fn ship(
    st: &AppState,
    id: String,
    shipping_label: String,
    customer_id: Option<String>,
) -> Result<Document, AppError> {
    st.require(Permission::SalesCreate)?;
    let ctx = st.till(Permission::DocumentsDelivery)?;
    let link = linked(st)?;
    let write = ctx.clone();
    let mut document = st
        .db
        .write(move |tx| orders::ship(tx, &write, &link, &id, shipping_label, customer_id))?;
    st.spend_approvals(&ctx)?;
    wake(st);
    redact::document(&mut document, &ctx.session);
    Ok(document)
}

pub(crate) fn refuse(
    st: &AppState,
    id: String,
    reason: RefuseReason,
    note: Option<String>,
) -> Result<OnlineOrder, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let link = linked(st)?;
    let out = st
        .db
        .write(move |tx| orders::refuse(tx, &ctx, &link, &id, reason, note))?;
    wake(st);
    Ok(out)
}

/// A flag DZBuild shares for this phone: the local salted hashes; with `cloud.customers.sync`
/// on and DZBuild reachable, asked again (3 s) first. Never blocks a sale: a warning only.
pub(crate) fn customer_flag(
    st: &AppState,
    phone: String,
) -> Result<Option<CustomerFlag>, AppError> {
    st.require_any(&[Permission::SalesCreate, Permission::CustomersView])?;
    let Some(link) = st
        .db
        .read(store::active_link)?
        .filter(|l| l.status == LinkStatus::Linked)
    else {
        return Ok(None);
    };
    let settings = st.db.read(store::cloud_settings)?;
    if !settings.customers_flags {
        return Ok(None);
    }
    if settings.customers_sync
        && let Some(remote) = st.cloud().ok().and_then(|c| c.remote_now())
    {
        match pull::refresh_flag(&remote, &st.db, &link, &phone, st.now()) {
            Ok(flag) => return Ok(flag),
            Err(error) => {
                tracing::info!(target: "cloud", code = error.code(), "the flag lookup fell back to the local list");
            }
        }
    }
    let store = link.store_id;
    st.db.read(move |c| pull::lookup_flag(c, &store, &phone))
}
