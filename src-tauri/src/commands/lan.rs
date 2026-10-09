//! LAN commands (spec §4.6): the role of this PC, pairing codes, the tills, notices and the
//! firewall on the main PC; joining, sync, making a till the main PC and leaving on a till.
//! Thin: guards, then `pos_core::sync` / `pos_server`, then the runtime of `crate::lan`. Nothing
//! here logs a pairing code, a token or a PIN.

use super::blocking;
use crate::lan::{LanPairProgress, StatusSink};
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::catalog::search::{ProductHit, SearchQuery};
use pos_core::error::AppError;
use pos_core::fiscal::archive::TillJournals;
use pos_core::sync::notices::{self, Notice};
use pos_core::sync::pairing::{self, IssuedCode, PairingDesk};
use pos_core::sync::status::LanStatus;
use pos_core::sync::tills::{TillInfo, TillZ};
use pos_core::sync::{devices, outbox, replica, role};
use pos_core::terminal::TerminalRole;
use pos_server::discovery::DiscoveredHost;
use pos_server::firewall::FirewallStatus;
use pos_server::wire::HealthInfo;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::sync::{Arc, MutexGuard, PoisonError};
use tauri::{AppHandle, Manager, State};

/// What the new till's owner types in the wizard. No `Debug`: it carries the pairing code and
/// the owner's PIN.
#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JoinInput {
    pub address: String,
    pub code: String,
    pub terminal_code: String,
    pub terminal_name: String,
    /// The owner's PIN, required once this PC is set up (Settings › Réseau); a new PC joining
    /// from the setup wizard has no user yet and sends none.
    #[serde(default)]
    #[specta(optional)]
    pub owner_pin: Option<String>,
}

/// Bytes of the main PC's copy received so far, and its size when the main PC sent it.
pub(crate) type JoinProgress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JoinResult {
    pub host_name: String,
    pub terminal_code: String,
}

/// Whether the tills can reach this PC, and the commands an administrator runs when they cannot.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FirewallCheck {
    pub status: FirewallStatus,
    pub commands: Vec<String>,
}

/// A status sink that tells nobody (tests).
#[cfg(test)]
pub(crate) fn quiet() -> StatusSink {
    Arc::new(|_| {})
}

fn desk(st: &AppState) -> MutexGuard<'_, PairingDesk> {
    st.lan.desk.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn status(st: &AppState) -> Result<LanStatus, AppError> {
    st.session()?;
    st.lan.status(st)
}

pub(crate) fn sync_now(st: &AppState) -> Result<(), AppError> {
    st.session()?;
    st.lan.sync_now();
    Ok(())
}

/// Swaps the write hook and restarts the runtime for the role the database now has.
fn apply_role(st: &SharedState, sink: &StatusSink) -> Result<LanStatus, AppError> {
    let place = st.db.read(role::current)?;
    st.lan.set_role(place.role);
    st.db.set_write_hook(st.lan.hook(&place, &st.terminal.id));
    st.lan.restart(st, sink.clone());
    st.lan.status(st)
}

/// Changing the role of this PC, making a till the main PC or leaving: the owner only.
pub(crate) fn may_change_role(st: &AppState) -> Result<(), AppError> {
    st.demo_guard()?;
    st.require_owner(Permission::SettingsData).map(drop)
}

pub(crate) fn become_host(st: &SharedState, sink: &StatusSink) -> Result<LanStatus, AppError> {
    may_change_role(st)?;
    let actor = st.session()?.actor();
    let now = st.now();
    st.db
        .write(move |tx| role::become_host(tx, &actor, now).map(drop))?;
    apply_role(st, sink)
}

pub(crate) fn become_standalone(
    st: &SharedState,
    sink: &StatusSink,
) -> Result<LanStatus, AppError> {
    may_change_role(st)?;
    let actor = st.session()?.actor();
    // A code shown on this main PC stops working: no till pairs with a single PC.
    desk(st).cancel();
    let now = st.now();
    st.db
        .write(move |tx| role::become_standalone(tx, &actor, now))?;
    apply_role(st, sink)
}

