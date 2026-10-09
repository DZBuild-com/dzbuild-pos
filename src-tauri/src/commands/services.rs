//! Deposits, vouchers and service tickets (Task 20; CRD-007, ORD-005, ORD-006): the deposit's
//! figures, taken with its `AC…` receipt, refunded, listed; what a voucher reference would spend;
//! a repair or a service from its intake slip (`RP…`) through its statuses, technician and parts
//! to the pickup that sells it with the deposit applied.
//!
//! What is held is a balance and the `AC…` and `RP…` series are shop-wide, so every write here is
//! a host service: on a till it runs on the main PC as the till's user, and
//! offline it is refused (`host_required`). The slips and receipts print on this PC, and the cash
//! the main PC booked for this till goes into this till's drawer (`route::keep_cash`, Task 20
//! review).

use super::{blocking, redact};
use crate::lan::route::{forward, forward_money};
use crate::printing::{PackSlip, PrintRequest};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::billing::settings::Trilingual;
use pos_core::deposits::{self, Deposit, DepositInput, DepositQuote, OwnerType};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::payments::PaymentMethod;
use pos_core::service_tickets::{
    self, BoardFilter, IntakeInput, PickedUp, PickupInput, ServiceItemInput, ServiceStatus,
    ServiceSummary, ServiceTicket,
};
use pos_core::vouchers::{self, VoucherInfo};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::State;

