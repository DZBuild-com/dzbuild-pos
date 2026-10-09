//! On a till, the few host-only operations a till needs go to the main PC; everything a till
//! sells, refunds or counts stays local.

use crate::state::AppState;
use pos_core::auth::Session;
use pos_core::cash::DrawerShare;
use pos_core::error::AppError;
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::sync::answers::{self, PendingCall};
use pos_core::till::TillContext;
use pos_server::wire::RpcGrant;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

/// How long a till waits for the main PC to answer a host service.
pub(crate) const RPC_TIMEOUT: Duration = Duration::from_secs(15);

/// The manager approvals pending for this till's sign-in (`auth_manager_override` here): they
/// go with every call, for the main PC to confirm against its own users and roles.
fn grants(st: &AppState, session: &Session) -> Result<Vec<RpcGrant>, AppError> {
    Ok(st
        .approvals
        .pending(session, st.now())?
        .into_iter()
        .map(|g| RpcGrant {
            approver_id: g.approver_id,
            permission: g.permission,
            granted_at: g.granted_at,
        })
        .collect())
}

/// `Some(answer)` from the main PC on a till (`host_required` when it is not connected); `None`
/// elsewhere: the command runs locally. The main PC checks the user's permissions itself, with
/// the manager approvals pending here (Task 20 review).
pub fn forward<T: DeserializeOwned>(
    st: &AppState,
    service: &str,
    input: Value,
) -> Result<Option<T>, AppError> {
    if !st.lan.is_client() {
        return Ok(None);
    }
    let session = st.session()?;
    let approvals = grants(st, &session)?;
    let reply = st
        .lan
        .rpc(service, &session.user_id, approvals, input, RPC_TIMEOUT)?;
    Ok(Some(serde_json::from_value(reply.output)?))
}

/// [`forward`] for a till write that spends the manager approvals it carries, as the same write
/// made on this PC does (`AppState::spend_approvals`): once the main PC has answered, they are
/// spent here.
pub fn forward_spent<T: DeserializeOwned>(
    st: &AppState,
    service: &str,
    input: Value,
) -> Result<Option<T>, AppError> {
    if !st.lan.is_client() {
        return Ok(None);
    }
    let session = st.session()?;
    let approvals = grants(st, &session)?;
    let spend = !approvals.is_empty();
    let reply = st
        .lan
        .rpc(service, &session.user_id, approvals, input, RPC_TIMEOUT)?;
    if spend {
        st.approvals.spend(&session)?;
    }
    Ok(Some(serde_json::from_value(reply.output)?))
}

/// [`forward`] for a call the till makes on its own (the maintenance loop, maybe nobody signed
/// in): as the signed-in user, else as the shop's first owner. For reads that need no right.
pub fn forward_as_system<T: DeserializeOwned>(
    st: &AppState,
    service: &str,
    input: Value,
) -> Result<Option<T>, AppError> {
    if !st.lan.is_client() {
        return Ok(None);
    }
    let user = match st.current_session()? {
        Some(session) => session.user_id,
        None => match st.db.read(pos_core::auth::users::first_active_owner)? {
            Some(owner) => owner,
            None => return Ok(None),
        },
    };
    let reply = st.lan.rpc(service, &user, Vec::new(), input, RPC_TIMEOUT)?;
    Ok(Some(serde_json::from_value(reply.output)?))
}

/// The till's half of a host service that took or gave money for it (Task 20 review): the main
/// PC booked the operation on this till's open drawer without moving any; the movement goes
/// into this till's drawer here (`pos_core::cash::record_share`, one `cash.host_share` row), so
/// the till's Z holds every dinar it holds.
pub fn keep_cash(st: &AppState, shares: Vec<DrawerShare>) -> Result<(), AppError> {
    if shares.is_empty() {
        return Ok(());
    }
    let ctx = TillContext::new(st.session()?, st.datapack.clone(), st.now());
    st.db.write(move |tx| {
        for share in &shares {
            pos_core::cash::record_share(tx, &ctx, share)?;
        }
        Ok(())
    })
}

