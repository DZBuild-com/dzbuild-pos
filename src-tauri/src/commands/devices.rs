//! Scale, displays and printer status for the till and the settings (HW-008/009/030/042/043).
//! The device threads run on their own (`crate::devices`); these commands read them, send them a
//! frame or a tare, and never wait longer than their bound (3 s for a stable weight, 2 s for a VFD
//! test, the printer's own timeout for a status).

use super::blocking;
use crate::devices::DeviceStatus;
use crate::printing::hw_error;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_hw::config::LabelPrinterConfig;
use pos_hw::display::{DisplayFrame, DisplayService, VfdConfig};
use pos_hw::scale::{ScaleProtocol, WeightEvent};
use pos_hw::serial::SerialSettings;
use pos_hw::{PrintOutcome, PrinterIssue, PrinterStatus};
use serde::Serialize;
use specta::Type;
use std::time::Duration;
use tauri::{AppHandle, State};

/// The till weighs, the settings test: either permission.
const TILL_OR_SETTINGS: &[Permission] = &[Permission::SalesCreate, Permission::SettingsHardware];

/// How long `probe` listens for each protocol and speed.
const PROBE_PER_TRY: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleProbe {
    pub protocol: ScaleProtocol,
    pub serial: SerialSettings,
}

/// The shop as the customer screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisplayShop {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    /// `data:` URL of the shop logo, when shown.
    pub logo: Option<String>,
    pub show_logo: bool,
    pub thanks_seconds: u8,
    /// "Commandez en ligne": the linked DZBuild store and its QR, when the owner prints it.
    pub store_url: Option<String>,
    pub store_qr: Option<pos_docs::a4::qr::QrMatrix>,
}

/// Any signed-in user: the top bar's device dots.
#[tauri::command]
#[specta::specta]
pub async fn devices_status(state: State<'_, SharedState>) -> Result<Vec<DeviceStatus>, AppError> {
    let st = state.inner().clone();
    blocking(move || status(&st)).await
}

