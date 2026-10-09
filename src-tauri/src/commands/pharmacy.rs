//! Pharmacy and parapharmacy (master spec §30 VRT-030, PRD-051…053): a medicine's DCI and
//! controlled flag, its generics, the expiry board, the Chifa reference list imported from the
//! pharmacist's file and the controlled-medicine register (health data, `pharmacy.register`
//! only). The Chifa file is opened by M2's import dialog (`imports_open_file`: the webview holds a
//! token, never a path), checked, then applied in one write. Product and list writes are the
//! main PC's, and so is the register (kept there only): a till forwards them
//! (`host_required` offline). Everything refuses with `module_disabled` while the
//! pharmacy module is off.

use super::{blocking, redact};
use crate::lan::route::forward;
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::catalog::RETAIL_LEVEL_ID;
use pos_core::error::AppError;
use pos_core::money::Money;
use pos_core::packs::{self, Module};
use pos_core::pharmacy::chifa::{self, ChifaApplied, ChifaCheck, ChifaMapping, ChifaPrice};
use pos_core::pharmacy::register::{self, RegisterEntry};
use pos_core::pharmacy::{self, Equivalent, ExpiryBoard, PharmacyProduct, PharmacyProductInput};
use pos_core::time::business_date;
use serde_json::json;
use tauri::State;

