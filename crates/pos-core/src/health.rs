//! Start-up health (AUD-006: a clock set back is journaled) and the database snapshot the
//! diagnostic zip carries: settings, versions, counts, the journal check and every other chain
//! check (documents, Z closures, versement receipts), never an identity.

use crate::audit::{self, Actor};
use crate::db::meta_get;
use crate::error::AppError;
use rusqlite::{Connection, Transaction};
use serde::Serialize;
use serde_json::{Map, Value, json};
use specta::Type;

/// A clock resync (NTP, a CMOS drift) moves a few seconds; five minutes back is a change.
pub const CLOCK_TOLERANCE_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClockJump {
    pub last_seen_at: i64,
    pub now: i64,
}

/// Compares `now` (this PC's clock) with the newest journal row. When it is more than
/// [`CLOCK_TOLERANCE_MS`] behind, journals `clock.moved_back { lastSeenAt, now }` dated at the
/// last time seen, so the journal never goes back in time, and returns the jump.
pub fn check_clock(
    tx: &Transaction<'_>,
    actor: &Actor,
    now: i64,
) -> Result<Option<ClockJump>, AppError> {
    let last: Option<i64> = tx.query_row("SELECT MAX(at) FROM audit_log", [], |r| r.get(0))?;
    let Some(last) = last else {
        return Ok(None);
    };
    if now + CLOCK_TOLERANCE_MS >= last {
        return Ok(None);
    }
    let jump = ClockJump {
        last_seen_at: last,
        now,
    };
    audit::append_at(
        tx,
        &actor.event(
            "clock.moved_back",
            "clock",
            None,
            json!({ "lastSeenAt": last, "now": now }),
        ),
        last,
    )?;
    Ok(Some(jump))
}

fn count(conn: &Connection, table: &str) -> Result<i64, AppError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    // `table` comes from the fixed list in `snapshot`, never from input.
    Ok(
        conn.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
            r.get(0)
        })?,
    )
}

/// What support needs to know about the database, and nothing that identifies anybody.
pub fn snapshot(conn: &Connection) -> Result<Value, AppError> {
    let schema: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let mut settings = Map::new();
    let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (key, value) = row?;
        if key == "support.contact" || key.starts_with("company.") {
            continue; // the shop's own phone numbers and social handles stay home
        }
        settings.insert(key, serde_json::from_str(&value).unwrap_or(Value::Null));
    }
    let company = conn
        .query_row(
            "SELECT tax_regime, price_mode, wilaya FROM company WHERE id = 1",
            [],
            |r| {
                Ok(json!({
                    "taxRegime": r.get::<_, String>(0)?,
                    "priceMode": r.get::<_, String>(1)?,
                    "wilaya": r.get::<_, Option<String>>(2)?,
                }))
            },
        )
        .unwrap_or(Value::Null);
    let mut counts = Map::new();
    for table in [
        "products",
        "customers",
        "suppliers",
        "documents",
        "audit_log",
        "users",
        "cash_sessions",
    ] {
        counts.insert(
            table.trim_end_matches("_log").to_owned(),
            json!(count(conn, table)?),
        );
    }
    let all = crate::fiscal::archive::chains(conn)?;
    let journal = &all.audit;
    // Counts and the first place at fault only: no hash, no party, no terminal id.
    let documents: Vec<Value> = all
        .documents
        .iter()
        .map(|c| {
            json!({
                "docType": c.doc_type,
                "series": c.series,
                "rows": c.rows,
                "firstBrokenSeq": c.first_broken_seq,
                "firstBrokenNumber": c.first_broken_number,
            })
        })
        .collect();
    let closures: Vec<Value> = all
        .closures
        .iter()
        .map(|t| {
            json!({
                "terminal": t.terminal_code,
                "rows": t.report.rows,
                "firstBrokenZ": t.report.first_broken_z,
            })
        })
        .collect();
    let till_journals: Vec<Value> = all
        .till_journals
        .iter()
        .map(|t| {
            json!({
                "terminal": t.terminal_code,
                "rows": t.report.rows,
                "broken": t.report.broken,
                "firstBrokenSeq": t.report.first_broken_seq,
            })
        })
        .collect();
    Ok(json!({
        "schemaVersion": schema,
        "appVersion": meta_get(conn, crate::updates::META_APP_VERSION)?,
        "settings": settings,
        "company": company,
        "counts": counts,
        "journal": { "rows": journal.rows, "firstBrokenSeq": journal.first_broken_seq },
        "chains": {
            "documents": documents,
            "closures": closures,
            "receipts": {
                "rows": all.receipts.rows,
                "firstBrokenNumber": all.receipts.first_broken_number,
            },
            "tillJournals": till_journals,
            "tillMerge": { "firstBrokenSeq": all.till_merge_first_broken },
        },
        "lan": lan(conn, crate::time::now_ms())?,
    }))
}

