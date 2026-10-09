//! Cash sessions (M1a Tasks 14, 19): open with a float, paid in/out, expenses, X report, close
//! with the count sheet (variance above the threshold needs a manager, CSH-005), Z report.

use super::blocking;
use crate::printing::{hw_error, shop_name};
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::cash::closing::{self, ClosureChainReport};
use pos_core::cash::{
    self, CashMovement, CashSession, CloseInput, Closed, ExpenseCategory, ManualKind,
    SessionSummary, ZReport,
};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::settings;
use pos_core::till::TillContext;
use pos_hw::PrintOutcome;
use serde::Deserialize;
use serde_json::json;
use specta::Type;
use std::time::{Duration, Instant};
use tauri::State;

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CashMoveInput {
    pub kind: ManualKind,
    pub amount: Money,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseInput {
    pub category: ExpenseCategory,
    pub amount: Money,
    pub note: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn cash_current(state: State<'_, SharedState>) -> Result<Option<CashSession>, AppError> {
    let st = state.inner().clone();
    blocking(move || current(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_open_session(
    state: State<'_, SharedState>,
    opening_float: Money,
) -> Result<CashSession, AppError> {
    let st = state.inner().clone();
    blocking(move || open_session(&st, opening_float)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_move(
    state: State<'_, SharedState>,
    input: CashMoveInput,
) -> Result<CashMovement, AppError> {
    let st = state.inner().clone();
    blocking(move || move_cash(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_expense(
    state: State<'_, SharedState>,
    input: ExpenseInput,
) -> Result<CashMovement, AppError> {
    let st = state.inner().clone();
    blocking(move || expense(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_movements(
    state: State<'_, SharedState>,
    session_id: String,
) -> Result<Vec<CashMovement>, AppError> {
    let st = state.inner().clone();
    blocking(move || movements(&st, session_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_x_report(state: State<'_, SharedState>) -> Result<SessionSummary, AppError> {
    let st = state.inner().clone();
    blocking(move || x_report(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_close_session(
    state: State<'_, SharedState>,
    input: CloseInput,
) -> Result<Closed, AppError> {
    let st = state.inner().clone();
    blocking(move || close_session(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_close_offline(
    state: State<'_, SharedState>,
    input: CloseInput,
) -> Result<Closed, AppError> {
    let st = state.inner().clone();
    blocking(move || close_offline(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_z_report(
    state: State<'_, SharedState>,
    session_id: String,
) -> Result<ZReport, AppError> {
    let st = state.inner().clone();
    blocking(move || z_report(&st, session_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_sessions(
    state: State<'_, SharedState>,
    limit: u32,
) -> Result<Vec<CashSession>, AppError> {
    let st = state.inner().clone();
    blocking(move || sessions(&st, limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_verify_closures(
    state: State<'_, SharedState>,
) -> Result<ClosureChainReport, AppError> {
    let st = state.inner().clone();
    blocking(move || verify_closures(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_print_z(
    state: State<'_, SharedState>,
    session_id: String,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || print_z(&st, session_id, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn cash_print_x(
    state: State<'_, SharedState>,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || print_x(&st, lang)).await
}

/// Whether the till is open, for anyone signed in (the top bar shows it).
pub(crate) fn current(st: &AppState) -> Result<Option<CashSession>, AppError> {
    let terminal_id = st.session()?.terminal_id;
    st.db.read(move |c| cash::current(c, &terminal_id))
}

/// A till whose clock is off the main PC's by more than 15 minutes does not open a session while
/// connected (`lan_clock_skew`): its tickets would carry the wrong time.
pub(crate) fn open_session(st: &AppState, opening_float: Money) -> Result<CashSession, AppError> {
    let ctx = st.till(Permission::CashOpen)?;
    if st.lan.is_client() {
        pos_core::sync::skew::check_session_open(st.lan.skew_ms(), st.lan.is_online())?;
    }
    let write = ctx.clone();
    let session = st
        .db
        .write(move |tx| cash::open_session(tx, &write, opening_float))?;
    st.spend_approvals(&ctx)?;
    Ok(session)
}

pub(crate) fn move_cash(st: &AppState, input: CashMoveInput) -> Result<CashMovement, AppError> {
    let ctx = st.till(Permission::CashPaidInOut)?;
    let write = ctx.clone();
    let movement = st.db.write(move |tx| {
        cash::move_cash(tx, &write, input.kind, input.amount, input.note.as_deref())
    })?;
    st.spend_approvals(&ctx)?;
    Ok(movement)
}

pub(crate) fn expense(st: &AppState, input: ExpenseInput) -> Result<CashMovement, AppError> {
    let ctx = st.till(Permission::CashPaidInOut)?;
    let write = ctx.clone();
    let movement = st.db.write(move |tx| {
        cash::expense(
            tx,
            &write,
            input.category,
            input.amount,
            input.note.as_deref(),
        )
    })?;
    st.spend_approvals(&ctx)?;
    Ok(movement)
}

/// Every drawer movement with its amount adds up to the expected cash: shown to those who may
/// see it or the history, never to a blind closer (CSH-004).
pub(crate) fn movements(st: &AppState, session_id: String) -> Result<Vec<CashMovement>, AppError> {
    st.require_any(&[Permission::CashViewExpected, Permission::CashViewHistory])?;
    st.db.read(move |c| cash::movements(c, &session_id))
}

fn open_session_id(st: &AppState, terminal_id: String) -> Result<String, AppError> {
    st.db
        .read(move |c| cash::require_open(c, &terminal_id))
        .map(|s| s.id)
}

/// Expected cash is shown only with `cash.view_expected` and when blind close is off (CSH-004).
fn reveal(st: &AppState, session: &Session) -> Result<bool, AppError> {
    let blind: bool = st.db.read(|c| settings::get(c, "cash.blind_close"))?;
    Ok(session.has(Permission::CashViewExpected) && !blind)
}

pub(crate) fn x_report(st: &AppState) -> Result<SessionSummary, AppError> {
    let session = st.require(Permission::CashClose)?;
    let reveal = reveal(st, &session)?;
    let id = open_session_id(st, session.terminal_id)?;
    st.db.read(move |c| cash::x_report(c, &id, reveal))
}

/// How long an online till's Z waits for the engine to send what the last sales left.
const CLOSE_DRAIN: Duration = Duration::from_secs(10);

/// The till's operations not yet on the main PC; 0 off a till.
fn pending_to_send(st: &AppState) -> Result<i64, AppError> {
    if !st.lan.is_client() {
        return Ok(0);
    }
    Ok(st.db.read(pos_core::sync::outbox::depth)?.pending)
}

/// On a till, what is still to send once the engine had its chance: online, it is woken and
/// given up to [`CLOSE_DRAIN`] (the last sale and the manager's approval are usually on their
/// way); offline, the count at once.
fn pending_after_drain(st: &AppState) -> Result<i64, AppError> {
    let mut left = pending_to_send(st)?;
    if left > 0 && st.lan.is_online() {
        st.lan.sync_now();
        let deadline = Instant::now() + CLOSE_DRAIN;
        while left > 0 && st.lan.is_online() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            left = pending_to_send(st)?;
        }
    }
    Ok(left)
}

/// Not inside `Db::write`: M1a commits, checkpoints the WAL and takes the verified backup, then
/// (M6) its second copy when this PC chose a folder. A close refused for its variance is
/// journaled (`cash.close_refused`) in a write of its own, so every count, recounts included,
/// is on record (CSH-005). On a till the Z waits until the main PC has every operation of the
/// session (`outbox_pending_at_close` with `details.pending` otherwise: send them, or close an
/// offline Z with `close_offline`).
pub(crate) fn close_session(st: &AppState, input: CloseInput) -> Result<Closed, AppError> {
    let ctx = st.till(Permission::CashClose)?;
    let pending = pending_after_drain(st)?;
    if pending > 0 {
        st.lan.sync_now();
        return Err(AppError::rejected(
            json!({ "pending": pending }),
            "outbox_pending_at_close",
        ));
    }
    close_with(st, &ctx, input, None)
}

/// A Z on a till that still has operations to send (the main PC is down at closing time): a
/// manager approves it (`cash.view_expected`, like a variance; an owner closes alone); the Z
/// says how many were pending, and the main PC raises `offline_z` when it receives it. The
/// variance is judged first, on the cashier's own rights as on the online path (CSH-005): above
/// the threshold the count is journaled as refused and `variance_needs_approval` asks for the
/// recount, and only an approval given after the last refused count covers the variance, so an
/// approval for the offline Z alone never accepts a variance nobody saw.
pub(crate) fn close_offline(st: &AppState, input: CloseInput) -> Result<Closed, AppError> {
    let ctx = st.till(Permission::CashClose)?;
    if !st.lan.is_client() {
        return Err(AppError::conflict("lan_role_invalid"));
    }
    let bare = TillContext {
        approvals: Vec::new(),
        ..ctx.clone()
    };
    let (check, counts) = (bare.clone(), input.clone());
    if st
        .db
        .read(move |c| closing::variance_needs_approval(c, &check, &counts))?
    {
        let terminal = ctx.session.terminal_id.clone();
        let refused_at = st
            .db
            .read(move |c| closing::last_refused_at(c, &terminal))?;
        // The manager approved after the refused count was shown, never before it.
        let seen = refused_at.is_some_and(|at| {
            ctx.approvals
                .iter()
                .any(|g| g.permission == Permission::CashViewExpected && g.granted_at >= at)
        });
        if !seen {
            let refused = input.clone();
            st.db
                .write(move |tx| closing::record_refused_close(tx, &bare, &refused))?;
            return Err(AppError::conflict("variance_needs_approval"));
        }
    }
    if ctx.approved_by(Permission::CashViewExpected).is_none() && !ctx.session.is_owner {
        return Err(AppError::conflict("offline_z_needs_approval"));
    }
    let pending = Some(pending_to_send(st)?).filter(|p| *p > 0);
    close_with(st, &ctx, input, pending)
}

/// The Z, with `offline_pending` operations still to send on a till closing offline.
fn close_with(
    st: &AppState,
    ctx: &TillContext,
    input: CloseInput,
    offline_pending: Option<i64>,
) -> Result<Closed, AppError> {
    let closed = match closing::close_session_with_backup_lan(
        &st.db,
        ctx.clone(),
        input.clone(),
        offline_pending,
    ) {
        Err(refused) if refused.code() == "variance_needs_approval" => {
            let write = ctx.clone();
            st.db
                .write(move |tx| closing::record_refused_close(tx, &write, &input))?;
            return Err(refused);
        }
        other => other?,
    };
    st.spend_approvals(ctx)?;
    if let Some(info) = &closed.backup {
        crate::backups::after_backup(st, info, ctx.session.actor());
    }
    Ok(closed)
}

pub(crate) fn z_report(st: &AppState, session_id: String) -> Result<ZReport, AppError> {
    st.require_any(&[Permission::CashClose, Permission::CashViewHistory])?;
    st.db.read(move |c| cash::z_report(c, &session_id))
}

pub(crate) fn sessions(st: &AppState, limit: u32) -> Result<Vec<CashSession>, AppError> {
    let terminal_id = st.require(Permission::CashViewHistory)?.terminal_id;
    st.db
        .read(move |c| cash::list_sessions(c, &terminal_id, limit.clamp(1, 200)))
}

pub(crate) fn verify_closures(st: &AppState) -> Result<ClosureChainReport, AppError> {
    let terminal_id = st.require(Permission::AuditView)?.terminal_id;
    st.db.read(move |c| {
        pos_core::fiscal::archive::in_snapshot(c, |c| closing::verify_closures(c, &terminal_id))
    })
}

/// The Z prints the manager who accepted the variance by name: the report keeps their user id,
/// which never reaches paper.
pub(crate) fn print_z(
    st: &AppState,
    session_id: String,
    lang: Lang,
) -> Result<PrintOutcome, AppError> {
    let z = z_report(st, session_id)?;
    let approver = match z.approved_by.clone() {
        Some(id) => st.db.read(move |c| {
            Ok(pos_core::auth::users::get(c, &id)
                .ok()
                .map(|u| u.display_name))
        })?,
        None => None,
    };
    let shop = shop_name(&st.db, lang)?;
    st.hw
        .print_z(&z, &shop, lang, approver.as_deref())
        .map_err(hw_error)
}

pub(crate) fn print_x(st: &AppState, lang: Lang) -> Result<PrintOutcome, AppError> {
    let summary = x_report(st)?;
    let shop = shop_name(&st.db, lang)?;
    st.hw
        .print_x(&summary, &shop, lang, st.now())
        .map_err(hw_error)
}