/// A deposit taken, with what its receipt prints: the owner's number, the customer and everything
/// held so far (read in the same transaction, on the PC that took it).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TakenDeposit {
    pub deposit: Deposit,
    pub owner_number: String,
    pub customer: Option<String>,
    pub held: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickedUpDone {
    pub picked: PickedUp,
    /// The receipt (or the drawer) was queued; failures arrive later as `print-failed`.
    pub print_queued: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn deposits_quote(
    state: State<'_, SharedState>,
    input: DepositInput,
) -> Result<DepositQuote, AppError> {
    let st = state.inner().clone();
    blocking(move || deposit_quote(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn deposits_take(
    state: State<'_, SharedState>,
    input: DepositInput,
) -> Result<Deposit, AppError> {
    let st = state.inner().clone();
    blocking(move || deposit_take(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn deposits_refund(
    state: State<'_, SharedState>,
    owner_type: OwnerType,
    owner_id: String,
    amount: Money,
    method: PaymentMethod,
    reference: Option<String>,
) -> Result<Deposit, AppError> {
    let st = state.inner().clone();
    blocking(move || deposit_refund(&st, owner_type, owner_id, amount, method, reference)).await
}

#[tauri::command]
#[specta::specta]
pub async fn deposits_list(
    state: State<'_, SharedState>,
    owner_type: OwnerType,
    owner_id: String,
) -> Result<Vec<Deposit>, AppError> {
    let st = state.inner().clone();
    blocking(move || deposit_list(&st, owner_type, owner_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vouchers_describe(
    state: State<'_, SharedState>,
    reference: String,
) -> Result<Option<VoucherInfo>, AppError> {
    let st = state.inner().clone();
    blocking(move || voucher_describe(&st, reference)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_intake(
    state: State<'_, SharedState>,
    input: IntakeInput,
) -> Result<ServiceTicket, AppError> {
    let st = state.inner().clone();
    blocking(move || intake(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<ServiceTicket, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_board(
    state: State<'_, SharedState>,
    filter: BoardFilter,
) -> Result<Vec<ServiceSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || board(&st, filter)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_set_status(
    state: State<'_, SharedState>,
    id: String,
    status: ServiceStatus,
    note: Option<String>,
) -> Result<ServiceTicket, AppError> {
    let st = state.inner().clone();
    blocking(move || set_status(&st, id, status, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_assign(
    state: State<'_, SharedState>,
    id: String,
    technician_id: Option<String>,
) -> Result<ServiceTicket, AppError> {
    let st = state.inner().clone();
    blocking(move || assign(&st, id, technician_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_set_items(
    state: State<'_, SharedState>,
    id: String,
    items: Vec<ServiceItemInput>,
) -> Result<ServiceTicket, AppError> {
    let st = state.inner().clone();
    blocking(move || set_items(&st, id, items)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_ready_message(
    state: State<'_, SharedState>,
    id: String,
    lang: Lang,
) -> Result<String, AppError> {
    let st = state.inner().clone();
    blocking(move || ready_message(&st, id, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_pickup(
    state: State<'_, SharedState>,
    input: PickupInput,
    print_receipt: bool,
) -> Result<PickedUpDone, AppError> {
    let st = state.inner().clone();
    blocking(move || pickup(&st, input, print_receipt)).await
}

#[tauri::command]
#[specta::specta]
pub async fn service_uncollected(
    state: State<'_, SharedState>,
) -> Result<Vec<ServiceSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || uncollected(&st)).await
}

/// Today's business day on this PC's clock, `YYYY-MM-DD`.
pub(crate) fn business_day(st: &AppState) -> String {
    pos_core::time::business_date(st.now())
        .format("%Y-%m-%d")
        .to_string()
}

/// The shop's name in `lang` and whether it is the demo shop: the head of every pack slip.
pub(crate) fn slip_shop(st: &AppState, lang: Lang) -> Result<(String, bool), AppError> {
    let name = crate::printing::shop_name(&st.db, lang)?;
    let demo = st.db.read(pos_core::company::get)?.is_some_and(|c| c.demo);
    Ok((name, demo))
}

/// Queues a laid-out slip; one that cannot be laid out is logged and the write stands.
pub(crate) fn queue_slip(st: &AppState, what: &str, built: Result<PrintRequest, AppError>) -> bool {
    match built {
        Ok(request) => st.printer.enqueue(request),
        Err(error) => {
            tracing::warn!(slip = what, %error, "slip not laid out; the write stands");
            false
        }
    }
}

/// The ticket's receipt (and the drawer for its cash) after a pack sale, like `sales_create`.
pub(crate) fn queue_receipt(
    st: &AppState,
    sale: &pos_core::sales::ValidatedSale,
    print_receipt: bool,
) -> bool {
    let cash_moved = sale.settlement.cash_tendered.is_positive();
    (print_receipt || cash_moved)
        && st.printer.enqueue(PrintRequest {
            document_id: sale.document.id.clone(),
            receipt: print_receipt,
            cash_moved,
            versement: false,
            slip: None,
            layout: None,
        })
}

/// The `AC…` receipt of `taken` in `lang`, the drawer opened for its cash.
pub(crate) fn print_deposit(st: &AppState, taken: &TakenDeposit, lang: Lang) -> bool {
    let built = slip_shop(st, lang).map(|(shop, demo)| {
        let slip = pos_docs::slips::deposit_slip(
            &taken.deposit,
            &taken.owner_number,
            taken.customer.as_deref(),
            taken.held,
            lang,
        );
        let at = pos_core::time::algiers_datetime(taken.deposit.created_at)
            .format("%d/%m/%Y %H:%M")
            .to_string();
        let number = taken
            .deposit
            .receipt_number
            .clone()
            .unwrap_or_else(|| taken.deposit.id.clone());
        let layout =
            pos_hw::slips::with_demo(pos_hw::slips::deposit_layout(&slip, &shop, &at), lang, demo);
        PrintRequest {
            cash_moved: taken.deposit.method.is_some_and(PaymentMethod::is_cash),
            ..PrintRequest::layout(PackSlip::Deposit, &number, layout)
        }
    });
    queue_slip(st, "deposit", built)
}

/// The deposit just taken and what its receipt prints: its owner's number, its customer and
/// everything held for that owner (this one included).
pub(crate) fn taken_deposit(st: &AppState, deposit: Deposit) -> Result<TakenDeposit, AppError> {
    st.db.read(move |c| {
        Ok(TakenDeposit {
            owner_number: deposits::owner_number(c, deposit.owner_type, &deposit.owner_id)?,
            customer: deposits::customer_name(c, deposit.customer_id.as_deref())?,
            held: deposits::held(c, deposit.owner_type, &deposit.owner_id)?,
            deposit,
        })
    })
}

pub(crate) fn deposit_quote(st: &AppState, input: DepositInput) -> Result<DepositQuote, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.read(move |c| deposits::quote(c, &ctx, &input))
}

/// The deposit is taken where the balance lives (`deposits.take` on a till), then its `AC…`
/// receipt prints here in the cashier's language.
pub(crate) fn deposit_take(st: &AppState, input: DepositInput) -> Result<Deposit, AppError> {
    let done = match forward_money::<TakenDeposit>(
        st,
        "deposits.take",
        json!({ "input": input }),
        true,
    )? {
        Some(done) => done,
        None => take_here(st, input)?,
    };
    print_deposit(st, &done, st.session()?.lang);
    Ok(done.deposit)
}

/// The host service `deposits.take`: the write and the receipt's figures, nothing printed.
pub(crate) fn take_here(st: &AppState, input: DepositInput) -> Result<TakenDeposit, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let write = ctx.clone();
    let deposit = st.db.write(move |tx| deposits::take(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    taken_deposit(st, deposit)
}

/// Gives back part of what is held (never more); cash out of this till opens its drawer.
pub(crate) fn deposit_refund(
    st: &AppState,
    owner_type: OwnerType,
    owner_id: String,
    amount: Money,
    method: PaymentMethod,
    reference: Option<String>,
) -> Result<Deposit, AppError> {
    let refunded: Deposit = match forward_money::<Deposit>(
        st,
        "deposits.refund",
        json!({
            "ownerType": owner_type,
            "ownerId": owner_id,
            "amount": amount,
            "method": method,
            "reference": reference,
        }),
        true,
    )? {
        Some(done) => done,
        None => refund_here(st, owner_type, owner_id, amount, method, reference)?,
    };
    if method.is_cash() {
        st.printer.enqueue(PrintRequest {
            document_id: refunded.id.clone(),
            receipt: false,
            cash_moved: true,
            versement: false,
            slip: None,
            layout: None,
        });
    }
    Ok(refunded)
}

/// The host service `deposits.refund`.
pub(crate) fn refund_here(
    st: &AppState,
    owner_type: OwnerType,
    owner_id: String,
    amount: Money,
    method: PaymentMethod,
    reference: Option<String>,
) -> Result<Deposit, AppError> {
    let ctx = st.till(Permission::SalesReturn)?;
    let write = ctx.clone();
    let done = st.db.write(move |tx| {
        deposits::refund(tx, &write, owner_type, &owner_id, amount, method, reference)
    })?;
    st.spend_approvals(&ctx)?;
    Ok(done)
}

pub(crate) fn deposit_list(
    st: &AppState,
    owner_type: OwnerType,
    owner_id: String,
) -> Result<Vec<Deposit>, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db
        .read(move |c| deposits::list(c, owner_type, &owner_id))
}

/// What a scanned or typed voucher reference would spend (`None`: the customer's store credit).
pub(crate) fn voucher_describe(
    st: &AppState,
    reference: String,
) -> Result<Option<VoucherInfo>, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |c| vouchers::describe(c, &reference))
}

/// The intake slip in the customer's language, the shop's conditions under it.
fn print_intake(st: &AppState, ticket: &ServiceTicket) -> bool {
    queue_slip(st, "intake", intake_request(st, ticket))
}

fn intake_request(st: &AppState, ticket: &ServiceTicket) -> Result<PrintRequest, AppError> {
    let lang = ticket.lang;
    let conditions: Trilingual = st
        .db
        .read(|c| pos_core::settings::get(c, "repairs.conditions"))?;
    let (shop, demo) = slip_shop(st, lang)?;
    let slip = pos_docs::slips::intake_slip(
        ticket,
        &shop,
        conditions.in_lang(lang),
        lang,
        ticket.created_at,
    );
    let layout = pos_hw::slips::with_demo(pos_hw::slips::intake_layout(&slip), lang, demo);
    Ok(PrintRequest::layout(
        PackSlip::Intake,
        &ticket.number,
        layout,
    ))
}

/// Opens the ticket where the `RP…` series lives (`service.intake` on a till), then prints its
/// intake slip here.
pub(crate) fn intake(st: &AppState, input: IntakeInput) -> Result<ServiceTicket, AppError> {
    let ticket = match forward(st, "service.intake", json!({ "input": input }))? {
        Some(done) => done,
        None => intake_here(st, input)?,
    };
    print_intake(st, &ticket);
    Ok(ticket)
}

/// The host service `service.intake`.
pub(crate) fn intake_here(st: &AppState, input: IntakeInput) -> Result<ServiceTicket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| service_tickets::intake(tx, &ctx, input))
}

pub(crate) fn get(st: &AppState, id: String) -> Result<ServiceTicket, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |c| service_tickets::get(c, &id))
}

pub(crate) fn board(st: &AppState, filter: BoardFilter) -> Result<Vec<ServiceSummary>, AppError> {
    st.require(Permission::SalesCreate)?;
    let today = business_day(st);
    st.db
        .read(move |c| service_tickets::board(c, &filter, &today))
}

pub(crate) fn set_status(
    st: &AppState,
    id: String,
    status: ServiceStatus,
    note: Option<String>,
) -> Result<ServiceTicket, AppError> {
    if let Some(done) = forward(
        st,
        "service.set_status",
        json!({ "id": id, "status": status, "note": note }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| service_tickets::set_status(tx, &ctx, &id, status, note))
}

pub(crate) fn assign(
    st: &AppState,
    id: String,
    technician_id: Option<String>,
) -> Result<ServiceTicket, AppError> {
    if let Some(done) = forward(
        st,
        "service.assign",
        json!({ "id": id, "technicianId": technician_id }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| service_tickets::assign(tx, &ctx, &id, technician_id.as_deref()))
}

pub(crate) fn set_items(
    st: &AppState,
    id: String,
    items: Vec<ServiceItemInput>,
) -> Result<ServiceTicket, AppError> {
    if let Some(done) = forward(st, "service.set_items", json!({ "id": id, "items": items }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| service_tickets::set_items(tx, &ctx, &id, items))
}

/// The "your device is ready" text to copy into an SMS or WhatsApp (nothing is sent).
pub(crate) fn ready_message(st: &AppState, id: String, lang: Lang) -> Result<String, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db
        .read(move |c| service_tickets::ready_message(c, &id, lang))
}

/// Hands the item back: its sale is made where the deposit is held (`service.pickup` on a till),
/// then the ticket's receipt prints here like any sale's.
pub(crate) fn pickup(
    st: &AppState,
    input: PickupInput,
    print_receipt: bool,
) -> Result<PickedUpDone, AppError> {
    let mut picked =
        match forward_money::<PickedUp>(st, "service.pickup", json!({ "input": input }), true)? {
            Some(done) => done,
            None => pickup_here(st, input)?,
        };
    let print_queued = queue_receipt(st, &picked.sale, print_receipt);
    redact::document(&mut picked.sale.document, &st.session()?);
    Ok(PickedUpDone {
        picked,
        print_queued,
    })
}

/// The host service `service.pickup`.
pub(crate) fn pickup_here(st: &AppState, input: PickupInput) -> Result<PickedUp, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let write = ctx.clone();
    let picked = st
        .db
        .write(move |tx| service_tickets::pickup(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    Ok(picked)
}

/// Ready tickets waiting longer than `repairs.uncollected_days` (RPT-069).
pub(crate) fn uncollected(st: &AppState) -> Result<Vec<ServiceSummary>, AppError> {
    st.require(Permission::SalesCreate)?;
    let today = business_day(st);
    st.db.read(move |c| service_tickets::uncollected(c, &today))
}
