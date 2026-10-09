//! The controlled-medicine register and the jeweller register are kept on the main PC
//! only (`HostOnly`). A till's sale checks and writes down its entries in its own transaction
//! (`pos_core::sales::HostRegisters`, kept in this PC's `app_meta`); once the sale has committed
//! they go to the main PC, which numbers and keeps them against the till's sale
//! (`registers.record`). Entries the main PC could not take yet stay here and go with the next
//! sale or the maintenance loop. While offline, a till refuses such a sale up front
//! (`host_required`): pharmacies and jewellers with tills sell those lines online. The serials a
//! till sells, its top-ups and its deposit and voucher tenders go the same way (Task 31): the
//! main PC links them and books them on its ledgers against the till's ticket.
//!
//! The till checks those against its copy, counting what its sales still waiting here claim; the
//! main PC checks them again and may refuse one (another PC spent the voucher or the float, or
//! sold the serial, first). That sale's ticket stands; its entries wait here, marked with the
//! refusal, for the owner, and the till's later entries (the legal registers' among them) go on
//! to the main PC past it (Task 31 review).

use crate::state::AppState;
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::sales::extensions::{
    HostBooking, HostRegisters, PendingHostEntry, host_registers_answered, host_registers_refused,
    host_registers_retry, needs_host_registers, pending_host_entries, pending_host_registers,
    record_host_registers,
};
use serde::Serialize;
use specta::Type;

/// `host_required` on a till that cannot reach the main PC when selling `product_ids` writes a
/// legal register entry (a controlled medicine, a precious-metal piece); `Ok` everywhere else.
pub fn require_host(st: &AppState, product_ids: Vec<String>) -> Result<(), AppError> {
    if !st.lan.is_client() || st.lan.is_online() {
        return Ok(());
    }
    let needs = st
        .db
        .read(move |c| needs_host_registers(c, product_ids.iter().map(String::as_str)))?;
    if needs {
        return Err(AppError::conflict("host_required"));
    }
    Ok(())
}

/// The seller of the sale whose entries these are: the main PC writes them as that user. The
/// register entries name them; a sale with serials or ledgers alone (Task 31) is the ticket's.
fn seller(st: &AppState, registers: &HostRegisters) -> Option<String> {
    registers
        .controlled
        .first()
        .map(|r| r.user_id.clone())
        .or_else(|| registers.jewels.first().map(|r| r.user_id.clone()))
        .or_else(|| {
            let id = registers.document_id.clone();
            st.db
                .read(move |c| pos_core::documents::get(c, &id))
                .ok()
                .map(|ticket| ticket.user_id)
        })
}

/// The call did not get the main PC's answer on these entries: the link is down, or the main PC
/// does not have the till's ticket yet (its outbox is behind or stopped). The later sales are in
/// the same case, so the sending stops there and starts again later in the same order.
fn waits_for_the_link(error: &AppError) -> bool {
    matches!(error, AppError::Sync { .. })
        || matches!(
            error.code().as_str(),
            "host_required" | "replay_rejected" | "document_not_found"
        )
}

