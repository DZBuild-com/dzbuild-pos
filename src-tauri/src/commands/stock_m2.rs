//! Stock back office (M2): lots and expiry (PRD-051/052), serial numbers (PRD-050), counts
//! (STK-009…012), transfers (STK-005), batch adjustments (STK-006), per-location minimums
//! (STK-013) and the level and movement views (STK-002/015). Costs are hidden without
//! `reports.profit`; a blind count shows no expected quantity without `stock.adjust`.

use super::{blocking, redact};
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::error::AppError;
use pos_core::stock::counts::{
    self, CountInput, CountLineView, CountLotLineView, CountMode, CountResult, CountStatus,
    StockCount, Uncounted,
};
use pos_core::stock::lots::{self, ExpiryRow, LotStock};
use pos_core::stock::minimums::{self, LocationMinimum};
use pos_core::stock::serials::{self, Serial, SerialLink, SerialQuery};
use pos_core::stock::transfers::{self, TransferDone, TransferInput, TransferSummary};
use pos_core::stock::views::{self, LevelQuery, LevelRow, MovementQuery, MovementRow};
use pos_core::stock::{self, AdjustBatchInput, StockMovement};
use pos_core::time::business_date;
use tauri::State;

/// The serial register is read in the back office and at the till (SAL-025).
const SERIALS_READ: &[Permission] = &[Permission::StockView, Permission::SalesCreate];
/// Serials of stock on hand are recorded by whoever keeps the stock or receives the goods.
const SERIALS_WRITE: &[Permission] = &[Permission::StockAdjust, Permission::PurchasesManage];
const COUNTS_LIST: &[Permission] = &[Permission::StockCount, Permission::StockView];
/// The counts listed, newest first.
const COUNTS_LIMIT: u32 = 500;

#[tauri::command]
#[specta::specta]
pub async fn lots_of_product(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Vec<LotStock>, AppError> {
    let st = state.inner().clone();
    blocking(move || lots_of(&st, product_id)).await
}

/// `within_days`: `None` for the `stock.expiry_alert_days` setting.
#[tauri::command]
#[specta::specta]
pub async fn lots_expiry(
    state: State<'_, SharedState>,
    within_days: Option<i64>,
) -> Result<Vec<ExpiryRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || expiry(&st, within_days)).await
}

