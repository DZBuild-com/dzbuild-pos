//! Settings commands. `ui.*` keys work before login (login screen language, theme).

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::error::AppError;
use pos_core::json::JsonValue;
use pos_core::settings;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::State;

/// A setting's value as plain JSON. specta rc.25 cannot export `serde_json::Value` itself
/// (see `pos_core::json`), so it crosses the IPC in this newtype, typed `JsonValue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct SettingValue(#[specta(type = JsonValue)] pub Value);

#[tauri::command]
#[specta::specta]
pub async fn settings_get(
    state: State<'_, SharedState>,
    key: String,
) -> Result<SettingValue, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, key).map(SettingValue)).await
}

#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    state: State<'_, SharedState>,
    key: String,
    value: SettingValue,
) -> Result<SettingValue, AppError> {
    let st = state.inner().clone();
    blocking(move || set(&st, key, value.0).map(SettingValue)).await
}

/// Keys without a permission (`ui.*`) read without a session; the others need one.
pub(crate) fn get(st: &AppState, key: String) -> Result<Value, AppError> {
    let def = settings::def(&key)?;
    if def.permission.is_some() {
        st.session()?;
    }
    st.db.read(move |c| settings::get_value(c, &key))
}

/// The key's permission, or none for `ui.*` keys (journaled with the user when one is signed in).
/// A `cloud.*` switch wakes the sync engine, so it applies now rather than at the next cycle.
pub(crate) fn set(st: &AppState, key: String, value: Value) -> Result<Value, AppError> {
    let def = settings::def(&key)?;
    let session = match def.permission {
        Some(permission) => Some(st.require(permission)?),
        None => st.current_session()?,
    };
    let actor = st.actor(session.as_ref());
    let cloud_key = key.starts_with("cloud.");
    let saved = st
        .db
        .write(move |tx| settings::set_value(tx, &actor, &key, value))?;
    if cloud_key && let Ok(cloud) = st.cloud() {
        cloud.wake();
    }
    Ok(saved)
}
