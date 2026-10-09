//! A till's outbox (spec §4.6): every local business write leaves, in the same transaction, one
//! item holding the rows it made for this till (full images, in statement order) and its audit
//! rows. Items are replayed to the main PC in `local_seq` order with a stable idempotency key:
//! `sale:<document id>` for a sale (spec §4.6: the sale id), `return:<id>` for a return, the
//! id of the write's first journal row otherwise. An item is frozen once written; sent items
//! stay 30 days (a new main PC gets them again, Task 12), unsent ones stay forever.

use super::catalogue::{self, SyncClass};
use super::feed::Op;
use crate::audit::{self, Actor, AuditRow, canonical_json};
use crate::db::WriteHook;
use crate::error::AppError;
use crate::ids::new_id;
use crate::sqlenum::sql_text_enum;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use specta::Type;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const KEEP_SENT_MS: i64 = 30 * 24 * 3_600_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleRow {
    pub table: String,
    pub op: Op,
    pub image: Map<String, Value>,
}

/// One `audit_log` row of the till, as it is stored there (its hash covers these fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalImage {
    pub seq: i64,
    pub id: String,
    pub at: i64,
    pub user_id: Option<String>,
    pub terminal_id: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub payload: String,
    pub prev_hash: String,
    pub hash: String,
}

impl JournalImage {
    pub fn from_audit(row: &AuditRow) -> JournalImage {
        JournalImage {
            seq: row.seq,
            id: row.id.clone(),
            at: row.at,
            user_id: row.user_id.clone(),
            terminal_id: row.terminal_id.clone(),
            action: row.action.clone(),
            entity_type: row.entity_type.clone(),
            entity_id: row.entity_id.clone(),
            payload: canonical_json(&row.payload),
            prev_hash: row.prev_hash.clone(),
            hash: row.hash.clone(),
        }
    }