/// Sends this till's pending register entries to the main PC, oldest sale first, each as the user
/// who made the sale; an answered sale's entries leave this PC. Stops at the first one the main
/// PC cannot answer yet ([`waits_for_the_link`], logged: it goes again later). One the main PC
/// refuses stays here marked with the refusal (logged) and the next ones go on. Returns how many
/// sales' entries were kept.
pub fn send_pending(st: &AppState) -> usize {
    if !st.lan.is_client() {
        return 0;
    }
    let pending = match st.db.read(pending_host_registers) {
        Ok(pending) => pending,
        Err(error) => {
            tracing::warn!(%error, "the register entries waiting for the main PC cannot be read");
            return 0;
        }
    };
    let mut kept = 0;
    for registers in pending {
        let document_id = registers.document_id.clone();
        let Some(user) = seller(st, &registers) else {
            continue;
        };
        let sent = serde_json::to_value(&registers)
            .map_err(AppError::from)
            .and_then(|input| {
                st.lan.rpc(
                    "registers.record",
                    &user,
                    Vec::new(),
                    serde_json::json!({ "registers": input }),
                    crate::lan::route::RPC_TIMEOUT,
                )
            });
        let booking = match sent {
            // The main PC answers what it booked; the legal registers' entries are kept even
            // when it refuses the serials or the ledgers (final review).
            Ok(reply) => serde_json::from_value::<HostBooking>(reply.output).unwrap_or_default(),
            Err(error) if waits_for_the_link(&error) => {
                tracing::warn!(document = %document_id, code = %error.code(), "the main PC has not kept this sale's register entries yet");
                break;
            }
            Err(error) => {
                let (id, code) = (document_id.clone(), error.code());
                tracing::error!(document = %document_id, code = %code, "the main PC refused this sale's register entries: they wait here for the owner");
                if let Err(error) = st
                    .db
                    .write(move |tx| host_registers_refused(tx, &id, &code))
                {
                    tracing::warn!(document = %document_id, %error, "the main PC's refusal of these register entries is not marked here");
                }
                continue;
            }
        };
        if let Some(code) = &booking.ledgers_refused {
            tracing::error!(document = %document_id, code = %code, "the main PC kept this sale's register entries and refused its ledgers: they wait here for the owner");
        }
        let (id, answered) = (document_id.clone(), booking.clone());
        if let Err(error) = st
            .db
            .write(move |tx| host_registers_answered(tx, &id, &answered))
        {
            tracing::warn!(document = %document_id, %error, "register entries kept by the main PC are still listed here");
            break;
        }
        if booking.ledgers_refused.is_none() {
            kept += 1;
        }
    }
    kept
}

/// This till's tickets whose host entries wait for the main PC, for the card in
/// Settings › Réseau: number, day, what waits and the main PC's refusal; never the entries'
/// health or identity data.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingEntries {
    pub entries: Vec<PendingHostEntry>,
    /// The money calls whose answer was lost (final review), waiting to go again.
    pub calls: Vec<PendingCallInfo>,
    /// How many entries and calls the main PC refused.
    pub refused: u32,
    /// How many refused entries hold legal-register entries (the warning badge).
    pub refused_registers: u32,
}

/// A money call kept on this till after its answer was lost (`route::forward_money`): the host
/// service, when it was made and the main PC's refusal, never its input.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingCallInfo {
    pub service: String,
    pub at: i64,
    /// The main PC answered; its cash waits for this till's drawer to be open.
    pub answered: bool,
    pub refused: Option<String>,
}

/// [`PendingEntries`] of this PC (none on a main PC or a standalone one).
pub fn pending_entries(st: &AppState) -> Result<PendingEntries, AppError> {
    st.require(Permission::SettingsData)?;
    let entries = st.db.read(pending_host_entries)?;
    let calls: Vec<PendingCallInfo> = st
        .db
        .read(pos_core::sync::answers::pending_calls)?
        .into_iter()
        .map(|c| PendingCallInfo {
            service: c.service,
            at: c.at,
            answered: c.output.is_some(),
            refused: c.refused,
        })
        .collect();
    let refused = entries.iter().filter(|e| e.refused.is_some()).count()
        + calls.iter().filter(|c| c.refused.is_some()).count();
    let refused_registers = entries
        .iter()
        .filter(|e| e.refused.is_some() && e.holds_registers())
        .count();
    Ok(PendingEntries {
        entries,
        calls,
        refused: u32::try_from(refused).unwrap_or(u32::MAX),
        refused_registers: u32::try_from(refused_registers).unwrap_or(u32::MAX),
    })
}

/// « Réessayer »: the refusal marks are cleared and every waiting entry goes to the
/// main PC again now. Returns the entries still waiting.
pub fn retry_pending(st: &AppState) -> Result<PendingEntries, AppError> {
    st.require(Permission::SettingsData)?;
    st.db.write(|tx| {
        host_registers_retry(tx)?;
        pos_core::sync::answers::retry_calls(tx).map(drop)
    })?;
    crate::lan::route::replay_calls(st);
    send_pending(st);
    pending_entries(st)
}

/// The host service `registers.record`: the main PC numbers and keeps a till's register entries
/// (as the till's user, who made the sale). Returns what was booked.
pub fn record(st: &AppState, registers: HostRegisters) -> Result<HostBooking, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db
        .write(move |tx| record_host_registers(tx, &ctx, &registers))
}