/// [`forward`] for a till write that moves money (final review): the call carries an
/// idempotency key, so the main PC never writes it twice; its answer puts the money in this
/// till's drawer ([`keep_cash`], from [`drawer_shares`]); `spend` spends the manager approvals it
/// carried, as [`forward_spent`]. A call whose answer was lost (no answer in time, the link
/// down mid-call) is kept here with its key ([`answers::PendingCall`]) and sent again before the
/// next money call and by the maintenance loop ([`replay_calls`]) until the main PC answers; the
/// cashier reads `lan_answer_pending`: the operation will be completed, it is not made again.
pub fn forward_money<T: DeserializeOwned>(
    st: &AppState,
    service: &str,
    input: Value,
    spend: bool,
) -> Result<Option<T>, AppError> {
    if !st.lan.is_client() {
        return Ok(None);
    }
    replay_calls(st);
    let session = st.session()?;
    let approvals = grants(st, &session)?;
    let spend = spend && !approvals.is_empty();
    let key = pos_core::ids::new_id();
    let sent = st.lan.rpc_keyed(
        service,
        &session.user_id,
        approvals.clone(),
        input.clone(),
        RPC_TIMEOUT,
        Some(key.clone()),
    );
    let mut call = PendingCall {
        key,
        service: service.to_owned(),
        user_id: session.user_id.clone(),
        approvals: serde_json::to_value(&approvals)?,
        input,
        at: st.now(),
        output: None,
        refused: None,
    };
    let reply = match sent {
        Ok(reply) => reply,
        Err(AppError::Sync { .. }) => {
            st.db.write(move |tx| answers::save_call(tx, &call))?;
            if spend {
                st.approvals.spend(&session)?;
            }
            tracing::warn!(
                service,
                "the main PC's answer was lost: the call waits here and goes again"
            );
            return Err(AppError::sync("lan_answer_pending"));
        }
        Err(error) => return Err(error),
    };
    if spend {
        st.approvals.spend(&session)?;
    }
    let shares = drawer_shares(st, service, &call.input, &reply.output)?;
    if let Err(error) = keep_cash(st, shares) {
        // The main PC did it: its cash goes into this drawer once it can (its session closed?).
        tracing::warn!(service, code = %error.code(), "a host service's cash is not in this till's drawer yet");
        call.output = Some(reply.output.clone());
        st.db.write(move |tx| answers::save_call(tx, &call))?;
    }
    Ok(Some(serde_json::from_value(reply.output)?))
}

/// Sends again this till's money calls whose answer was lost, oldest first, with their keys (the
/// main PC answers a call it already made from what it answered then), and puts their money in
/// this till's drawer as the call's user. Stops at the first one the link cannot carry yet; a
/// refused one is marked and waits for « Réessayer » (Settings › Réseau). Returns how many
/// were settled.
pub fn replay_calls(st: &AppState) -> usize {
    if !st.lan.is_client() {
        return 0;
    }
    let calls = match st.db.read(answers::pending_calls) {
        Ok(calls) => calls,
        Err(error) => {
            tracing::warn!(%error, "the calls waiting for the main PC cannot be read");
            return 0;
        }
    };
    let mut settled = 0;
    for mut call in calls {
        if call.refused.is_some() {
            continue;
        }
        let output = match call.output.clone() {
            Some(output) => output,
            None => {
                let approvals: Vec<RpcGrant> =
                    serde_json::from_value(call.approvals.clone()).unwrap_or_default();
                match st.lan.rpc_keyed(
                    &call.service,
                    &call.user_id,
                    approvals,
                    call.input.clone(),
                    RPC_TIMEOUT,
                    Some(call.key.clone()),
                ) {
                    Ok(reply) => reply.output,
                    Err(error)
                        if matches!(error, AppError::Sync { .. })
                            || error.code() == "host_required" =>
                    {
                        break;
                    }
                    Err(error) => {
                        tracing::error!(service = %call.service, code = %error.code(), "the main PC refused a call kept here");
                        call.refused = Some(error.code());
                        if let Err(error) = st.db.write(move |tx| answers::save_call(tx, &call)) {
                            tracing::warn!(%error, "the refusal of a kept call is not marked");
                        }
                        continue;
                    }
                }
            }
        };
        let kept = drawer_shares(st, &call.service, &call.input, &output)
            .and_then(|shares| keep_cash_as(st, &call.user_id, shares));
        let key = call.key.clone();
        let done = match kept {
            Ok(()) => st.db.write(move |tx| answers::call_done(tx, &key)),
            Err(error) => {
                tracing::warn!(service = %call.service, code = %error.code(), "a kept call's cash is not in this till's drawer yet");
                call.output = Some(output);
                st.db.write(move |tx| answers::save_call(tx, &call))
            }
        };
        match done {
            Ok(()) => settled += 1,
            Err(error) => tracing::warn!(%error, "a kept call is still listed here"),
        }
    }
    settled
}

