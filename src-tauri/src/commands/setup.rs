//! First-run wizard commands.

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::Session;
use pos_core::error::AppError;
use pos_core::setup::{self, FirstRunInput, SetupStatus};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn setup_status(state: State<'_, SharedState>) -> Result<SetupStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || st.db.read(setup::status)).await
}

#[tauri::command]
#[specta::specta]
pub async fn setup_complete_first_run(
    state: State<'_, SharedState>,
    input: FirstRunInput,
) -> Result<Session, AppError> {
    let st = state.inner().clone();
    blocking(move || complete_first_run(&st, input)).await
}

/// The shop exists from here on: the pole display greets with its name in this first session.
pub(crate) fn complete_first_run(st: &AppState, input: FirstRunInput) -> Result<Session, AppError> {
    let session = setup::complete_first_run(&st.db, st.datapack.clone(), &st.terminal, input)?;
    st.set_session(Some(session.clone()))?;
    // Set up as a new shop: a till's place kept from a copy set aside no longer applies (the
    // pairing writes its own when this PC joins a main PC instead).
    if !st.paths.demo {
        let path = std::path::Path::new(&st.paths.data_dir).join(pos_core::local::FILE_NAME);
        if let Err(error) = pos_core::local::record_lan(&path, None) {
            tracing::warn!(code = %error.code(), "local.json kept the till's former place");
        }
    }
    match st.db.read(pos_core::company::get) {
        Ok(Some(company)) => st.devices.set_shop(crate::devices::latin_name(&company)),
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "the pole display waits for the shop's name"),
    }
    Ok(session)
}
