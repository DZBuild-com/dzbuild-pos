//! The tills' journals on the main PC (spec §5.3, M5). A till keeps its own hash-chained
//! `audit_log`; each row reaches the host in the till's outbox and is stored in `lan_journal`
//! as it was written: its link to the till's previous row (or to the anchor recorded when the
//! till got its snapshot, which a re-paired till's new snapshot moves past the rows it sent
//! before) and its own hash are checked on arrival and the result kept in
//! `intact` (the row is stored either way: evidence is never dropped). The host then chains the
//! rows of every till again in arrival order (`merge_hash`), so deleting or rewriting a stored
//! row breaks a chain only the host holds. A Z closed on a till anchors into this journal.

use super::outbox::JournalImage;
use crate::audit::{self, Actor, ChainFields, GENESIS_HASH};
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appended {
    /// Where the row sits in the host's merge chain (for a `collided` row: the row holding its
    /// `seq`).
    pub merge_seq: i64,
    pub intact: bool,
    pub duplicate: bool,
    /// Another row of the till already holds this `seq` (the till went back to an older copy
    /// of its database and wrote again): `UNIQUE (terminal_id, seq)` keeps the first one, so
    /// this one is not stored; the caller keeps what it was and raises the journal notice.
    pub collided: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JournalReport {
    pub rows: i64,
    /// Rows stored with `intact = 0` or failing the check now.
    pub broken: i64,
    pub first_broken_seq: Option<i64>,
    /// The tail the till's next row links to: its last row after the device's anchor, or the
    /// anchor itself.
    pub last_seq: i64,
    pub last_hash: String,
}

/// `hex(SHA-256(prev ␟ terminal ␟ seq ␟ hash ␟ received_at))`.
pub fn merge_hash(prev: &str, terminal_id: &str, seq: i64, hash: &str, received_at: i64) -> String {
    let mut hasher = Sha256::new();
    for (i, part) in [
        prev,
        terminal_id,
        &seq.to_string(),
        hash,
        &received_at.to_string(),
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            hasher.update([0x1f_u8]);
        }
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize().as_slice())
}

fn own_hash(row: &JournalImage) -> String {
    ChainFields {
        prev_hash: &row.prev_hash,
        seq: row.seq,
        at: row.at,
        user_id: row.user_id.as_deref(),
        terminal_id: row.terminal_id.as_deref(),
        action: &row.action,
        entity_type: &row.entity_type,
        entity_id: row.entity_id.as_deref(),
        payload: &row.payload,
    }
    .hash()
}

