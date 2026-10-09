//! Diagnostic and support commands (PLT-019, ONB-012). Nothing is sent anywhere: the zip is
//! saved where the owner chooses with a Rust dialog; the support links come from the
//! `support.contact` setting and open in the system browser, named by an enum, never a URL.

use super::{blocking, save_dialog};
use crate::diagnostics::{CrashRecord, export_to, last_crash, system_info};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::json::JsonValue;
use pos_core::local::LogLevel;
use pos_core::settings;
use pos_docs::diagnostics::BundleSummary;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticInfo {
    /// `crate::diagnostics::system_info`'s keys (`appVersion`, `ramTotalMb`, `webview2`, …).
    #[specta(type = JsonValue)]
    pub system: serde_json::Value,
    pub last_crash: Option<CrashRecord>,
    pub log_level: LogLevel,
}

/// The `support.contact` setting: the shop's reseller or technician (ONB-012).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupportContact {
    /// International digits only (`213550123456`).
    pub whatsapp: Option<String>,
    /// An `https://` page.
    pub url: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SupportChannel {
    Whatsapp,
    Web,
}

fn webview(app: &AppHandle) -> Option<String> {
    app.try_state::<crate::webview::WebviewInfo>()
        .and_then(|w| w.0.clone())
}

pub(crate) fn info(st: &AppState, webview: Option<String>) -> Result<DiagnosticInfo, AppError> {
    st.require(Permission::SettingsData)?;
    Ok(DiagnosticInfo {
        system: system_info(webview, Some(st), &st.paths),
        last_crash: last_crash(Path::new(&st.paths.logs_dir)),
        log_level: st.local.get().log_level,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn diagnostics_info(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<DiagnosticInfo, AppError> {
    let st = state.inner().clone();
    let webview = webview(&app);
    blocking(move || info(&st, webview)).await
}

/// The guard of `diagnostics_export`, checked before the dialog opens.
pub(crate) fn may_export(st: &AppState) -> Result<(), AppError> {
    st.require(Permission::SettingsData).map(drop)
}

/// Saves the zip where the owner says (`dzbuild-pos-diagnostic-<YYYYMMDD-HHMM>.zip`); `None`
/// when the dialog is cancelled.
#[tauri::command]
#[specta::specta]
pub async fn diagnostics_export(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<BundleSummary>, AppError> {
    let st = state.inner().clone();
    may_export(&st)?;
    let webview = webview(&app);
    blocking(move || {
        let name = format!(
            "dzbuild-pos-diagnostic-{}.zip",
            pos_core::time::algiers_datetime(st.now()).format("%Y%m%d-%H%M")
        );
        let Some(out) = save_dialog(&app, &window, &name, &["zip"])? else {
            return Ok(None);
        };
        let system = system_info(webview, Some(&st), &st.paths);
        export_to(&st, &st.paths, system, &out).map(Some)
    })
    .await
}

/// Saved in `local.json` (journaled `local.updated`) and applied to the running log at once.
pub(crate) fn set_log_level(st: &AppState, level: LogLevel) -> Result<LogLevel, AppError> {
    let session = st.require(Permission::SettingsData)?;
    st.local
        .update(&st.db, session.actor(), |c| c.log_level = level)?;
    if let Err(error) = crate::logging::set_level(level) {
        tracing::warn!(%error, "the log level applies at the next start");
    }
    Ok(level)
}

#[tauri::command]
#[specta::specta]
pub async fn diagnostics_set_log_level(
    state: State<'_, SharedState>,
    level: LogLevel,
) -> Result<LogLevel, AppError> {
    let st = state.inner().clone();
    blocking(move || set_log_level(&st, level)).await
}

/// Anyone signed in sees the last crash (About): a cashier tells the owner.
pub(crate) fn crash(st: &AppState) -> Result<Option<CrashRecord>, AppError> {
    st.session()?;
    Ok(last_crash(Path::new(&st.paths.logs_dir)))
}

#[tauri::command]
#[specta::specta]
pub async fn app_last_crash(
    state: State<'_, SharedState>,
) -> Result<Option<CrashRecord>, AppError> {
    let st = state.inner().clone();
    blocking(move || crash(&st)).await
}

/// The saved contact; an unreadable value counts as none.
pub(crate) fn contact(st: &AppState) -> Result<SupportContact, AppError> {
    st.session()?;
    let value = st.db.read(|c| settings::get_value(c, "support.contact"))?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// `https://wa.me/<digits>` or the saved `https://` page; `support_contact_missing` when the
/// setting has none for `channel`.
pub(crate) fn support_url(
    contact: &SupportContact,
    channel: SupportChannel,
) -> Result<String, AppError> {
    let missing = || AppError::conflict("support_contact_missing");
    match channel {
        SupportChannel::Whatsapp => contact
            .whatsapp
            .as_deref()
            .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            .map(|n| format!("https://wa.me/{n}"))
            .ok_or_else(missing),
        SupportChannel::Web => contact
            .url
            .clone()
            .filter(|u| u.starts_with("https://") && !u.contains(char::is_whitespace))
            .ok_or_else(missing),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn support_contact(state: State<'_, SharedState>) -> Result<SupportContact, AppError> {
    let st = state.inner().clone();
    blocking(move || contact(&st)).await
}

/// Opens the support page or WhatsApp in the system browser. Offline, the browser says so;
/// nothing here waits for the network.
#[tauri::command]
#[specta::specta]
pub async fn support_open(
    app: AppHandle,
    state: State<'_, SharedState>,
    channel: SupportChannel,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || {
        let url = support_url(&contact(&st)?, channel)?;
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| AppError::Io(e.to_string()))
    })
    .await
}
