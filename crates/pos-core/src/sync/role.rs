//! This PC's place on the shop's network (spec §4.6): a single PC (the default, nothing runs), a
//! main PC (serves the tills; capture triggers on), a till (sells on its copy; its own capture and
//! guard triggers, an outbox). Changing role is an owner's decision, journaled.

use super::capture::{self, CaptureSet};
use super::changelog::{self, LAN_OUTBOX_CONSUMER};
use super::outbox;
use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::numbering::{self, Skip};
use crate::terminal::{self, TerminalRole};
use rusqlite::{Connection, Transaction, params};
use serde::Serialize;
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LanRole {
    pub role: TerminalRole,
    pub host_id: Option<String>,
    pub host_name: Option<String>,
    pub host_address: Option<String>,
    pub applied_seq: i64,
    pub paired_at: Option<i64>,
    pub last_contact_at: Option<i64>,
    pub skew_ms: Option<i64>,
    pub host_schema: Option<i64>,
}

pub fn current(conn: &Connection) -> Result<LanRole, AppError> {
    Ok(conn.query_row(
        "SELECT role, host_id, host_name, host_address, applied_seq, paired_at, last_contact_at, skew_ms, host_schema
         FROM lan_local WHERE id = 1",
        [],
        |r| {
            Ok(LanRole {
                role: r.get(0)?,
                host_id: r.get(1)?,
                host_name: r.get(2)?,
                host_address: r.get(3)?,
                applied_seq: r.get(4)?,
                paired_at: r.get(5)?,
                last_contact_at: r.get(6)?,
                skew_ms: r.get(7)?,
                host_schema: r.get(8)?,
            })
        },
    )?)
}

pub fn is_client(conn: &Connection) -> Result<bool, AppError> {
    Ok(current(conn)?.role == TerminalRole::Client)
}

pub fn seeding_allowed(conn: &Connection) -> Result<bool, AppError> {
    Ok(!is_client(conn)?)
}

pub fn capture_set(conn: &Connection) -> Result<Option<CaptureSet>, AppError> {
    Ok(match current(conn)?.role {
        TerminalRole::Standalone => None,
        TerminalRole::Host => Some(CaptureSet::Host),
        TerminalRole::Client => {
            let local =
                terminal::local(conn)?.ok_or_else(|| AppError::conflict("lan_not_paired"))?;
            Some(CaptureSet::Client {
                terminal_id: local.id,
            })
        }
    })
}

fn local_id(conn: &Connection) -> Result<String, AppError> {
    Ok(terminal::local(conn)?
        .ok_or_else(|| AppError::conflict("setup_required"))?
        .id)
}

fn require(conn: &Connection, role: TerminalRole) -> Result<LanRole, AppError> {
    let now = current(conn)?;
    if now.role != role {
        return Err(AppError::conflict("lan_role_invalid"));
    }
    Ok(now)
}

pub fn become_host(tx: &Transaction<'_>, actor: &Actor, now: i64) -> Result<String, AppError> {
    require(tx, TerminalRole::Standalone)?;
    let host_id = new_id();
    tx.execute(
        "UPDATE lan_local SET role = 'host', host_id = ?1, updated_at = ?2 WHERE id = 1",
        params![host_id, now],
    )?;
    tx.execute(
        "UPDATE terminals SET role = 'host' WHERE id = ?1",
        [local_id(tx)?],
    )?;
    capture::install(tx, &CaptureSet::Host)?;
    audit::append(
        tx,
        &actor.event("lan.host_enabled", "lan", Some(&host_id), json!({})),
    )?;
    Ok(host_id)
}