/// The device's journal anchor: the journal tail of the last snapshot the till was given (the
/// start of its current pairing), genesis when the terminal is not a paired till.
fn device_anchor(conn: &Connection, terminal_id: &str) -> Result<(i64, String), AppError> {
    Ok(conn
        .query_row(
            "SELECT journal_anchor_seq, journal_anchor_hash FROM lan_devices WHERE terminal_id = ?1",
            [terminal_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or_else(|| (0, GENESIS_HASH.to_owned())))
}

/// The row the till's next row must link to: its last stored row, unless a later snapshot
/// re-anchored the till (a re-paired till keeps its terminal) at or past that row.
fn anchor(conn: &Connection, terminal_id: &str) -> Result<(i64, String), AppError> {
    let stored: Option<(i64, String)> = conn
        .query_row(
            "SELECT seq, hash FROM lan_journal WHERE terminal_id = ?1 ORDER BY seq DESC LIMIT 1",
            [terminal_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let device = device_anchor(conn, terminal_id)?;
    Ok(match stored {
        Some(last) if last.0 > device.0 => last,
        _ => device,
    })
}

/// Stores a till's row with its arrival check. A caller storing a row that is not `intact`
/// raises `NoticeKind::JournalBroken` in the same transaction (replay does): the till dashboard
/// (`tills::list`) reads that notice instead of walking every journal again. A row over a `seq`
/// the host already holds for the till under another id is not stored (`collided`): the caller
/// keeps what it was and raises the same notice.
pub fn append(
    tx: &Transaction<'_>,
    terminal_id: &str,
    row: &JournalImage,
    received_at: i64,
) -> Result<Appended, AppError> {
    let existing: Option<(i64, bool)> = tx
        .query_row(
            "SELECT merge_seq, intact FROM lan_journal WHERE id = ?1",
            [&row.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((merge_seq, intact)) = existing {
        return Ok(Appended {
            merge_seq,
            intact,
            duplicate: true,
            collided: false,
        });
    }
    let holder: Option<i64> = tx
        .query_row(
            "SELECT merge_seq FROM lan_journal WHERE terminal_id = ?1 AND seq = ?2",
            params![terminal_id, row.seq],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(merge_seq) = holder {
        return Ok(Appended {
            merge_seq,
            intact: false,
            duplicate: false,
            collided: true,
        });
    }
    let (prev_seq, prev_hash) = anchor(tx, terminal_id)?;
    let sound = own_hash(row) == row.hash;
    let mut intact = row.seq == prev_seq + 1 && row.prev_hash == prev_hash && sound;
    if !intact && sound && adoptable(tx, terminal_id)? && row.seq >= 1 {
        adopt(tx, terminal_id, row)?;
        intact = true;
    }
    let merge_prev: String = tx
        .query_row(
            "SELECT merge_hash FROM lan_journal ORDER BY merge_seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| GENESIS_HASH.to_owned());
    let merge = merge_hash(&merge_prev, terminal_id, row.seq, &row.hash, received_at);
    let merge_seq: i64 = tx.query_row(
        "INSERT INTO lan_journal (terminal_id, seq, id, at, user_id, action, entity_type, entity_id, payload,
                                  prev_hash, hash, intact, received_at, merge_prev, merge_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         RETURNING merge_seq",
        params![
            terminal_id,
            row.seq,
            row.id,
            row.at,
            row.user_id,
            row.action,
            row.entity_type,
            row.entity_id,
            row.payload,
            row.prev_hash,
            row.hash,
            intact,
            received_at,
            merge_prev,
            merge
        ],
        |r| r.get(0),
    )?;
    Ok(Appended {
        merge_seq,
        intact,
        duplicate: false,
        collided: false,
    })
}

/// A device this main PC holds nothing of: no row stored, the anchor still the default genesis
/// one (no snapshot of this main PC was ever given to it). A till joining a main PC it never
/// was paired with (a promoted till, LAN-010) first sends its kept items, whose journal
/// continues the chain of its previous main PC.
fn adoptable(tx: &Transaction<'_>, terminal_id: &str) -> Result<bool, AppError> {
    Ok(tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM lan_devices WHERE terminal_id = ?1
                         AND journal_anchor_seq = 0 AND journal_anchor_hash = ?2)
            AND NOT EXISTS (SELECT 1 FROM lan_journal WHERE terminal_id = ?1)",
        params![terminal_id, GENESIS_HASH],
        |r| r.get(0),
    )?)
}

/// The till's chain starts here on this main PC: the device anchors at the first row's link,
/// journaled (`lan.journal_adopted`) so the start of the chain the host checks is on record.
fn adopt(tx: &Transaction<'_>, terminal_id: &str, row: &JournalImage) -> Result<(), AppError> {
    tx.execute(
        "UPDATE lan_devices SET journal_anchor_seq = ?2, journal_anchor_hash = ?3 WHERE terminal_id = ?1",
        params![terminal_id, row.seq - 1, row.prev_hash],
    )?;
    let code: Option<String> = tx
        .query_row(
            "SELECT code FROM terminals WHERE id = ?1",
            [terminal_id],
            |r| r.get(0),
        )
        .optional()?;
    audit::append(
        tx,
        &Actor::system(Some(terminal_id.to_owned())).event(
            "lan.journal_adopted",
            "terminal",
            Some(terminal_id),
            json!({ "terminal": code, "fromSeq": row.seq, "prevHash": row.prev_hash }),
        ),
    )?;
    Ok(())
}

/// The main PC keeps this terminal's journal (it is a paired till).
pub fn holds(conn: &Connection, terminal_id: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM lan_devices WHERE terminal_id = ?1)",
        [terminal_id],
        |r| r.get(0),
    )?)
}

pub fn hash_at(conn: &Connection, terminal_id: &str, seq: i64) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT hash FROM lan_journal WHERE terminal_id = ?1 AND seq = ?2",
            params![terminal_id, seq],
            |r| r.get(0),
        )
        .optional()?)
}

