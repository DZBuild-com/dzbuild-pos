//! DZBuild link and sync commands (spec §4.7). Linking and unlinking are the owner's; the status
//! is everyone's; nothing here is on the path of a sale. A command that needs DZBuild answers
//! `cloud_unavailable` offline and changes nothing locally.

use super::blocking;
use crate::cloud::loopback_pages;
use crate::state::{AppState, SharedState};
use pos_core::auth::{self, Permission, Session};
use pos_core::cloud::api;
use pos_core::cloud::config::check_url;
use pos_core::cloud::engine::SyncStatus;
use pos_core::cloud::link::{self, AttemptView, UnlinkOutcome};
use pos_core::cloud::outbox::{self, FailedItem};
use pos_core::cloud::publish::{self, PublishQuery, PublishRow};
use pos_core::cloud::pull::{self, ImportReport, PullContext};
use pos_core::cloud::store::{self, CloudLink, CloudNotice, LinkStatus};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::terminal::TerminalRole;
use pos_docs::a4::qr::{QrMatrix, qr};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LinkStart {
    pub attempt: AttemptView,
    /// The phone flow: the verification link as a QR the owner scans.
    pub qr: Option<QrMatrix>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CloudPage {
    /// Create a DZBuild account (with the POS campaign).
    Signup,
    /// DZBuild's connected devices page (revoke this till from there too).
    Devices,
    /// The linked online store.
    Store,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PublishChange {
    pub product_ids: Vec<String>,
    pub publish: bool,
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub id: String,
    pub name: Option<String>,
    pub app_version: Option<String>,
    pub last_seen_at: Option<i64>,
    pub this_till: bool,
}

/// The link and sync state (the top bar's dot, Settings › DZBuild): any signed-in user.
#[tauri::command]
#[specta::specta]
pub async fn cloud_status(state: State<'_, SharedState>) -> Result<SyncStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || status(&st)).await
}

/// Starts the browser sign-in and opens the system browser on DZBuild's page.
#[tauri::command]
#[specta::specta]
pub async fn cloud_link_start(
    state: State<'_, SharedState>,
    lang: Lang,
) -> Result<LinkStart, AppError> {
    let st = state.inner().clone();
    blocking(move || link_start(&st, lang)).await
}

/// Starts the phone sign-in: a code and its QR, approved on DZBuild from a phone.
#[tauri::command]
#[specta::specta]
pub async fn cloud_device_flow_start(state: State<'_, SharedState>) -> Result<LinkStart, AppError> {
    let st = state.inner().clone();
    blocking(move || device_flow_start(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_link_poll(
    state: State<'_, SharedState>,
) -> Result<Option<AttemptView>, AppError> {
    let st = state.inner().clone();
    blocking(move || link_poll(&st)).await
}

/// Links the store the owner chose (fetches its profile, registers this till: seconds on a slow
/// line).
#[tauri::command]
#[specta::specta]
pub async fn cloud_link_choose(
    state: State<'_, SharedState>,
    store_id: String,
) -> Result<CloudLink, AppError> {
    let st = state.inner().clone();
    blocking(move || link_choose(&st, store_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_link_cancel(state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || link_cancel(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_unlink(
    state: State<'_, SharedState>,
    pin: String,
) -> Result<UnlinkOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || unlink(&st, pin)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_sync_now(state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || sync_now(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_notices(state: State<'_, SharedState>) -> Result<Vec<CloudNotice>, AppError> {
    let st = state.inner().clone();
    blocking(move || notices(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_notice_dismiss(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || notice_dismiss(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_outbox_failed(
    state: State<'_, SharedState>,
) -> Result<Vec<FailedItem>, AppError> {
    let st = state.inner().clone();
    blocking(move || outbox_failed(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_outbox_retry(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || outbox_retry(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_publish_list(
    state: State<'_, SharedState>,
    query: PublishQuery,
) -> Result<Vec<PublishRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || publish_list(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_publish_set(
    state: State<'_, SharedState>,
    change: PublishChange,
) -> Result<u32, AppError> {
    let st = state.inner().clone();
    blocking(move || publish_set(&st, change)).await
}

/// "Recaler le stock en ligne": the next sync sends the local level of `product_ids` (every
/// product sent so far when `None`).
#[tauri::command]
#[specta::specta]
pub async fn cloud_stock_reconcile(
    state: State<'_, SharedState>,
    product_ids: Option<Vec<String>>,
) -> Result<u32, AppError> {
    let st = state.inner().clone();
    blocking(move || stock_reconcile(&st, product_ids)).await
}

/// "Importer les produits de ma boutique en ligne" (a store that already sells online).
#[tauri::command]
#[specta::specta]
pub async fn cloud_import_online(state: State<'_, SharedState>) -> Result<ImportReport, AppError> {
    let st = state.inner().clone();
    blocking(move || import_online(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_devices(state: State<'_, SharedState>) -> Result<Vec<DeviceRow>, AppError> {
    let st = state.inner().clone();
    blocking(move || devices(&st)).await
}

/// Opens a DZBuild page in the system browser (never in the app's window).
#[tauri::command]
#[specta::specta]
pub async fn cloud_open_page(
    state: State<'_, SharedState>,
    page: CloudPage,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || open_page(&st, page)).await
}

const PUBLISHERS: &[Permission] = &[Permission::SettingsDzbuild, Permission::ProductsEdit];

pub(crate) fn status(st: &AppState) -> Result<SyncStatus, AppError> {
    st.session()?;
    let mut status = st.cloud.as_ref().map(|c| c.status()).unwrap_or_default();
    // `cloud.*` settings are the main PC's: a till names it, read-only
    let lan = st.db.read(pos_core::sync::role::current)?;
    if lan.role == TerminalRole::Client {
        status.main_pc = Some(lan.host_name.or(lan.host_address).unwrap_or_default());
    }
    Ok(status)
}

fn owner(st: &AppState) -> Result<Session, AppError> {
    st.require_owner(Permission::SettingsDzbuild)
}

/// The DZBuild link runs on the main PC only (spec §4.6, LAN-012): a till's cloud tables are its
/// own and never reach the main PC, and its engine never starts, so linking, importing,
/// publishing, a reconcile or a retry there is refused (`host_required`).
pub(crate) fn main_pc(st: &AppState) -> Result<(), AppError> {
    if st.db.read(pos_core::sync::role::is_client)? {
        return Err(AppError::conflict("host_required"));
    }
    Ok(())
}

/// The linked store (also waiting for a relink, or revoked), else `None`.
fn active_store(st: &AppState) -> Result<Option<String>, AppError> {
    Ok(st.db.read(store::active_link)?.map(|l| l.store_id))
}

fn store_id(st: &AppState) -> Result<String, AppError> {
    active_store(st)?.ok_or_else(|| AppError::sync("cloud_not_linked"))
}

pub(crate) fn link_start(st: &AppState, lang: Lang) -> Result<LinkStart, AppError> {
    st.demo_guard()?;
    let session = owner(st)?;
    main_pc(st)?;
    let cloud = st.cloud()?;
    cloud.clear_attempt();
    let attempt = link::start_browser(
        cloud.ctx.clone(),
        session.actor(),
        lang,
        loopback_pages(lang),
    )?;
    cloud.set_attempt(attempt.clone());
    let view = attempt.view();
    if let Some(url) = view.authorize_url.as_deref()
        && let Err(error) = cloud.open_url(url)
    {
        // the wizard shows the link with "Copier" when no browser opened
        tracing::warn!(target: "cloud", code = %error.code(), "the browser did not open for the link");
    }
    Ok(LinkStart {
        attempt: view,
        qr: None,
    })
}

pub(crate) fn device_flow_start(st: &AppState) -> Result<LinkStart, AppError> {
    st.demo_guard()?;
    let session = owner(st)?;
    main_pc(st)?;
    let cloud = st.cloud()?;
    cloud.clear_attempt();
    let attempt = link::start_device(cloud.ctx.clone(), session.actor())?;
    cloud.set_attempt(attempt.clone());
    let view = attempt.view();
    let target = view.device.as_ref().map(|d| {
        d.verification_uri_complete
            .clone()
            .unwrap_or_else(|| d.verification_uri.clone())
    });
    Ok(LinkStart {
        qr: target.as_deref().and_then(qr),
        attempt: view,
    })
}

pub(crate) fn link_poll(st: &AppState) -> Result<Option<AttemptView>, AppError> {
    owner(st)?;
    Ok(st.cloud()?.attempt().map(|a| a.view()))
}

pub(crate) fn link_choose(st: &AppState, store_id: String) -> Result<CloudLink, AppError> {
    owner(st)?;
    main_pc(st)?;
    let cloud = st.cloud()?;
    let attempt = cloud
        .attempt()
        .ok_or_else(|| AppError::sync("cloud_attempt_not_found"))?;
    link::choose_store(&cloud.ctx, &attempt, &store_id)
}

pub(crate) fn link_cancel(st: &AppState) -> Result<(), AppError> {
    owner(st)?;
    st.cloud()?.clear_attempt();
    Ok(())
}

/// Owner + PIN. The engine stops first (its token manager must not refresh while the refresh
/// token is revoked), then DZBuild is told (device deleted, token revoked — best effort
/// offline), the token is deleted, the link row kept as `unlinked`; the engine resumes idle.
pub(crate) fn unlink(st: &AppState, pin: String) -> Result<UnlinkOutcome, AppError> {
    let session = owner(st)?;
    auth::confirm_pin(&st.db, &st.terminal, &session, &pin, st.now())?;
    let cloud = st.cloud()?;
    cloud.clear_attempt();
    cloud.stop();
    let outcome = link::unlink(&cloud.ctx, &session.actor());
    if let Err(error) = cloud.resume() {
        tracing::error!(target: "cloud", code = %error.code(), "the sync engine did not resume");
    }
    outcome
}

pub(crate) fn sync_now(st: &AppState) -> Result<(), AppError> {
    st.require_any(&[Permission::SettingsDzbuild, Permission::SalesCreate])?;
    st.cloud()?.wake();
    Ok(())
}

pub(crate) fn notices(st: &AppState) -> Result<Vec<CloudNotice>, AppError> {
    st.require(Permission::SettingsDzbuild)?;
    st.db.read(|c| store::notices(c, false))
}

pub(crate) fn notice_dismiss(st: &AppState, id: String) -> Result<(), AppError> {
    st.require(Permission::SettingsDzbuild)?;
    let now = st.now();
    st.db.write(move |tx| store::dismiss_notice(tx, &id, now))
}

pub(crate) fn outbox_failed(st: &AppState) -> Result<Vec<FailedItem>, AppError> {
    st.require(Permission::SettingsDzbuild)?;
    let Some(store) = active_store(st)? else {
        return Ok(Vec::new());
    };
    st.db.read(move |c| outbox::failed(c, &store, 200))
}

pub(crate) fn outbox_retry(st: &AppState, id: String) -> Result<(), AppError> {
    st.require(Permission::SettingsDzbuild)?;
    main_pc(st)?;
    let now = st.now();
    st.db.write(move |tx| outbox::retry(tx, &id, now))?;
    wake(st);
    Ok(())
}

pub(crate) fn publish_list(
    st: &AppState,
    query: PublishQuery,
) -> Result<Vec<PublishRow>, AppError> {
    st.require_any(PUBLISHERS)?;
    let store = active_store(st)?.unwrap_or_default();
    st.db.read(move |c| publish::list(c, &store, &query))
}

pub(crate) fn publish_set(st: &AppState, change: PublishChange) -> Result<u32, AppError> {
    let session = st.require_any(PUBLISHERS)?;
    main_pc(st)?;
    let (actor, now) = (session.actor(), st.now());
    let n = st.db.write(move |tx| {
        publish::set(
            tx,
            &actor,
            &change.product_ids,
            change.publish,
            change.draft,
            now,
        )
    })?;
    wake(st);
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

pub(crate) fn stock_reconcile(
    st: &AppState,
    product_ids: Option<Vec<String>>,
) -> Result<u32, AppError> {
    st.require(Permission::SettingsDzbuild)?;
    main_pc(st)?;
    let store = store_id(st)?;
    let now = st.now();
    let n = st.db.write(move |tx| {
        pos_core::cloud::stock::request_reset(tx, &store, product_ids.as_deref(), now)
    })?;
    wake(st);
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

pub(crate) fn import_online(st: &AppState) -> Result<ImportReport, AppError> {
    let session = owner(st)?;
    main_pc(st)?;
    let cloud = st.cloud()?;
    let remote = cloud.remote()?;
    let link = st
        .db
        .read(store::active_link)?
        .ok_or_else(|| AppError::sync("cloud_not_linked"))?;
    let settings = st.db.read(store::cloud_settings)?;
    let report = pull::import_online(&PullContext {
        remote: &remote,
        db: &st.db,
        link: &link,
        settings: &settings,
        actor: session.actor(),
        now: st.now(),
    })
    .map_err(|e| e.to_app())?;
    cloud.wake();
    Ok(report)
}

pub(crate) fn devices(st: &AppState) -> Result<Vec<DeviceRow>, AppError> {
    st.require(Permission::SettingsDzbuild)?;
    let cloud = st.cloud()?;
    let remote = cloud.remote()?;
    let mine = st.db.read(store::active_link)?.and_then(|l| l.device_id);
    // a `GET /v1/devices` DZBuild has not shipped yet is `cloud_not_ready`
    let list = api::list_devices(&remote).map_err(|e| e.to_app())?;
    Ok(list
        .into_iter()
        .map(|d| DeviceRow {
            this_till: mine.as_deref() == Some(d.id.as_str()),
            id: d.id,
            name: d.name,
            app_version: d.app_version,
            last_seen_at: d.last_seen_at.map(|t| t.0),
        })
        .collect())
}

/// The store's address comes from DZBuild: only an `https` one opens (never another scheme
/// handed to the system's opener).
pub(crate) fn open_page(st: &AppState, page: CloudPage) -> Result<(), AppError> {
    st.session()?;
    let cloud = st.cloud()?;
    let url = match page {
        CloudPage::Signup => cloud.ctx.config.signup_link(),
        CloudPage::Devices => cloud.ctx.config.devices_url.clone(),
        CloudPage::Store => st
            .db
            .read(store::active_link)?
            .filter(|l| l.status == LinkStatus::Linked)
            .and_then(|l| l.public_url)
            .filter(|u| check_url(u).is_ok())
            .ok_or_else(|| AppError::sync("cloud_not_linked"))?,
    };
    cloud.open_url(&url)
}

/// A local change the engine sends at once rather than at its next cycle (nothing without a
/// runtime: the change waits in the database).
fn wake(st: &AppState) {
    if let Ok(cloud) = st.cloud() {
        cloud.wake();
    }
}
