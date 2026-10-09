//! The change feed of spec §5.1 (`changelog`), shared by the DZBuild link (M7) and the LAN (M5).
//! Triggers of migration 0006 write it only while a reader is registered; each reader keeps its
//! position in `sync_cursors` (kind `changelog`) and moves it in the same transaction as what it
//! derived from the rows; rows every reader has passed are pruned. Payloads carry routing fields
//! only: readers load the current row by `row_id`.

use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;

pub const CLOUD_CONSUMER: &str = "changelog:cloud";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOp {
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangeRow {
    pub seq: i64,
    pub at: i64,
    pub table_name: String,
    pub row_id: String,
    pub op: ChangeOp,
    pub payload: Value,
}

impl ChangeRow {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.payload.get(key).and_then(Value::as_str)
    }
}

/// The feed's high-water mark (it survives pruning).
pub fn max_seq(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT MAX(COALESCE((SELECT MAX(seq) FROM changelog), 0),
                    COALESCE((SELECT seq FROM sqlite_sequence WHERE name = 'changelog'), 0))",
        [],
        |r| r.get(0),
    )?)
}

/// Registers `consumer` at the head of the feed (idempotent); returns its position.
pub fn register(tx: &Transaction<'_>, consumer: &str, now: i64) -> Result<i64, AppError> {
    let head = max_seq(tx)?;
    tx.execute(
        "INSERT INTO sync_cursors (name, kind, last_seq, updated_at) VALUES (?1, 'changelog', ?2, ?3)
         ON CONFLICT (name) DO NOTHING",
        params![consumer, head, now],
    )?;
    Ok(position(tx, consumer)?.unwrap_or(head))
}

pub fn unregister(tx: &Transaction<'_>, consumer: &str) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM sync_cursors WHERE name = ?1 AND kind = 'changelog'",
        [consumer],
    )?;
    Ok(())
}

pub fn position(conn: &Connection, consumer: &str) -> Result<Option<i64>, AppError> {
    Ok(conn
        .query_row(
            "SELECT last_seq FROM sync_cursors WHERE name = ?1 AND kind = 'changelog'",
            [consumer],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn read_after(conn: &Connection, after: i64, limit: usize) -> Result<Vec<ChangeRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT seq, at, table_name, row_id, op, payload FROM changelog WHERE seq > ?1 ORDER BY seq LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(
            params![after, i64::try_from(limit).unwrap_or(i64::MAX)],
            |r| {
                let op: String = r.get(4)?;
                let payload: String = r.get(5)?;
                Ok(ChangeRow {
                    seq: r.get(0)?,
                    at: r.get(1)?,
                    table_name: r.get(2)?,
                    row_id: r.get(3)?,
                    op: match op.as_str() {
                        "insert" => ChangeOp::Insert,
                        "delete" => ChangeOp::Delete,
                        _ => ChangeOp::Update,
                    },
                    payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Moves `consumer` to `seq`, never backwards.
pub fn advance(tx: &Transaction<'_>, consumer: &str, seq: i64, now: i64) -> Result<(), AppError> {
    tx.execute(
        "UPDATE sync_cursors SET last_seq = MAX(last_seq, ?2), updated_at = ?3
         WHERE name = ?1 AND kind = 'changelog'",
        params![consumer, seq, now],
    )?;
    Ok(())
}

/// Deletes the rows every reader has passed; all rows when no reader is left.
pub fn prune(tx: &Transaction<'_>) -> Result<usize, AppError> {
    Ok(tx.execute(
        "DELETE FROM changelog WHERE seq <= COALESCE(
             (SELECT MIN(last_seq) FROM sync_cursors WHERE kind = 'changelog'), 9223372036854775807)",
        [],
    )?)
}

/// A till's own reader: the capture triggers need one to write, the outbox hook empties the feed.
pub const LAN_OUTBOX_CONSUMER: &str = "changelog:lan:outbox";

/// The main PC's reader for one till.
pub fn lan_consumer(terminal_id: &str) -> String {
    format!("changelog:lan:{terminal_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn touch_product(db: &Db) {
        db.write(|tx| {
            tx.execute_batch(
                "INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
                   VALUES ('00000000-0000-7000-8000-00000000e401', 'TVA19', 1900, 0, 0)
                   ON CONFLICT DO NOTHING;
                 INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
                   VALUES (lower(hex(randomblob(4))) || '-0000-7000-8000-000000000000', 'P',
                           '00000000-0000-7000-8000-000000000101', '00000000-0000-7000-8000-00000000e401', 100, 0, 0);",
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn readers_register_at_the_head_read_advance_and_prune() {
        let db = Db::open_memory().unwrap();
        touch_product(&db);
        let start = db.write(|tx| register(tx, CLOUD_CONSUMER, 1)).unwrap();
        assert_eq!(start, 0, "nothing was written before the first reader");
        // Registered before the rows: a reader registered later starts at the head and has
        // passed every row already written.
        db.write(move |tx| register(tx, "changelog:lan:C2", 2).map(drop))
            .unwrap();
        touch_product(&db);
        touch_product(&db);
        let rows = db.read(move |c| read_after(c, start, 100)).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].table_name, "products");
        assert_eq!(rows[0].op, ChangeOp::Insert);
        let late = db.write(|tx| register(tx, "changelog:lan:C3", 2)).unwrap();
        assert_eq!(late, rows[1].seq, "a later reader starts at the head");
        db.write(|tx| unregister(tx, "changelog:lan:C3")).unwrap();
        let first = rows[0].seq;
        db.write(move |tx| advance(tx, CLOUD_CONSUMER, first, 3))
            .unwrap();
        assert_eq!(
            db.write(|tx| prune(tx)).unwrap(),
            0,
            "the LAN reader has not passed any row"
        );
        db.write(move |tx| advance(tx, "changelog:lan:C2", first + 1, 4))
            .unwrap();
        assert_eq!(
            db.write(|tx| prune(tx)).unwrap(),
            1,
            "only rows every reader passed"
        );
        db.write(move |tx| advance(tx, CLOUD_CONSUMER, first - 1, 5))
            .unwrap();
        assert_eq!(
            db.read(|c| position(c, CLOUD_CONSUMER)).unwrap(),
            Some(first),
            "positions never go back"
        );
        db.write(|tx| unregister(tx, CLOUD_CONSUMER)).unwrap();
        db.write(|tx| unregister(tx, "changelog:lan:C2")).unwrap();
        touch_product(&db);
        assert_eq!(
            db.write(|tx| prune(tx)).unwrap(),
            1,
            "no reader left: everything goes"
        );
        assert!(db.read(|c| read_after(c, 0, 10)).unwrap().is_empty());
        assert_eq!(
            db.read(max_seq).unwrap(),
            2,
            "the high-water mark survives pruning (no reader, no row: the last write left none)"
        );
    }
}
