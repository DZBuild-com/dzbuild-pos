//! Instalment sales (Task 20; TMP-001…009): the quote while the cashier types, the contract
//! signed (its ticket sold with the down payment and the rest on the customer's account), the
//! contract page for the print host, the plans and their dues, a versement on a plan with its
//! `RV…` receipt, the reminders to copy and note, the supporting documents (never a cheque).
//!
//! The plans and their `CT…` series are the main PC's: signing, noting a
//! reminder and adding a document are host services, refused offline (`host_required`). A
//! versement is M2's (`kridi_needs_host` on a till that lost the main PC).

use super::print::{A4Printed, party_blocks, print_page};
use super::services::{business_day, queue_receipt};
use super::{blocking, redact};
use crate::lan::route::{forward, forward_money};
use crate::print_host::{HostPage, PrintHost};
use crate::printing::PrintRequest;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::instalments::{
    self, Guarantee, GuaranteeInput, InstalmentQuote, Plan, PlanFilter, PlanPayment, QuoteInput,
    Reminder, SignInput, Signed,
};
use pos_core::lang::Lang;
use serde::Serialize;
use serde_json::json;
use specta::Type;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SignedDone {
    pub signed: Signed,
    /// The ticket's receipt (or the drawer for the down payment's cash) was queued.
    pub print_queued: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_quote(
    state: State<'_, SharedState>,
    input: QuoteInput,
) -> Result<InstalmentQuote, AppError> {
    let st = state.inner().clone();
    blocking(move || quote(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_sign(
    state: State<'_, SharedState>,
    input: SignInput,
    print_receipt: bool,
) -> Result<SignedDone, AppError> {
    let st = state.inner().clone();
    blocking(move || sign(&st, input, print_receipt)).await
}

/// The contract's page, for the in-app preview (the print host renders the same page).
#[tauri::command]
#[specta::specta]
pub async fn instalments_contract(
    state: State<'_, SharedState>,
    plan_id: String,
    lang: Lang,
) -> Result<HostPage, AppError> {
    let st = state.inner().clone();
    blocking(move || contract(&st, plan_id, lang)).await
}

/// Prints the contract on this PC's A4 printer through the print host.
#[tauri::command]
#[specta::specta]
pub async fn instalments_contract_print(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    plan_id: String,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_contract(&st, &host, plan_id, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_get(
    state: State<'_, SharedState>,
    plan_id: String,
) -> Result<Plan, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, plan_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_list(
    state: State<'_, SharedState>,
    filter: PlanFilter,
) -> Result<Vec<Plan>, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, filter)).await
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_collect(
    state: State<'_, SharedState>,
    plan_id: String,
    payment: PlanPayment,
    print_receipt: bool,
) -> Result<super::credit::CollectDone, AppError> {
    let st = state.inner().clone();
    blocking(move || collect(&st, plan_id, payment, print_receipt)).await
}

/// Dues within `within_days` (the `instalments.reminder_days` setting when absent) and late ones.
#[tauri::command]
#[specta::specta]
pub async fn instalments_reminders(
    state: State<'_, SharedState>,
    within_days: Option<u32>,
) -> Result<Vec<Reminder>, AppError> {
    let st = state.inner().clone();
    blocking(move || reminders(&st, within_days)).await
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_reminder_text(
    state: State<'_, SharedState>,
    plan_id: String,
    seq: u8,
    lang: Lang,
) -> Result<String, AppError> {
    let st = state.inner().clone();
    blocking(move || reminder_text(&st, plan_id, seq, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_note_reminder(
    state: State<'_, SharedState>,
    plan_id: String,
    seq: u8,
    note: Option<String>,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || note_reminder(&st, plan_id, seq, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn instalments_add_guarantee(
    state: State<'_, SharedState>,
    plan_id: String,
    guarantee: GuaranteeInput,
) -> Result<Guarantee, AppError> {
    let st = state.inner().clone();
    blocking(move || add_guarantee(&st, plan_id, guarantee)).await
}

/// What signing would do (the duty on the down payment's cash, the schedule): nothing written.
pub(crate) fn quote(st: &AppState, input: QuoteInput) -> Result<InstalmentQuote, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.read(move |c| instalments::quote(c, &ctx, &input))
}

/// Signs where the plans live (`instalments.sign` on a till), then the ticket's receipt prints
/// here; the contract prints from the screen (`instalments_contract_print`).
pub(crate) fn sign(
    st: &AppState,
    input: SignInput,
    print_receipt: bool,
) -> Result<SignedDone, AppError> {
    // The down payment the main PC booked for this till goes into this till's drawer, once
    // (`forward_money`: keyed, kept and sent again after a lost answer).
    let mut signed =
        match forward_money::<Signed>(st, "instalments.sign", json!({ "input": input }), true)? {
            Some(done) => done,
            None => sign_here(st, input)?,
        };
    let print_queued = queue_receipt(st, &signed.sale, print_receipt);
    redact::document(&mut signed.sale.document, &st.session()?);
    Ok(SignedDone {
        signed,
        print_queued,
    })
}

/// The host service `instalments.sign`.
pub(crate) fn sign_here(st: &AppState, input: SignInput) -> Result<Signed, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let write = ctx.clone();
    let signed = st
        .db
        .write(move |tx| instalments::sign(tx, &write, input))?;
    st.spend_approvals(&ctx)?;
    Ok(signed)
}

/// The contract of a plan in `lang` (TMP-001): the parties as the contract's ticket prints them,
/// the goods, the figures, the schedule, the documents given and the clauses.
pub(crate) fn contract(st: &AppState, plan_id: String, lang: Lang) -> Result<HostPage, AppError> {
    st.require_any(&[Permission::SalesCreate, Permission::CreditView])?;
    let today = business_day(st);
    let (plan, doc) = st.db.read(move |c| {
        let plan = instalments::get(c, &plan_id, &today)?;
        let doc = pos_core::documents::get(c, &plan.document_id)?;
        Ok((plan, doc))
    })?;
    let (seller, buyer) = party_blocks(st, &doc.id, lang)?;
    Ok(HostPage::Contract(Box::new(
        pos_docs::a4::packs::build_contract(&plan, &doc, seller, buyer, lang),
    )))
}

pub(crate) fn print_contract(
    st: &AppState,
    host: &PrintHost,
    plan_id: String,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let page = contract(st, plan_id, lang)?;
    print_page(st, host, page)
}

pub(crate) fn get(st: &AppState, plan_id: String) -> Result<Plan, AppError> {
    st.require(Permission::CreditView)?;
    let today = business_day(st);
    st.db.read(move |c| instalments::get(c, &plan_id, &today))
}

pub(crate) fn list(st: &AppState, filter: PlanFilter) -> Result<Vec<Plan>, AppError> {
    st.require(Permission::CreditView)?;
    let today = business_day(st);
    st.db.read(move |c| instalments::list(c, &filter, &today))
}

/// A versement on a contract (TMP-003): M2's versement settling the contract first, its `RV…`
/// receipt and the drawer for its cash through the print queue. On a till it needs the main PC
/// to answer, where the balance lives (`kridi_needs_host`).
pub(crate) fn collect(
    st: &AppState,
    plan_id: String,
    payment: PlanPayment,
    print_receipt: bool,
) -> Result<super::credit::CollectDone, AppError> {
    let ctx = st.till(Permission::CreditCollect)?;
    crate::lan::route::require_live_for_kridi(st, true)?;
    let write = ctx.clone();
    let collected = st
        .db
        .write(move |tx| instalments::collect(tx, &write, &plan_id, payment))?;
    st.spend_approvals(&ctx)?;
    let cash_moved = collected.entry.method.is_some_and(|m| m.is_cash());
    let print_queued = (print_receipt || cash_moved)
        && st.printer.enqueue(PrintRequest::versement(
            &collected.entry.id,
            print_receipt,
            cash_moved,
        ));
    Ok(super::credit::CollectDone {
        collected,
        print_queued,
    })
}

pub(crate) fn reminders(
    st: &AppState,
    within_days: Option<u32>,
) -> Result<Vec<Reminder>, AppError> {
    st.require(Permission::CreditView)?;
    let today = business_day(st);
    st.db.read(move |c| {
        let days = match within_days {
            Some(days) => days,
            None => pos_core::settings::get(c, "instalments.reminder_days")?,
        };
        instalments::reminders(c, &today, days)
    })
}

/// The reminder of a due to copy into an SMS or WhatsApp (nothing is sent).
pub(crate) fn reminder_text(
    st: &AppState,
    plan_id: String,
    seq: u8,
    lang: Lang,
) -> Result<String, AppError> {
    st.require(Permission::CreditView)?;
    let today = business_day(st);
    st.db
        .read(move |c| instalments::reminder_text(c, &plan_id, seq, lang, &today))
}

pub(crate) fn note_reminder(
    st: &AppState,
    plan_id: String,
    seq: u8,
    note: Option<String>,
) -> Result<(), AppError> {
    if let Some(done) = forward(
        st,
        "instalments.note_reminder",
        json!({ "planId": plan_id, "seq": seq, "note": note }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::CreditView)?;
    st.db
        .write(move |tx| instalments::note_reminder(tx, &ctx, &plan_id, seq, note))
}

pub(crate) fn add_guarantee(
    st: &AppState,
    plan_id: String,
    guarantee: GuaranteeInput,
) -> Result<Guarantee, AppError> {
    if let Some(done) = forward(
        st,
        "instalments.add_guarantee",
        json!({ "planId": plan_id, "guarantee": guarantee }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::CustomersManage)?;
    st.db
        .write(move |tx| instalments::add_guarantee(tx, &ctx, &plan_id, guarantee))
}