    fn from_image(image: &Map<String, Value>) -> Option<JournalImage> {
        let text = |k: &str| image.get(k).and_then(Value::as_str).map(str::to_owned);
        Some(JournalImage {
            seq: image.get("seq")?.as_i64()?,
            id: text("id")?,
            at: image.get("at")?.as_i64()?,
            user_id: text("user_id"),
            terminal_id: text("terminal_id"),
            action: text("action")?,
            entity_type: text("entity_type")?,
            entity_id: text("entity_id"),
            payload: text("payload")?,
            prev_hash: text("prev_hash")?,
            hash: text("hash")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub id: String,
    pub key: String,
    pub terminal_id: String,
    pub kind: String,
    pub created_at: i64,
    pub rows: Vec<BundleRow>,
    pub audit: Vec<JournalImage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireItem {
    pub local_seq: i64,
    pub bundle: Bundle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Sent,
    Rejected,
}

impl OutboxStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            OutboxStatus::Pending => "pending",
            OutboxStatus::Sent => "sent",
            OutboxStatus::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<OutboxStatus> {
        [
            OutboxStatus::Pending,
            OutboxStatus::Sent,
            OutboxStatus::Rejected,
        ]
        .into_iter()
        .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(OutboxStatus);

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxItem {
    pub local_seq: i64,
    pub bundle: Bundle,
    pub status: OutboxStatus,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub sent_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OutboxDepth {
    pub pending: i64,
    pub rejected: i64,
    pub oldest_pending_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Applied,
    Duplicate,
    Rejected,
}

/// A field the main PC recorded differently from the till (a rebased `balance_after`, a chain
/// link recomputed from the host's anchor): the till takes the host's value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjustment {
    pub table: String,
    pub row_id: String,
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub key: String,
    pub local_seq: i64,
    pub status: ItemStatus,
    pub code: Option<String>,
    pub adjustments: Vec<Adjustment>,
    /// The main PC's feed head after the item: the till is up to date once it applied it.
    pub host_seq: i64,
}

pub fn idempotency_key(audit: &[JournalImage], bundle_id: &str) -> String {
    match audit.first() {
        Some(a) if a.action == "sale.created" => match &a.entity_id {
            Some(id) => format!("sale:{id}"),
            None => format!("tx:{}", a.id),
        },
        Some(a) if a.action == "return.created" => match &a.entity_id {
            Some(id) => format!("return:{id}"),
            None => format!("tx:{}", a.id),
        },
        Some(a) => format!("tx:{}", a.id),
        None => format!("tx:{bundle_id}"),
    }
}

/// Writes one item with `rows` and `audit`; returns its `local_seq`.
pub fn enqueue(
    tx: &Transaction<'_>,
    terminal_id: &str,
    rows: Vec<BundleRow>,
    audit: Vec<JournalImage>,
    now: i64,
) -> Result<i64, AppError> {
    let id = new_id();
    let key = idempotency_key(&audit, &id);
    let kind = audit
        .first()
        .map_or_else(|| "lan.write".to_owned(), |a| a.action.clone());
    let count = i64::try_from(rows.len() + audit.len()).unwrap_or(i64::MAX);
    let payload = serde_json::to_string(&json!({ "rows": rows, "audit": audit }))?;
    Ok(tx.query_row(
        "INSERT INTO lan_outbox (id, idempotency_key, terminal_id, kind, payload, rows, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING local_seq",
        params![id, key, terminal_id, kind, payload, count, now],
        |r| r.get(0),
    )?)
}

/// Moves this transaction's feed rows into one item (own till rows and journal rows only) and
/// empties the feed. `None` when there was nothing to send.
pub fn enqueue_from_changelog(
    tx: &Transaction<'_>,
    terminal_id: &str,
    now: i64,
) -> Result<Option<i64>, AppError> {
    let mut stmt =
        tx.prepare_cached("SELECT table_name, op, payload FROM changelog ORDER BY seq")?;
    let raw = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if raw.is_empty() {
        return Ok(None);
    }
    let (mut rows, mut audit) = (Vec::new(), Vec::new());
    for (table, op, payload) in raw {
        let Ok(Value::Object(mut image)) = serde_json::from_str::<Value>(&payload) else {
            continue;
        };
        if image.remove("_lan").is_none() {
            continue; // another reader's routing row
        }
        let owner = image.remove("_t");
        match catalogue::class_of(&table) {
            Some(SyncClass::Journal) => audit.extend(JournalImage::from_image(&image)),
            Some(SyncClass::TillOwned)
                if owner.as_ref().and_then(Value::as_str) == Some(terminal_id) =>
            {
                let op = match op.as_str() {
                    "insert" => Op::Insert,
                    "delete" => Op::Delete,
                    _ => Op::Update,
                };
                rows.push(BundleRow { table, op, image });
            }
            _ => {}
        }
    }
    tx.execute("DELETE FROM changelog", [])?;
    if rows.is_empty() && audit.is_empty() {
        return Ok(None);
    }
    enqueue(tx, terminal_id, rows, audit, now).map(Some)
}

/// The till's write hook: the outbox item commits with the business write, then the engine is
/// poked to send it at once when the link is up. A write that queued nothing (a sign-in, the
/// engine's own bookkeeping) pokes nobody, so the engine never wakes itself.
pub struct OutboxHook {
    terminal_id: String,
    queued: AtomicBool,
    poke: Arc<dyn Fn() + Send + Sync>,
}

impl OutboxHook {
    pub fn new(terminal_id: String, poke: Arc<dyn Fn() + Send + Sync>) -> OutboxHook {
        OutboxHook {
            terminal_id,
            queued: AtomicBool::new(false),
            poke,
        }
    }
}

impl WriteHook for OutboxHook {
    fn before_commit(&self, tx: &Transaction<'_>) -> Result<(), AppError> {
        if enqueue_from_changelog(tx, &self.terminal_id, crate::time::now_ms())?.is_some() {
            self.queued.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn after_commit(&self) {
        if self.queued.swap(false, Ordering::SeqCst) {
            (self.poke)();
        }
    }
}

const ITEM_SELECT: &str =
    "SELECT local_seq, payload, id, idempotency_key, terminal_id, kind, created_at,
        status, attempts, last_error, sent_at FROM lan_outbox";

fn map_item(r: &Row<'_>) -> rusqlite::Result<(OutboxItem, String)> {
    let payload: String = r.get(1)?;
    Ok((
        OutboxItem {
            local_seq: r.get(0)?,
            bundle: Bundle {
                id: r.get(2)?,
                key: r.get(3)?,
                terminal_id: r.get(4)?,
                kind: r.get(5)?,
                created_at: r.get(6)?,
                rows: Vec::new(),
                audit: Vec::new(),
            },
            status: r.get(7)?,
            attempts: r.get(8)?,
            last_error: r.get(9)?,
            sent_at: r.get(10)?,
        },
        payload,
    ))
}

#[derive(Deserialize)]
struct Payload {
    rows: Vec<BundleRow>,
    audit: Vec<JournalImage>,
}

fn items(
    conn: &Connection,
    sql: &str,
    bind: impl rusqlite::Params,
) -> Result<Vec<OutboxItem>, AppError> {
    let mut stmt = conn.prepare(sql)?;
    let raw = stmt
        .query_map(bind, map_item)?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(|(mut item, payload)| {
            let p: Payload = serde_json::from_str(&payload)?;
            item.bundle.rows = p.rows;
            item.bundle.audit = p.audit;
            Ok(item)
        })
        .collect()
}

fn wire(items: Vec<OutboxItem>) -> Vec<WireItem> {
    items
        .into_iter()
        .map(|i| WireItem {
            local_seq: i.local_seq,
            bundle: i.bundle,
        })
        .collect()
}

/// The pending items in order, up to the first rejected one (order matters: counters, sessions).
pub fn pending(conn: &Connection, limit: usize) -> Result<Vec<WireItem>, AppError> {
    let blocked: Option<i64> = conn.query_row(
        "SELECT MIN(local_seq) FROM lan_outbox WHERE status = 'rejected'",
        [],
        |r| r.get(0),
    )?;
    let all = items(
        conn,
        &format!(
            "{ITEM_SELECT} WHERE status = 'pending' AND local_seq < ?1 ORDER BY local_seq LIMIT ?2"
        ),
        params![
            blocked.unwrap_or(i64::MAX),
            i64::try_from(limit).unwrap_or(i64::MAX)
        ],
    )?;
    Ok(wire(all))
}

/// Pending items and those sent since `since`, in order: what a new main PC must see again.
pub fn kept_since(conn: &Connection, since: i64) -> Result<Vec<WireItem>, AppError> {
    Ok(wire(items(
        conn,
        &format!(
            "{ITEM_SELECT} WHERE status = 'pending' OR (status = 'sent' AND sent_at >= ?1) ORDER BY local_seq"
        ),
        [since],
    )?))
}

/// Items from `seq` on, pending or sent (a main PC that lost items asks for them again), up to
/// the first rejected one: the line stops there, as in [`pending`].
pub fn from_seq(conn: &Connection, seq: i64, limit: usize) -> Result<Vec<WireItem>, AppError> {
    let blocked: Option<i64> = conn.query_row(
        "SELECT MIN(local_seq) FROM lan_outbox WHERE status = 'rejected'",
        [],
        |r| r.get(0),
    )?;
    Ok(wire(items(
        conn,
        &format!(
            "{ITEM_SELECT} WHERE local_seq >= ?1 AND local_seq < ?3 AND status IN ('pending', 'sent')
             ORDER BY local_seq LIMIT ?2"
        ),
        params![
            seq,
            i64::try_from(limit).unwrap_or(i64::MAX),
            blocked.unwrap_or(i64::MAX)
        ],
    )?))
}

pub fn get(conn: &Connection, local_seq: i64) -> Result<OutboxItem, AppError> {
    items(
        conn,
        &format!("{ITEM_SELECT} WHERE local_seq = ?1"),
        [local_seq],
    )?
    .into_iter()
    .next()
    .ok_or_else(|| AppError::not_found("record"))
}

pub fn depth(conn: &Connection) -> Result<OutboxDepth, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(status = 'pending'), 0), COALESCE(SUM(status = 'rejected'), 0),
                MIN(CASE WHEN status = 'pending' THEN created_at END)
         FROM lan_outbox",
        [],
        |r| {
            Ok(OutboxDepth {
                pending: r.get(0)?,
                rejected: r.get(1)?,
                oldest_pending_at: r.get(2)?,
            })
        },
    )?)
}

pub fn mark_sent(
    tx: &Transaction<'_>,
    local_seq: i64,
    host_id: &str,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE lan_outbox SET status = 'sent', sent_at = ?2, host_id = ?3, last_error = NULL
         WHERE local_seq = ?1 AND status <> 'sent'",
        params![local_seq, now, host_id],
    )?;
    Ok(())
}

pub fn record_failure(tx: &Transaction<'_>, local_seq: i64, code: &str) -> Result<(), AppError> {
    tx.execute(
        "UPDATE lan_outbox SET attempts = attempts + 1, last_error = ?2
         WHERE local_seq = ?1 AND status = 'pending'",
        params![local_seq, code],
    )?;
    Ok(())
}

/// The main PC refused the item for good: the line stops at it. Journaled on the till
/// (`lan.item_rejected`, with the item's key and the code), since a retry clears the state.
pub fn mark_rejected(tx: &Transaction<'_>, local_seq: i64, code: &str) -> Result<(), AppError> {
    let item: Option<(String, String)> = tx
        .query_row(
            "SELECT idempotency_key, terminal_id FROM lan_outbox WHERE local_seq = ?1 AND status = 'pending'",
            [local_seq],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((key, terminal_id)) = item else {
        return Ok(());
    };
    tx.execute(
        "UPDATE lan_outbox SET status = 'rejected', attempts = attempts + 1, last_error = ?2
         WHERE local_seq = ?1 AND status = 'pending'",
        params![local_seq, code],
    )?;
    audit::append(
        tx,
        &Actor::system(Some(terminal_id)).event(
            "lan.item_rejected",
            "lan_item",
            Some(&key),
            json!({ "localSeq": local_seq, "code": code }),
        ),
    )?;
    Ok(())
}

/// Puts the rejected item back in line (the owner fixed its cause on the main PC), journaled
/// with the reason it was refused (`lan.replay_retried`).
pub fn retry_rejected(tx: &Transaction<'_>, actor: &Actor) -> Result<Option<i64>, AppError> {
    let item: Option<(i64, String, Option<String>)> = tx
        .query_row(
            "SELECT local_seq, idempotency_key, last_error FROM lan_outbox WHERE status = 'rejected'
             ORDER BY local_seq LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((seq, key, last_error)) = item else {
        return Ok(None);
    };
    tx.execute(
        "UPDATE lan_outbox SET status = 'pending' WHERE local_seq = ?1",
        [seq],
    )?;
    audit::append(
        tx,
        &actor.event(
            "lan.replay_retried",
            "lan_item",
            Some(&key),
            json!({ "localSeq": seq, "lastError": last_error }),
        ),
    )?;
    Ok(Some(seq))
}

pub fn prune_sent(tx: &Transaction<'_>, now: i64) -> Result<usize, AppError> {
    Ok(tx.execute(
        "DELETE FROM lan_outbox WHERE status = 'sent' AND sent_at < ?1",
        [now - KEEP_SENT_MS],
    )?)
}

fn json_of(value: SqlValue) -> Result<Value, AppError> {
    Ok(match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => json!(i),
        SqlValue::Text(t) => Value::String(t),
        SqlValue::Blob(b) => Value::String(hex::encode_upper(b)),
        SqlValue::Real(_) => return Err(AppError::conflict("lan_version_mismatch")),
    })
}

/// Takes the main PC's values for this till's rows (inside `Db::write_replica`: validated
/// documents are immutable to triggers). Each change is journaled with before and after and
/// the journal row goes into the outbox here, since no capture trigger runs.
pub fn reconcile(
    tx: &Transaction<'_>,
    terminal_id: &str,
    adjustments: &[Adjustment],
    now: i64,
) -> Result<usize, AppError> {
    let mut done = 0;
    for adj in adjustments {
        let spec = catalogue::spec(&adj.table)
            .ok_or_else(|| AppError::conflict("lan_version_mismatch"))?;
        if spec.class != SyncClass::TillOwned || adj.fields.is_empty() {
            return Err(AppError::conflict("lan_version_mismatch"));
        }
        let cols = super::capture::columns(tx, spec.name)?;
        let names: Vec<&String> = adj.fields.keys().collect();
        if names
            .iter()
            .any(|n| !cols.iter().any(|c| &&c.name == n && c.pk == 0))
        {
            return Err(AppError::conflict("lan_version_mismatch"));
        }
        let select = format!(
            "SELECT {} FROM {} WHERE \"id\" = ?1",
            names
                .iter()
                .map(|n| catalogue::quote(n))
                .collect::<Vec<_>>()
                .join(", "),
            catalogue::quote(spec.name)
        );
        let before: Option<Vec<SqlValue>> = tx
            .query_row(&select, [&adj.row_id], |r| {
                (0..names.len()).map(|i| r.get::<_, SqlValue>(i)).collect()
            })
            .optional()?;
        let Some(before) = before else {
            continue; // a row this till does not have: nothing to correct
        };
        let mut before_json = Map::new();
        for (name, value) in names.iter().zip(before) {
            before_json.insert((*name).clone(), json_of(value)?);
        }
        let set = names
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{} = ?{}", catalogue::quote(n), i + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let mut bound: Vec<SqlValue> = vec![SqlValue::Text(adj.row_id.clone())];
        for name in &names {
            bound.push(match &adj.fields[*name] {
                Value::Null => SqlValue::Null,
                Value::Number(n) => SqlValue::Integer(
                    n.as_i64()
                        .ok_or_else(|| AppError::conflict("lan_version_mismatch"))?,
                ),
                Value::String(s) => SqlValue::Text(s.clone()),
                Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
                other => SqlValue::Text(other.to_string()),
            });
        }
        tx.execute(
            &format!(
                "UPDATE {} SET {set} WHERE \"id\" = ?1",
                catalogue::quote(spec.name)
            ),
            rusqlite::params_from_iter(bound),
        )?;
        let row = audit::append(
            tx,
            &Actor::system(Some(terminal_id.to_owned())).event(
                "lan.reconciled",
                &adj.table,
                Some(&adj.row_id),
                json!({ "table": adj.table, "before": before_json, "after": adj.fields }),
            ),
        )?;
        enqueue(
            tx,
            terminal_id,
            Vec::new(),
            vec![JournalImage::from_audit(&row)],
            now,
        )?;
        done += 1;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::sync::testkit::lan_pair;
    use crate::testkit::NOON;
    use serde_json::json;

    fn items(lan: &crate::sync::testkit::LanPair) -> Vec<WireItem> {
        lan.till.read(|c| pending(c, 100)).unwrap()
    }

    #[test]
    fn the_sale_and_its_outbox_item_commit_together() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        lan.open_till_session(0);
        let sale = lan.sell_on_till(&milk.id, 1_000, 10_500);
        let all = items(&lan);
        assert_eq!(all.len(), 2, "the session, then the sale");
        let item = &all[1];
        assert_eq!(item.bundle.key, format!("sale:{}", sale.document.id));
        assert_eq!(item.bundle.kind, "sale.created");
        let tables: std::collections::BTreeSet<&str> =
            item.bundle.rows.iter().map(|r| r.table.as_str()).collect();
        for t in [
            "documents",
            "document_lines",
            "document_taxes",
            "payments",
            "stock_movements",
            "cash_movements",
            "counters",
        ] {
            assert!(tables.contains(t), "{t}");
        }
        assert_eq!(item.bundle.audit.len(), 1);
        assert_eq!(item.bundle.audit[0].action, "sale.created");
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM changelog"),
            0,
            "the feed is emptied into the item"
        );
        assert!(all[0].local_seq < all[1].local_seq);
    }

    #[test]
    fn a_failing_hook_rolls_the_sale_back() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        lan.open_till_session(0);
        lan.till
            .write_replica(|tx| {
                tx.execute_batch(
                    "CREATE TRIGGER disk_full BEFORE INSERT ON lan_outbox BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
                )?;
                Ok(())
            })
            .unwrap();
        let ctx = lan.till_ctx();
        let input = crate::sales::SaleInput {
            cart: crate::sales::Cart::new(
                crate::lang::Lang::Fr,
                vec![crate::sales::CartLine::product(
                    &milk.id,
                    crate::money::Qty(1_000),
                )],
            ),
            payments: vec![crate::payments::PaymentInput::cash(Money(10_500))],
        };
        let err = lan
            .till
            .write(move |tx| crate::sales::create_sale(tx, &ctx, input))
            .unwrap_err();
        assert_eq!(err.code(), "constraint_violation");
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM documents WHERE series = 'C2'"),
            0
        );
        assert_eq!(
            lan.count_till(
                "SELECT COUNT(*) FROM counters WHERE series = 'C2' AND doc_type = 'ticket'"
            ),
            0
        );
        assert_eq!(lan.count_till("SELECT COUNT(*) FROM stock_movements"), 0);
    }