/// A new code (the previous one stops working), journaled without its digits. On a main PC only.
pub(crate) fn pairing_code(st: &AppState) -> Result<IssuedCode, AppError> {
    st.demo_guard()?;
    let actor = st.require(Permission::SettingsData)?.actor();
    if st.lan.role() != TerminalRole::Host {
        return Err(AppError::conflict("lan_role_invalid"));
    }
    let issued = desk(st).issue(st.now())?;
    let expires_at = issued.expires_at;
    if let Err(error) = st
        .db
        .write(move |tx| pairing::journal_issued(tx, &actor, expires_at))
    {
        // A code nobody journaled is never shown.
        desk(st).cancel();
        return Err(error);
    }
    Ok(issued)
}

pub(crate) fn cancel_pairing(st: &AppState) -> Result<(), AppError> {
    st.require(Permission::SettingsData)?;
    desk(st).cancel();
    Ok(())
}

pub(crate) fn tills(st: &AppState) -> Result<Vec<TillInfo>, AppError> {
    st.require(Permission::SettingsData)?;
    let now = st.now();
    st.db.read(move |c| pos_core::sync::tills::list(c, now))
}

pub(crate) fn revoke_till(st: &AppState, terminal_id: String) -> Result<(), AppError> {
    let actor = st.require_owner(Permission::SettingsData)?.actor();
    let now = st.now();
    st.db
        .write(move |tx| devices::revoke(tx, &actor, &terminal_id, now))
}

pub(crate) fn till_closures(
    st: &AppState,
    terminal_id: String,
    limit: u32,
) -> Result<Vec<TillZ>, AppError> {
    st.require(Permission::CashViewHistory)?;
    st.db
        .read(move |c| pos_core::sync::tills::closures(c, &terminal_id, limit))
}

/// The tills' journals this main PC keeps, each checked, and its merge chain (Settings ›
/// Journal, "Vérifier"), in one snapshot.
pub(crate) fn verify_journals(st: &AppState) -> Result<TillJournals, AppError> {
    st.require(Permission::AuditView)?;
    st.db.read(|c| {
        pos_core::fiscal::archive::in_snapshot(c, pos_core::fiscal::archive::till_journals)
    })
}

pub(crate) fn notices(st: &AppState, include_acknowledged: bool) -> Result<Vec<Notice>, AppError> {
    st.require(Permission::SettingsData)?;
    st.db
        .read(move |c| notices::list(c, include_acknowledged, 200))
}

pub(crate) fn acknowledge_notice(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::SettingsData)?.actor();
    let now = st.now();
    st.db
        .write(move |tx| notices::acknowledge(tx, &actor, &id, now))
}

pub(crate) fn may_see_firewall(st: &AppState) -> Result<(), AppError> {
    st.require(Permission::SettingsData).map(drop)
}

/// Windows PowerShell answers in seconds: a blocking task.
fn firewall(st: &AppState) -> Result<FirewallCheck, AppError> {
    may_see_firewall(st)?;
    let exe = std::env::current_exe()?;
    Ok(FirewallCheck {
        status: pos_server::firewall::check(),
        commands: pos_server::firewall::install_commands(&exe).to_vec(),
    })
}

/// A PC not yet set up may join a main PC (it only receives data once the owner of the main PC
/// shows a code); once set up, only its owner may replace its database by a till's copy, and
/// never while it holds paired tills' books or is linked to DZBuild ([`join_refused`]).
pub(crate) fn may_join(st: &AppState) -> Result<(), AppError> {
    st.demo_guard()?;
    owner_once_set_up(st)?;
    join_refused(st)
}

/// Checked before any network call, so the wizard says it at its first step and no pairing code
/// is spent: a main PC with paired tills would archive their tickets, Z and journals and orphan
/// them (`lan_tills_paired`, as `become_standalone`); a PC linked to DZBuild would drop its link
/// silently and archive its unsent uploads (`cloud_unlink_first`).
fn join_refused(st: &AppState) -> Result<(), AppError> {
    st.db.read(|c| {
        role::host_releasable(c)?;
        pos_core::cloud::store::refuse_while_linked(c)
    })
}

/// The owner's session once this PC is set up; `None` on a new PC (no shop, no user yet).
fn owner_once_set_up(st: &AppState) -> Result<Option<Session>, AppError> {
    if !st.db.read(pos_core::setup::status)?.completed {
        return Ok(None);
    }
    st.require_owner(Permission::SettingsData).map(Some)
}

