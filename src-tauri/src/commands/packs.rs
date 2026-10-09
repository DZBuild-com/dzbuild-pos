//! Activity templates and modules (ONB-008, PLT-016, UX-020): the setup wizard and Settings ›
//! Activity and modules; and the superette pack: regulated staples (PRC-012, SAL-030, VRT-001)
//! and department keys (SAL-019). Template, module and master-data writes are the main PC's
//! (on a till they run there as the till's user, `host_required` offline); a pack read
//! refuses with `module_disabled` while its module is off.

use super::blocking;
use crate::lan::route::forward;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::catalog::RETAIL_LEVEL_ID;
use pos_core::catalog::departments::{self, DepartmentKey, DepartmentKeyInput};
use pos_core::company::ShopActivity;
use pos_core::error::AppError;
use pos_core::packs::templates::{self, TemplateInfo};
use pos_core::packs::{self, ApplyReport, Module, PackState, TillMode};
use pos_core::regulated::{self, RegulatedFlag, RegulatedProduct};
use serde_json::json;
use tauri::State;

/// Every activity template with its modules, till mode and seeds (the setup wizard lists them
/// before the owner exists).
#[tauri::command]
#[specta::specta]
pub async fn packs_templates() -> Result<Vec<TemplateInfo>, AppError> {
    Ok(templates::infos())
}

/// The shop's activity, the modules on and the till mode: every screen reads it.
#[tauri::command]
#[specta::specta]
pub async fn packs_state(state: State<'_, SharedState>) -> Result<PackState, AppError> {
    let st = state.inner().clone();
    blocking(move || self::state(&st)).await
}

/// Makes `activity` the shop's template; with `seed`, its categories, services and the modules'
/// own records are created (none twice).
#[tauri::command]
#[specta::specta]
pub async fn packs_apply_template(
    state: State<'_, SharedState>,
    activity: ShopActivity,
    seed: bool,
) -> Result<ApplyReport, AppError> {
    let st = state.inner().clone();
    blocking(move || apply_template(&st, activity, seed)).await
}

/// The modules the owner picked by hand (their data always stays when one goes off).
#[tauri::command]
#[specta::specta]
pub async fn packs_set_modules(
    state: State<'_, SharedState>,
    modules: Vec<Module>,
) -> Result<PackState, AppError> {
    let st = state.inner().clone();
    blocking(move || set_modules(&st, modules)).await
}