    #[test]
    fn each_business_write_is_one_item_with_a_stable_key() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        lan.open_till_session(10_000);
        let ctx = lan.till_ctx();
        lan.till
            .write(move |tx| {
                crate::cash::move_cash(
                    tx,
                    &ctx,
                    crate::cash::ManualKind::PaidIn,
                    Money(5_000),
                    Some("monnaie"),
                )
            })
            .unwrap();
        let sale = lan.sell_on_till(&milk.id, 1_000, 10_500);
        let (id, actor) = (sale.document.id.clone(), lan.till_cashier.actor());
        lan.till
            .write(move |tx| crate::documents::record_reprint(tx, &actor, &id))
            .unwrap();
        let keys: Vec<(String, String)> = items(&lan)
            .into_iter()
            .map(|i| (i.bundle.kind, i.bundle.key))
            .collect();
        assert_eq!(keys.len(), 4);
        assert_eq!(keys[0].0, "cash.session_opened");
        assert!(keys[0].1.starts_with("tx:"));
        assert_eq!(keys[1].0, "cash.paid_in");
        assert_eq!(
            keys[2],
            ("sale.created".into(), format!("sale:{}", sale.document.id))
        );
        assert_eq!(keys[3].0, "document.reprinted");
        let again: Vec<String> = items(&lan).into_iter().map(|i| i.bundle.key).collect();
        assert_eq!(
            again,
            keys.into_iter().map(|k| k.1).collect::<Vec<_>>(),
            "keys are frozen"
        );
    }

    #[test]
    fn a_write_without_till_rows_or_journal_makes_no_item() {
        let lan = lan_pair();
        let cashier = lan.till_cashier.user_id.clone();
        lan.till
            .write(move |tx| {
                Ok(tx.execute(
                    "UPDATE users SET last_login_at = 5 WHERE id = ?1",
                    [&cashier],
                )?)
            })
            .unwrap();
        assert!(items(&lan).is_empty());
    }

    #[test]
    fn sent_items_are_kept_30_days_and_rejected_ones_wait_for_a_retry() {
        let lan = lan_pair();
        lan.open_till_session(0);
        let first = items(&lan)[0].local_seq;
        let before = lan.till.read(depth).unwrap();
        assert_eq!((before.pending, before.rejected), (1, 0));
        lan.till
            .write(move |tx| record_failure(tx, first, "lan_offline"))
            .unwrap();
        let item = lan.till.read(move |c| get(c, first)).unwrap();
        assert_eq!(
            (item.attempts, item.last_error.as_deref()),
            (1, Some("lan_offline"))
        );
        lan.till
            .write(move |tx| mark_rejected(tx, first, "replay_forbidden_row"))
            .unwrap();
        assert!(
            lan.till.read(|c| pending(c, 10)).unwrap().is_empty(),
            "the line stops at a rejection"
        );
        assert_eq!(lan.till.read(depth).unwrap().rejected, 1);
        let owner = lan.till_owner.actor();
        assert_eq!(
            lan.till
                .write(move |tx| retry_rejected(tx, &owner))
                .unwrap(),
            Some(first)
        );
        let owner = lan.till_owner.actor();
        assert_eq!(
            lan.till
                .write(move |tx| retry_rejected(tx, &owner))
                .unwrap(),
            None
        );
        let journaled: Vec<(String, String)> = lan
            .till
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT action, payload FROM audit_log
                     WHERE action IN ('lan.item_rejected', 'lan.replay_retried') ORDER BY seq",
                )?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(journaled.len(), 2, "{journaled:?}");
        assert_eq!(journaled[0].0, "lan.item_rejected");
        assert!(journaled[0].1.contains("replay_forbidden_row"));
        assert_eq!(journaled[1].0, "lan.replay_retried");
        assert!(
            journaled[1].1.contains("replay_forbidden_row"),
            "the retry keeps the reason it was refused"
        );
        lan.till
            .write(move |tx| mark_sent(tx, first, crate::sync::testkit::HOST_ID, NOON))
            .unwrap();
        let later = NOON + KEEP_SENT_MS - 1;
        assert_eq!(lan.till.write(move |tx| prune_sent(tx, later)).unwrap(), 0);
        let kept = lan.till.read(|c| kept_since(c, 0)).unwrap();
        assert_eq!(
            kept.len(),
            3,
            "a new main PC gets the last 30 days again: the item and the journal of its refusal and retry"
        );
        let much_later = NOON + KEEP_SENT_MS + 1;
        assert_eq!(
            lan.till
                .write(move |tx| prune_sent(tx, much_later))
                .unwrap(),
            1
        );
    }

    #[test]
    fn reconciliation_replaces_fields_and_sends_its_journal_row() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        lan.open_till_session(0);
        let sale = lan.sell_on_till(&milk.id, 1_000, 10_500);
        let before = items(&lan).len();
        let adjustment = Adjustment {
            table: "documents".into(),
            row_id: sale.document.id.clone(),
            fields: json!({ "chain_seq": 1, "prev_hash": crate::audit::GENESIS_HASH, "hash": "b".repeat(64) })
                .as_object()
                .unwrap()
                .clone(),
        };
        let me = lan.till_terminal.id.clone();
        let n = lan
            .till
            .write_replica(move |tx| reconcile(tx, &me, &[adjustment], NOON))
            .unwrap();
        assert_eq!(n, 1);
        let id = sale.document.id.clone();
        let hash: String = lan
            .till
            .read(move |c| {
                Ok(
                    c.query_row("SELECT hash FROM documents WHERE id = ?1", [&id], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(hash, "b".repeat(64));
        let all = items(&lan);
        assert_eq!(
            all.len(),
            before + 1,
            "the journal row travels like any other"
        );
        let last = all.last().unwrap();
        assert_eq!(last.bundle.kind, "lan.reconciled");
        let payload: serde_json::Value =
            serde_json::from_str(&last.bundle.audit[0].payload).unwrap();
        assert_eq!(payload["table"], "documents");
        assert_eq!(
            payload["before"]["hash"],
            sale.document.hash.clone().unwrap()
        );
        assert_eq!(payload["after"]["hash"], "b".repeat(64));
        assert!(
            lan.till
                .read(crate::audit::verify_chain)
                .unwrap()
                .first_broken_seq
                .is_none()
        );
    }

    #[test]
    fn items_from_a_seq_on_are_sent_and_pending_ones_in_order() {
        let lan = lan_pair();
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        lan.pull();
        lan.open_till_session(0);
        lan.sell_on_till(&milk.id, 1_000, 10_500);
        lan.sell_on_till(&milk.id, 1_000, 10_500);
        let all = items(&lan);
        assert_eq!(all.len(), 3);
        let seqs: Vec<i64> = all.iter().map(|i| i.local_seq).collect();
        let (first, second) = (seqs[0], seqs[1]);
        lan.till
            .write(move |tx| {
                mark_sent(tx, first, crate::sync::testkit::HOST_ID, NOON)?;
                mark_sent(tx, second, crate::sync::testkit::HOST_ID, NOON)
            })
            .unwrap();
        let again = lan.till.read(move |c| from_seq(c, second, 10)).unwrap();
        assert_eq!(
            again.iter().map(|i| i.local_seq).collect::<Vec<_>>(),
            seqs[1..],
            "a main PC that lost items gets the sent ones again, then the pending ones"
        );
        assert_eq!(again[0].bundle, all[1].bundle, "the same frozen item");
        let one = lan.till.read(move |c| from_seq(c, first, 1)).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].local_seq, first);
        let third = seqs[2];
        lan.till
            .write(move |tx| mark_rejected(tx, third, "replay_invalid"))
            .unwrap();
        let without = lan.till.read(move |c| from_seq(c, first, 10)).unwrap();
        assert_eq!(
            without.iter().map(|i| i.local_seq).collect::<Vec<_>>(),
            seqs[..2],
            "a rejected item is not resent"
        );
    }

    #[test]
    fn the_hook_pokes_the_engine_only_after_a_write_that_queued_an_item() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let lan = lan_pair();
        let pokes = Arc::new(AtomicUsize::new(0));
        let counted = pokes.clone();
        lan.till.set_write_hook(Some(Arc::new(OutboxHook::new(
            lan.till_terminal.id.clone(),
            Arc::new(move || {
                counted.fetch_add(1, Ordering::SeqCst);
            }),
        ))));
        let cashier = lan.till_cashier.user_id.clone();
        lan.till
            .write(move |tx| {
                Ok(tx.execute(
                    "UPDATE users SET last_login_at = 5 WHERE id = ?1",
                    [&cashier],
                )?)
            })
            .unwrap();
        assert_eq!(
            pokes.load(Ordering::SeqCst),
            0,
            "nothing to send: the engine's own bookkeeping never wakes it"
        );
        lan.open_till_session(0);
        assert_eq!(pokes.load(Ordering::SeqCst), 1, "one item, one poke");
        let first = items(&lan)[0].local_seq;
        lan.till
            .write(move |tx| mark_sent(tx, first, crate::sync::testkit::HOST_ID, NOON))
            .unwrap();
        assert_eq!(pokes.load(Ordering::SeqCst), 1);
    }
}