/// The scale's last reading; `None` without a scale or before its first reading.
#[tauri::command]
#[specta::specta]
pub async fn scale_latest(state: State<'_, SharedState>) -> Result<Option<WeightEvent>, AppError> {
    let st = state.inner().clone();
    blocking(move || latest(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn scale_tare(state: State<'_, SharedState>) -> Result<WeightEvent, AppError> {
    let st = state.inner().clone();
    blocking(move || tare(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn scale_clear_tare(
    state: State<'_, SharedState>,
) -> Result<Option<WeightEvent>, AppError> {
    let st = state.inner().clone();
    blocking(move || clear_tare(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn scale_zero(state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || zero(&st)).await
}

/// The next stable weight within `timeout_ms` (100 to 3 000 ms).
#[tauri::command]
#[specta::specta]
pub async fn scale_read_stable(
    state: State<'_, SharedState>,
    timeout_ms: u32,
) -> Result<WeightEvent, AppError> {
    let st = state.inner().clone();
    blocking(move || read_stable(&st, timeout_ms)).await
}

/// The settings' "Detect": the protocol and speed a scale on `port` answers to.
#[tauri::command]
#[specta::specta]
pub async fn scale_probe(
    state: State<'_, SharedState>,
    port: String,
) -> Result<Option<ScaleProbe>, AppError> {
    let st = state.inner().clone();
    blocking(move || probe(&st, port)).await
}

/// The till's frame: to the pole display and to the customer screen.
#[tauri::command]
#[specta::specta]
pub async fn display_show(
    app: AppHandle,
    state: State<'_, SharedState>,
    frame: DisplayFrame,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || {
        show(&st, frame.clone())?;
        crate::events::to_customer_display(&app, frame);
        Ok(())
    })
    .await
}

/// No session: the customer screen asks for it when it opens.
#[tauri::command]
#[specta::specta]
pub async fn display_current(
    state: State<'_, SharedState>,
) -> Result<Option<DisplayFrame>, AppError> {
    let st = state.inner().clone();
    blocking(move || current(&st)).await
}

/// No session: the customer screen's names and logo.
#[tauri::command]
#[specta::specta]
pub async fn display_shop(state: State<'_, SharedState>) -> Result<DisplayShop, AppError> {
    let st = state.inner().clone();
    blocking(move || shop_branding(&st)).await
}

/// The test pattern on a pole display, saved or not yet.
#[tauri::command]
#[specta::specta]
pub async fn display_test_vfd(
    state: State<'_, SharedState>,
    config: VfdConfig,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || test_vfd(&st, config)).await
}

#[tauri::command]
#[specta::specta]
pub async fn display_monitors(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<Vec<crate::customer_display::MonitorInfo>, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        may_manage_screen(&st)?;
        crate::customer_display::monitors(&app)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn display_open_preview(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || {
        may_manage_screen(&st)?;
        crate::customer_display::open_preview(&app)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn display_close(app: AppHandle, state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || {
        may_manage_screen(&st)?;
        crate::customer_display::close(&app)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn hardware_printer_status(
    state: State<'_, SharedState>,
    printer_id: String,
) -> Result<PrinterStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || printer_status(&st, printer_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn hardware_doctor(state: State<'_, SharedState>) -> Result<Vec<PrinterIssue>, AppError> {
    let st = state.inner().clone();
    blocking(move || doctor(&st)).await
}

/// A test label on `printer`, saved or not yet.
#[tauri::command]
#[specta::specta]
pub async fn hardware_test_label(
    state: State<'_, SharedState>,
    printer: LabelPrinterConfig,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || test_label(&st, printer, lang)).await
}

pub(crate) fn status(st: &AppState) -> Result<Vec<DeviceStatus>, AppError> {
    st.session()?;
    Ok(st.devices.states())
}

pub(crate) fn latest(st: &AppState) -> Result<Option<WeightEvent>, AppError> {
    st.require_any(TILL_OR_SETTINGS)?;
    match st.devices.with_scale(|s| Ok(s.latest())) {
        Err(pos_hw::HwError::ScaleNotConfigured) => Ok(None),
        other => other.map_err(hw_error),
    }
}

pub(crate) fn tare(st: &AppState) -> Result<WeightEvent, AppError> {
    st.require_any(TILL_OR_SETTINGS)?;
    st.devices.with_scale(|s| s.tare()).map_err(hw_error)
}

pub(crate) fn clear_tare(st: &AppState) -> Result<Option<WeightEvent>, AppError> {
    st.require_any(TILL_OR_SETTINGS)?;
    st.devices
        .with_scale(|s| Ok(s.clear_tare()))
        .map_err(hw_error)
}

pub(crate) fn zero(st: &AppState) -> Result<(), AppError> {
    st.require_any(TILL_OR_SETTINGS)?;
    st.devices.with_scale(|s| s.zero()).map_err(hw_error)
}

pub(crate) fn read_stable(st: &AppState, timeout_ms: u32) -> Result<WeightEvent, AppError> {
    st.require(Permission::SalesCreate)?;
    let timeout = Duration::from_millis(u64::from(timeout_ms.clamp(100, 3_000)));
    st.devices
        .with_scale(|s| s.read_stable(timeout))
        .map_err(hw_error)
}

/// The scale's own service gives the port up while the probe runs, then starts again.
pub(crate) fn probe(st: &AppState, port: String) -> Result<Option<ScaleProbe>, AppError> {
    st.require(Permission::SettingsHardware)?;
    let links = st.hw.links();
    let found = st.devices.with_port_free(&port, links.clone(), || {
        pos_hw::scale::probe(links.as_ref(), &port, PROBE_PER_TRY)
    });
    Ok(found.map(|(protocol, serial)| ScaleProbe { protocol, serial }))
}

pub(crate) fn show(st: &AppState, frame: DisplayFrame) -> Result<(), AppError> {
    st.require(Permission::SalesCreate)?;
    frame.validate().map_err(hw_error)?;
    st.devices.show(frame);
    Ok(())
}

pub(crate) fn current(st: &AppState) -> Result<Option<DisplayFrame>, AppError> {
    Ok(st.devices.current_frame())
}

pub(crate) fn shop_branding(st: &AppState) -> Result<DisplayShop, AppError> {
    let screen = st.hw.config().map_err(hw_error)?.display.screen;
    let (company, logo, url) = st.db.read(|c| {
        Ok((
            pos_core::company::get(c)?,
            pos_core::company::logo(c)?,
            pos_core::cloud::store::receipt_url(c)?,
        ))
    })?;
    let company = company.ok_or_else(|| AppError::conflict("setup_required"))?;
    let logo = logo
        .filter(|_| screen.show_logo)
        .map(|bytes| data_url(&bytes));
    Ok(DisplayShop {
        name_ar: company.name_ar,
        name_fr: company.name_fr,
        name_en: company.name_en,
        logo,
        show_logo: screen.show_logo,
        thanks_seconds: screen.thanks_seconds,
        store_qr: url.as_deref().and_then(pos_docs::a4::qr::qr),
        store_url: url,
    })
}

/// The logo (a PNG or a JPEG, `company_set_logo`) as a `data:` URL the window's CSP allows.
fn data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        "image/jpeg"
    };
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// The running pole display on the same port pauses for the test.
pub(crate) fn test_vfd(st: &AppState, config: VfdConfig) -> Result<(), AppError> {
    st.require(Permission::SettingsHardware)?;
    let links = st.hw.links();
    let port = config.serial.port.clone();
    st.devices.with_port_free(&port, links.clone(), || {
        let service = DisplayService::start(Some(config), links, pos_hw::device::silent_status())
            .map_err(hw_error)?;
        let tested = service.test().map_err(hw_error);
        service.stop();
        tested
    })
}

/// The window commands check this before they touch the customer screen.
pub(crate) fn may_manage_screen(st: &AppState) -> Result<(), AppError> {
    st.require(Permission::SettingsHardware).map(drop)
}

pub(crate) fn printer_status(st: &AppState, printer_id: String) -> Result<PrinterStatus, AppError> {
    st.require_any(TILL_OR_SETTINGS)?;
    st.hw.printer_status(&printer_id).map_err(hw_error)
}

pub(crate) fn doctor(st: &AppState) -> Result<Vec<PrinterIssue>, AppError> {
    st.require(Permission::SettingsHardware)?;
    st.hw.doctor().map_err(hw_error)
}

pub(crate) fn test_label(
    st: &AppState,
    printer: LabelPrinterConfig,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    st.require(Permission::SettingsHardware)?;
    st.hw.label_test(&printer, lang).map_err(hw_error)
}
