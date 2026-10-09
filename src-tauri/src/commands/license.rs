//! The licence commands. `license_status`, `license_activate` and
//! `license_import_file` work before the setup and without a session (the activation screen runs
//! before the shop exists); `license_remove` needs the owner and the PIN again. The checks are
//! `pos_license`'s, through `AppState.license`.

use super::blocking;
use crate::license::{LicenseSeller, qr_text, seller};
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission};
use pos_core::error::AppError;
use pos_docs::a4::qr::{QrMatrix, qr};
use pos_license::codes;
use pos_license::key::{MAX_FILE_LEN, MAX_KEY_LEN};
use pos_license::state::{Licence, LicenseState};
use serde::Serialize;
use specta::Type;
use std::path::Path;
use tauri::{AppHandle, State, Window};

/// A trial shows the orange banner from this many days left.
pub const WARN_DAYS: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LicenseStateKind {
    Demo,
    Unlicensed,
    Active,
    Expired,
    Refused,
    OtherDevice,
    Tampered,
    DeviceUnreadable,
}

/// What the activation screen, the banner and Settings › Licence show.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub state: LicenseStateKind,
    pub can_write: bool,
    /// The code a write answers now (`license_expired`…); `None` while writes are allowed.
    pub refusal: Option<String>,
    pub device_code: Option<String>,
    pub licence: Option<Licence>,
    /// A trial's whole days left (0 once ended); `None` for a lifetime licence or none.
    pub days_left: Option<i64>,
    /// An active trial with [`WARN_DAYS`] days or fewer left.
    pub warn: bool,
    /// The other PC's code when only its licence was found here (this PC changed).
    pub previous_device: Option<String>,
    pub seller: LicenseSeller,
    /// What the customer scans with a phone: a WhatsApp message to the seller with this PC's code,
    /// or the code itself (`license::qr_text`).
    pub qr: Option<QrMatrix>,
}

pub(crate) fn status_of(
    state: &LicenseState,
    device_code: Option<&str>,
    high_water: i64,
) -> LicenseStatus {
    let kind = match state {
        LicenseState::Demo => LicenseStateKind::Demo,
        LicenseState::Unlicensed => LicenseStateKind::Unlicensed,
        LicenseState::Active(_) => LicenseStateKind::Active,
        LicenseState::Expired(_) => LicenseStateKind::Expired,
        LicenseState::Refused { .. } => LicenseStateKind::Refused,
        LicenseState::OtherDevice { .. } => LicenseStateKind::OtherDevice,
        LicenseState::Tampered => LicenseStateKind::Tampered,
        LicenseState::DeviceUnreadable => LicenseStateKind::DeviceUnreadable,
    };
    let licence = state.licence().cloned();
    let days_left = licence.as_ref().and_then(|l| l.days_left(high_water));
    let seller = seller();
    LicenseStatus {
        state: kind,
        can_write: state.allows_writes(),
        refusal: state.refusal().map(|e| e.code()),
        device_code: device_code.map(str::to_owned),
        warn: matches!(state, LicenseState::Active(_)) && days_left.is_some_and(|d| d <= WARN_DAYS),
        days_left,
        licence,
        previous_device: match state {
            LicenseState::OtherDevice { previous } => Some(previous.clone()),
            _ => None,
        },
        qr: device_code.and_then(|code| qr(&qr_text(&seller, code))),
        seller,
    }
}

pub(crate) fn status(st: &AppState) -> LicenseStatus {
    let now = st.now();
    status_of(
        &st.license.state(now),
        st.license.device_code(),
        st.license.high_water(now),
    )
}

/// Activates a pasted key (whoever is at the screen: before the setup nobody is signed in).
pub(crate) fn activate(st: &AppState, key: String) -> Result<LicenseStatus, AppError> {
    if key.len() > MAX_KEY_LEN {
        return Err(AppError::conflict(codes::INVALID));
    }
    let session = st.current_session()?;
    st.license
        .activate(&st.db, &st.actor(session.as_ref()), st.now(), &key)?;
    Ok(status(st))
}

/// Activates the key of a `.dzlic` file the Rust dialog picked.
pub(crate) fn import_from(st: &AppState, path: &Path) -> Result<LicenseStatus, AppError> {
    if std::fs::metadata(path)?.len() > MAX_FILE_LEN {
        return Err(AppError::conflict(codes::INVALID));
    }
    let text = std::fs::read_to_string(path).map_err(|_| AppError::conflict(codes::INVALID))?;
    activate(st, text)
}

/// The owner removes the licence of this PC (it then reads only), PIN typed again.
pub(crate) fn remove(st: &AppState, pin: String) -> Result<LicenseStatus, AppError> {
    let session = st.require_owner(Permission::SettingsData)?;
    auth::confirm_pin(&st.db, &st.terminal, &session, &pin, st.now())?;
    st.license.remove(&st.db, &session.actor(), st.now())?;
    Ok(status(st))
}

#[tauri::command]
#[specta::specta]
pub async fn license_status(state: State<'_, SharedState>) -> Result<LicenseStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || Ok(status(&st))).await
}

#[tauri::command]
#[specta::specta]
pub async fn license_activate(
    state: State<'_, SharedState>,
    key: String,
) -> Result<LicenseStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || activate(&st, key)).await
}

/// Rust's file dialog (`.dzlic`); `None` when it is cancelled.
#[tauri::command]
#[specta::specta]
pub async fn license_import_file(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<LicenseStatus>, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        let Some(path) = super::pick_dialog(&app, &window, "DZBuild POS", &["dzlic"])? else {
            return Ok(None);
        };
        import_from(&st, &path).map(Some)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn license_remove(
    state: State<'_, SharedState>,
    pin: String,
) -> Result<LicenseStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || remove(&st, pin)).await
}