/// Joining replaces this PC's books with the main PC's copy at the next start, the same swap as
/// M0's restore: once set up, the owner types their PIN again (a wrong one counts toward their
/// lockout). Checked before any network call, so a wrong PIN never spends the pairing code.
fn may_join_with_pin(st: &AppState, owner_pin: Option<&str>) -> Result<(), AppError> {
    st.demo_guard()?;
    let Some(session) = owner_once_set_up(st)? else {
        return Ok(());
    };
    join_refused(st)?;
    let pin = owner_pin.ok_or_else(|| AppError::validation("ownerPin", "required"))?;
    pos_core::auth::confirm_pin(&st.db, &st.terminal, &session, pin, st.now())
}

fn discover(st: &AppState) -> Result<Vec<DiscoveredHost>, AppError> {
    may_join(st)?;
    pos_server::discovery::browse(std::time::Duration::from_secs(3))
}

fn probe(st: &AppState, address: String) -> Result<HealthInfo, AppError> {
    may_join(st)?;
    pos_server::client::join::probe(&address)
}

/// Pairs with the code, sends what this PC still owes as a till, downloads and stages the main
/// PC's copy (installed at the next start, before the database opens) and keeps the till's
/// token in the vault. `progress` follows the download.
pub(crate) fn join(
    st: &AppState,
    input: JoinInput,
    progress: JoinProgress,
) -> Result<JoinResult, AppError> {
    may_join_with_pin(st, input.owner_pin.as_deref())?;
    // The vault must keep the till's token: checked before the code is spent and the till is
    // registered on the main PC.
    st.lan.vault.get()?;
    let existing = st.lan.is_client().then(|| st.db.clone());
    let install_id = st
        .db
        .read(|c| pos_core::db::meta_get(c, pos_core::db::META_INSTALL_ID))?
        .unwrap_or_default();
    // A till whose damaged copy was set aside joins from a new, empty database: the terminal it
    // was (kept in `local.json`) lets it take its own terminal and series back.
    let set_up = st.db.read(pos_core::setup::status)?.completed;
    let former_terminal = (existing.is_none() && !set_up)
        .then(|| crate::lan::former_terminal(Path::new(&st.paths.data_dir)))
        .flatten();
    let joined = pos_server::client::join::join(
        existing,
        Path::new(&st.paths.data_dir),
        pos_server::client::join::JoinRequest {
            address: input.address,
            code: input.code,
            terminal_code: input.terminal_code,
            terminal_name: input.terminal_name,
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: pos_core::db::migrations::LATEST_VERSION,
            install_id,
            former_terminal,
        },
        progress,
    )?;
    if let Err(error) = st.lan.vault.set(&joined.token) {
        // A copy installed without its token would be a till that never syncs.
        if let Err(unstaged) = replica::unstage(Path::new(&st.paths.data_dir)) {
            tracing::error!(code = %unstaged.code(), "the staged copy could not be removed");
        }
        return Err(error);
    }
    Ok(JoinResult {
        host_name: joined.pending.host_name,
        terminal_code: joined.pending.terminal_code,
    })
}

/// The oldest rejected item goes back in line (its cause was fixed on the main PC).
pub(crate) fn retry_rejected(st: &AppState) -> Result<(), AppError> {
    let actor = st.require(Permission::SettingsData)?.actor();
    st.db
        .write(move |tx| outbox::retry_rejected(tx, &actor).map(drop))?;
    st.lan.sync_now();
    Ok(())
}

/// The owner, who typed their PIN again (a wrong one counts toward their lockout).
fn owner_with_pin(st: &AppState, pin: &str) -> Result<Session, AppError> {
    may_change_role(st)?;
    let session = st.session()?;
    pos_core::auth::confirm_pin(&st.db, &st.terminal, &session, pin, st.now())?;
    Ok(session)
}

/// The role has changed for good: a token the vault cannot forget is only logged (it is useless
/// to a main PC and replaced at the next pairing), the restart must still come.
fn forget_token(st: &AppState) {
    if let Err(error) = st.lan.vault.delete() {
        tracing::warn!(code = %error.code(), "the till's token could not be removed");
    }
}

