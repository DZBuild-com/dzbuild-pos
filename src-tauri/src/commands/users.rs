//! Users and roles commands. Users need `settings.users`; creating, changing or removing a role
//! is the owner's alone (spec §4.5), a `settings.users` holder only assigns existing roles.
//! Every edit then rebuilds the caller's session, so its permissions never go stale.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::users::{self, NewUser, User, UserUpdate};
use pos_core::auth::{self, Permission, PermissionGroupInfo, Role, RoleInput, roles};
use pos_core::error::AppError;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn users_list(state: State<'_, SharedState>) -> Result<Vec<User>, AppError> {
    let st = state.inner().clone();
    blocking(move || list_users(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn users_create(state: State<'_, SharedState>, input: NewUser) -> Result<User, AppError> {
    let st = state.inner().clone();
    blocking(move || create_user(&st, input)).await
}

/// Editing one's own account applies to the session at once (name, language, role).
#[tauri::command]
#[specta::specta]
pub async fn users_update(
    state: State<'_, SharedState>,
    id: String,
    input: UserUpdate,
) -> Result<User, AppError> {
    let st = state.inner().clone();
    blocking(move || update_user(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn users_set_pin(
    state: State<'_, SharedState>,
    id: String,
    pin: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || set_pin(&st, id, pin)).await
}

#[tauri::command]
#[specta::specta]
pub async fn users_deactivate(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || deactivate(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn roles_list(state: State<'_, SharedState>) -> Result<Vec<Role>, AppError> {
    let st = state.inner().clone();
    blocking(move || list_roles(&st)).await
}

/// Pure: reads the in-memory session only.
#[tauri::command]
#[specta::specta]
pub async fn roles_catalog(
    state: State<'_, SharedState>,
) -> Result<Vec<PermissionGroupInfo>, AppError> {
    role_catalog(&state)
}

#[tauri::command]
#[specta::specta]
pub async fn roles_create(
    state: State<'_, SharedState>,
    input: RoleInput,
) -> Result<Role, AppError> {
    let st = state.inner().clone();
    blocking(move || create_role(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn roles_update(
    state: State<'_, SharedState>,
    id: String,
    input: RoleInput,
) -> Result<Role, AppError> {
    let st = state.inner().clone();
    blocking(move || update_role(&st, id, input)).await
}

/// Archives the role (never deleted); `role_in_use` while any user, even deactivated, holds it.
#[tauri::command]
#[specta::specta]
pub async fn roles_delete(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || delete_role(&st, id)).await
}

/// Hands back an edit's result once the caller's session is rebuilt from the database: the
/// edit may have changed the caller's own role, permissions, name or language.
fn refreshed<T>(st: &AppState, done: T) -> Result<T, AppError> {
    st.refresh_session()?;
    Ok(done)
}

pub(crate) fn list_users(st: &AppState) -> Result<Vec<User>, AppError> {
    st.require(Permission::SettingsUsers)?;
    st.db.read(users::list)
}

/// On the owner role, pos-core also requires the caller to be an owner (`owner_only`).
pub(crate) fn create_user(st: &AppState, input: NewUser) -> Result<User, AppError> {
    let session = st.require(Permission::SettingsUsers)?;
    let pin_hash = auth::hash_pin(&input.pin)?;
    let actor = session.actor();
    let user = st
        .db
        .write(move |tx| users::create(tx, &actor, &input, &pin_hash))?;
    refreshed(st, user)
}

/// On an owner account or to the owner role, pos-core also requires an owner (`owner_only`).
pub(crate) fn update_user(st: &AppState, id: String, input: UserUpdate) -> Result<User, AppError> {
    let session = st.require(Permission::SettingsUsers)?;
    let actor = session.actor();
    let user = st
        .db
        .write(move |tx| users::update(tx, &actor, &id, &input))?;
    refreshed(st, user)
}

pub(crate) fn set_pin(st: &AppState, id: String, pin: String) -> Result<(), AppError> {
    let session = st.require(Permission::SettingsUsers)?;
    let pin_hash = auth::hash_pin(&pin)?;
    let actor = session.actor();
    st.db
        .write(move |tx| users::set_pin_hash(tx, &actor, &id, &pin_hash))?;
    refreshed(st, ())
}

pub(crate) fn deactivate(st: &AppState, id: String) -> Result<(), AppError> {
    let session = st.require(Permission::SettingsUsers)?;
    let actor = session.actor();
    st.db.write(move |tx| users::deactivate(tx, &actor, &id))?;
    refreshed(st, ())
}

/// A `settings.users` holder sees the roles to assign them.
pub(crate) fn list_roles(st: &AppState) -> Result<Vec<Role>, AppError> {
    st.require(Permission::SettingsUsers)?;
    st.db.read(roles::list)
}

pub(crate) fn role_catalog(st: &AppState) -> Result<Vec<PermissionGroupInfo>, AppError> {
    st.require(Permission::SettingsUsers)?;
    Ok(roles::catalog())
}

pub(crate) fn create_role(st: &AppState, input: RoleInput) -> Result<Role, AppError> {
    let session = st.require_owner(Permission::SettingsUsers)?;
    let actor = session.actor();
    let role = st.db.write(move |tx| roles::create(tx, &actor, input))?;
    refreshed(st, role)
}

pub(crate) fn update_role(st: &AppState, id: String, input: RoleInput) -> Result<Role, AppError> {
    let session = st.require_owner(Permission::SettingsUsers)?;
    let actor = session.actor();
    let role = st
        .db
        .write(move |tx| roles::update(tx, &actor, &id, input))?;
    refreshed(st, role)
}

pub(crate) fn delete_role(st: &AppState, id: String) -> Result<(), AppError> {
    let session = st.require_owner(Permission::SettingsUsers)?;
    let actor = session.actor();
    st.db.write(move |tx| roles::archive(tx, &actor, &id))?;
    refreshed(st, ())
}