pub fn active_tills(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM lan_devices WHERE revoked_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

/// A main PC with paired tills (not revoked) stays one: it holds their books (their tickets, Z
/// and journals), so it neither becomes a single PC nor joins another main PC as a till, which
/// would archive those books and orphan the tills (`lan_tills_paired`, with their count).
pub fn host_releasable(conn: &Connection) -> Result<(), AppError> {
    if current(conn)?.role != TerminalRole::Host {
        return Ok(());
    }
    let tills = active_tills(conn)?;
    if tills > 0 {
        return Err(AppError::rejected(
            json!({ "tills": tills }),
            "lan_tills_paired",
        ));
    }
    Ok(())
}

pub fn become_standalone(tx: &Transaction<'_>, actor: &Actor, now: i64) -> Result<(), AppError> {
    let was = require(tx, TerminalRole::Host)?;
    host_releasable(tx)?;
    capture::uninstall(tx)?;
    tx.execute(
        "UPDATE lan_local SET role = 'standalone', host_id = NULL, updated_at = ?1 WHERE id = 1",
        [now],
    )?;
    tx.execute(
        "UPDATE terminals SET role = 'standalone' WHERE id = ?1",
        [local_id(tx)?],
    )?;
    audit::append(
        tx,
        &actor.event(
            "lan.host_disabled",
            "lan",
            was.host_id.as_deref(),
            json!({}),
        ),
    )?;
    Ok(())
}

/// Numbers the old main PC may have printed after this PC last heard from it: every counter it
/// continued (the company's documents and codes, its own tickets, return notes and Z) jumps this
/// far ahead at a promotion, journaled in `lan.promoted` (`numbering::journaled_skips`).
pub const PROMOTION_MARGIN: i64 = 100;

/// Moves the counters of `series` past [`PROMOTION_MARGIN`]: the latest key of each counter (or
/// this year's, when the counter restarts yearly and has none yet), as skips of `promotion_id`.
fn skip_after_promotion(
    tx: &Transaction<'_>,
    series: &str,
    promotion_id: &str,
    now: i64,
) -> Result<Vec<Skip>, AppError> {
    let year_now = i64::from(crate::time::fiscal_year(now));
    let counters: Vec<(String, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT doc_type, MAX(fiscal_year) FROM counters WHERE series = ?1
             GROUP BY doc_type ORDER BY doc_type",
        )?;
        stmt.query_map([series], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut skipped = Vec::new();
    for (counter, latest) in counters {
        let year = if latest == 0 { 0 } else { latest.max(year_now) };
        let last: i64 = tx.query_row(
            "SELECT COALESCE((SELECT last_number FROM counters
                              WHERE doc_type = ?1 AND series = ?2 AND fiscal_year = ?3), 0)",
            params![counter, series, year],
            |r| r.get(0),
        )?;
        if let Some(skip) = numbering::fast_forward(
            tx,
            &counter,
            series,
            year,
            last.saturating_add(PROMOTION_MARGIN),
            promotion_id,
            now,
        )? {
            skipped.push(skip);
        }
    }
    Ok(skipped)
}

