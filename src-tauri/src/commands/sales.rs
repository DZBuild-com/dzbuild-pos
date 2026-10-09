//! The till (M1a Tasks 15–17): live preview, the sale, held tickets, line voids. The receipt is
//! queued after the commit (Task 8); nothing here waits for paper.

use super::{blocking, redact};
use crate::lan::{registers, route};
use crate::printing::PrintRequest;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::money::Money;
use pos_core::payments::{self, PaymentInput};
use pos_core::sales::held::{self, CartEventInput, HeldCart};
use pos_core::sales::{
    self, Cart, PaymentPreview, RepricedCart, SaleExtras, SaleInput, ValidatedSale, cart,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInput {
    pub cart: Cart,
    pub tenders: Vec<PaymentInput>,
    /// The returned value of an exchange (from `returns_quote`), used as store credit.
    pub store_credit: Option<Money>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaleRequest {
    pub sale: SaleInput,
    /// SAL-052: print the receipt, or not.
    pub print_receipt: bool,
    /// The packs' part of the sale (M8: prescriptions, serials, the jeweller's buyer, payment
    /// types, the van); empty by default, which is every sale while the packs' modules are off.
    #[serde(default)]
    #[specta(optional)]
    pub extras: SaleExtras,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaleDone {
    pub sale: ValidatedSale,
    /// A print or drawer job was queued; failures arrive later as `print-failed`.
    pub print_queued: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn sales_preview(
    state: State<'_, SharedState>,
    input: PreviewInput,
) -> Result<PaymentPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || preview(&st, input)).await
}

/// LOY-002: the points the ticket being paid earns `customer_id` (`total_ttc` paid by
/// `payments`), by the rule of the sale's earn row; `null` while the loyalty module is off or for
/// a customer without a live card.
#[tauri::command]
#[specta::specta]
pub async fn sales_points_preview(
    state: State<'_, SharedState>,
    customer_id: String,
    total_ttc: Money,
    payments: Vec<PaymentInput>,
) -> Result<Option<i64>, AppError> {
    let st = state.inner().clone();
    blocking(move || points_preview(&st, customer_id, total_ttc, payments)).await
}

#[tauri::command]
#[specta::specta]
pub async fn sales_create(
    state: State<'_, SharedState>,
    request: SaleRequest,
) -> Result<SaleDone, AppError> {
    let st = state.inner().clone();
    blocking(move || create(&st, request)).await
}

#[tauri::command]
#[specta::specta]
pub async fn sales_hold(
    state: State<'_, SharedState>,
    label: String,
    cart: Cart,
) -> Result<HeldCart, AppError> {
    let st = state.inner().clone();
    blocking(move || hold(&st, label, cart)).await
}

#[tauri::command]
#[specta::specta]
pub async fn sales_held(state: State<'_, SharedState>) -> Result<Vec<HeldCart>, AppError> {
    let st = state.inner().clone();
    blocking(move || held(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn sales_recall(
    state: State<'_, SharedState>,
    id: String,
) -> Result<RepricedCart, AppError> {
    let st = state.inner().clone();
    blocking(move || recall(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn sales_discard_held(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || discard(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn sales_record_event(
    state: State<'_, SharedState>,
    event: CartEventInput,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || record_event(&st, event)).await
}

/// Live totals, duty and change (SAL-048, SAL-057); pending approvals apply, none is spent.
pub(crate) fn preview(st: &AppState, input: PreviewInput) -> Result<PaymentPreview, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let session = ctx.session.clone();
    let mut out = st.db.read(move |conn| match input.store_credit {
        None => sales::preview_payment(conn, &ctx, &input.cart, &input.tenders),
        Some(credit) => {
            let priced = cart::price_cart(conn, &ctx, &input.cart)?;
            let mut rules = cart::settle_rules(conn)?;
            // The returned value pays the new ticket up to its total; the rest is refunded.
            rules.store_credit = credit.min(priced.totals.total_ttc);
            let settlement =
                payments::preview(&ctx.pack, priced.totals.total_ttc, &input.tenders, rules)?;
            Ok(PaymentPreview {
                cart: priced,
                settlement,
            })
        }
    })?;
    redact::priced(&mut out.cart, &session);
    Ok(out)
}

pub(crate) fn points_preview(
    st: &AppState,
    customer_id: String,
    total_ttc: Money,
    payments: Vec<PaymentInput>,
) -> Result<Option<i64>, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |conn| {
        pos_core::loyalty::preview_points(conn, &customer_id, total_ttc, &payments)
    })
}

/// On a till, a kridi tender or store credit needs the main PC to answer (`kridi_needs_host`), and
/// so does a controlled medicine or a precious-metal piece (their registers are the main
/// PC's, which writes their entries once the till's sale committed); every other sale is the
/// till's own, online or not.
pub(crate) fn create(st: &AppState, request: SaleRequest) -> Result<SaleDone, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    route::require_live_for_kridi(st, route::uses_account(&request.sale.payments))?;
    registers::require_host(st, product_ids(&request.sale.cart))?;
    let write = ctx.clone();
    let mut sale = st
        .db
        .write(move |tx| sales::create_sale_with(tx, &write, request.sale, &request.extras))?;
    st.spend_approvals(&ctx)?;
    registers::send_pending(st);
    let cash_moved = sale.settlement.cash_tendered.is_positive();
    let print_queued = (request.print_receipt || cash_moved)
        && st.printer.enqueue(PrintRequest {
            document_id: sale.document.id.clone(),
            receipt: request.print_receipt,
            cash_moved,
            versement: false,
            slip: None,
            layout: None,
        });
    redact::document(&mut sale.document, &ctx.session);
    Ok(SaleDone { sale, print_queued })
}

/// The products a cart sells (to check the registers it writes).
pub(crate) fn product_ids(cart: &Cart) -> Vec<String> {
    cart.lines
        .iter()
        .filter_map(|line| line.product_id.clone())
        .collect()
}

pub(crate) fn hold(st: &AppState, label: String, cart: Cart) -> Result<HeldCart, AppError> {
    let ctx = st.till(Permission::SalesHold)?;
    st.db.write(move |tx| held::hold(tx, &ctx, &label, cart))
}

pub(crate) fn held(st: &AppState) -> Result<Vec<HeldCart>, AppError> {
    let terminal_id = st.require(Permission::SalesHold)?.terminal_id;
    st.db.read(move |c| held::list(c, &terminal_id))
}

pub(crate) fn recall(st: &AppState, id: String) -> Result<RepricedCart, AppError> {
    let ctx = st.till(Permission::SalesHold)?;
    let session = ctx.session.clone();
    let mut recalled = st.db.write(move |tx| held::recall(tx, &ctx, &id))?;
    redact::priced(&mut recalled.priced, &session);
    Ok(recalled)
}

/// Throwing a held ticket away cancels a whole ticket: `sales.void_line` (or a manager's
/// approval of it, spent here), exactly like cancelling the cart on screen (SAL-041).
pub(crate) fn discard(st: &AppState, id: String) -> Result<(), AppError> {
    let ctx = st.till(Permission::SalesVoidLine)?;
    let write = ctx.clone();
    st.db.write(move |tx| held::discard(tx, &write, &id))?;
    st.spend_approvals(&ctx)
}

/// SAL-040/041, AUD-001: the void (a line, the cart, or part of a line's quantity) is journaled
/// before it leaves the screen, valued by the backend from the cart the till sends.
pub(crate) fn record_event(st: &AppState, event: CartEventInput) -> Result<(), AppError> {
    let ctx = st.till(Permission::SalesVoidLine)?;
    let write = ctx.clone();
    st.db
        .write(move |tx| held::record_cart_event(tx, &write, event))?;
    st.spend_approvals(&ctx)
}