/// This till becomes the main PC: only when the old one does not answer (LAN-010).
pub(crate) fn promote(st: &AppState, pin: &str) -> Result<(), AppError> {
    let actor = owner_with_pin(st, pin)?.actor();
    let place = st.db.read(role::current)?;
    if place.role != TerminalRole::Client {
        return Err(AppError::conflict("lan_role_invalid"));
    }
    let answers = place
        .host_address
        .as_deref()
        .is_some_and(|address| pos_server::client::join::probe(address).is_ok());
    if answers || st.lan.is_online() {
        return Err(AppError::conflict("lan_host_alive"));
    }
    st.lan.stop();
    // The till's outbox hook would take the new main PC's first feed rows into an outbox item:
    // the promotion commits without a hook, then this PC gets the hook of the role it has.
    st.db.set_write_hook(None);
    let now = st.now();
    let promoted = st
        .db
        .write(move |tx| role::promote(tx, &actor, now).map(drop));
    let place = st.db.read(role::current)?;
    st.lan.set_role(place.role);
    st.db.set_write_hook(st.lan.hook(&place, &st.terminal.id));
    promoted?;
    forget_token(st);
    Ok(())
}

/// This till leaves the network: its copy is archived at the next start, never deleted. The
/// token stays until that archive is done (`AppState::open_with`): a till whose copy could not
/// be archived is still a till, and keeps syncing.
pub(crate) fn leave(st: &AppState, pin: &str) -> Result<(), AppError> {
    let actor = owner_with_pin(st, pin)?.actor();
    let now = st.now();
    st.db.write(move |tx| role::leave(tx, &actor, now))?;
    st.lan.stop();
    replica::schedule_reset(Path::new(&st.paths.data_dir))?;
    Ok(())
}