/// The medicine's pharmacy row (`null` for a product without one).
#[tauri::command]
#[specta::specta]
pub async fn pharmacy_product_get(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Option<PharmacyProduct>, AppError> {
    let st = state.inner().clone();
    blocking(move || product_get(&st, product_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn pharmacy_product_set(
    state: State<'_, SharedState>,
    product_id: String,
    input: PharmacyProductInput,
) -> Result<PharmacyProduct, AppError> {
    let st = state.inner().clone();
    blocking(move || product_set(&st, product_id, input)).await
}

/// The other live medicines of the same DCI at `level_id` (the retail level when absent),
/// cheapest first (the substitution notice).
#[tauri::command]
#[specta::specta]
pub async fn pharmacy_equivalents(
    state: State<'_, SharedState>,
    product_id: String,
    level_id: Option<String>,
) -> Result<Vec<Equivalent>, AppError> {
    let st = state.inner().clone();
    blocking(move || equivalents(&st, product_id, level_id)).await
}

/// Lots expired and expiring within `within_days` (`null`: the `stock.expiry_alert_days`
/// setting), valued at cost for whoever sees costs.
#[tauri::command]
#[specta::specta]
pub async fn pharmacy_expiry_board(
    state: State<'_, SharedState>,
    within_days: Option<i64>,
) -> Result<ExpiryBoard, AppError> {
    let st = state.inner().clone();
    blocking(move || expiry_board(&st, within_days)).await
}

/// The dry run of the Chifa list staged under `token` (first sheet, under its header row).
#[tauri::command]
#[specta::specta]
pub async fn chifa_check(
    state: State<'_, SharedState>,
    token: String,
    mapping: ChifaMapping,
) -> Result<ChifaCheck, AppError> {
    let st = state.inner().clone();
    blocking(move || check_chifa(&st, token, mapping)).await
}

/// Writes the staged list, refused as a whole when any row has a problem; the file leaves the
/// staging area.
#[tauri::command]
#[specta::specta]
pub async fn chifa_apply(
    state: State<'_, SharedState>,
    token: String,
    mapping: ChifaMapping,
) -> Result<ChifaApplied, AppError> {
    let st = state.inner().clone();
    blocking(move || apply_chifa(&st, token, mapping)).await
}

/// The list's prices whose code or name contains `text`, `limit` at most.
#[tauri::command]
#[specta::specta]
pub async fn chifa_list(
    state: State<'_, SharedState>,
    text: String,
    limit: u32,
) -> Result<Vec<ChifaPrice>, AppError> {
    let st = state.inner().clone();
    blocking(move || chifa_prices(&st, text, limit)).await
}

/// The reference price of a medicine, for the till's information line.
#[tauri::command]
#[specta::specta]
pub async fn chifa_of_product(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Option<ChifaPrice>, AppError> {
    let st = state.inner().clone();
    blocking(move || chifa_price_of(&st, product_id)).await
}

/// The register entries of the business days `from`..=`to` (`YYYY-MM-DD`), in register order.
/// Printed and exported through the reports (`controlled_register`).
#[tauri::command]
#[specta::specta]
pub async fn controlled_register_entries(
    state: State<'_, SharedState>,
    from: String,
    to: String,
) -> Result<Vec<RegisterEntry>, AppError> {
    let st = state.inner().clone();
    blocking(move || register_entries(&st, from, to)).await
}

/// Medicine reads shared by the till and the back office.
const MEDICINE_READ: &[Permission] = &[Permission::SalesCreate, Permission::ProductsView];

pub(crate) fn product_get(
    st: &AppState,
    product_id: String,
) -> Result<Option<PharmacyProduct>, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(move |c| {
        packs::require(c, Module::Pharmacy)?;
        pharmacy::get(c, &product_id)
    })
}

pub(crate) fn product_set(
    st: &AppState,
    product_id: String,
    input: PharmacyProductInput,
) -> Result<PharmacyProduct, AppError> {
    if let Some(done) = forward(
        st,
        "pharmacy.product_set",
        json!({ "productId": product_id, "input": input }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db
        .write(move |tx| pharmacy::set(tx, &actor, &product_id, input))
}

pub(crate) fn equivalents(
    st: &AppState,
    product_id: String,
    level_id: Option<String>,
) -> Result<Vec<Equivalent>, AppError> {
    let lang = st.require_any(MEDICINE_READ)?.lang;
    let level = level_id.unwrap_or_else(|| RETAIL_LEVEL_ID.to_owned());
    st.db.read(move |c| {
        packs::require(c, Module::Pharmacy)?;
        pharmacy::equivalents(c, &product_id, lang, &level)
    })
}

pub(crate) fn expiry_board(
    st: &AppState,
    within_days: Option<i64>,
) -> Result<ExpiryBoard, AppError> {
    let session = st.require(Permission::StockView)?;
    let today = business_date(st.now()).format("%Y-%m-%d").to_string();
    let mut board = st.db.read(move |c| {
        packs::require(c, Module::Pharmacy)?;
        let days = match within_days {
            Some(days) => days,
            None => pos_core::settings::get(c, "stock.expiry_alert_days")?,
        };
        pharmacy::expiry_board(c, &today, days)
    })?;
    if !redact::sees_costs(&session) {
        board.expired_cost = Money::ZERO;
        board.soon_cost = Money::ZERO;
        for lot in board.expired.iter_mut().chain(board.soon.iter_mut()) {
            lot.cost = Money::ZERO;
        }
    }
    Ok(board)
}

/// The data rows (under the header row the reader found) of the first sheet of the file staged
/// under `token` for this user, and the file's name.
fn staged_rows(
    st: &AppState,
    session: &Session,
    token: &str,
) -> Result<(String, Vec<Vec<String>>), AppError> {
    st.staging.with(token, &session.user_id, st.now(), |s| {
        let sheet = s
            .file
            .sheets
            .first()
            .ok_or_else(|| AppError::validation("sheet", "import_sheet_not_found"))?;
        let header = pos_docs::import::header_row(sheet);
        let rows = sheet.rows.iter().skip(header + 1).cloned().collect();
        Ok((s.file_name.clone(), rows))
    })
}

pub(crate) fn check_chifa(
    st: &AppState,
    token: String,
    mapping: ChifaMapping,
) -> Result<ChifaCheck, AppError> {
    let session = st.require(Permission::ProductsEdit)?;
    let (_, rows) = staged_rows(st, &session, &token)?;
    st.db.read(move |c| {
        packs::require(c, Module::Pharmacy)?;
        chifa::check(c, &rows, &mapping)
    })
}

/// The rows are read here (the file is this PC's) and written on the main PC.
pub(crate) fn apply_chifa(
    st: &AppState,
    token: String,
    mapping: ChifaMapping,
) -> Result<ChifaApplied, AppError> {
    let session = st.require(Permission::ProductsEdit)?;
    let (file_name, rows) = staged_rows(st, &session, &token)?;
    let done = match forward(
        st,
        "pharmacy.chifa_apply",
        json!({ "fileName": file_name, "rows": rows, "mapping": mapping }),
    )? {
        Some(done) => done,
        None => apply_chifa_rows(st, file_name, rows, mapping)?,
    };
    st.staging.remove(&token)?;
    Ok(done)
}

/// The host service of `chifa_apply`: the rows a till read from its file.
pub(crate) fn apply_chifa_rows(
    st: &AppState,
    file_name: String,
    rows: Vec<Vec<String>>,
    mapping: ChifaMapping,
) -> Result<ChifaApplied, AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db
        .write(move |tx| chifa::apply(tx, &actor, &rows, &mapping, &file_name))
}

pub(crate) fn chifa_prices(
    st: &AppState,
    text: String,
    limit: u32,
) -> Result<Vec<ChifaPrice>, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(move |c| {
        packs::require(c, Module::Pharmacy)?;
        chifa::list(c, &text, limit)
    })
}

pub(crate) fn chifa_price_of(
    st: &AppState,
    product_id: String,
) -> Result<Option<ChifaPrice>, AppError> {
    st.require_any(MEDICINE_READ)?;
    st.db.read(move |c| {
        packs::require(c, Module::Pharmacy)?;
        chifa::of_product(c, &product_id)
    })
}

/// On a till the register is read on the main PC: it is kept there only.
pub(crate) fn register_entries(
    st: &AppState,
    from: String,
    to: String,
) -> Result<Vec<RegisterEntry>, AppError> {
    if let Some(done) = forward(
        st,
        "pharmacy.register_entries",
        json!({ "from": from, "to": to }),
    )? {
        return Ok(done);
    }
    st.require(Permission::PharmacyRegister)?;
    st.db.read(move |c| {
        packs::require(c, Module::Pharmacy)?;
        register::entries(c, &from, &to)
    })
}