/// The LAN at a glance (M5): this PC's role, its tills (how many, how many called in the last
/// minute), a till's outbox, the open notices by kind. Counts and codes only: no address, no
/// token, no id.
fn lan(conn: &Connection, now: i64) -> Result<Value, AppError> {
    let role = crate::sync::role::current(conn)?;
    let (tills, online): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(last_seen_at IS NOT NULL AND last_seen_at >= ?1), 0)
         FROM lan_devices WHERE revoked_at IS NULL",
        [now - crate::sync::tills::ONLINE_WITHIN_MS],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let depth = crate::sync::outbox::depth(conn)?;
    let mut notices = Map::new();
    let mut stmt = conn.prepare(
        "SELECT kind, COUNT(*) FROM lan_notices WHERE acknowledged_at IS NULL GROUP BY kind ORDER BY kind",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (kind, n) = row?;
        notices.insert(kind, json!(n));
    }
    Ok(json!({
        "role": role.role,
        "tills": tills,
        "tillsOnline": online,
        "outboxPending": depth.pending,
        "outboxRejected": depth.rejected,
        "openNotices": notices,
    }))
}

#[cfg(test)]
mod chains_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn a_clock_set_back_is_journaled_once_per_start_and_in_time_order() {
        let shop = shop();
        let actor = crate::audit::Actor::system(None);
        let last: i64 = shop.count("SELECT MAX(at) FROM audit_log");
        let fine = shop.db.write({
            let actor = actor.clone();
            move |tx| check_clock(tx, &actor, last + 1_000)
        });
        assert_eq!(fine.unwrap(), None);
        let slightly = shop.db.write({
            let actor = actor.clone();
            move |tx| check_clock(tx, &actor, last - 60_000)
        });
        assert_eq!(
            slightly.unwrap(),
            None,
            "within five minutes: a clock resync, not a change"
        );
        let back = last - 3 * 86_400_000;
        let jump = shop
            .db
            .write(move |tx| check_clock(tx, &actor, back))
            .unwrap();
        assert_eq!(
            jump,
            Some(ClockJump {
                last_seen_at: last,
                now: back
            })
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'clock.moved_back'"),
            1
        );
        assert_eq!(
            shop.count("SELECT at FROM audit_log WHERE action = 'clock.moved_back'"),
            last,
            "the row carries the last time seen, never a time before it"
        );
        let report = shop.db.read(crate::audit::verify_chain).unwrap();
        assert_eq!(report.first_broken_seq, None);
    }

    /// The main PC's integrity checks walk every till's journal and its merge chain (M5): a
    /// rewritten `lan_journal` row shows in `chains()` and in the diagnostic snapshot, which
    /// also says how the LAN is doing, by code and count only.
    #[test]
    fn a_rewritten_till_journal_row_surfaces_in_the_chains_and_the_snapshot() {
        let lan = crate::sync::testkit::lan_pair();
        lan.open_till_session(0);
        assert!(!lan.push().is_empty());
        let chains = lan.host.db.read(crate::fiscal::archive::chains).unwrap();
        assert_eq!(chains.till_journals.len(), 1);
        assert_eq!(chains.till_journals[0].terminal_code, "C2");
        assert_eq!(chains.till_journals[0].report.broken, 0);
        assert_eq!(chains.till_merge_first_broken, None);
        let healthy = lan.host.db.read(snapshot).unwrap();
        assert_eq!(healthy["lan"]["role"], "host");
        assert_eq!(healthy["lan"]["tills"], 1);
        assert_eq!(healthy["chains"]["tillJournals"][0]["broken"], 0);
        lan.host
            .db
            .write(|tx| {
                Ok(tx.execute_batch(
                    "DROP TRIGGER lan_journal_no_update;
                     UPDATE lan_journal SET payload = '{\"amount\":1}', received_at = received_at + 1
                     WHERE merge_seq = (SELECT MIN(merge_seq) FROM lan_journal);",
                )?)
            })
            .unwrap();
        let chains = lan.host.db.read(crate::fiscal::archive::chains).unwrap();
        assert_eq!(chains.till_journals[0].report.broken, 1);
        assert!(chains.till_merge_first_broken.is_some());
        let broken = lan.host.db.read(snapshot).unwrap();
        assert_eq!(broken["chains"]["tillJournals"][0]["terminal"], "C2");
        assert_eq!(broken["chains"]["tillJournals"][0]["broken"], 1);
        assert!(!broken["chains"]["tillMerge"]["firstBrokenSeq"].is_null());
        let text = broken.to_string();
        assert!(
            !text.contains(&lan.till_terminal.id),
            "a till is named by its code, never its id"
        );
    }

    #[test]
    fn the_snapshot_holds_settings_and_counts_but_no_identity() {
        use crate::customers::credit::{self, CollectInput};
        use crate::documents::DocType;
        use crate::documents::tests_support::validated_of;
        let shop = shop();
        let karim = shop.customer("Karim Benali");
        validated_of(&shop.db, DocType::Invoice);
        let (ctx, id) = (shop.owner_till(), karim.id.clone());
        shop.db
            .write(move |tx| credit::set_opening(tx, &ctx, &id, crate::money::Money(500_000), None))
            .unwrap();
        shop.open_till(0);
        let ctx = shop.till();
        let versement = CollectInput {
            customer_id: karim.id,
            amount: crate::money::Money(200_000),
            method: crate::payments::PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            allocate_to: Vec::new(),
            note: None,
        };
        shop.db
            .write(move |tx| credit::collect(tx, &ctx, versement))
            .unwrap();
        let intact = shop.db.read(snapshot).unwrap();
        assert_eq!(
            intact["chains"]["receipts"],
            serde_json::json!({ "rows": 1, "firstBrokenNumber": null })
        );
        assert!(
            intact["chains"]["documents"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["firstBrokenSeq"].is_null())
        );
        shop.db
            .write(|tx| {
                Ok(tx.execute_batch(
                    "DROP TRIGGER customer_ledger_no_update;
                     UPDATE customer_ledger SET amount = -100000 WHERE receipt_number IS NOT NULL;
                     DROP TRIGGER documents_final_once_validated;
                     UPDATE documents SET hash = prev_hash WHERE doc_type = 'invoice';",
                )?)
            })
            .unwrap();
        let broken = shop.db.read(snapshot).unwrap();
        assert_eq!(
            broken["chains"]["receipts"]["firstBrokenNumber"],
            serde_json::json!("RV2026-000001")
        );
        assert!(
            broken["chains"]["documents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["docType"] == "invoice" && !c["firstBrokenSeq"].is_null()),
            "{}",
            broken["chains"]
        );
        let snapshot = broken;
        let text = snapshot.to_string();
        assert_eq!(snapshot["counts"]["customers"], serde_json::json!(1));
        assert_eq!(snapshot["company"]["taxRegime"], serde_json::json!("reel"));
        assert!(snapshot["settings"].is_object());
        assert_eq!(
            snapshot["journal"]["firstBrokenSeq"],
            serde_json::Value::Null
        );
        for leaked in ["Karim", "El Amel", "000116001234567", "Larbi Ben M'hidi"] {
            assert!(!text.contains(leaked), "{leaked} in the snapshot");
        }
    }
}
