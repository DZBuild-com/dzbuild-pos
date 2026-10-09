//! Update commands (spec §13). The webview has no updater permission: everything goes here.

use super::blocking;
use crate::state::{AppState, SharedState};
use crate::updates::{UpdateProgress, UpdateService, UpdateStatus};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_specta::Event;

type Service<'a> = State<'a, Arc<UpdateService>>;

/// `update-progress` at most every 256 KiB, and at the end.
const PROGRESS_STEP: u64 = 256 * 1024;

#[tauri::command]
#[specta::specta]
pub async fn updates_status(
    state: State<'_, SharedState>,
    service: Service<'_>,
) -> Result<UpdateStatus, AppError> {
    let (st, service) = (state.inner().clone(), service.inner().clone());
    blocking(move || {
        status_guard(&st)?;
        service.status(&st)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn updates_check(
    state: State<'_, SharedState>,
    service: Service<'_>,
) -> Result<UpdateStatus, AppError> {
    let st = state.inner().clone();
    st.require(Permission::SettingsData)?;
    let service = service.inner().clone();
    service.check_now(&st, true).await
}

/// Starts the download in the background and answers at once; `update-progress` events follow.
#[tauri::command]
#[specta::specta]
pub async fn updates_download(
    app: AppHandle,
    state: State<'_, SharedState>,
    service: Service<'_>,
) -> Result<UpdateStatus, AppError> {
    let st = state.inner().clone();
    st.require(Permission::SettingsData)?;
    let service = service.inner().clone();
    let (task_st, task_service, emitter) = (st.clone(), service.clone(), app.clone());
    tauri::async_runtime::spawn(async move {
        let mut last_sent = 0_u64;
        let result = task_service
            .download(&task_st, |received, total| {
                if received.saturating_sub(last_sent) >= PROGRESS_STEP || Some(received) == total {
                    last_sent = received;
                    if let Err(error) = (UpdateProgress { received, total }).emit(&emitter) {
                        tracing::debug!(%error, "could not send the update progress");
                    }
                }
            })
            .await;
        match result {
            Ok(status) => {
                crate::tray::set_update_ready(&emitter, status.offer.map(|o| o.version));
            }
            Err(error) => tracing::warn!(%error, "update download failed"),
        }
    });
    blocking(move || service.status(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn updates_install(
    state: State<'_, SharedState>,
    service: Service<'_>,
) -> Result<(), AppError> {
    let (st, service) = (state.inner().clone(), service.inner().clone());
    blocking(move || {
        let session = st.require(Permission::SettingsData)?;
        service.install(&st, session.actor())
    })
    .await
}

/// Anyone signed in sees the update status (the banner); the guard table probes it.
pub(crate) fn status_guard(st: &AppState) -> Result<(), AppError> {
    st.session().map(drop)
}
