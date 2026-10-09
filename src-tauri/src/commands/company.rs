//! Shop identity commands.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::company::{self, Company, CompanyInput};
use pos_core::error::AppError;
use tauri::State;

/// No permission: the login screen and the top bar show the shop name.
#[tauri::command]
#[specta::specta]
pub async fn company_get(state: State<'_, SharedState>) -> Result<Option<Company>, AppError> {
    let st = state.inner().clone();
    blocking(move || st.db.read(company::get)).await
}

#[tauri::command]
#[specta::specta]
pub async fn company_update(
    state: State<'_, SharedState>,
    input: CompanyInput,
) -> Result<Company, AppError> {
    let st = state.inner().clone();
    blocking(move || update(&st, input)).await
}

/// `logo`: the bytes of a PNG or JPEG of at most 512 KB, sent as a number array
/// (`Array.from(new Uint8Array(buffer))`); `null` removes the logo.
#[tauri::command]
#[specta::specta]
pub async fn company_set_logo(
    state: State<'_, SharedState>,
    logo: Option<Vec<u8>>,
) -> Result<Company, AppError> {
    let st = state.inner().clone();
    blocking(move || set_logo(&st, logo)).await
}

/// Saved, then the pole display's idle screen takes the new Latin name.
pub(crate) fn update(st: &AppState, input: CompanyInput) -> Result<Company, AppError> {
    let session = st.require(Permission::SettingsCompany)?;
    let actor = session.actor();
    let pack = st.datapack.clone();
    let company = st
        .db
        .write(move |tx| company::save(tx, &actor, &pack, input))?;
    st.devices.set_shop(crate::devices::latin_name(&company));
    Ok(company)
}

pub(crate) fn set_logo(st: &AppState, logo: Option<Vec<u8>>) -> Result<Company, AppError> {
    let session = st.require(Permission::SettingsCompany)?;
    let actor = session.actor();
    st.db
        .write(move |tx| company::set_logo(tx, &actor, logo.as_deref()))
}