/// The money a host service's answer moved in this till's drawer, from its input and
/// its output: what each call site used to put in the drawer itself.
pub fn drawer_shares(
    st: &AppState,
    service: &str,
    input: &Value,
    output: &Value,
) -> Result<Vec<DrawerShare>, AppError> {
    fn of<T: DeserializeOwned>(value: &Value) -> Result<T, AppError> {
        serde_json::from_value(value.clone())
            .map_err(|_| AppError::validation("output", "value_invalid"))
    }
    let shares: Vec<DrawerShare> = match service {
        "instalments.sign" => of::<pos_core::instalments::Signed>(output)?
            .sale
            .drawer_share()?
            .into_iter()
            .collect(),
        "orders.fulfil" | "orders.keep_deposit" => of::<pos_core::orders::Fulfilled>(output)?
            .sale
            .drawer_share()?
            .into_iter()
            .collect(),
        "service.pickup" => of::<pos_core::service_tickets::PickedUp>(output)?
            .sale
            .drawer_share()?
            .into_iter()
            .collect(),
        "orders.deliver" => of::<pos_core::billing::validate::ValidatedDocument>(output)?
            .drawer_share()?
            .into_iter()
            .collect(),
        "orders.create" => of::<crate::commands::orders::CreatedOrder>(output)?
            .deposit
            .and_then(|d| pos_core::deposits::drawer_share(&d.deposit))
            .into_iter()
            .collect(),
        "orders.pay_in" | "deposits.take" => pos_core::deposits::drawer_share(
            &of::<crate::commands::services::TakenDeposit>(output)?.deposit,
        )
        .into_iter()
        .collect(),
        "deposits.refund" => {
            pos_core::deposits::drawer_share(&of::<pos_core::deposits::Deposit>(output)?)
                .into_iter()
                .collect()
        }
        "vouchers.sell" => {
            let voucher: pos_core::gift_vouchers::Voucher = of(output)?;
            let method: PaymentMethod = of(&input["input"]["method"])?;
            pos_core::gift_vouchers::drawer_share(&voucher, method)
                .into_iter()
                .collect()
        }
        "flexy.purchase" => pos_core::flexy::purchase_share(&of(&input["input"])?)?
            .into_iter()
            .collect(),
        "staff.advance" | "staff.repay" => {
            let entry: pos_core::staff::StaffEntry = of(output)?;
            let from_till =
                service == "staff.advance" && input["input"]["fromTill"].as_bool().unwrap_or(false);
            st.db
                .read(move |c| pos_core::staff::drawer_share(c, &entry, from_till))?
                .into_iter()
                .collect()
        }
        "returns.create" => {
            let method: PaymentMethod = of(&input["request"]["input"]["refundMethod"])?;
            of::<pos_core::returns::ValidatedReturn>(output)?
                .drawer_share(method)?
                .into_iter()
                .collect()
        }
        "returns.exchange" => {
            let method: PaymentMethod = of(&input["request"]["ret"]["refundMethod"])?;
            let done: pos_core::returns::Exchange = of(output)?;
            done.returned
                .drawer_share(method)?
                .into_iter()
                .chain(done.sale.drawer_share()?)
                .collect()
        }
        "rounds.close" => of::<pos_core::wholesale::rounds::ClosedRound>(output)?
            .versements
            .iter()
            .filter_map(|v| v.drawer_share())
            .collect(),
        _ => Vec::new(),
    };
    Ok(shares)
}

/// [`keep_cash`] as `user_id` (a kept call settled by the maintenance loop, whoever is signed in).
fn keep_cash_as(st: &AppState, user_id: &str, shares: Vec<DrawerShare>) -> Result<(), AppError> {
    if shares.is_empty() {
        return Ok(());
    }
    let (user, terminal, now) = (user_id.to_owned(), st.terminal.clone(), st.now());
    let session = st
        .db
        .read(move |c| pos_core::auth::session::build_session(c, &user, &terminal, now))?;
    let ctx = TillContext::new(session, st.datapack.clone(), st.now());
    st.db.write(move |tx| {
        for share in &shares {
            pos_core::cash::record_share(tx, &ctx, share)?;
        }
        Ok(())
    })
}

/// [`forward`] for a caller that reads the main PC's change back from the till's copy (a label
/// print reloads the barcodes the main PC just assigned): `lan_catching_up` when the answer came
/// but the copy has not received it yet. The main PC's write is done; a retry a moment later
/// finds it in the copy.
pub fn forward_settled<T: DeserializeOwned>(
    st: &AppState,
    service: &str,
    input: Value,
) -> Result<Option<T>, AppError> {
    if !st.lan.is_client() {
        return Ok(None);
    }
    let session = st.session()?;
    let approvals = grants(st, &session)?;
    let reply = st
        .lan
        .rpc(service, &session.user_id, approvals, input, RPC_TIMEOUT)?;
    if !reply.caught_up {
        return Err(AppError::conflict("lan_catching_up"));
    }
    Ok(Some(serde_json::from_value(reply.output)?))
}

/// Kridi on a till needs a live link to the main PC: balances and limits are the main PC's.
pub fn require_live_for_kridi(st: &AppState, uses_account: bool) -> Result<(), AppError> {
    if uses_account && st.lan.is_client() && !st.lan.is_online() {
        return Err(AppError::conflict("kridi_needs_host"));
    }
    Ok(())
}

/// The customer's account pays (kridi) or store credit is spent.
pub fn uses_account(payments: &[PaymentInput]) -> bool {
    payments.iter().any(|p| on_account(p.method))
}

/// A refund or a tender that moves the customer's balance.
pub fn on_account(method: PaymentMethod) -> bool {
    matches!(method, PaymentMethod::Credit | PaymentMethod::Voucher)
}
