//! Audit journal commands (`audit.view`): the whole journal, one record's history (AUD-002).

use super::{blocking, redact};
use crate::state::{AppState, SharedState};
use pos_core::audit::{self, AuditFilter, AuditPage, AuditQuery, ChainReport};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn audit_list(
    state: State<'_, SharedState>,
    query: AuditQuery,
) -> Result<AuditPage, AppError> {
    let st = state.inner().clone();
    blocking(move || list(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn audit_verify_chain(state: State<'_, SharedState>) -> Result<ChainReport, AppError> {
    let st = state.inner().clone();
    blocking(move || verify(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn audit_history(
    state: State<'_, SharedState>,
    filter: AuditFilter,
) -> Result<AuditPage, AppError> {
    let st = state.inner().clone();
    blocking(move || history(&st, filter)).await
}

pub(crate) fn list(st: &AppState, query: AuditQuery) -> Result<AuditPage, AppError> {
    let session = st.require(Permission::AuditView)?;
    let mut page = st.db.read(move |c| audit::list(c, &query))?;
    redact::audit(&mut page, &session);
    Ok(page)
}

pub(crate) fn verify(st: &AppState) -> Result<ChainReport, AppError> {
    st.require(Permission::AuditView)?;
    st.db.read(audit::verify_chain)
}

pub(crate) fn history(st: &AppState, filter: AuditFilter) -> Result<AuditPage, AppError> {
    let session = st.require(Permission::AuditView)?;
    let mut page = st.db.read(move |c| audit::list_filtered(c, &filter))?;
    redact::audit(&mut page, &session);
    Ok(page)
}
