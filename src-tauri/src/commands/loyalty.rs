//! Loyalty points and vouchers (LOY-001/002 lite, PAY-010/011 lite): the card at the till, the
//! points account, rewards turned into loyalty vouchers, gift vouchers sold at the counter and
//! their slips. Points and voucher balances are ledgers of the main PC: on a
//! till every write here is the main PC's host service (`host_required` offline), and the slip
//! prints on the till's own printer once the main PC answered.

use super::{blocking, redact};
use crate::lan::route::{forward, forward_money};
use crate::printing::{PackSlip, PrintRequest};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::customers::{self, Customer};
use pos_core::error::AppError;
use pos_core::gift_vouchers::{self, SaleQuote, SellInput, Voucher, VoucherFilter};
use pos_core::lang::Lang;
use pos_core::loyalty::{self, Account, LoyaltyCard, PointsEntry};
use serde::Serialize;
use serde_json::json;
use specta::Type;
use tauri::State;

/// The card and the account are read at the till (F3) and in the customer's file.
const READ: &[Permission] = &[Permission::CustomersView, Permission::SalesCreate];

/// A voucher written, and whether its slip went to the print queue (a printer failure reaches
/// the window as `print-failed`; `vouchers_reprint` prints it again).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoucherDone {
    pub voucher: Voucher,
    pub print_queued: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn loyalty_enrol(
    state: State<'_, SharedState>,
    customer_id: String,
    code: Option<String>,
) -> Result<LoyaltyCard, AppError> {
    let st = state.inner().clone();
    blocking(move || enrol(&st, customer_id, code)).await
}

#[tauri::command]
#[specta::specta]
pub async fn loyalty_block(
    state: State<'_, SharedState>,
    code: String,
) -> Result<LoyaltyCard, AppError> {
    let st = state.inner().clone();
    blocking(move || block(&st, code)).await
}

#[tauri::command]
#[specta::specta]
pub async fn loyalty_card_lookup(
    state: State<'_, SharedState>,
    code: String,
) -> Result<Option<Customer>, AppError> {
    let st = state.inner().clone();
    blocking(move || card_lookup(&st, code)).await
}

