//! This PC's display choices (UX-009 font scaling, light mode for slow PCs, on-screen keyboard).
//! They live in `local.json` (Task 2), never in the shop database: a restore, an import or a LAN
//! sync never changes how this screen looks.

use super::blocking;
use crate::state::{AppPaths, AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::local::{LocalConfig, TextSize};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPrefs {
    pub light_mode: bool,
    pub text_size: TextSize,
    pub onscreen_keyboard: bool,
}

impl From<&LocalConfig> for DisplayPrefs {
    fn from(c: &LocalConfig) -> DisplayPrefs {
        DisplayPrefs {
            light_mode: c.light_mode,
            text_size: c.text_size,
            onscreen_keyboard: c.onscreen_keyboard,
        }
    }
}

/// The running store once the database has opened; before it (login, wizard, a failed start)
/// the file is read directly.
pub(crate) fn get(state: Option<&AppState>, paths: &AppPaths) -> DisplayPrefs {
    match state {
        Some(st) => DisplayPrefs::from(&st.local.get()),
        None => DisplayPrefs::from(&LocalConfig::load(Path::new(&paths.local_config))),
    }
}

/// Public: the login screen and the setup wizard apply them too. Without a data folder (the
/// fatal screen of a start that found none) the defaults.
#[tauri::command]
#[specta::specta]
pub async fn display_get(app: AppHandle) -> Result<DisplayPrefs, AppError> {
    let state = app.try_state::<SharedState>().map(|s| s.inner().clone());
    let Some(paths) = app.try_state::<AppPaths>().map(|p| p.inner().clone()) else {
        return Ok(DisplayPrefs::from(&LocalConfig::default()));
    };
    blocking(move || Ok(get(state.as_deref(), &paths))).await
}

/// `settings.hardware` (this PC's hardware and screen); each changed choice is journaled
/// `local.updated`.
pub(crate) fn set(st: &AppState, prefs: DisplayPrefs) -> Result<DisplayPrefs, AppError> {
    let session = st.require(Permission::SettingsHardware)?;
    let saved = st.local.update(&st.db, session.actor(), |c| {
        c.light_mode = prefs.light_mode;
        c.text_size = prefs.text_size;
        c.onscreen_keyboard = prefs.onscreen_keyboard;
    })?;
    Ok(DisplayPrefs::from(&saved))
}

#[tauri::command]
#[specta::specta]
pub async fn display_set(
    state: State<'_, SharedState>,
    prefs: DisplayPrefs,
) -> Result<DisplayPrefs, AppError> {
    let st = state.inner().clone();
    blocking(move || set(&st, prefs)).await
}