/// The main PC's search on a till (a product created there a moment ago); this PC's own search
/// elsewhere.
pub(crate) fn search_host(st: &AppState, query: SearchQuery) -> Result<Vec<ProductHit>, AppError> {
    st.require_any(&[Permission::ProductsView, Permission::SalesCreate])?;
    match crate::lan::route::forward(st, "products.search", serde_json::json!({ "query": query }))?
    {
        Some(hits) => Ok(hits),
        None => super::products::search(st, query),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn lan_status(state: State<'_, SharedState>) -> Result<LanStatus, AppError> {
    let st = state.inner().clone();
    blocking(move || status(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_sync_now(state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || sync_now(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_become_host(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<LanStatus, AppError> {
    let (st, sink) = (state.inner().clone(), crate::lan::window_sink(&app));
    blocking(move || become_host(&st, &sink)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_become_standalone(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<LanStatus, AppError> {
    let (st, sink) = (state.inner().clone(), crate::lan::window_sink(&app));
    blocking(move || become_standalone(&st, &sink)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_pairing_code(state: State<'_, SharedState>) -> Result<IssuedCode, AppError> {
    let st = state.inner().clone();
    blocking(move || pairing_code(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_cancel_pairing(state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || cancel_pairing(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_tills(state: State<'_, SharedState>) -> Result<Vec<TillInfo>, AppError> {
    let st = state.inner().clone();
    blocking(move || tills(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_revoke_till(
    state: State<'_, SharedState>,
    terminal_id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || revoke_till(&st, terminal_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_till_closures(
    state: State<'_, SharedState>,
    terminal_id: String,
    limit: u32,
) -> Result<Vec<TillZ>, AppError> {
    let st = state.inner().clone();
    blocking(move || till_closures(&st, terminal_id, limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_verify_journals(state: State<'_, SharedState>) -> Result<TillJournals, AppError> {
    let st = state.inner().clone();
    blocking(move || verify_journals(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_notices(
    state: State<'_, SharedState>,
    include_acknowledged: bool,
) -> Result<Vec<Notice>, AppError> {
    let st = state.inner().clone();
    blocking(move || notices(&st, include_acknowledged)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_acknowledge_notice(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || acknowledge_notice(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_firewall(state: State<'_, SharedState>) -> Result<FirewallCheck, AppError> {
    let st = state.inner().clone();
    blocking(move || firewall(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_discover(state: State<'_, SharedState>) -> Result<Vec<DiscoveredHost>, AppError> {
    let st = state.inner().clone();
    blocking(move || discover(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_probe(
    state: State<'_, SharedState>,
    address: String,
) -> Result<HealthInfo, AppError> {
    let st = state.inner().clone();
    blocking(move || probe(&st, address)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_join(
    app: AppHandle,
    state: State<'_, SharedState>,
    input: JoinInput,
) -> Result<JoinResult, AppError> {
    let st = state.inner().clone();
    let progress: JoinProgress = Arc::new(move |received, total| {
        use tauri_specta::Event;
        if let Err(error) = (LanPairProgress { received, total }).emit(&app) {
            tracing::debug!(%error, "could not tell the window the copy's progress");
        }
    });
    blocking(move || join(&st, input, progress)).await
}

/// The staged copy is installed by the next start, before the database opens.
#[tauri::command]
#[specta::specta]
pub async fn lan_restart(app: AppHandle, state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || may_join(&st)).await?;
    super::backup::restart_soon(app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn lan_retry_rejected(state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || retry_rejected(&st)).await
}

/// This till's tickets whose host entries (registers, serials, top-ups, deposits,
/// vouchers) wait for the main PC or were refused by it.
#[tauri::command]
#[specta::specta]
pub async fn lan_pending_entries(
    state: State<'_, SharedState>,
) -> Result<crate::lan::registers::PendingEntries, AppError> {
    let st = state.inner().clone();
    blocking(move || crate::lan::registers::pending_entries(&st)).await
}

/// « Réessayer »: the refused entries go to the main PC again now.
#[tauri::command]
#[specta::specta]
pub async fn lan_pending_retry(
    state: State<'_, SharedState>,
) -> Result<crate::lan::registers::PendingEntries, AppError> {
    let st = state.inner().clone();
    blocking(move || crate::lan::registers::retry_pending(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn lan_promote(
    app: AppHandle,
    state: State<'_, SharedState>,
    pin: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || promote(&st, &pin)).await?;
    super::backup::restart_soon(app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn lan_leave(
    app: AppHandle,
    state: State<'_, SharedState>,
    pin: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || leave(&st, &pin)).await?;
    super::backup::restart_soon(app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn lan_search_host(
    state: State<'_, SharedState>,
    query: SearchQuery,
) -> Result<Vec<ProductHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || search_host(&st, query)).await
}

/// "Fermer" in the main PC's close confirmation: the next close is not held, and the window
/// closes again, through the backup on close when it is due (DAT-024, `closing`), then the app
/// exits (`RunEvent::Exit` stops the server and closes the database cleanly). Public: the main
/// PC closes from the login screen too.
#[tauri::command]
#[specta::specta]
pub async fn lan_confirm_close(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<(), AppError> {
    state.lan.confirm_close();
    match app.get_webview_window("main").map(|window| window.close()) {
        Some(Ok(())) => {}
        Some(Err(error)) => {
            tracing::warn!(%error, "could not close the main window; exiting now");
            app.exit(0);
        }
        None => app.exit(0),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testshop::{OWNER_PIN, shop};

    #[test]
    fn a_single_pc_becomes_the_main_pc_hands_out_codes_and_comes_back() {
        let shop = shop();
        assert_eq!(
            pairing_code(&shop.st).unwrap_err().code(),
            "lan_role_invalid",
            "codes on a main PC only"
        );
        let status = become_host(&shop.st, &quiet()).unwrap();
        assert_eq!(status.role, pos_core::terminal::TerminalRole::Host);
        assert!(shop.st.db.has_write_hook());
        let issued = pairing_code(&shop.st).unwrap();
        assert_eq!(issued.code.len(), 6);
        let digits = format!("%{}%", issued.code);
        assert_eq!(
            shop.st
                .db
                .read(move |c| Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'lan.pairing_code_issued' AND payload NOT LIKE ?1",
                    [digits],
                    |r| r.get::<_, i64>(0)
                )?))
                .unwrap(),
            1,
            "journaled without the code"
        );
        cancel_pairing(&shop.st).unwrap();
        let back = become_standalone(&shop.st, &quiet()).unwrap();
        assert_eq!(back.role, pos_core::terminal::TerminalRole::Standalone);
        assert!(!shop.st.db.has_write_hook());
        shop.st.lan.stop();
    }

    #[test]
    fn joining_is_open_before_setup_and_owner_only_after() {
        let st = crate::state::AppState::for_tests();
        may_join(&st).unwrap(); // a brand-new PC: no shop, no user yet
        crate::commands::tests::first_run(&st);
        crate::commands::auth::logout(&st).unwrap();
        assert_eq!(may_join(&st).unwrap_err().code(), "session_required");
    }

    #[test]
    fn promote_and_leave_need_the_owners_pin_and_a_till() {
        let shop = shop();
        assert_eq!(promote(&shop.st, "0000").unwrap_err().code(), "invalid_pin");
        assert_eq!(
            promote(&shop.st, OWNER_PIN).unwrap_err().code(),
            "lan_role_invalid"
        );
        assert_eq!(
            leave(&shop.st, OWNER_PIN).unwrap_err().code(),
            "lan_role_invalid"
        );
    }
}
