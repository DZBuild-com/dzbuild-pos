//! The MVP completion rows (Task 16): custom payment types such as the prélèvement CCP
//! (PAY-006/007), a role or a user made on the model of another (USR-015), the variant grid
//! (CAT-012) and the shop's BaridiMob QR for the customer screen (HW-042).

use super::blocking;
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission, Role, User, roles, users};
use pos_core::catalog::variant_grid::{self, VariantGrid};
use pos_core::error::AppError;
use pos_core::payment_types::{self, PaymentType, PaymentTypeInput};
use pos_docs::a4::qr::QrMatrix;
use serde::Deserialize;
use specta::Type;
use tauri::State;

/// The three names of a new role (the French one at least).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoleNames {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
}

#[tauri::command]
#[specta::specta]
pub async fn payment_types_list(
    state: State<'_, SharedState>,
    include_archived: bool,
) -> Result<Vec<PaymentType>, AppError> {
    let st = state.inner().clone();
    blocking(move || list_types(&st, include_archived)).await
}

/// Creates the type when `id` is `None`, else changes it.
#[tauri::command]
#[specta::specta]
pub async fn payment_type_save(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: PaymentTypeInput,
) -> Result<PaymentType, AppError> {
    let st = state.inner().clone();
    blocking(move || save_type(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn payment_type_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || archive_type(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn roles_duplicate(
    state: State<'_, SharedState>,
    role_id: String,
    names: RoleNames,
) -> Result<Role, AppError> {
    let st = state.inner().clone();
    blocking(move || duplicate_role(&st, role_id, names)).await
}

/// A new user with the role, language and discount limit of `templateId`, their own name and
/// PIN.
#[tauri::command]
#[specta::specta]
pub async fn users_duplicate(
    state: State<'_, SharedState>,
    template_id: String,
    display_name: String,
    pin: String,
) -> Result<User, AppError> {
    let st = state.inner().clone();
    blocking(move || duplicate_user(&st, template_id, display_name, pin)).await
}

/// The variants of `parentId` as a grid (rows × columns of two axes), with the stock of
/// `locationId` or of every location.
#[tauri::command]
#[specta::specta]
pub async fn variant_grid(
    state: State<'_, SharedState>,
    parent_id: String,
    location_id: Option<String>,
    row_axis: Option<String>,
) -> Result<VariantGrid, AppError> {
    let st = state.inner().clone();
    blocking(move || grid(&st, parent_id, location_id, row_axis)).await
}

#[tauri::command]
#[specta::specta]
pub async fn payments_baridpay_qr(
    state: State<'_, SharedState>,
) -> Result<Option<QrMatrix>, AppError> {
    let st = state.inner().clone();
    blocking(move || baridpay_qr(&st)).await
}

/// The payment screen's tiles (the till) and the settings list.
pub(crate) fn list_types(
    st: &AppState,
    include_archived: bool,
) -> Result<Vec<PaymentType>, AppError> {
    st.require_any(&[Permission::SalesCreate, Permission::SettingsCompany])?;
    st.db
        .read(move |c| payment_types::list(c, include_archived))
}

pub(crate) fn save_type(
    st: &AppState,
    id: Option<String>,
    input: PaymentTypeInput,
) -> Result<PaymentType, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| match id {
        Some(id) => payment_types::update(tx, &actor, &id, input),
        None => payment_types::create(tx, &actor, input),
    })
}

/// Archived, never deleted: the tickets that used it keep it in the report.
pub(crate) fn archive_type(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| payment_types::archive(tx, &actor, &id))
}

/// A custom role is the owner's alone, like M0's `roles_create` (spec §4.5).
pub(crate) fn duplicate_role(
    st: &AppState,
    role_id: String,
    names: RoleNames,
) -> Result<Role, AppError> {
    let actor = st.require_owner(Permission::SettingsUsers)?.actor();
    let role = st.db.write(move |tx| {
        roles::duplicate(
            tx,
            &actor,
            &role_id,
            (&names.name_ar, &names.name_fr, &names.name_en),
        )
    })?;
    st.refresh_session()?;
    Ok(role)
}

/// The PIN follows M0's rules and is hashed (Argon2id) like `users_create`'s.
pub(crate) fn duplicate_user(
    st: &AppState,
    template_id: String,
    display_name: String,
    pin: String,
) -> Result<User, AppError> {
    let actor = st.require(Permission::SettingsUsers)?.actor();
    let hash = auth::hash_pin(&pin)?;
    st.db
        .write(move |tx| users::duplicate(tx, &actor, &template_id, &display_name, &hash))
}

pub(crate) fn grid(
    st: &AppState,
    parent_id: String,
    location_id: Option<String>,
    row_axis: Option<String>,
) -> Result<VariantGrid, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(move |c| {
        variant_grid::grid(c, &parent_id, location_id.as_deref(), row_axis.as_deref())
    })
}

/// The shop's BaridiMob payment QR as a module matrix (`None` until the owner sets its text).
pub(crate) fn baridpay_qr(st: &AppState) -> Result<Option<QrMatrix>, AppError> {
    st.require(Permission::SalesCreate)?;
    let text = st.db.read(payment_types::baridpay_qr)?;
    Ok(text.and_then(|t| pos_docs::a4::qr::qr(&t)))
}
