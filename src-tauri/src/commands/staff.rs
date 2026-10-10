//! Employees lite and the zakat statement (EMP-002/003 lite, FIN-007): the profiles, salary
//! advances and their repayments, the shifts read from the cash sessions, and the indicative
//! zakat statement printed through M3's print host. Advances owed and the zakat statements are
//! the main PC's: on a till the writes are its host services, and
//! the statements — never copied to a till — are read from it.

use super::blocking;
use super::print::{A4Printed, print_page};
use crate::lan::route::{forward, forward_money};
use crate::print_host::{HostPage, PrintHost};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::packs::{self, Module};
use pos_core::payments::PaymentMethod;
use pos_core::staff::{self, AdvanceInput, ProfileInput, ShiftRow, StaffEntry, StaffProfile};
use pos_core::zakat::{self, ZakatInput, ZakatPreview, ZakatStatement};
use serde_json::json;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn staff_list(state: State<'_, SharedState>) -> Result<Vec<StaffProfile>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn staff_profile_set(
    state: State<'_, SharedState>,
    user_id: String,
    input: ProfileInput,
) -> Result<StaffProfile, AppError> {
    let st = state.inner().clone();
    blocking(move || profile_set(&st, user_id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn staff_advance(
    state: State<'_, SharedState>,
    input: AdvanceInput,
) -> Result<StaffEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || advance(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn staff_repay(
    state: State<'_, SharedState>,
    user_id: String,
    amount: Money,
    method: PaymentMethod,
    note: Option<String>,
) -> Result<StaffEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || repay(&st, user_id, amount, method, note)).await
}

/// `month` is the salary month, `YYYY-MM`.
#[tauri::command]
#[specta::specta]
pub async fn staff_deduct(
    state: State<'_, SharedState>,
    user_id: String,
    amount: Money,
    month: String,
    note: Option<String>,
) -> Result<StaffEntry, AppError> {
    let st = state.inner().clone();
    blocking(move || deduct(&st, user_id, amount, month, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn staff_entries(
    state: State<'_, SharedState>,
    user_id: String,
) -> Result<Vec<StaffEntry>, AppError> {
    let st = state.inner().clone();
    blocking(move || entries(&st, user_id)).await
}

/// One row per cash session opened between `from` and `to` (business days), of `userId` or of
/// everyone.
#[tauri::command]
#[specta::specta]
pub async fn staff_shifts(
    state: State<'_, SharedState>,
    from: String,
    to: String,
    user_id: Option<String>,
) -> Result<Vec<ShiftRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || shifts(&st, from, to, user_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn zakat_preview(
    state: State<'_, SharedState>,
    input: ZakatInput,
) -> Result<ZakatPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || preview_zakat(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn zakat_save(
    state: State<'_, SharedState>,
    input: ZakatInput,
) -> Result<ZakatStatement, AppError> {
    let st = state.inner().clone();
    blocking(move || save_zakat(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn zakat_list(state: State<'_, SharedState>) -> Result<Vec<ZakatStatement>, AppError> {
    let st = state.inner().clone();
    blocking(move || list_zakat(&st)).await
}

/// The saved statement `id` on this PC's A4 printer, in `lang`.
#[tauri::command]
#[specta::specta]
pub async fn zakat_statement(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    id: String,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_zakat(&st, &host, id, lang)).await
}

pub(crate) fn list(st: &AppState) -> Result<Vec<StaffProfile>, AppError> {
    st.require(Permission::SettingsUsers)?;
    st.db.read(staff::list)
}

pub(crate) fn profile_set(
    st: &AppState,
    user_id: String,
    input: ProfileInput,
) -> Result<StaffProfile, AppError> {
    if let Some(done) = forward(
        st,
        "staff.profile_set",
        json!({ "userId": user_id, "input": input }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsUsers)?.actor();
    st.db
        .write(move |tx| staff::set_profile(tx, &actor, &user_id, input))
}

/// An advance paid out (from the drawer when `fromTill`, never a till's drawer), written by the
/// main PC.
/// An advance; paid from a till's drawer, the main PC books it on this till's open session and
/// this till records the wages expense in its own drawer (`keep_cash`).
pub(crate) fn advance(st: &AppState, input: AdvanceInput) -> Result<StaffEntry, AppError> {
    if let Some(done) =
        forward_money::<StaffEntry>(st, "staff.advance", json!({ "input": input }), false)?
    {
        return Ok(done);
    }
    advance_here(st, input)
}

/// The advance's write on this PC (a till's, on the main PC).
pub(crate) fn advance_here(st: &AppState, input: AdvanceInput) -> Result<StaffEntry, AppError> {
    let ctx = st.till(Permission::CashPaidInOut)?;
    st.db.write(move |tx| staff::advance(tx, &ctx, input))
}

pub(crate) fn repay(
    st: &AppState,
    user_id: String,
    amount: Money,
    method: PaymentMethod,
    note: Option<String>,
) -> Result<StaffEntry, AppError> {
    // In cash it comes into this till's drawer, booked by the main PC on its session.
    if let Some(done) = forward_money::<StaffEntry>(
        st,
        "staff.repay",
        json!({ "userId": user_id, "amount": amount, "method": method, "note": note }),
        false,
    )? {
        return Ok(done);
    }
    repay_here(st, user_id, amount, method, note)
}

/// The repayment's write on this PC (a till's, on the main PC).
pub(crate) fn repay_here(
    st: &AppState,
    user_id: String,
    amount: Money,
    method: PaymentMethod,
    note: Option<String>,
) -> Result<StaffEntry, AppError> {
    let ctx = st.till(Permission::CashPaidInOut)?;
    st.db
        .write(move |tx| staff::repay(tx, &ctx, &user_id, amount, method, note))
}

pub(crate) fn deduct(
    st: &AppState,
    user_id: String,
    amount: Money,
    month: String,
    note: Option<String>,
) -> Result<StaffEntry, AppError> {
    if let Some(done) = forward(
        st,
        "staff.deduct",
        json!({ "userId": user_id, "amount": amount, "month": month, "note": note }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::SettingsUsers)?;
    st.db
        .write(move |tx| staff::deduct(tx, &ctx, &user_id, amount, &month, note))
}

pub(crate) fn entries(st: &AppState, user_id: String) -> Result<Vec<StaffEntry>, AppError> {
    st.require(Permission::SettingsUsers)?;
    st.db.read(move |c| staff::entries(c, &user_id))
}

pub(crate) fn shifts(
    st: &AppState,
    from: String,
    to: String,
    user_id: Option<String>,
) -> Result<Vec<ShiftRow>, AppError> {
    st.require(Permission::ReportsCash)?;
    st.db
        .read(move |c| staff::shifts(c, &from, &to, user_id.as_deref()))
}

/// The figures as they would be saved now: the nisab the shop entered (`zakat_nisab_missing`
/// until it has), the rate from the data pack.
pub(crate) fn preview_zakat(st: &AppState, input: ZakatInput) -> Result<ZakatPreview, AppError> {
    let ctx = st.till(Permission::ReportsProfit)?;
    st.db.read(move |c| zakat::preview(c, &ctx, &input))
}

/// Saved under the next `ZK` number of the main PC.
pub(crate) fn save_zakat(st: &AppState, input: ZakatInput) -> Result<ZakatStatement, AppError> {
    if let Some(done) = forward(st, "zakat.save", json!({ "input": input }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::ReportsProfit)?;
    st.db.write(move |tx| zakat::save(tx, &ctx, input))
}

/// The statements live on the main PC only: a till asks it.
pub(crate) fn list_zakat(st: &AppState) -> Result<Vec<ZakatStatement>, AppError> {
    if let Some(done) = forward(st, "zakat.list", json!({}))? {
        return Ok(done);
    }
    st.require(Permission::ReportsProfit)?;
    st.db.read(|c| {
        packs::require(c, Module::Zakat)?;
        zakat::list(c)
    })
}

/// One saved statement, from the main PC on a till.
pub(crate) fn get_zakat(st: &AppState, id: String) -> Result<ZakatStatement, AppError> {
    if let Some(done) = forward(st, "zakat.get", json!({ "id": id }))? {
        return Ok(done);
    }
    st.require(Permission::ReportsProfit)?;
    st.db.read(move |c| {
        packs::require(c, Module::Zakat)?;
        zakat::get(c, &id)
    })
}

/// The A4 page of the statement `id` in `lang`: the shop's identity, both dates, the seven
/// figures, the base against the nisab the shop entered with its own note on the source (the
/// data pack's source only for a statement saved before the shop entered its nisab), and the
/// "not a fatwa" disclaimer.
pub(crate) fn zakat_page(st: &AppState, id: String, lang: Lang) -> Result<HostPage, AppError> {
    st.require(Permission::ReportsProfit)?;
    let statement = get_zakat(st, id)?;
    let seller = st
        .db
        .read(move |c| pos_core::billing::validate::seller_party(c, lang, None))?;
    let pack = st.datapack.clone();
    let block = pos_docs::a4::document::party_block(&seller, &pack, lang);
    let print = pos_docs::a4::packs::build_zakat(&statement, block, &pack.zakat.source, lang);
    Ok(HostPage::ZakatStatement(Box::new(print)))
}

pub(crate) fn print_zakat(
    st: &AppState,
    host: &PrintHost,
    id: String,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let page = zakat_page(st, id, lang)?;
    print_page(st, host, page)
}

/// A statement whose only asset is `bank` centimes in the bank, for the command tests.
#[cfg(test)]
pub(crate) fn zakat_input(bank: Money) -> ZakatInput {
    ZakatInput {
        day: Some("2026-09-28".into()),
        valuation: zakat::Valuation::Cost,
        bank,
        other_assets: Money::ZERO,
        other_debts: Money::ZERO,
        note: None,
    }
}