#[tauri::command]
#[specta::specta]
pub async fn loyalty_account(
    state: State<'_, SharedState>,
    customer_id: String,
) -> Result<Account, AppError> {
    let st = state.inner().clone();
    blocking(move || account(&st, customer_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn loyalty_entries(
    state: State<'_, SharedState>,
    customer_id: String,
) -> Result<Vec<PointsEntry>, AppError> {
    let st = state.inner().clone();
    blocking(move || entries(&st, customer_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn loyalty_redeem(
    state: State<'_, SharedState>,
    customer_id: String,
    points: i64,
) -> Result<VoucherDone, AppError> {
    let st = state.inner().clone();
    blocking(move || redeem(&st, customer_id, points)).await
}

#[tauri::command]
#[specta::specta]
pub async fn loyalty_adjust(
    state: State<'_, SharedState>,
    customer_id: String,
    points: i64,
    note: String,
) -> Result<Account, AppError> {
    let st = state.inner().clone();
    blocking(move || adjust(&st, customer_id, points, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vouchers_quote_sale(
    state: State<'_, SharedState>,
    input: SellInput,
) -> Result<SaleQuote, AppError> {
    let st = state.inner().clone();
    blocking(move || voucher_quote(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vouchers_sell(
    state: State<'_, SharedState>,
    input: SellInput,
) -> Result<VoucherDone, AppError> {
    let st = state.inner().clone();
    blocking(move || voucher_sell(&st, input)).await
}

/// A voucher by its printed code or its scanned `298…` barcode.
#[tauri::command]
#[specta::specta]
pub async fn vouchers_get(
    state: State<'_, SharedState>,
    code: String,
) -> Result<Voucher, AppError> {
    let st = state.inner().clone();
    blocking(move || voucher_get(&st, code)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vouchers_list(
    state: State<'_, SharedState>,
    filter: VoucherFilter,
) -> Result<Vec<Voucher>, AppError> {
    let st = state.inner().clone();
    blocking(move || voucher_list(&st, filter)).await
}

#[tauri::command]
#[specta::specta]
pub async fn vouchers_reprint(
    state: State<'_, SharedState>,
    code: String,
) -> Result<VoucherDone, AppError> {
    let st = state.inner().clone();
    blocking(move || voucher_reprint(&st, code)).await
}

fn today(st: &AppState) -> String {
    pos_core::time::business_date(st.now())
        .format("%Y-%m-%d")
        .to_string()
}

/// Queues the slip of `voucher` in `lang`: `false` (logged) when it could not be laid out or the
/// queue is gone; the voucher stands and `vouchers_reprint` prints it.
fn print_voucher(st: &AppState, voucher: &Voucher, lang: Lang, cash_moved: bool) -> bool {
    match crate::printing::voucher_slip_layout(&st.db, voucher, lang) {
        Ok(layout) => st.printer.enqueue(PrintRequest {
            cash_moved,
            ..PrintRequest::layout(PackSlip::Voucher, &voucher.code, layout)
        }),
        Err(error) => {
            tracing::warn!(voucher = %voucher.code, %error, "voucher slip not laid out");
            false
        }
    }
}

/// Enrols the customer with a typed or generated card; the `loyalty` consent is logged.
pub(crate) fn enrol(
    st: &AppState,
    customer_id: String,
    code: Option<String>,
) -> Result<LoyaltyCard, AppError> {
    if let Some(done) = forward(
        st,
        "loyalty.enrol",
        json!({ "customerId": customer_id, "code": code }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| loyalty::enrol(tx, &ctx, &customer_id, code))
}

/// A lost or stolen card: the customer keeps his points.
pub(crate) fn block(st: &AppState, code: String) -> Result<LoyaltyCard, AppError> {
    if let Some(done) = forward(st, "loyalty.block", json!({ "code": code }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::CustomersManage)?;
    st.db.write(move |tx| loyalty::block(tx, &ctx, &code))
}

/// The customer of a scanned live card (`None` for another code), as the customer's file shows
/// him to this user.
pub(crate) fn card_lookup(st: &AppState, code: String) -> Result<Option<Customer>, AppError> {
    let session = st.require_any(READ)?;
    let found = st
        .db
        .read(move |c| match loyalty::customer_of_card(c, &code)? {
            Some(id) => customers::get(c, &id).map(Some),
            None => Ok(None),
        })?;
    Ok(found.map(|mut customer| {
        redact::customer(&mut customer, &session);
        customer
    }))
}

pub(crate) fn account(st: &AppState, customer_id: String) -> Result<Account, AppError> {
    st.require_any(READ)?;
    let today = today(st);
    st.db
        .read(move |c| loyalty::account(c, &customer_id, &today))
}

pub(crate) fn entries(st: &AppState, customer_id: String) -> Result<Vec<PointsEntry>, AppError> {
    st.require(Permission::CustomersView)?;
    st.db.read(move |c| loyalty::entries(c, &customer_id))
}

/// Points turned into a loyalty voucher, written by the main PC and printed here.
pub(crate) fn redeem(
    st: &AppState,
    customer_id: String,
    points: i64,
) -> Result<VoucherDone, AppError> {
    let voucher = match forward(
        st,
        "loyalty.redeem",
        json!({ "customerId": customer_id, "points": points }),
    )? {
        Some(voucher) => voucher,
        None => redeem_here(st, customer_id, points)?,
    };
    let lang = st.session()?.lang;
    let print_queued = print_voucher(st, &voucher, lang, false);
    Ok(VoucherDone {
        voucher,
        print_queued,
    })
}

/// The redemption's write on this PC (a till's, on the main PC, prints nothing there).
pub(crate) fn redeem_here(
    st: &AppState,
    customer_id: String,
    points: i64,
) -> Result<Voucher, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| loyalty::redeem(tx, &ctx, &customer_id, points))
}

/// Points added or removed by hand with their reason (`customers.manage`).
pub(crate) fn adjust(
    st: &AppState,
    customer_id: String,
    points: i64,
    note: String,
) -> Result<Account, AppError> {
    if let Some(done) = forward(
        st,
        "loyalty.adjust",
        json!({ "customerId": customer_id, "points": points, "note": note }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::CustomersManage)?;
    st.db
        .write(move |tx| loyalty::adjust(tx, &ctx, &customer_id, points, &note))
}

/// The duty on the cash, the part charged and what to receive, before the voucher is sold.
pub(crate) fn voucher_quote(st: &AppState, input: SellInput) -> Result<SaleQuote, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .read(move |c| gift_vouchers::quote_sale(c, &ctx, &input))
}

/// A gift voucher sold: numbered (`BA…`) by the main PC, its slip printed here, the drawer
/// opened for its cash. On a till the main PC books the cash on this till's open session and
/// this till records it in its own drawer (`keep_cash`).
pub(crate) fn voucher_sell(st: &AppState, input: SellInput) -> Result<VoucherDone, AppError> {
    let cash = input.method.is_cash();
    let voucher =
        match forward_money::<Voucher>(st, "vouchers.sell", json!({ "input": input }), false)? {
            Some(voucher) => voucher,
            None => sell_here(st, input)?,
        };
    let lang = st.session()?.lang;
    let print_queued = print_voucher(st, &voucher, lang, cash);
    Ok(VoucherDone {
        voucher,
        print_queued,
    })
}

/// The sale's write on this PC (a till's, on the main PC, prints nothing there).
pub(crate) fn sell_here(st: &AppState, input: SellInput) -> Result<Voucher, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| gift_vouchers::sell(tx, &ctx, input))
}

pub(crate) fn voucher_get(st: &AppState, code: String) -> Result<Voucher, AppError> {
    st.require_any(&[Permission::SalesCreate, Permission::ReportsCash])?;
    st.db.read(move |c| gift_vouchers::get(c, &code))
}

/// The vouchers report's rows: the liability of the vouchers still active.
pub(crate) fn voucher_list(st: &AppState, filter: VoucherFilter) -> Result<Vec<Voucher>, AppError> {
    st.require(Permission::ReportsCash)?;
    let today = today(st);
    st.db.read(move |c| gift_vouchers::list(c, &filter, &today))
}

/// The slip of a voucher again, in this user's language.
pub(crate) fn voucher_reprint(st: &AppState, code: String) -> Result<VoucherDone, AppError> {
    let session = st.require(Permission::SalesReprint)?;
    let voucher = st.db.read(move |c| gift_vouchers::get(c, &code))?;
    let print_queued = print_voucher(st, &voucher, session.lang, false);
    Ok(VoucherDone {
        voucher,
        print_queued,
    })
}

/// A gift voucher of `amount` centimes paid by `method`, for the command tests.
#[cfg(test)]
pub(crate) fn voucher_sale(amount: i64, method: pos_core::payments::PaymentMethod) -> SellInput {
    SellInput {
        amount: pos_core::money::Money(amount),
        method,
        reference: (method != pos_core::payments::PaymentMethod::Cash).then(|| "REF-1".into()),
        cheque_bank: None,
        cheque_date: None,
        customer_id: None,
        buyer_name: Some("Karim".into()),
    }
}
