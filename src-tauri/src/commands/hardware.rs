//! Printers, drawer and scanner settings of this PC; the no-sale drawer (AUD-009); retrying a
//! receipt that failed after a sale.

use super::blocking;
use crate::printing::hw_error;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::cash;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_hw::PrintOutcome;
use pos_hw::config::{HardwareConfig, PrinterConfig};
use pos_hw::discover::{Discovered, discover};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::{AppHandle, State};

/// Longest no-sale note kept in the journal, like the notes of cash movements.
const NOTE_MAX: usize = 200;

/// Why the drawer opens without a sale; `other` needs a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoSaleReason {
    ChangeRequest,
    Check,
    Other,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoSaleInput {
    pub reason: NoSaleReason,
    pub note: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn hardware_get(state: State<'_, SharedState>) -> Result<HardwareConfig, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st)).await
}

/// Saved, the devices restarted as needed, then the customer screen moved, opened or closed (a
/// window that cannot open is logged: the saved settings stand).
#[tauri::command]
#[specta::specta]
pub async fn hardware_save(
    app: AppHandle,
    state: State<'_, SharedState>,
    config: HardwareConfig,
) -> Result<HardwareConfig, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        let saved = save(&st, config)?;
        if let Err(error) = crate::customer_display::sync(&app, &saved.display.screen) {
            tracing::warn!(%error, "the customer screen did not follow the new settings");
        }
        Ok(saved)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn hardware_discover(state: State<'_, SharedState>) -> Result<Vec<Discovered>, AppError> {
    let st = state.inner().clone();
    blocking(move || discover_devices(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn hardware_test_print(
    state: State<'_, SharedState>,
    printer: PrinterConfig,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || test_print(&st, printer, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn hardware_open_drawer(
    state: State<'_, SharedState>,
    input: NoSaleInput,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || open_drawer(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn hardware_retry_print(
    state: State<'_, SharedState>,
    document_id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || retry_print(&st, document_id)).await
}

/// Any signed-in user: the till needs the scanner layout and the receipt options.
pub(crate) fn get(st: &AppState) -> Result<HardwareConfig, AppError> {
    st.session()?;
    st.hw.config().map_err(hw_error)
}

/// Saved to `hardware.json`, the scale, pole display and serial scanner whose settings changed
/// restarted, then journaled with the settings before and after (AUD-007).
pub(crate) fn save(st: &AppState, config: HardwareConfig) -> Result<HardwareConfig, AppError> {
    let session = st.require(Permission::SettingsHardware)?;
    let before = st.hw.config().map_err(hw_error)?;
    let saved = st.hw.save(config).map_err(hw_error)?;
    st.devices.apply(&saved, st.hw.links());
    let actor = session.actor();
    let payload = json!({ "before": before, "after": saved });
    st.db.write(move |tx| {
        pos_core::audit::append(
            tx,
            &actor.event("hardware.updated", "hardware", None, payload),
        )?;
        Ok(())
    })?;
    Ok(saved)
}

pub(crate) fn discover_devices(st: &AppState) -> Result<Vec<Discovered>, AppError> {
    st.require(Permission::SettingsHardware)?;
    Ok(discover())
}

/// A test page on `printer`, saved or not yet (the wizard tests before saving).
pub(crate) fn test_print(
    st: &AppState,
    printer: PrinterConfig,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    st.require(Permission::SettingsHardware)?;
    st.hw.test_page(&printer, lang).map_err(hw_error)
}

/// Journaled before the kick: the attempt is what AUD-009 wants on record.
pub(crate) fn open_drawer(st: &AppState, input: NoSaleInput) -> Result<(), AppError> {
    let ctx = st.till(Permission::CashPaidInOut)?;
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned);
    if input.reason == NoSaleReason::Other && note.is_none() {
        return Err(AppError::validation("note", "required"));
    }
    if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_MAX) {
        return Err(AppError::validation("note", "too_long"));
    }
    let terminal_id = ctx.session.terminal_id.clone();
    let write_ctx = ctx.clone();
    st.db.write(move |tx| {
        let session = cash::require_open(tx, &terminal_id)?;
        let approved_by = write_ctx
            .approved_by(Permission::CashPaidInOut)
            .map(str::to_owned);
        let payload = json!({ "reason": input.reason, "note": note, "approvedBy": approved_by });
        pos_core::audit::append(
            tx,
            &write_ctx
                .actor()
                .event("drawer.opened", "cash_session", Some(&session.id), payload),
        )?;
        Ok(())
    })?;
    st.spend_approvals(&ctx)?;
    st.hw.open_drawer().map(|_| ()).map_err(hw_error)
}

/// The original again, once, after its print failed (`print_not_pending` otherwise).
pub(crate) fn retry_print(st: &AppState, document_id: String) -> Result<(), AppError> {
    st.require_any(&[Permission::SalesCreate, Permission::SalesReprint])?;
    st.printer.retry(&document_id)
}
