//! Butcher, charcuterie, fruit and vegetables (master spec §30 FRS): carcass cutting yields
//! (VRT-011) with their templates, and the daily waste log (STK-026). A yield run takes a
//! shop-wide `DC…` number, so on a till it is made on the main PC (`host_required`
//! offline), like the templates; waste is a stock adjustment of the PC that records it. Costs
//! and values go to whoever sees costs (`reports.profit`), zeros otherwise.

use super::{blocking, redact};
use crate::lan::route::forward;
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::butcher::waste::{self, WasteDay, WasteInput};
use pos_core::butcher::yields::{
    self, CutResult, RunInput, RunPreview, YieldRun, YieldTemplate, YieldTemplateInput,
};
use pos_core::error::AppError;
use pos_core::money::Money;
use pos_core::packs::{self, Module};
use pos_core::stock::StockMovement;
use serde_json::json;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn yield_templates(
    state: State<'_, SharedState>,
) -> Result<Vec<YieldTemplate>, AppError> {
    let st = state.inner().clone();
    blocking(move || templates(&st)).await
}

/// Creates a cutting template (`id: null`) or changes one.
#[tauri::command]
#[specta::specta]
pub async fn yield_template_save(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: YieldTemplateInput,
) -> Result<YieldTemplate, AppError> {
    let st = state.inner().clone();
    blocking(move || template_save(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn yield_template_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || template_archive(&st, id)).await
}

/// The run's figures without writing anything (the screen's live preview).
#[tauri::command]
#[specta::specta]
pub async fn yield_preview(
    state: State<'_, SharedState>,
    input: RunInput,
) -> Result<RunPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || preview(&st, input)).await
}

/// Cuts the carcass up: the carcass leaves the stock, the cuts enter it at their share of its
/// cost, under a `DC…` number.
#[tauri::command]
#[specta::specta]
pub async fn yield_run(
    state: State<'_, SharedState>,
    input: RunInput,
) -> Result<YieldRun, AppError> {
    let st = state.inner().clone();
    blocking(move || run(&st, input)).await
}

/// The runs of the business days `from`..=`to`.
#[tauri::command]
#[specta::specta]
pub async fn yield_runs(
    state: State<'_, SharedState>,
    from: String,
    to: String,
) -> Result<Vec<YieldRun>, AppError> {
    let st = state.inner().clone();
    blocking(move || runs(&st, from, to)).await
}

/// Records the day's waste (damage, expiry, loss, own use) as stock adjustments.
#[tauri::command]
#[specta::specta]
pub async fn waste_record(
    state: State<'_, SharedState>,
    input: WasteInput,
) -> Result<Vec<StockMovement>, AppError> {
    let st = state.inner().clone();
    blocking(move || record_waste(&st, input)).await
}

/// The waste of one business day per product and reason, and its total at cost.
#[tauri::command]
#[specta::specta]
pub async fn waste_day(state: State<'_, SharedState>, day: String) -> Result<WasteDay, AppError> {
    let st = state.inner().clone();
    blocking(move || waste_of_day(&st, day)).await
}

fn redact_cuts(cuts: &mut [CutResult]) {
    for cut in cuts {
        cut.cost = Money::ZERO;
        cut.unit_cost = Money::ZERO;
    }
}

fn redact_run(run: &mut YieldRun, session: &Session) {
    if !redact::sees_costs(session) {
        run.input_cost = Money::ZERO;
        redact_cuts(&mut run.cuts);
    }
}

pub(crate) fn templates(st: &AppState) -> Result<Vec<YieldTemplate>, AppError> {
    st.require(Permission::StockView)?;
    st.db.read(|c| {
        packs::require(c, Module::Yield)?;
        yields::templates(c, false)
    })
}

pub(crate) fn template_save(
    st: &AppState,
    id: Option<String>,
    input: YieldTemplateInput,
) -> Result<YieldTemplate, AppError> {
    if let Some(done) = forward(
        st,
        "butcher.template_save",
        json!({ "id": id, "input": input }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::StockAdjust)?.actor();
    st.db
        .write(move |tx| yields::save_template(tx, &actor, id.as_deref(), input))
}

pub(crate) fn template_archive(st: &AppState, id: String) -> Result<(), AppError> {
    if let Some(done) = forward(st, "butcher.template_archive", json!({ "id": id }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::StockAdjust)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Yield)?;
        yields::archive_template(tx, &actor, &id)
    })
}

pub(crate) fn preview(st: &AppState, input: RunInput) -> Result<RunPreview, AppError> {
    let session = st.require(Permission::StockAdjust)?;
    let lang = session.lang;
    let mut preview = st.db.read(move |c| {
        packs::require(c, Module::Yield)?;
        yields::preview(c, &input, lang)
    })?;
    if !redact::sees_costs(&session) {
        preview.input_cost = Money::ZERO;
        redact_cuts(&mut preview.cuts);
    }
    Ok(preview)
}

pub(crate) fn run(st: &AppState, input: RunInput) -> Result<YieldRun, AppError> {
    if let Some(done) = forward(st, "butcher.yield_run", json!({ "input": input }))? {
        return Ok(done);
    }
    let ctx = st.till(Permission::StockAdjust)?;
    let session = ctx.session.clone();
    let lang = session.lang;
    let mut done = st.db.write(move |tx| yields::run(tx, &ctx, input, lang))?;
    redact_run(&mut done, &session);
    Ok(done)
}

pub(crate) fn runs(st: &AppState, from: String, to: String) -> Result<Vec<YieldRun>, AppError> {
    let session = st.require(Permission::StockView)?;
    let lang = session.lang;
    let mut runs = st.db.read(move |c| {
        packs::require(c, Module::Yield)?;
        yields::runs(c, &from, &to, lang)
    })?;
    for run in &mut runs {
        redact_run(run, &session);
    }
    Ok(runs)
}

pub(crate) fn record_waste(
    st: &AppState,
    input: WasteInput,
) -> Result<Vec<StockMovement>, AppError> {
    let ctx = st.till(Permission::StockAdjust)?;
    let session = ctx.session.clone();
    let mut moved = st.db.write(move |tx| waste::record(tx, &ctx, input))?;
    for m in &mut moved {
        redact::movement(m, &session);
    }
    Ok(moved)
}

pub(crate) fn waste_of_day(st: &AppState, day: String) -> Result<WasteDay, AppError> {
    let session = st.require(Permission::StockView)?;
    let lang = session.lang;
    let mut summary = st.db.read(move |c| {
        packs::require(c, Module::Waste)?;
        waste::day_summary(c, &day, lang)
    })?;
    if !redact::sees_costs(&session) {
        summary.total_cost = Money::ZERO;
        for row in &mut summary.rows {
            row.cost = Money::ZERO;
        }
    }
    Ok(summary)
}