/// Back to the template's modules.
#[tauri::command]
#[specta::specta]
pub async fn packs_reset_modules(state: State<'_, SharedState>) -> Result<PackState, AppError> {
    let st = state.inner().clone();
    blocking(move || reset_modules(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn packs_set_till_mode(
    state: State<'_, SharedState>,
    mode: TillMode,
) -> Result<PackState, AppError> {
    let st = state.inner().clone();
    blocking(move || set_till_mode(&st, mode)).await
}

/// The regulated staples at `level_id` (the retail level when absent) against their ceiling.
#[tauri::command]
#[specta::specta]
pub async fn regulated_board(
    state: State<'_, SharedState>,
    level_id: Option<String>,
) -> Result<Vec<RegulatedProduct>, AppError> {
    let st = state.inner().clone();
    blocking(move || regulated_prices(&st, level_id)).await
}

/// Links a product to a ceiling of the data pack, or removes the link with `code: null`.
#[tauri::command]
#[specta::specta]
pub async fn regulated_set(
    state: State<'_, SharedState>,
    product_id: String,
    code: Option<String>,
) -> Result<Option<RegulatedFlag>, AppError> {
    let st = state.inner().clone();
    blocking(move || set_regulation(&st, product_id, code)).await
}

/// Creates the data pack's regulated staples not in the catalogue yet; the number created.
#[tauri::command]
#[specta::specta]
pub async fn regulated_seed(state: State<'_, SharedState>) -> Result<u32, AppError> {
    let st = state.inner().clone();
    blocking(move || seed_regulated(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn departments_list(
    state: State<'_, SharedState>,
    include_archived: bool,
) -> Result<Vec<DepartmentKey>, AppError> {
    let st = state.inner().clone();
    blocking(move || department_keys(&st, include_archived)).await
}

/// Creates a department key (`id: null`) or changes one.
#[tauri::command]
#[specta::specta]
pub async fn departments_save(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: DepartmentKeyInput,
) -> Result<DepartmentKey, AppError> {
    let st = state.inner().clone();
    blocking(move || save_department(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn departments_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || archive_department(&st, id)).await
}

pub(crate) fn state(st: &AppState) -> Result<PackState, AppError> {
    st.session()?;
    st.db.read(packs::state)
}

pub(crate) fn apply_template(
    st: &AppState,
    activity: ShopActivity,
    seed: bool,
) -> Result<ApplyReport, AppError> {
    if let Some(done) = forward(
        st,
        "packs.apply_template",
        json!({ "activity": activity, "seed": seed }),
    )? {
        return Ok(done);
    }
    let session = st.require(Permission::SettingsCompany)?;
    let (actor, pack, lang) = (session.actor(), st.datapack.clone(), session.lang);
    st.db
        .write(move |tx| packs::apply_template(tx, &actor, &pack, activity, lang, seed))
}

/// A module the owner turns on gets its own seeds at once (`packs::switch_modules`).
pub(crate) fn set_modules(st: &AppState, modules: Vec<Module>) -> Result<PackState, AppError> {
    if let Some(done) = forward(st, "packs.set_modules", json!({ "modules": modules }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    let pack = st.datapack.clone();
    st.db
        .write(move |tx| packs::switch_modules(tx, &actor, &pack, &modules))
}

pub(crate) fn reset_modules(st: &AppState) -> Result<PackState, AppError> {
    if let Some(done) = forward(st, "packs.reset_modules", json!({}))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| packs::reset_modules(tx, &actor))
}

pub(crate) fn set_till_mode(st: &AppState, mode: TillMode) -> Result<PackState, AppError> {
    if let Some(done) = forward(st, "packs.set_till_mode", json!({ "mode": mode }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| packs::set_till_mode(tx, &actor, mode))
}

pub(crate) fn regulated_prices(
    st: &AppState,
    level_id: Option<String>,
) -> Result<Vec<RegulatedProduct>, AppError> {
    st.require(Permission::ProductsView)?;
    let pack = st.datapack.clone();
    let level = level_id.unwrap_or_else(|| RETAIL_LEVEL_ID.to_owned());
    st.db.read(move |c| {
        packs::require(c, Module::RegulatedPrices)?;
        regulated::board(c, &pack, &level)
    })
}

pub(crate) fn set_regulation(
    st: &AppState,
    product_id: String,
    code: Option<String>,
) -> Result<Option<RegulatedFlag>, AppError> {
    if let Some(done) = forward(
        st,
        "regulated.set",
        json!({ "productId": product_id, "code": code }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsPrice)?.actor();
    let pack = st.datapack.clone();
    st.db
        .write(move |tx| regulated::set(tx, &actor, &pack, &product_id, code.as_deref()))
}

pub(crate) fn seed_regulated(st: &AppState) -> Result<u32, AppError> {
    if let Some(done) = forward(st, "regulated.seed", json!({}))? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsPrice)?.actor();
    let pack = st.datapack.clone();
    st.db
        .write(move |tx| regulated::seed_staples(tx, &actor, &pack))
}

/// The keys of the till's calculator and of their settings screen.
pub(crate) fn department_keys(
    st: &AppState,
    include_archived: bool,
) -> Result<Vec<DepartmentKey>, AppError> {
    st.require_any(&[Permission::SalesCreate, Permission::ProductsView])?;
    st.db.read(move |c| {
        packs::require(c, Module::DepartmentKeys)?;
        departments::list(c, include_archived)
    })
}

pub(crate) fn save_department(
    st: &AppState,
    id: Option<String>,
    input: DepartmentKeyInput,
) -> Result<DepartmentKey, AppError> {
    if let Some(done) = forward(st, "departments.save", json!({ "id": id, "input": input }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| match id {
        Some(id) => departments::update(tx, &actor, &id, input),
        None => departments::create(tx, &actor, input),
    })
}

pub(crate) fn archive_department(st: &AppState, id: String) -> Result<(), AppError> {
    if let Some(done) = forward(st, "departments.archive", json!({ "id": id }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| departments::archive(tx, &actor, &id))
}