/// The main PC is gone for good: this till becomes the main PC (LAN-010). The caller has checked
/// that the old one does not answer and the owner confirmed with the PIN. The company's counters
/// and the old main PC's series jump [`PROMOTION_MARGIN`] numbers ahead, so a number the old
/// main PC printed after this PC's last pull is never issued twice; the refused items of this
/// till's outbox are journaled with their reason before they are closed.
pub fn promote(tx: &Transaction<'_>, actor: &Actor, now: i64) -> Result<String, AppError> {
    let was = require(tx, TerminalRole::Client)?;
    let depth = outbox::depth(tx)?;
    let host_id = new_id();
    let rejected: Vec<serde_json::Value> = {
        let mut stmt = tx.prepare(
            "SELECT idempotency_key, local_seq, last_error FROM lan_outbox WHERE status = 'rejected'
             ORDER BY local_seq",
        )?;
        stmt.query_map([], |r| {
            Ok(json!({
                "key": r.get::<_, String>(0)?,
                "localSeq": r.get::<_, i64>(1)?,
                "lastError": r.get::<_, Option<String>>(2)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    tx.execute(
        "UPDATE lan_outbox SET status = 'sent', sent_at = ?1, host_id = ?2, last_error = NULL
         WHERE status IN ('pending', 'rejected')",
        params![now, host_id],
    )?;
    changelog::unregister(tx, LAN_OUTBOX_CONSUMER)?;
    tx.execute("DELETE FROM changelog", [])?;
    capture::install(tx, &CaptureSet::Host)?;
    let me = local_id(tx)?;
    let old_hosts: Vec<String> = {
        let mut stmt = tx
            .prepare("SELECT code FROM terminals WHERE role = 'host' AND id <> ?1 ORDER BY code")?;
        stmt.query_map([&me], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };
    let mut skipped = skip_after_promotion(tx, "", &host_id, now)?;
    for code in &old_hosts {
        skipped.extend(skip_after_promotion(tx, code, &host_id, now)?);
    }
    tx.execute(
        "UPDATE terminals SET role = 'standalone' WHERE role = 'host' AND id <> ?1",
        [&me],
    )?;
    tx.execute("UPDATE terminals SET role = 'host' WHERE id = ?1", [&me])?;
    tx.execute(
        "UPDATE lan_local SET role = 'host', host_id = ?1, host_name = NULL, host_address = NULL,
                paired_at = NULL, last_contact_at = NULL, skew_ms = NULL, host_schema = NULL, updated_at = ?2
         WHERE id = 1",
        params![host_id, now],
    )?;
    audit::append(
        tx,
        &actor.event(
            "lan.promoted",
            "lan",
            Some(&host_id),
            json!({
                "oldHostId": was.host_id,
                "oldHosts": old_hosts,
                "pending": depth.pending,
                "rejected": depth.rejected,
                "rejectedItems": rejected,
                "appliedSeq": was.applied_seq,
                "skipped": skipped,
            }),
        ),
    )?;
    Ok(host_id)
}

pub fn leave(tx: &Transaction<'_>, actor: &Actor, _now: i64) -> Result<(), AppError> {
    let was = require(tx, TerminalRole::Client)?;
    let depth = outbox::depth(tx)?;
    if depth.pending > 0 || depth.rejected > 0 {
        return Err(AppError::rejected(
            json!({ "pending": depth.pending, "rejected": depth.rejected }),
            "lan_outbox_not_empty",
        ));
    }
    audit::append(
        tx,
        &actor.event("lan.left", "lan", was.host_id.as_deref(), json!({})),
    )?;
    Ok(())
}

/// M0's restore would rewind a till behind its main PC, or a main PC behind its tills.
pub fn restore_allowed(conn: &Connection) -> Result<(), AppError> {
    match current(conn)?.role {
        TerminalRole::Client => Err(AppError::conflict("lan_restore_blocked")),
        TerminalRole::Host if active_tills(conn)? > 0 => {
            Err(AppError::conflict("lan_restore_blocked"))
        }
        _ => Ok(()),
    }
}

/// The series of a counter a till increments for itself (versement receipts): its code on a till.
pub fn receipt_series(conn: &Connection, terminal_code: &str) -> Result<String, AppError> {
    Ok(if is_client(conn)? {
        terminal_code.to_owned()
    } else {
        String::new()
    })
}

pub fn set_contact(
    tx: &Transaction<'_>,
    now: i64,
    skew_ms: Option<i64>,
    host_schema: Option<i64>,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE lan_local SET last_contact_at = ?1, skew_ms = COALESCE(?2, skew_ms),
                host_schema = COALESCE(?3, host_schema), updated_at = ?1 WHERE id = 1",
        params![now, skew_ms, host_schema],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::money::Money;
    use crate::payments::PaymentMethod;
    use crate::sync::capture::installed;
    use crate::sync::outbox;
    use crate::sync::testkit::lan_pair;
    use crate::testkit::{NOON, shop};

    #[test]
    fn a_single_pc_becomes_the_main_pc_and_back() {
        let shop = shop();
        assert_eq!(
            shop.db.read(current).unwrap().role,
            TerminalRole::Standalone
        );
        assert!(shop.db.read(capture_set).unwrap().is_none());
        let actor = shop.owner.actor();
        let host_id = shop
            .db
            .write(move |tx| become_host(tx, &actor, NOON))
            .unwrap();
        let role = shop.db.read(current).unwrap();
        assert_eq!(
            (role.role, role.host_id.as_deref()),
            (TerminalRole::Host, Some(host_id.as_str()))
        );
        assert!(!shop.db.read(installed).unwrap().is_empty());
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM terminals WHERE role = 'host'"),
            1
        );
        let actor = shop.owner.actor();
        assert_eq!(
            shop.db
                .write(move |tx| become_host(tx, &actor, NOON))
                .unwrap_err()
                .code(),
            "lan_role_invalid"
        );
        shop.db
            .write(|tx| {
                tx.execute_batch(
                    "INSERT INTO terminals (id, code, name, role, created_at) VALUES ('00000000-0000-7000-8000-0000000000c2', 'C2', 'Caisse 2', 'client', 0);
                     INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at)
                       VALUES ('00000000-0000-7000-8000-0000000000c2', 'Caisse 2', 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 0);",
                )?;
                Ok(())
            })
            .unwrap();
        let actor = shop.owner.actor();
        assert_eq!(
            shop.db
                .write(move |tx| become_standalone(tx, &actor, NOON))
                .unwrap_err()
                .code(),
            "lan_tills_paired"
        );
        assert_eq!(
            shop.db.read(restore_allowed).unwrap_err().code(),
            "lan_restore_blocked"
        );
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                crate::sync::devices::revoke(
                    tx,
                    &actor,
                    "00000000-0000-7000-8000-0000000000c2",
                    NOON,
                )
            })
            .unwrap();
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| become_standalone(tx, &actor, NOON))
            .unwrap();
        assert!(shop.db.read(installed).unwrap().is_empty());
        shop.db.read(restore_allowed).unwrap();
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action IN ('lan.host_enabled', 'lan.host_disabled')"),
            2
        );
    }

    #[test]
    fn a_till_promoted_to_main_pc_keeps_its_rows_and_sheds_its_guards() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        lan.open_till_session(0);
        lan.sell_on_till(&milk.id, 1_000, 10_500); // the main PC died before this reached it
        assert_eq!(
            lan.till.read(restore_allowed).unwrap_err().code(),
            "lan_restore_blocked"
        );
        let actor = lan.till_owner.actor();
        let new_host = lan.till.write(move |tx| promote(tx, &actor, NOON)).unwrap();
        let role = lan.till.read(current).unwrap();
        assert_eq!(
            (role.role, role.host_id.as_deref()),
            (TerminalRole::Host, Some(new_host.as_str()))
        );
        assert!(
            lan.till
                .read(|c| outbox::pending(c, 10))
                .unwrap()
                .is_empty(),
            "its own rows are the books now"
        );
        assert_eq!(
            lan.count_till(
                "SELECT COUNT(*) FROM lan_outbox WHERE status = 'sent' AND host_id IS NOT NULL"
            ),
            2
        );
        lan.till
            .write(|tx| Ok(tx.execute("UPDATE products SET sell_price = 12000", [])?))
            .unwrap();
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM sync_cursors"),
            0,
            "no outbox reader any more"
        );
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM terminals WHERE role = 'host'"),
            1,
            "only this PC serves"
        );
        let payload: String = lan
            .till
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'lan.promoted'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["oldHostId"], crate::sync::testkit::HOST_ID);
        assert_eq!(payload["pending"], 2);
    }

    #[test]
    fn a_till_leaves_only_with_an_empty_outbox() {
        let lan = lan_pair();
        lan.open_till_session(0);
        let actor = lan.till_owner.actor();
        assert_eq!(
            lan.till
                .write(move |tx| leave(tx, &actor, NOON))
                .unwrap_err()
                .code(),
            "lan_outbox_not_empty"
        );
        lan.push();
        let actor = lan.till_owner.actor();
        lan.till.write(move |tx| leave(tx, &actor, NOON)).unwrap();
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM audit_log WHERE action = 'lan.left'"),
            1
        );
    }

    #[test]
    fn a_versement_on_a_till_is_numbered_in_its_own_series_and_recorded_on_the_host() {
        let lan = lan_pair();
        let karim = lan.host.customer("Karim");
        lan.pull();
        lan.open_till_session(0);
        let ctx =
            crate::till::TillContext::new(lan.till_owner.clone(), lan.host.pack.clone(), NOON);
        let input = crate::customers::credit::CollectInput {
            customer_id: karim.id.clone(),
            amount: Money(5_000),
            method: PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            allocate_to: Vec::new(),
            note: None,
        };
        let collected = lan
            .till
            .write(move |tx| crate::customers::credit::collect(tx, &ctx, input))
            .unwrap();
        assert_eq!(
            collected.entry.receipt_number.as_deref(),
            Some("RV2026-C2-000001")
        );
        let results = lan.push();
        assert!(
            results
                .iter()
                .all(|r| r.status == outbox::ItemStatus::Applied),
            "{results:?}"
        );
        assert_eq!(
            lan.count_host(
                "SELECT COUNT(*) FROM customer_ledger WHERE receipt_number = 'RV2026-C2-000001'"
            ),
            1
        );
        assert_eq!(
            lan.count_host("SELECT balance FROM customers"),
            -5_000,
            "an advance of 50 DA"
        );
        let _ = Actor::default();
    }

    #[test]
    fn a_till_seeds_nothing_at_start() {
        let lan = lan_pair();
        let terminal = crate::setup::bootstrap(&lan.till).unwrap();
        assert_eq!(terminal.id, lan.till_terminal.id);
        assert!(!lan.till.read(seeding_allowed).unwrap());
        assert_eq!(lan.till.read(|c| receipt_series(c, "C2")).unwrap(), "C2");
        assert_eq!(lan.host.db.read(|c| receipt_series(c, "C1")).unwrap(), "");
    }
}