/// `location_id`: `None` for the default location.
#[tauri::command]
#[specta::specta]
pub async fn serials_register(
    state: State<'_, SharedState>,
    product_id: String,
    numbers: Vec<String>,
    location_id: Option<String>,
) -> Result<Vec<Serial>, AppError> {
    let st = state.inner().clone();
    blocking(move || register_serials(&st, product_id, numbers, location_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn serials_list(
    state: State<'_, SharedState>,
    query: SerialQuery,
) -> Result<Vec<Serial>, AppError> {
    let st = state.inner().clone();
    blocking(move || list_serials(&st, query)).await
}

/// Every product that has this serial number (an IMEI is unique, a short serial may not be).
#[tauri::command]
#[specta::specta]
pub async fn serials_find(
    state: State<'_, SharedState>,
    number: String,
) -> Result<Vec<Serial>, AppError> {
    let st = state.inner().clone();
    blocking(move || find_serials(&st, number)).await
}

#[tauri::command]
#[specta::specta]
pub async fn serials_link_sale(
    state: State<'_, SharedState>,
    document_id: String,
    links: Vec<SerialLink>,
) -> Result<Vec<Serial>, AppError> {
    let st = state.inner().clone();
    blocking(move || link_sale(&st, document_id, links)).await
}

#[tauri::command]
#[specta::specta]
pub async fn counts_list(
    state: State<'_, SharedState>,
    status: Option<CountStatus>,
) -> Result<Vec<StockCount>, AppError> {
    let st = state.inner().clone();
    blocking(move || list_counts(&st, status)).await
}

#[tauri::command]
#[specta::specta]
pub async fn counts_start(
    state: State<'_, SharedState>,
    input: CountInput,
) -> Result<StockCount, AppError> {
    let st = state.inner().clone();
    blocking(move || start_count(&st, input)).await
}

/// `pending_only`: the lines not counted yet.
#[tauri::command]
#[specta::specta]
pub async fn counts_lines(
    state: State<'_, SharedState>,
    id: String,
    pending_only: bool,
) -> Result<Vec<CountLineView>, AppError> {
    let st = state.inner().clone();
    blocking(move || count_lines(&st, id, pending_only)).await
}

#[tauri::command]
#[specta::specta]
pub async fn counts_set(
    state: State<'_, SharedState>,
    id: String,
    product_id: String,
    qty: pos_core::money::Qty,
    mode: CountMode,
) -> Result<CountLineView, AppError> {
    let st = state.inner().clone();
    blocking(move || set_counted(&st, id, product_id, qty, mode)).await
}

/// STK-004: the lots of a count's lot-tracked products, counted or still to look for.
#[tauri::command]
#[specta::specta]
pub async fn counts_lot_lines(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Vec<CountLotLineView>, AppError> {
    let st = state.inner().clone();
    blocking(move || count_lot_lines(&st, id)).await
}

/// STK-004: counts one lot of a lot-tracked product (a lot number the books do not know is a lot
/// found on the shelf); answers the product's line.
#[tauri::command]
#[specta::specta]
pub async fn counts_set_lot(
    state: State<'_, SharedState>,
    id: String,
    product_id: String,
    lot_number: String,
    qty: pos_core::money::Qty,
    mode: CountMode,
) -> Result<CountLineView, AppError> {
    let st = state.inner().clone();
    blocking(move || set_counted_lot(&st, id, product_id, lot_number, qty, mode)).await
}

/// One scan adds one pack of the code read (a carton barcode counts its pieces).
#[tauri::command]
#[specta::specta]
pub async fn counts_scan(
    state: State<'_, SharedState>,
    id: String,
    code: String,
) -> Result<CountLineView, AppError> {
    let st = state.inner().clone();
    blocking(move || scan_count(&st, id, code)).await
}

#[tauri::command]
#[specta::specta]
pub async fn counts_close(
    state: State<'_, SharedState>,
    id: String,
    uncounted: Uncounted,
) -> Result<CountResult, AppError> {
    let st = state.inner().clone();
    blocking(move || close_count(&st, id, uncounted)).await
}

#[tauri::command]
#[specta::specta]
pub async fn counts_cancel(
    state: State<'_, SharedState>,
    id: String,
) -> Result<StockCount, AppError> {
    let st = state.inner().clone();
    blocking(move || cancel_count(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_transfer(
    state: State<'_, SharedState>,
    input: TransferInput,
) -> Result<TransferDone, AppError> {
    let st = state.inner().clone();
    blocking(move || transfer(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_transfers(
    state: State<'_, SharedState>,
    limit: u32,
) -> Result<Vec<TransferSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || transfers_list(&st, limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_adjust_batch(
    state: State<'_, SharedState>,
    input: AdjustBatchInput,
) -> Result<Vec<StockMovement>, AppError> {
    let st = state.inner().clone();
    blocking(move || adjust_batch(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_minimums_get(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Vec<LocationMinimum>, AppError> {
    let st = state.inner().clone();
    blocking(move || minimums_get(&st, product_id)).await
}

/// Replaces the product's per-location minimums; a zero minimum removes it.
#[tauri::command]
#[specta::specta]
pub async fn stock_minimums_set(
    state: State<'_, SharedState>,
    product_id: String,
    minimums: Vec<LocationMinimum>,
) -> Result<Vec<LocationMinimum>, AppError> {
    let st = state.inner().clone();
    blocking(move || minimums_set(&st, product_id, minimums)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_levels_view(
    state: State<'_, SharedState>,
    query: LevelQuery,
) -> Result<Vec<LevelRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || levels_view(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn stock_movement_log(
    state: State<'_, SharedState>,
    query: MovementQuery,
) -> Result<Vec<MovementRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || movement_log(&st, query)).await
}

pub(crate) fn lots_of(st: &AppState, product_id: String) -> Result<Vec<LotStock>, AppError> {
    st.require(Permission::StockView)?;
    st.db.read(move |c| lots::stock_of(c, &product_id))
}

pub(crate) fn expiry(st: &AppState, within_days: Option<i64>) -> Result<Vec<ExpiryRow>, AppError> {
    st.require(Permission::StockView)?;
    let today = business_date(st.now()).format("%Y-%m-%d").to_string();
    st.db.read(move |c| {
        let days = match within_days {
            Some(days) => days,
            None => pos_core::settings::get(c, "stock.expiry_alert_days")?,
        };
        lots::expiry_list(c, &today, days)
    })
}

pub(crate) fn register_serials(
    st: &AppState,
    product_id: String,
    numbers: Vec<String>,
    location_id: Option<String>,
) -> Result<Vec<Serial>, AppError> {
    let (actor, now) = (st.require_any(SERIALS_WRITE)?.actor(), st.now());
    st.db.write(move |tx| {
        serials::register_on_hand(
            tx,
            &actor,
            &product_id,
            location_id.as_deref(),
            &numbers,
            now,
        )
    })
}

pub(crate) fn list_serials(st: &AppState, query: SerialQuery) -> Result<Vec<Serial>, AppError> {
    st.require_any(SERIALS_READ)?;
    st.db.read(move |c| serials::list(c, &query))
}

pub(crate) fn find_serials(st: &AppState, number: String) -> Result<Vec<Serial>, AppError> {
    st.require_any(SERIALS_READ)?;
    st.db.read(move |c| serials::find(c, &number))
}

/// Serials are the main PC's; a till's IMEIs are linked there, once its ticket is
/// (the till's outbox goes first, `lan::route::forward`).
pub(crate) fn link_sale(
    st: &AppState,
    document_id: String,
    links: Vec<SerialLink>,
) -> Result<Vec<Serial>, AppError> {
    if let Some(done) = crate::lan::route::forward(
        st,
        "serials.link_sale",
        serde_json::json!({ "documentId": document_id, "links": links }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::SalesCreate)?.actor();
    st.db
        .write(move |tx| serials::link_sale(tx, &actor, &document_id, links))
}

/// A blind count keeps its expected quantities from whoever may not adjust the stock anyway.
fn reveals_expected(session: &Session, count: &StockCount) -> bool {
    !count.blind || session.has(Permission::StockAdjust)
}

pub(crate) fn list_counts(
    st: &AppState,
    status: Option<CountStatus>,
) -> Result<Vec<StockCount>, AppError> {
    st.require_any(COUNTS_LIST)?;
    let mut found = st.db.read(|c| counts::list(c, COUNTS_LIMIT))?;
    if let Some(status) = status {
        found.retain(|c| c.status == status);
    }
    Ok(found)
}

pub(crate) fn start_count(st: &AppState, input: CountInput) -> Result<StockCount, AppError> {
    let ctx = st.till(Permission::StockCount)?;
    let write = ctx.clone();
    let count = st.db.write(move |tx| counts::start(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    Ok(count)
}

pub(crate) fn count_lines(
    st: &AppState,
    id: String,
    pending_only: bool,
) -> Result<Vec<CountLineView>, AppError> {
    let session = st.require(Permission::StockCount)?;
    let reveal_cost = redact::sees_costs(&session);
    let mut lines = st.db.read(move |c| {
        let count = counts::get(c, &id)?;
        counts::lines(c, &id, reveals_expected(&session, &count), reveal_cost)
    })?;
    if pending_only {
        lines.retain(|l| l.counted_qty.is_none());
    }
    Ok(lines)
}

/// The line comes back as the counter may see it (no expected quantity on a blind count).
pub(crate) fn set_counted(
    st: &AppState,
    id: String,
    product_id: String,
    qty: pos_core::money::Qty,
    mode: CountMode,
) -> Result<CountLineView, AppError> {
    let ctx = st.till(Permission::StockCount)?;
    let write = ctx.clone();
    let line = st
        .db
        .write(move |tx| counts::set_counted(tx, &write, &id, &product_id, qty, mode))?;
    st.spend_approvals(&ctx)?;
    Ok(line)
}

/// The count's lot lines as the counter may see them (no expected quantity on a blind count).
pub(crate) fn count_lot_lines(
    st: &AppState,
    id: String,
) -> Result<Vec<CountLotLineView>, AppError> {
    let session = st.require(Permission::StockCount)?;
    st.db.read(move |c| {
        let count = counts::get(c, &id)?;
        counts::lot_lines(c, &id, reveals_expected(&session, &count))
    })
}

/// The product's line comes back as the counter may see it (no expected quantity on a blind
/// count).
pub(crate) fn set_counted_lot(
    st: &AppState,
    id: String,
    product_id: String,
    lot_number: String,
    qty: pos_core::money::Qty,
    mode: CountMode,
) -> Result<CountLineView, AppError> {
    let ctx = st.till(Permission::StockCount)?;
    let write = ctx.clone();
    let line = st.db.write(move |tx| {
        counts::set_counted_lot(tx, &write, &id, &product_id, &lot_number, qty, mode)
    })?;
    st.spend_approvals(&ctx)?;
    Ok(line)
}

pub(crate) fn scan_count(
    st: &AppState,
    id: String,
    code: String,
) -> Result<CountLineView, AppError> {
    let ctx = st.till(Permission::StockCount)?;
    let write = ctx.clone();
    let line = st.db.write(move |tx| {
        let scan =
            counts::line_for_code(tx, &code)?.ok_or_else(|| AppError::not_found("product"))?;
        counts::set_counted(
            tx,
            &write,
            &id,
            &scan.product_id,
            scan.pack_qty,
            CountMode::Add,
        )
    })?;
    st.spend_approvals(&ctx)?;
    Ok(line)
}

pub(crate) fn close_count(
    st: &AppState,
    id: String,
    uncounted: Uncounted,
) -> Result<CountResult, AppError> {
    let ctx = st.till(Permission::StockCount)?;
    let write = ctx.clone();
    let mut result = st
        .db
        .write(move |tx| counts::close(tx, &write, &id, uncounted))?;
    st.spend_approvals(&ctx)?;
    redact::count_result(&mut result, &ctx.session);
    Ok(result)
}

pub(crate) fn cancel_count(st: &AppState, id: String) -> Result<StockCount, AppError> {
    let ctx = st.till(Permission::StockCount)?;
    let write = ctx.clone();
    let count = st.db.write(move |tx| counts::cancel(tx, &write, &id))?;
    st.spend_approvals(&ctx)?;
    Ok(count)
}

pub(crate) fn transfer(st: &AppState, input: TransferInput) -> Result<TransferDone, AppError> {
    let ctx = st.till(Permission::StockTransfer)?;
    let write = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| transfers::transfer(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    redact::valued_at_cost(&mut done.document, &ctx.session);
    Ok(done)
}

pub(crate) fn transfers_list(st: &AppState, limit: u32) -> Result<Vec<TransferSummary>, AppError> {
    let session = st.require(Permission::StockView)?;
    let mut rows = st.db.read(move |c| transfers::list(c, limit))?;
    for row in &mut rows {
        redact::transfer_summary(row, &session);
    }
    Ok(rows)
}

pub(crate) fn adjust_batch(
    st: &AppState,
    input: AdjustBatchInput,
) -> Result<Vec<StockMovement>, AppError> {
    let session = st.require(Permission::StockAdjust)?;
    let (actor, now) = (session.actor(), st.now());
    let mut moved = st
        .db
        .write(move |tx| stock::adjust_batch(tx, &actor, input, now))?;
    for movement in &mut moved {
        redact::movement(movement, &session);
    }
    Ok(moved)
}

pub(crate) fn minimums_get(
    st: &AppState,
    product_id: String,
) -> Result<Vec<LocationMinimum>, AppError> {
    st.require(Permission::StockView)?;
    st.db.read(move |c| minimums::get(c, &product_id))
}

pub(crate) fn minimums_set(
    st: &AppState,
    product_id: String,
    minimums: Vec<LocationMinimum>,
) -> Result<Vec<LocationMinimum>, AppError> {
    let actor = st.require(Permission::StockAdjust)?.actor();
    st.db
        .write(move |tx| minimums::set(tx, &actor, &product_id, minimums))
}

pub(crate) fn levels_view(st: &AppState, query: LevelQuery) -> Result<Vec<LevelRow>, AppError> {
    let session = st.require(Permission::StockView)?;
    let mut rows = st.db.read(move |c| views::levels(c, &query))?;
    for row in &mut rows {
        redact::level_row(row, &session);
    }
    Ok(rows)
}

pub(crate) fn movement_log(
    st: &AppState,
    query: MovementQuery,
) -> Result<Vec<MovementRow>, AppError> {
    let session = st.require(Permission::StockView)?;
    let mut rows = st.db.read(move |c| views::movement_log(c, &query))?;
    for row in &mut rows {
        redact::movement_row(row, &session);
    }
    Ok(rows)
}

/// A full count of the default location, started as the signed-in user: its id (the collector
/// import tests of M4).
#[cfg(test)]
pub(crate) fn start_count_for_tests(st: &AppState) -> String {
    let input = CountInput {
        location_id: None,
        scope: counts::CountScope::Full,
        category_id: None,
        product_ids: Vec::new(),
        blind: false,
        note: None,
    };
    start_count(st, input).unwrap().id
}
