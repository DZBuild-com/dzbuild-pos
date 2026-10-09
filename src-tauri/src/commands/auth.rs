//! Login screen, session and manager override commands.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, OverrideGrant, Permission, Session, UserTile};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use serde::Deserialize;
use specta::Type;
use std::fmt;
use tauri::{AppHandle, State};

#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    /// User id (from the tile) or username.
    pub user: String,
    pub pin: String,
}

/// `Debug` is written by hand so that the plaintext PIN never reaches a log line.
impl fmt::Debug for LoginInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginInput")
            .field("user", &self.user)
            .field("pin", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OverrideInput {
    pub approver_id: String,
    pub pin: String,
    pub permission: Permission,
}

/// `Debug` is written by hand so that the plaintext PIN never reaches a log line.
impl fmt::Debug for OverrideInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OverrideInput")
            .field("approver_id", &self.approver_id)
            .field("pin", &"<redacted>")
            .field("permission", &self.permission)
            .finish()
    }
}

#[tauri::command]
#[specta::specta]
pub async fn auth_list_users(state: State<'_, SharedState>) -> Result<Vec<UserTile>, AppError> {
    let st = state.inner().clone();
    blocking(move || st.db.read(auth::users::list_tiles)).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_login(
    app: AppHandle,
    state: State<'_, SharedState>,
    input: LoginInput,
) -> Result<Session, AppError> {
    let st = state.inner().clone();
    let session = blocking(move || login(&st, &input)).await?;
    // The tray speaks the signed-in user's language.
    crate::tray::retitle(&app, session.lang);
    Ok(session)
}

#[tauri::command]
#[specta::specta]
pub async fn auth_logout(state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || logout(&st)).await
}

/// Sync on purpose: reads the in-memory session only, no SQLite or Argon2 (Global Constraints).
#[tauri::command]
#[specta::specta]
pub fn auth_session(state: State<'_, SharedState>) -> Result<Option<Session>, AppError> {
    state.current_session()
}

#[tauri::command]
#[specta::specta]
pub async fn auth_manager_override(
    state: State<'_, SharedState>,
    input: OverrideInput,
) -> Result<OverrideGrant, AppError> {
    let st = state.inner().clone();
    blocking(move || manager_override(&st, &input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_set_language(
    app: AppHandle,
    state: State<'_, SharedState>,
    lang: Lang,
) -> Result<Session, AppError> {
    let st = state.inner().clone();
    let session = blocking(move || set_language(&st, lang)).await?;
    crate::tray::retitle(&app, session.lang);
    Ok(session)
}

/// A sign-in that replaces another user's live session also journals that user's sign-out and
/// drops the files they opened for an import. Every pending manager approval is dropped: a
/// grant never outlives the sign-in it was for.
pub(crate) fn login(st: &AppState, input: &LoginInput) -> Result<Session, AppError> {
    let session = auth::login(&st.db, &st.terminal, &input.user, &input.pin)?;
    let replaced = st.replace_session(Some(session.clone()))?;
    st.approvals.clear()?;
    if let Some(previous) = replaced.filter(|previous| previous.user_id != session.user_id) {
        journal_logout(st, &previous);
        st.staging.clear_user(&previous.user_id)?;
    }
    Ok(session)
}

/// Fails closed: the session and its pending manager approvals are gone before the sign-out is
/// journaled, so a journal write that fails (disk full, database closed for shutdown) never
/// leaves the user signed in. The failure is logged; the sign-out itself always succeeds. The
/// files the user opened for an import go too.
pub(crate) fn logout(st: &AppState) -> Result<(), AppError> {
    let previous = st.replace_session(None)?;
    st.approvals.clear()?;
    if let Some(previous) = previous {
        journal_logout(st, &previous);
        st.staging.clear_user(&previous.user_id)?;
    }
    Ok(())
}

fn journal_logout(st: &AppState, session: &Session) {
    if let Err(error) = auth::logout(&st.db, session) {
        tracing::error!(%error, user = %session.user_id, "sign-out not journaled");
    }
}

pub(crate) fn manager_override(
    st: &AppState,
    input: &OverrideInput,
) -> Result<OverrideGrant, AppError> {
    let requester = st.session()?;
    let grant = auth::manager_override(
        &st.db,
        &st.terminal,
        &requester,
        &input.approver_id,
        &input.pin,
        input.permission,
        st.now(),
    )?;
    // Kept here for the next till operation; the webview only shows who approved.
    st.approvals.add(&requester, grant.clone())?;
    Ok(grant)
}

pub(crate) fn set_language(st: &AppState, lang: Lang) -> Result<Session, AppError> {
    let session = st.session()?;
    let updated = auth::change_language(&st.db, &session, lang)?;
    st.set_session(Some(updated.clone()))?;
    Ok(updated)
}