/// Checks a till's journal as the host holds it. The device's anchor starts the current pairing:
/// rows after it are walked strictly from the anchor; rows at or before it belong to earlier
/// pairings, whose anchors a later snapshot replaced, so they are checked for their own hash and
/// their links to each other, a pairing's first row (a jump in `seq`) resting on the check made
/// on its arrival (`intact`). A row stored with `intact = 0` counts as broken in either part.
pub fn verify(conn: &Connection, terminal_id: &str) -> Result<JournalReport, AppError> {
    let (anchor_seq, anchor_hash) = device_anchor(conn, terminal_id)?;
    let mut stmt = conn.prepare(
        "SELECT seq, id, at, user_id, terminal_id, action, entity_type, entity_id, payload, prev_hash, hash, intact
         FROM lan_journal WHERE terminal_id = ?1 ORDER BY seq",
    )?;
    let rows = stmt
        .query_map([terminal_id], |r| {
            Ok((
                JournalImage {
                    seq: r.get(0)?,
                    id: r.get(1)?,
                    at: r.get(2)?,
                    user_id: r.get(3)?,
                    terminal_id: r.get(4)?,
                    action: r.get(5)?,
                    entity_type: r.get(6)?,
                    entity_id: r.get(7)?,
                    payload: r.get(8)?,
                    prev_hash: r.get(9)?,
                    hash: r.get(10)?,
                },
                r.get::<_, bool>(11)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut report = JournalReport {
        rows: 0,
        broken: 0,
        first_broken_seq: None,
        last_seq: anchor_seq,
        last_hash: anchor_hash,
    };
    // The previous row of an earlier pairing.
    let mut earlier: Option<(i64, String)> = None;
    for (row, intact) in rows {
        report.rows += 1;
        let linked = if row.seq <= anchor_seq {
            let linked = match &earlier {
                Some((seq, hash)) if row.seq == seq + 1 => row.prev_hash == *hash,
                _ => true,
            };
            earlier = Some((row.seq, row.hash.clone()));
            linked
        } else {
            let linked = row.seq == report.last_seq + 1 && row.prev_hash == report.last_hash;
            report.last_seq = row.seq;
            report.last_hash = row.hash.clone();
            linked
        };
        if !(intact && linked && own_hash(&row) == row.hash) {
            report.broken += 1;
            report.first_broken_seq.get_or_insert(row.seq);
        }
    }
    Ok(report)
}

/// The first `merge_seq` whose merge link does not recompute, `None` when the host's chain over
/// every till's journal is intact.
pub fn verify_merge(conn: &Connection) -> Result<Option<i64>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT merge_seq, terminal_id, seq, hash, received_at, merge_prev, merge_hash FROM lan_journal ORDER BY merge_seq",
    )?;
    let mut rows = stmt.query([])?;
    let mut prev = GENESIS_HASH.to_owned();
    while let Some(r) = rows.next()? {
        let merge_seq: i64 = r.get(0)?;
        let terminal_id: String = r.get(1)?;
        let hash: String = r.get(3)?;
        let stored_prev: String = r.get(5)?;
        let stored: String = r.get(6)?;
        let recomputed = merge_hash(&prev, &terminal_id, r.get(2)?, &hash, r.get(4)?);
        if stored_prev != prev || stored != recomputed {
            return Ok(Some(merge_seq));
        }
        prev = stored;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{self, Actor, GENESIS_HASH};
    use crate::db::Db;
    use crate::sync::outbox::JournalImage;
    use serde_json::json;

    const C2: &str = "00000000-0000-7000-8000-0000000000c2";

    /// Three rows of a till's own journal, as its outbox would carry them.
    fn till_rows() -> Vec<JournalImage> {
        let till = Db::open_memory().unwrap();
        (0..3)
            .map(|i| {
                till.write(move |tx| {
                    audit::append(
                        tx,
                        &Actor {
                            user_id: Some("u".into()),
                            terminal_id: Some(C2.into()),
                        }
                        .event(
                            "cash.paid_in",
                            "cash_session",
                            Some("s1"),
                            json!({ "amount": 100 * (i + 1) }),
                        ),
                    )
                })
                .map(|row| JournalImage::from_audit(&row))
                .unwrap()
            })
            .collect()
    }

    fn host() -> Db {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO terminals (id, code, name, role, created_at) VALUES ('{C2}', 'C2', 'Caisse 2', 'client', 0);
                 INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at)
                   VALUES ('{C2}', 'Caisse 2', '{}', 0);",
                "a".repeat(64)
            ))?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn add(db: &Db, row: &JournalImage, at: i64) -> Appended {
        let row = row.clone();
        db.write(move |tx| append(tx, C2, &row, at)).unwrap()
    }

    #[test]
    fn a_tills_journal_is_checked_on_arrival_and_chained_again_by_the_host() {
        let host = host();
        let rows = till_rows();
        for (i, row) in rows.iter().enumerate() {
            let appended = add(&host, row, 100 + i64::try_from(i).unwrap());
            assert!(appended.intact && !appended.duplicate, "row {i}");
        }
        assert!(
            add(&host, &rows[1], 500).duplicate,
            "the same row is stored once"
        );
        let report = host.read(|c| verify(c, C2)).unwrap();
        assert_eq!(
            (
                report.rows,
                report.broken,
                report.first_broken_seq,
                report.last_seq
            ),
            (3, 0, None, 3)
        );
        assert_eq!(report.last_hash, rows[2].hash);
        assert_eq!(
            host.read(|c| hash_at(c, C2, 2)).unwrap().as_deref(),
            Some(rows[1].hash.as_str())
        );
        assert!(host.read(|c| holds(c, C2)).unwrap());
        assert!(
            !host
                .read(|c| holds(c, "00000000-0000-7000-8000-0000000000c9"))
                .unwrap()
        );
        assert_eq!(host.read(verify_merge).unwrap(), None);
    }

    #[test]
    fn a_gap_or_an_edited_row_is_kept_and_marked() {
        let host = host();
        let rows = till_rows();
        add(&host, &rows[0], 1);
        let gap = add(&host, &rows[2], 2);
        assert!(!gap.intact, "row 2 never arrived");
        let host = self::host();
        add(&host, &rows[0], 1);
        let mut edited = rows[1].clone();
        edited.payload = "{\"amount\":1}".into();
        assert!(!add(&host, &edited, 2).intact, "its hash no longer matches");
        let report = host.read(|c| verify(c, C2)).unwrap();
        assert_eq!((report.broken, report.first_broken_seq), (1, Some(2)));
    }

    #[test]
    fn the_merge_chain_catches_a_rewritten_row() {
        let host = host();
        for (i, row) in till_rows().iter().enumerate() {
            add(&host, row, i64::try_from(i).unwrap());
        }
        host.write(|tx| {
            tx.execute_batch(
                "DROP TRIGGER lan_journal_no_update;
                 UPDATE lan_journal SET received_at = 99 WHERE merge_seq = 2;",
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(host.read(verify_merge).unwrap(), Some(2));
        assert_eq!(
            merge_hash(GENESIS_HASH, C2, 1, &"b".repeat(64), 5).len(),
            64
        );
    }

    /// A till re-paired on the same terminal gets a new snapshot, whose journal tail (the main
    /// PC's `audit_log` at that moment) becomes the device's anchor: its next rows continue from
    /// there, not from the last row it sent under the previous pairing.
    mod reanchored {
        use super::*;
        use rusqlite::params;

        /// The journal tail of a snapshot taken after `before` rows of the main PC, and the
        /// `count` rows the till then writes on its copy, continuing that chain.
        fn after_snapshot(before: i64, count: i64) -> ((i64, String), Vec<JournalImage>) {
            let copy = Db::open_memory().unwrap();
            let mut tail = (0, GENESIS_HASH.to_owned());
            for i in 0..before {
                let row = copy
                    .write(move |tx| {
                        audit::append(
                            tx,
                            &Actor {
                                user_id: Some("u-owner".into()),
                                terminal_id: None,
                            }
                            .event(
                                "cash.paid_in",
                                "cash_session",
                                Some("s-host"),
                                json!({ "amount": i }),
                            ),
                        )
                    })
                    .unwrap();
                tail = (row.seq, row.hash);
            }
            let rows = (0..count)
                .map(|i| {
                    copy.write(move |tx| {
                        audit::append(
                            tx,
                            &Actor {
                                user_id: Some("u".into()),
                                terminal_id: Some(C2.into()),
                            }
                            .event(
                                "cash.paid_in",
                                "cash_session",
                                Some("s2"),
                                json!({ "amount": 7 * (i + 1) }),
                            ),
                        )
                    })
                    .map(|row| JournalImage::from_audit(&row))
                    .unwrap()
                })
                .collect();
            (tail, rows)
        }

        /// What `devices::set_journal_anchor` does when the snapshot is exported.
        fn re_anchor(db: &Db, anchor: &(i64, String)) {
            let (seq, hash) = anchor.clone();
            db.write(move |tx| {
                tx.execute(
                    "UPDATE lan_devices SET journal_anchor_seq = ?2, journal_anchor_hash = ?3
                     WHERE terminal_id = ?1",
                    params![C2, seq, hash],
                )?;
                Ok(())
            })
            .unwrap();
        }

        fn report(db: &Db) -> JournalReport {
            db.read(|c| verify(c, C2)).unwrap()
        }

        #[test]
        fn a_first_row_on_a_snapshot_anchor_is_intact() {
            let host = host();
            let (anchor, rows) = after_snapshot(4, 2);
            re_anchor(&host, &anchor);
            for (i, row) in rows.iter().enumerate() {
                assert!(
                    add(&host, row, i64::try_from(i).unwrap()).intact,
                    "row {}",
                    row.seq
                );
            }
            let report = report(&host);
            assert_eq!(
                (
                    report.rows,
                    report.broken,
                    report.first_broken_seq,
                    report.last_seq
                ),
                (2, 0, None, 6)
            );
            assert_eq!(report.last_hash, rows[1].hash);
        }

        #[test]
        fn a_re_paired_till_continues_from_its_new_anchor() {
            let host = host();
            for (i, row) in till_rows().iter().enumerate() {
                assert!(add(&host, row, i64::try_from(i).unwrap()).intact);
            }
            let (anchor, rows) = after_snapshot(6, 2);
            re_anchor(&host, &anchor);
            let before = report(&host);
            assert_eq!(
                (before.rows, before.broken, before.last_seq),
                (3, 0, 6),
                "the till's next row links to the new anchor"
            );
            assert_eq!(before.last_hash, anchor.1);
            let first = add(&host, &rows[0], 10);
            assert!(
                first.intact,
                "checked against the new anchor, not the previous pairing's last row"
            );
            assert!(add(&host, &rows[1], 11).intact);
            let after = report(&host);
            assert_eq!(
                (
                    after.rows,
                    after.broken,
                    after.first_broken_seq,
                    after.last_seq
                ),
                (5, 0, None, 8)
            );
            assert_eq!(after.last_hash, rows[1].hash);
        }

        #[test]
        fn each_earlier_pairing_is_checked_on_its_own() {
            let host = host();
            for row in &till_rows() {
                add(&host, row, 1);
            }
            let (second_anchor, second) = after_snapshot(6, 2);
            re_anchor(&host, &second_anchor);
            for row in &second {
                assert!(add(&host, row, 2).intact, "row {}", row.seq);
            }
            let (third_anchor, third) = after_snapshot(12, 1);
            re_anchor(&host, &third_anchor);
            assert!(add(&host, &third[0], 3).intact);
            let report = report(&host);
            assert_eq!(
                (
                    report.rows,
                    report.broken,
                    report.first_broken_seq,
                    report.last_seq
                ),
                (6, 0, None, 13)
            );
        }

        #[test]
        fn a_break_from_an_earlier_pairing_stays_visible() {
            let host = host();
            let first = till_rows();
            add(&host, &first[0], 1);
            assert!(!add(&host, &first[2], 2).intact, "row 2 never arrived");
            let (anchor, rows) = after_snapshot(6, 1);
            re_anchor(&host, &anchor);
            assert!(add(&host, &rows[0], 3).intact);
            let report = report(&host);
            assert_eq!(
                (
                    report.rows,
                    report.broken,
                    report.first_broken_seq,
                    report.last_seq
                ),
                (3, 1, Some(3), 7)
            );
        }

        /// A till joining a main PC it was never paired with (a promoted till, LAN-010) sends
        /// its kept items first: its journal continues its previous main PC's chain.
        #[test]
        fn a_till_new_to_this_main_pc_adopts_the_start_of_its_chain() {
            let host = host();
            let (_, rows) = after_snapshot(6, 2);
            assert!(
                add(&host, &rows[0], 1).intact,
                "adopted: this main PC held nothing of the till"
            );
            assert!(add(&host, &rows[1], 2).intact);
            let report = report(&host);
            assert_eq!(
                (
                    report.rows,
                    report.broken,
                    report.first_broken_seq,
                    report.last_seq
                ),
                (2, 0, None, 8)
            );
            let adopted: (i64, String) = host
                .read(|c| {
                    Ok(c.query_row(
                        "SELECT COUNT(*), MAX(payload) FROM audit_log WHERE action = 'lan.journal_adopted'",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?)
                })
                .unwrap();
            assert_eq!(adopted.0, 1, "journaled once");
            assert!(adopted.1.contains("\"fromSeq\":7"), "{}", adopted.1);
            let host = super::host();
            let mut edited = rows[0].clone();
            edited.payload = "{\"amount\":1}".into();
            assert!(
                !add(&host, &edited, 1).intact,
                "a row whose own hash fails is never adopted"
            );
        }

        #[test]
        fn a_row_that_ignores_the_new_anchor_is_marked() {
            let host = host();
            let (_, old) = after_snapshot(0, 4);
            for row in &old[..3] {
                assert!(add(&host, row, 1).intact, "row {}", row.seq);
            }
            let (anchor, _) = after_snapshot(6, 0);
            re_anchor(&host, &anchor);
            assert!(
                !add(&host, &old[3], 2).intact,
                "row 4 continues the previous pairing, not the snapshot the till was given"
            );
            let report = report(&host);
            assert_eq!(
                (report.rows, report.broken, report.first_broken_seq),
                (4, 1, Some(4))
            );
        }
    }
}
