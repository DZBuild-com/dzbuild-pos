//! Returns and exchanges (M1a Task 18): returnable lines, an exact quote through a dry run, the
//! return note, the exchange in one transaction. Receipts are queued like a sale's.

use super::{blocking, redact};
use crate::lan::route;
use crate::printing::PrintRequest;
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission};
use pos_core::error::AppError;
use pos_core::money::{Money, Qty};
use pos_core::payments::PaymentMethod;
use pos_core::returns::{self, Exchange, ReturnInput, ReturnableLine, ValidatedReturn};
use pos_core::sales::extensions::ReturnExtras;
use pos_core::sales::{SaleExtras, SaleInput};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QuotedLine {
    pub origin_line_id: Option<String>,
    pub product_id: Option<String>,
    pub qty: Qty,
    /// What this line gives back (TTC, positive).
    pub amount: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnQuote {
    /// The whole returned value: refund, or store credit in an exchange.
    pub value: Money,
    pub lines: Vec<QuotedLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnRequest {
    pub input: ReturnInput,
    pub print_receipt: bool,
    /// The packs' part of the return (M8: serials coming back, the exchange voucher); empty by
    /// default, which is every return while the packs' modules are off.
    #[serde(default)]
    #[specta(optional)]
    pub extras: ReturnExtras,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnDone {
    pub returned: ValidatedReturn,
    pub print_queued: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRequest {
    pub ret: ReturnInput,
    pub sale: SaleInput,
    pub print_receipt: bool,
    /// The packs' part of the return side (M8), empty by default.
    #[serde(default)]
    #[specta(optional)]
    pub return_extras: ReturnExtras,
    /// The packs' part of the new ticket (M8: serials, register entries), empty by default.
    #[serde(default)]
    #[specta(optional)]
    pub sale_extras: SaleExtras,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeDone {
    pub exchange: Exchange,
    pub print_queued: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn returns_returnable(
    state: State<'_, SharedState>,
    original_id: String,
) -> Result<Vec<ReturnableLine>, AppError> {
    let st = state.inner().clone();
    blocking(move || returnable(&st, original_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn returns_quote(
    state: State<'_, SharedState>,
    input: ReturnInput,
    extras: Option<ReturnExtras>,
) -> Result<ReturnQuote, AppError> {
    let st = state.inner().clone();
    blocking(move || quote_with(&st, input, extras.unwrap_or_default())).await
}

#[tauri::command]
#[specta::specta]
pub async fn returns_create(
    state: State<'_, SharedState>,
    request: ReturnRequest,
) -> Result<ReturnDone, AppError> {
    let st = state.inner().clone();
    blocking(move || create(&st, request)).await
}

#[tauri::command]
#[specta::specta]
pub async fn returns_exchange(
    state: State<'_, SharedState>,
    request: ExchangeRequest,
) -> Result<ExchangeDone, AppError> {
    let st = state.inner().clone();
    blocking(move || exchange(&st, request)).await
}

/// Shown before any approval: the cashier sees what can come back, the manager approves after.
pub(crate) fn returnable(
    st: &AppState,
    original_id: String,
) -> Result<Vec<ReturnableLine>, AppError> {
    let session = st.require_any(&[Permission::SalesReturn, Permission::SalesCreate])?;
    let mut lines = st.db.read(move |c| returns::returnable(c, &original_id))?;
    if !redact::sees_costs(&session) {
        for l in &mut lines {
            l.line.cost_price = Money::ZERO;
        }
    }
    Ok(lines)
}

/// [`quote_with`] without pack extras (the tests' quotes).
#[cfg(test)]
pub(crate) fn quote(st: &AppState, input: ReturnInput) -> Result<ReturnQuote, AppError> {
    quote_with(st, input, ReturnExtras::default())
}

/// The exact refund of `input`, computed by the real return inside a transaction that is
/// rolled back (numbers, stock and audit rows included): no second formula to drift. The
/// return's pack extras (the serials coming back, the exchange voucher) make the dry run the
/// return the till will make.
pub(crate) fn quote_with(
    st: &AppState,
    input: ReturnInput,
    extras: ReturnExtras,
) -> Result<ReturnQuote, AppError> {
    // A return the main PC makes is quoted there too (its dry run writes its ledgers).
    if on_main_pc(st, &input, &extras)?
        && let Some(quote) = route::forward::<ReturnQuote>(
            st,
            "returns.quote",
            serde_json::json!({ "input": input, "extras": extras }),
        )?
    {
        return Ok(quote);
    }
    let ctx = st.till(Permission::SalesReturn)?;
    st.db.dry_run(move |tx| {
        let done = returns::create_return_with(tx, &ctx, input, None, &extras)?;
        let value = done
            .refund
            .checked_add(done.store_credit)
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
        let lines = done
            .document
            .lines
            .iter()
            .filter(|l| l.parent_line_id.is_none())
            .map(|l| QuotedLine {
                origin_line_id: l.origin_line_id.clone(),
                product_id: l.product_id.clone(),
                qty: l.qty.checked_neg().unwrap_or(l.qty),
                amount: l.total_ttc.checked_neg().unwrap_or(l.total_ttc),
            })
            .collect();
        Ok(ReturnQuote { value, lines })
    })
}

/// On a till, whether this return is made on the main PC (it brings back a serial,
/// takes points back or becomes an exchange voucher: the main PC's ledgers). A return of a
/// ticket whose own entries still wait for the main PC is refused (`return_host_pending`): the
/// main PC must hold the sale's serials and balances before it takes them back.
fn on_main_pc(st: &AppState, input: &ReturnInput, extras: &ReturnExtras) -> Result<bool, AppError> {
    if !st.lan.is_client() {
        return Ok(false);
    }
    let (input, extras) = (input.clone(), extras.clone());
    let (pending, needs) = st.db.read(move |c| {
        let pending = match &input.original_id {
            Some(original) => pos_core::sales::extensions::pending_host_registers(c)?
                .iter()
                .any(|r| &r.document_id == original),
            None => false,
        };
        Ok((pending, returns::needs_host(c, &input, &extras)?))
    })?;
    if pending {
        return Err(AppError::conflict("return_host_pending"));
    }
    Ok(needs)
}

/// On a till, a refund to the customer's account needs the main PC to answer
/// (`kridi_needs_host`); a return of a serial, of points or as an exchange voucher is made on
/// the main PC (`host_required` offline), its cash refund in this till's drawer and
/// its note printed here.
pub(crate) fn create(st: &AppState, request: ReturnRequest) -> Result<ReturnDone, AppError> {
    let ctx = st.till(Permission::SalesReturn)?;
    route::require_live_for_kridi(st, route::on_account(request.input.refund_method))?;
    let cash_refund = request.input.refund_method == PaymentMethod::Cash;
    let print_receipt = request.print_receipt;
    let mut returned = if on_main_pc(st, &request.input, &request.extras)? {
        route::forward_money::<ValidatedReturn>(
            st,
            "returns.create",
            serde_json::json!({ "request": request }),
            true,
        )?
        .ok_or_else(|| AppError::conflict("host_required"))?
    } else {
        create_here(st, request)?
    };
    let cash_moved = cash_refund && returned.refund.is_positive();
    let print_queued = (print_receipt || cash_moved)
        && st.printer.enqueue(PrintRequest {
            document_id: returned.document.id.clone(),
            receipt: print_receipt,
            cash_moved,
            versement: false,
            slip: None,
            layout: None,
        });
    redact::document(&mut returned.document, &ctx.session);
    Ok(ReturnDone {
        returned,
        print_queued,
    })
}

/// The return's write on this PC (the host service `returns.create` on the main PC, for a
/// till's drawer): nothing printed.
pub(crate) fn create_here(
    st: &AppState,
    request: ReturnRequest,
) -> Result<ValidatedReturn, AppError> {
    let ctx = st.till(Permission::SalesReturn)?;
    let write = ctx.clone();
    let returned = st.db.write(move |tx| {
        returns::create_return_with(tx, &write, request.input, None, &request.extras)
    })?;
    st.spend_approvals(&ctx)?;
    Ok(returned)
}

/// On a till, the refund to the account or the new ticket's kridi or store credit needs the
/// main PC to answer (`kridi_needs_host`).
pub(crate) fn exchange(st: &AppState, request: ExchangeRequest) -> Result<ExchangeDone, AppError> {
    let ctx = st.till(Permission::SalesReturn)?;
    if !ctx.allows(Permission::SalesCreate) {
        auth::require(&ctx.session, Permission::SalesCreate)?;
    }
    route::require_live_for_kridi(
        st,
        route::on_account(request.ret.refund_method) || route::uses_account(&request.sale.payments),
    )?;
    // The new sale's controlled medicines and jewels are registered on the main PC
    crate::lan::registers::require_host(st, super::sales::product_ids(&request.sale.cart))?;
    let cash_refund = request.ret.refund_method == PaymentMethod::Cash;
    let print_receipt = request.print_receipt;
    // An exchange whose return brings back a serial or points is made on the main PC
    let mut done = if on_main_pc(st, &request.ret, &request.return_extras)? {
        route::forward_money::<Exchange>(
            st,
            "returns.exchange",
            serde_json::json!({ "request": request }),
            true,
        )?
        .ok_or_else(|| AppError::conflict("host_required"))?
    } else {
        let done = exchange_here(st, request)?;
        crate::lan::registers::send_pending(st);
        done
    };
    let cash_moved = done.sale.settlement.cash_tendered.is_positive()
        || (cash_refund && done.returned.refund.is_positive());
    let mut queued = false;
    if print_receipt {
        queued |= st.printer.enqueue(PrintRequest {
            document_id: done.returned.document.id.clone(),
            receipt: true,
            cash_moved: false,
            versement: false,
            slip: None,
            layout: None,
        });
    }
    if print_receipt || cash_moved {
        queued |= st.printer.enqueue(PrintRequest {
            document_id: done.sale.document.id.clone(),
            receipt: print_receipt,
            cash_moved,
            versement: false,
            slip: None,
            layout: None,
        });
    }
    redact::document(&mut done.returned.document, &ctx.session);
    redact::document(&mut done.sale.document, &ctx.session);
    Ok(ExchangeDone {
        exchange: done,
        print_queued: queued,
    })
}

/// The exchange's write on this PC (the host service `returns.exchange` on the main PC, for a
/// till's drawer): nothing printed.
pub(crate) fn exchange_here(st: &AppState, request: ExchangeRequest) -> Result<Exchange, AppError> {
    let ctx = st.till(Permission::SalesReturn)?;
    if !ctx.allows(Permission::SalesCreate) {
        auth::require(&ctx.session, Permission::SalesCreate)?;
    }
    let write = ctx.clone();
    let done = st.db.write(move |tx| {
        returns::exchange_with(
            tx,
            &write,
            request.ret,
            request.sale,
            &request.return_extras,
            &request.sale_extras,
        )
    })?;
    st.spend_approvals(&ctx)?;
    Ok(done)
}
