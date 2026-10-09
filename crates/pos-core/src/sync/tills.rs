//! The main PC's view of its tills (spec §15 "till dashboard"): who they are, when they last
//! called, what they still have to send, their clock and version, their journal, and each
//! till's Z list (CSH-009, RPT-042).

use super::notices::NoticeKind;
use crate::cash::closing::ZReport;
use crate::error::AppError;
use crate::money::Money;
use rusqlite::{Connection, params};
use serde::Serialize;
use specta::Type;

pub const ONLINE_WITHIN_MS: i64 = 60_000;

/// A till silent this long (or never seen since it paired) holds the change feed back: its
/// cursor does not move, so the feed and the commit marks are never pruned past it.
pub const PINS_FEED_AFTER_MS: i64 = 3 * 24 * 3_600_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TillInfo {
    pub terminal_id: String,
    pub code: String,
    pub name: String,
    pub online: bool,
    pub last_seen_at: Option<i64>,
    pub last_address: Option<String>,
    pub app_version: Option<String>,
    pub schema_version: Option<i64>,
    pub outbox_pending: i64,
    pub skew_ms: Option<i64>,
    pub last_replayed_seq: Option<i64>,
    pub revoked: bool,
    pub paired_at: i64,
    /// No row of the till's journal failed its check on arrival (`lan_journal.intact`): replay
    /// raises a `journal_broken` notice in the transaction that stores such a row, and notices
    /// are never deleted. The full recomputation of every row is `journal::verify`, left to the
    /// integrity checks.
    pub journal_intact: bool,
    pub notices_open: i64,
    /// Not revoked and never seen, or silent for [`PINS_FEED_AFTER_MS`]: the owner revokes it
    /// (a join that failed after pairing, a till gone for good) so the feed can be pruned.
    pub pins_feed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TillZ {
    pub session_id: String,
    pub z_number: String,
    pub business_date: String,
    pub closed_at: i64,
    pub closed_by_name: String,
    pub net_ttc: Money,
    pub variance: Money,
    pub grand_total: Money,
    pub offline_pending: Option<i64>,
}

/// Every paired till, for a dashboard that refreshes every few seconds: its cost does not grow
/// with the tills' journals (see [`TillInfo::journal_intact`]).
pub fn list(conn: &Connection, now: i64) -> Result<Vec<TillInfo>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT d.terminal_id, t.code, d.name, d.last_seen_at, d.last_address, d.app_version, d.schema_version,
                d.outbox_pending, d.skew_ms, d.last_replayed_seq, d.revoked_at IS NOT NULL, d.paired_at,
                (SELECT COUNT(*) FROM lan_notices n WHERE n.terminal_id = d.terminal_id AND n.acknowledged_at IS NULL),
                NOT EXISTS (SELECT 1 FROM lan_notices b WHERE b.terminal_id = d.terminal_id AND b.kind = ?1)
         FROM lan_devices d JOIN terminals t ON t.id = d.terminal_id
         ORDER BY d.revoked_at IS NOT NULL, t.code",
    )?;
    let rows = stmt
        .query_map([NoticeKind::JournalBroken], |r| {
            let last_seen_at: Option<i64> = r.get(3)?;
            let revoked: bool = r.get(10)?;
            Ok(TillInfo {
                terminal_id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                online: last_seen_at.is_some_and(|at| now - at <= ONLINE_WITHIN_MS),
                last_seen_at,
                last_address: r.get(4)?,
                app_version: r.get(5)?,
                schema_version: r.get(6)?,
                outbox_pending: r.get(7)?,
                skew_ms: r.get(8)?,
                last_replayed_seq: r.get(9)?,
                revoked,
                paired_at: r.get(11)?,
                journal_intact: r.get(13)?,
                notices_open: r.get(12)?,
                pins_feed: !revoked && last_seen_at.is_none_or(|at| now - at > PINS_FEED_AFTER_MS),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn closures(conn: &Connection, terminal_id: &str, limit: u32) -> Result<Vec<TillZ>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT session_id, z_number, business_date, closed_at, report FROM cash_closures
         WHERE terminal_id = ?1 ORDER BY z_seq DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![terminal_id, i64::from(limit.clamp(1, 500))], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(session_id, z_number, business_date, closed_at, report)| {
            let z: ZReport = serde_json::from_str(&report)?;
            Ok(TillZ {
                session_id,
                z_number,
                business_date,
                closed_at,
                closed_by_name: z.closed_by_name,
                net_ttc: z.summary.net_ttc,
                variance: z.variance,
                grand_total: z.grand_total,
                offline_pending: z.offline_pending,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cash::closing::{CloseInput, close_session, verify_closures};
    use crate::money::Money;
    use crate::sync::testkit::lan_pair;
    use crate::testkit::NOON;

    #[test]
    fn a_tills_z_is_listed_and_verified_on_the_main_pc() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        lan.open_till_session(0);
        lan.sell_on_till(&milk.id, 1_000, 10_500);
        let ctx = lan.till_ctx();
        lan.till
            .write(move |tx| {
                close_session(
                    tx,
                    &ctx,
                    CloseInput {
                        counts: vec![crate::cash::closing::CountLine {
                            denomination: Money(10_500),
                            count: 1,
                        }],
                        notes: None,
                    },
                )
            })
            .unwrap();
        lan.push();
        let c2 = lan.till_terminal.id.clone();
        let zs = lan
            .host
            .db
            .read({
                let c2 = c2.clone();
                move |c| closures(c, &c2, 10)
            })
            .unwrap();
        assert_eq!(zs.len(), 1);
        assert_eq!(
            (zs[0].z_number.as_str(), zs[0].net_ttc),
            ("ZC2-000001", Money(10_000))
        );
        let host_view = lan
            .host
            .db
            .read({
                let c2 = c2.clone();
                move |c| verify_closures(c, &c2)
            })
            .unwrap();
        assert_eq!(
            host_view.first_broken_z, None,
            "anchored in the till's journal kept on the main PC"
        );
        let till_view = lan
            .till
            .read({
                let c2 = c2.clone();
                move |c| verify_closures(c, &c2)
            })
            .unwrap();
        assert_eq!(till_view.first_broken_z, None);
        let tills = lan.host.db.read(move |c| list(c, NOON + 2_000)).unwrap();
        assert_eq!(tills.len(), 1);
        let till = &tills[0];
        assert_eq!(
            (
                till.code.as_str(),
                till.online,
                till.journal_intact,
                till.revoked
            ),
            ("C2", true, true, false)
        );
        let later = lan
            .host
            .db
            .read(move |c| list(c, NOON + 3_600_000))
            .unwrap();
        assert!(!later[0].online, "no contact for an hour");
        assert!(!later[0].pins_feed, "an hour is not long silent");
        let days = lan
            .host
            .db
            .read(move |c| list(c, NOON + PINS_FEED_AFTER_MS + 2_000))
            .unwrap();
        assert!(days[0].pins_feed, "silent for days: it holds the feed back");
    }

    /// The till dashboard refreshes every few seconds: it reads the checks made when each journal
    /// row arrived, and leaves the full walk over every till's whole journal (`journal::verify`,
    /// one SHA-256 per row ever received) to the integrity checks.
    mod journal_state {
        use super::*;
        use crate::audit::Actor;
        use crate::sync::notices::{self, NoticeKind};
        use crate::sync::replay::{Received, apply_batch};
        use crate::sync::testkit::LanPair;
        use crate::sync::{journal, outbox};

        fn intact_on_dashboard(lan: &LanPair) -> bool {
            let tills = lan.host.db.read(|c| list(c, NOON)).unwrap();
            assert_eq!(tills.len(), 1);
            tills[0].journal_intact
        }

        #[test]
        fn a_journal_that_arrived_broken_stays_flagged_on_the_dashboard() {
            let lan = lan_pair();
            lan.open_till_session(0);
            let mut items = lan.till.read(|c| outbox::pending(c, 10)).unwrap();
            items[0].bundle.audit[0].payload = r#"{"edited":true}"#.to_owned();
            apply_batch(
                &lan.host.db,
                &lan.sender(),
                items,
                &Received {
                    at: NOON + 1_000,
                    skew_ms: Some(0),
                },
            )
            .unwrap();
            assert!(
                !intact_on_dashboard(&lan),
                "its hash no longer matched on arrival"
            );
            let open = lan.host.db.read(|c| notices::list(c, false, 10)).unwrap();
            let broken = open
                .iter()
                .find(|n| n.kind == NoticeKind::JournalBroken)
                .unwrap()
                .id
                .clone();
            lan.host
                .db
                .write(move |tx| notices::acknowledge(tx, &Actor::system(None), &broken, NOON))
                .unwrap();
            assert!(
                !intact_on_dashboard(&lan),
                "acknowledging the notice does not mend the journal"
            );
        }

        #[test]
        fn the_dashboard_leaves_the_full_walk_to_the_integrity_check() {
            let lan = lan_pair();
            lan.open_till_session(0);
            lan.push();
            assert!(intact_on_dashboard(&lan));
            // A row rewritten after it arrived (the append-only trigger dropped by hand).
            lan.host
                .db
                .write(|tx| {
                    tx.execute_batch(
                        "DROP TRIGGER lan_journal_no_update;
                         UPDATE lan_journal SET payload = '{\"edited\":true}';",
                    )?;
                    Ok(())
                })
                .unwrap();
            assert!(
                intact_on_dashboard(&lan),
                "the dashboard does not re-hash every row it ever received"
            );
            let t = lan.till_terminal.id.clone();
            let report = lan.host.db.read(move |c| journal::verify(c, &t)).unwrap();
            assert_eq!(
                (report.broken, report.first_broken_seq.is_some()),
                (1, true),
                "the integrity check recomputes every row"
            );
        }
    }
}
