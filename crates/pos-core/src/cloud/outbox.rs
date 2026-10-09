//! The cloud outbox (`sync_outbox`, target `cloud`, spec §5.1): durable across restarts and weeks
//! offline, one row per thing to tell DZBuild, each with its `Idempotency-Key`. Pending and failed
//! rows are never deleted; sent and skipped ones are pruned after 30 days. A transient failure
//! backs off (30 s → 1 h); a 4xx fails the row until the owner retries it (with a new key, since
//! DZBuild replays a stored 4xx for 24 h).

use super::http::Endpoint;
use super::store::{CloudNoticeKind, raise_notice};
use crate::error::AppError;
use crate::ids::new_id;
use crate::json::JsonValue;
use crate::sqlenum::sql_text_enum;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

pub const PRUNE_AFTER_MS: i64 = 30 * 24 * 3_600_000;
const BACKOFF_MS: [i64; 6] = [30_000, 60_000, 120_000, 300_000, 900_000, 3_600_000];

pub fn backoff_ms(attempts: i64) -> i64 {
    let index = usize::try_from(attempts.max(0))
        .unwrap_or(usize::MAX)
        .min(BACKOFF_MS.len() - 1);
    BACKOFF_MS[index]
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum OutboxKind {
    ProductUpsert,
    MediaUpload,
    StockBatch,
    PosSale,
    PosRefund,
    Closure,
    OrderClaim,
    OrderStatus,
    OrderCancel,
}

impl OutboxKind {
    /// The push order: a product before its photo and its stock, a sale before its refund.
    pub const ALL: [OutboxKind; 9] = [
        OutboxKind::ProductUpsert,
        OutboxKind::MediaUpload,
        OutboxKind::StockBatch,
        OutboxKind::PosSale,
        OutboxKind::PosRefund,
        OutboxKind::Closure,
        OutboxKind::OrderClaim,
        OutboxKind::OrderStatus,
        OutboxKind::OrderCancel,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            OutboxKind::ProductUpsert => "product_upsert",
            OutboxKind::MediaUpload => "media_upload",
            OutboxKind::StockBatch => "stock_batch",
            OutboxKind::PosSale => "pos_sale",
            OutboxKind::PosRefund => "pos_refund",
            OutboxKind::Closure => "closure",
            OutboxKind::OrderClaim => "order_claim",
            OutboxKind::OrderStatus => "order_status",
            OutboxKind::OrderCancel => "order_cancel",
        }
    }

    pub fn parse(s: &str) -> Option<OutboxKind> {
        OutboxKind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    pub const fn endpoint(self) -> Endpoint {
        match self {
            OutboxKind::ProductUpsert => Endpoint::ProductsBatch,
            OutboxKind::MediaUpload => Endpoint::MediaUploads,
            OutboxKind::StockBatch => Endpoint::InventoryAdjustments,
            OutboxKind::PosSale => Endpoint::PosSales,
            OutboxKind::PosRefund => Endpoint::PosRefunds,
            OutboxKind::Closure => Endpoint::PosClosures,
            OutboxKind::OrderClaim => Endpoint::OrderClaim,
            OutboxKind::OrderStatus => Endpoint::OrderUpdate,
            OutboxKind::OrderCancel => Endpoint::OrderCancel,
        }
    }
}

sql_text_enum!(OutboxKind);

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxItem {
    pub id: String,
    pub kind: OutboxKind,
    pub entity_id: String,
    pub payload: Value,
    /// The payload exactly as stored: a frozen body is sent from this text, never from
    /// `payload` (parsing JSON into a `Value` reads numbers as floats).
    pub raw: String,
    pub idempotency_key: String,
    pub attempts: i64,
    pub depends_on: Option<String>,
}

impl OutboxItem {
    /// Nothing built yet (`{}`): the sender builds the body once and freezes it.
    pub fn is_unbuilt(&self) -> bool {
        self.payload
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewItem {
    pub store_id: String,
    pub kind: OutboxKind,
    pub entity_id: String,
    pub payload: Value,
    pub key: String,
    pub depends_on: Option<String>,
}

/// Adds the item; `None` when an item with this key exists, or when a not-yet-attempted upsert
/// of the same product or photo is already waiting (`idx_sync_outbox_coalesce`). A waiting
/// product upsert reads the latest state when it is sent, so it stays as it is; a waiting photo
/// names its file, so it takes the newer one and its key (nothing of it has reached DZBuild yet,
/// and the frozen triggers allow it before the first attempt). SQLite tries the conflict targets
/// in the order written, so a key another row already holds ends in `DO NOTHING` and is never
/// moved onto the waiting photo.
pub fn enqueue(tx: &Transaction<'_>, item: NewItem, now: i64) -> Result<Option<String>, AppError> {
    let id = new_id();
    let row: Option<String> = tx
        .query_row(
            "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, payload, idempotency_key,
                 depends_on, created_at)
             VALUES (?1, 'cloud', ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (idempotency_key) DO NOTHING
             ON CONFLICT (target, kind, entity_id)
               WHERE status = 'pending' AND attempts = 0 AND kind IN ('product_upsert', 'media_upload')
               DO UPDATE SET payload = excluded.payload, idempotency_key = excluded.idempotency_key
               WHERE excluded.kind = 'media_upload'
             RETURNING id",
            params![id, item.store_id, item.kind, item.entity_id, item.payload.to_string(), item.key, item.depends_on, now],
            |r| r.get(0),
        )
        .optional()?;
    Ok(row.filter(|r| *r == id))
}

fn map_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxItem> {
    let payload: String = r.get(3)?;
    Ok(OutboxItem {
        id: r.get(0)?,
        kind: r.get(1)?,
        entity_id: r.get(2)?,
        payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
        raw: payload,
        idempotency_key: r.get(4)?,
        attempts: r.get(5)?,
        depends_on: r.get(6)?,
    })
}

/// Pending items of `kind` whose time has come, oldest first; an item waiting on another one
/// (a refund on its sale) only once that one is sent.
pub fn due(
    conn: &Connection,
    store: &str,
    kind: OutboxKind,
    now: i64,
    limit: usize,
) -> Result<Vec<OutboxItem>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT o.id, o.kind, o.entity_id, o.payload, o.idempotency_key, o.attempts, o.depends_on
         FROM sync_outbox o
         WHERE o.target = 'cloud' AND o.store_id = ?1 AND o.kind = ?2 AND o.status = 'pending'
           AND o.next_attempt_at <= ?3
           AND (o.depends_on IS NULL OR EXISTS (SELECT 1 FROM sync_outbox d WHERE d.id = o.depends_on AND d.status = 'sent'))
         ORDER BY o.created_at, o.rowid LIMIT ?4",
    )?;
    let rows = stmt
        .query_map(
            params![store, kind, now, i64::try_from(limit).unwrap_or(i64::MAX)],
            map_item,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<OutboxItem, AppError> {
    conn.query_row(
        "SELECT id, kind, entity_id, payload, idempotency_key, attempts, depends_on FROM sync_outbox WHERE id = ?1",
        [id],
        map_item,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("cloud_outbox_item"))
}

/// Stores the body built for the first attempt; later attempts resend exactly this.
/// Freezes the item's body as `text`, the bytes its first request sends and every resend sends
/// again (serialized once, never through a `Value`).
pub fn freeze(tx: &Transaction<'_>, id: &str, text: &str) -> Result<(), AppError> {
    tx.execute(
        "UPDATE sync_outbox SET payload = ?2 WHERE id = ?1 AND attempts = 0",
        params![id, text],
    )?;
    Ok(())
}

pub fn mark_sent(
    tx: &Transaction<'_>,
    id: &str,
    remote_id: Option<&str>,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE sync_outbox SET status = 'sent', sent_at = ?2, remote_id = COALESCE(?3, remote_id),
             last_error = NULL, error_details = NULL WHERE id = ?1",
        params![id, now, remote_id],
    )?;
    Ok(())
}

/// A 4xx: the item waits for the owner (Settings › DZBuild › "À corriger"); one notice per kind.
pub fn mark_failed(
    tx: &Transaction<'_>,
    store: &str,
    item: &OutboxItem,
    code: &str,
    details: Option<Value>,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE sync_outbox SET status = 'failed', attempts = attempts + 1, last_error = ?2, error_details = ?3
         WHERE id = ?1",
        params![item.id, code, details.map(|d| d.to_string())],
    )?;
    let failed: i64 = tx.query_row(
        "SELECT COUNT(*) FROM sync_outbox WHERE target = 'cloud' AND store_id = ?1 AND kind = ?2 AND status = 'failed'",
        params![store, item.kind],
        |r| r.get(0),
    )?;
    raise_notice(
        tx,
        CloudNoticeKind::FailedItems,
        "cloud_request_invalid",
        item.kind.as_str(),
        json!({ "count": failed }),
        now,
    )?;
    Ok(())
}

/// A transient failure: try again later (`attempts` is the count before this one).
pub fn reschedule(tx: &Transaction<'_>, id: &str, attempts: i64, now: i64) -> Result<(), AppError> {
    tx.execute(
        "UPDATE sync_outbox SET attempts = attempts + 1, next_attempt_at = ?2, last_error = 'transient' WHERE id = ?1",
        params![id, now + backoff_ms(attempts + 1)],
    )?;
    Ok(())
}

/// An endpoint not shipped yet: every pending item of `kind` waits until `until`.
pub fn defer_kind(
    tx: &Transaction<'_>,
    store: &str,
    kind: OutboxKind,
    until: i64,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE sync_outbox SET next_attempt_at = MAX(next_attempt_at, ?3)
         WHERE target = 'cloud' AND store_id = ?1 AND kind = ?2 AND status = 'pending'",
        params![store, kind, until],
    )?;
    Ok(())
}

pub fn skip(tx: &Transaction<'_>, id: &str, reason: &str) -> Result<(), AppError> {
    tx.execute(
        "UPDATE sync_outbox SET status = 'skipped', last_error = ?2 WHERE id = ?1",
        params![id, reason],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EntityCount {
    pub kind: OutboxKind,
    pub pending: i64,
    pub failed: i64,
    pub last_sent_at: Option<i64>,
}

pub fn counts(conn: &Connection, store: &str) -> Result<Vec<EntityCount>, AppError> {
    OutboxKind::ALL
        .into_iter()
        .map(|kind| {
            conn.query_row(
                "SELECT COALESCE(SUM(status = 'pending'), 0), COALESCE(SUM(status = 'failed'), 0), MAX(sent_at)
                 FROM sync_outbox WHERE target = 'cloud' AND store_id = ?1 AND kind = ?2",
                params![store, kind],
                |r| Ok(EntityCount { kind, pending: r.get(0)?, failed: r.get(1)?, last_sent_at: r.get(2)? }),
            )
            .map_err(AppError::from)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FailedItem {
    pub id: String,
    pub kind: OutboxKind,
    pub entity_id: String,
    /// The document number or the product name, for the owner.
    pub label: Option<String>,
    pub code: String,
    #[specta(type = Option<JsonValue>)]
    pub details: Option<Value>,
    pub attempts: i64,
    pub created_at: i64,
}

pub fn failed(conn: &Connection, store: &str, limit: usize) -> Result<Vec<FailedItem>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT o.id, o.kind, o.entity_id,
                COALESCE((SELECT d.number FROM documents d WHERE d.id = o.entity_id),
                         (SELECT COALESCE(NULLIF(p.name_fr, ''), NULLIF(p.name_ar, ''), p.name_en) FROM products p WHERE p.id = o.entity_id)),
                COALESCE(o.last_error, ''), o.error_details, o.attempts, o.created_at
         FROM sync_outbox o WHERE o.target = 'cloud' AND o.store_id = ?1 AND o.status = 'failed'
         ORDER BY o.created_at DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(
            params![store, i64::try_from(limit).unwrap_or(i64::MAX)],
            |r| {
                let details: Option<String> = r.get(5)?;
                Ok(FailedItem {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    entity_id: r.get(2)?,
                    label: r.get(3)?,
                    code: r.get(4)?,
                    details: details.and_then(|d| serde_json::from_str(&d).ok()),
                    attempts: r.get(6)?,
                    created_at: r.get(7)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The owner fixed the cause: send again with a new key (`…:r<n>`) and a rebuilt body. A stock
/// batch is not resent — its baseline was released and the next derive plans it again.
pub fn retry(tx: &Transaction<'_>, id: &str, now: i64) -> Result<(), AppError> {
    let item = get(tx, id)?;
    let status: String =
        tx.query_row("SELECT status FROM sync_outbox WHERE id = ?1", [id], |r| {
            r.get(0)
        })?;
    if status != "failed" {
        return Err(AppError::not_found("cloud_outbox_item"));
    }
    if item.kind == OutboxKind::StockBatch {
        return skip(tx, id, "rederived");
    }
    let (base, n) = match item.idempotency_key.rsplit_once(":r") {
        Some((base, n)) if n.parse::<u32>().is_ok() => {
            (base.to_owned(), n.parse::<u32>().unwrap_or(0) + 1)
        }
        _ => (item.idempotency_key.clone(), 1),
    };
    // A base key is at most 44 characters for every one-shot kind (`closure:<uuid>` is the
    // longest), so the suffix always fits the 64-character CHECK.
    let key = format!("{base}:r{n}");
    debug_assert!(key.len() <= 64);
    // An order update keeps its request; a refund keeps the sale it goes to (`cloud::push` finds
    // the path by it); a photo keeps the file it uploads (an empty one is skipped as missing);
    // every other body is built again.
    tx.execute(
        "UPDATE sync_outbox SET status = 'pending', attempts = 0, next_attempt_at = ?2, idempotency_key = ?3,
             payload = CASE WHEN kind IN ('order_claim', 'order_status', 'order_cancel') THEN payload
                            WHEN kind = 'pos_refund' AND json_extract(payload, '$.sale') IS NOT NULL
                              THEN json_object('sale', json_extract(payload, '$.sale'))
                            WHEN kind = 'media_upload' AND json_extract(payload, '$.image') IS NOT NULL
                              THEN json_object('image', json_extract(payload, '$.image'))
                            ELSE '{}' END,
             last_error = NULL, error_details = NULL WHERE id = ?1",
        params![id, now, key],
    )?;
    Ok(())
}

pub fn prune_sent(tx: &Transaction<'_>, now: i64) -> Result<usize, AppError> {
    Ok(tx.execute(
        "DELETE FROM sync_outbox WHERE status IN ('sent', 'skipped') AND COALESCE(sent_at, created_at) < ?1",
        [now - PRUNE_AFTER_MS],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    fn item(kind: OutboxKind, entity: &str, key: &str) -> NewItem {
        NewItem {
            store_id: "141".into(),
            kind,
            entity_id: entity.into(),
            payload: json!({}),
            key: key.into(),
            depends_on: None,
        }
    }

    #[test]
    fn a_failed_sale_retries_with_a_new_key_and_open_items_survive_pruning() {
        let db = Db::open_memory().unwrap();
        let id = db
            .write(|tx| enqueue(tx, item(OutboxKind::PosSale, "d1", "sale:d1"), 0))
            .unwrap()
            .unwrap();
        assert_eq!(
            db.write(|tx| enqueue(tx, item(OutboxKind::PosSale, "d1", "sale:d1"), 0))
                .unwrap(),
            None
        );
        let listed = db
            .read(|c| due(c, "141", OutboxKind::PosSale, 0, 10))
            .unwrap();
        assert_eq!(listed.len(), 1);
        let first = listed[0].clone();
        db.write(move |tx| {
            mark_failed(
                tx,
                "141",
                &first,
                "validation_failed",
                Some(json!({ "field": "lines" })),
                5,
            )
        })
        .unwrap();
        assert!(
            db.read(|c| due(c, "141", OutboxKind::PosSale, 10, 10))
                .unwrap()
                .is_empty(),
            "a 4xx is not retried by itself"
        );
        let failed = db.read(|c| failed(c, "141", 10)).unwrap();
        assert_eq!(
            (failed.len(), failed[0].code.as_str()),
            (1, "validation_failed")
        );
        let retry_id = id.clone();
        db.write(move |tx| retry(tx, &retry_id, 20)).unwrap();
        let again = db
            .read(|c| due(c, "141", OutboxKind::PosSale, 20, 10))
            .unwrap();
        assert_eq!(
            again[0].idempotency_key, "sale:d1:r1",
            "the stored 4xx answer is not replayed"
        );
        assert_eq!(
            db.write(|tx| prune_sent(tx, i64::MAX / 2)).unwrap(),
            0,
            "open items are kept"
        );
        let sent_id = id.clone();
        db.write(move |tx| mark_sent(tx, &sent_id, Some("99120"), 30))
            .unwrap();
        assert_eq!(
            db.write(|tx| prune_sent(tx, 30 + PRUNE_AFTER_MS + 1))
                .unwrap(),
            1
        );
    }

    #[test]
    fn a_retried_refund_is_rebuilt_and_still_names_its_sale() {
        let db = Db::open_memory().unwrap();
        let refund = NewItem {
            payload: json!({ "sale": "d1" }),
            ..item(OutboxKind::PosRefund, "n1", "refund:n1")
        };
        let id = db.write(move |tx| enqueue(tx, refund, 0)).unwrap().unwrap();
        let built = id.clone();
        db.write(move |tx| {
            freeze(
                tx,
                &built,
                r#"{"sale":"d1","body":{"external_id":"n1","total":"100.00"}}"#,
            )
        })
        .unwrap();
        let read = id.clone();
        let refused = db.read(move |c| get(c, &read)).unwrap();
        db.write(move |tx| mark_failed(tx, "141", &refused, "validation_failed", None, 1))
            .unwrap();
        let retried = id.clone();
        db.write(move |tx| retry(tx, &retried, 2)).unwrap();
        let again = db.read(move |c| get(c, &id)).unwrap();
        assert_eq!(
            (again.payload, again.idempotency_key.as_str()),
            (json!({ "sale": "d1" }), "refund:n1:r1"),
            "the body is built again, the sale it goes to is kept"
        );
    }

    /// The owner's retry of a failed photo keeps the file it uploads: an empty payload would be
    /// skipped as `media_missing`, its key kept taken for 30 days.
    #[test]
    fn a_retried_photo_keeps_its_file() {
        let db = Db::open_memory().unwrap();
        let id = db
            .write(|tx| enqueue(tx, photo("a.jpg"), 0))
            .unwrap()
            .unwrap();
        let read = id.clone();
        let refused = db.read(move |c| get(c, &read)).unwrap();
        db.write(move |tx| mark_failed(tx, "141", &refused, "media_upload_failed", None, 1))
            .unwrap();
        let retried = id.clone();
        db.write(move |tx| retry(tx, &retried, 2)).unwrap();
        let again = db.read(move |c| get(c, &id)).unwrap();
        assert_eq!(
            (again.payload, again.idempotency_key.as_str()),
            (json!({ "image": "a.jpg" }), "media:p1:a.jpg:r1")
        );
    }

    fn photo(image: &str) -> NewItem {
        NewItem {
            payload: json!({ "image": image }),
            ..item(OutboxKind::MediaUpload, "p1", &format!("media:p1:{image}"))
        }
    }

    fn waiting(db: &Db, kind: OutboxKind) -> Vec<OutboxItem> {
        db.read(move |c| due(c, "141", kind, 0, 10)).unwrap()
    }

    #[test]
    fn a_waiting_photo_takes_the_newer_file_and_a_waiting_product_stays_as_it_is() {
        let db = Db::open_memory().unwrap();
        let first = db
            .write(|tx| enqueue(tx, photo("a.jpg"), 0))
            .unwrap()
            .unwrap();
        assert_eq!(
            db.write(|tx| enqueue(tx, photo("b.jpg"), 1)).unwrap(),
            None,
            "no second row"
        );
        let listed = waiting(&db, OutboxKind::MediaUpload);
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (
                listed[0].id.as_str(),
                listed[0].payload["image"].as_str(),
                listed[0].idempotency_key.as_str()
            ),
            (first.as_str(), Some("b.jpg"), "media:p1:b.jpg"),
            "the newer photo, under its own key"
        );
        // the newer photo is sent; a later one waits; a photo whose key an older row already
        // holds never breaks the queue
        db.write(move |tx| mark_sent(tx, &first, None, 2)).unwrap();
        let later = db
            .write(|tx| enqueue(tx, photo("c.jpg"), 3))
            .unwrap()
            .unwrap();
        assert_eq!(db.write(|tx| enqueue(tx, photo("b.jpg"), 4)).unwrap(), None);
        let listed = waiting(&db, OutboxKind::MediaUpload);
        assert_eq!((listed.len(), listed[0].id.as_str()), (1, later.as_str()));

        let product = db
            .write(|tx| enqueue(tx, item(OutboxKind::ProductUpsert, "p1", "product:x"), 0))
            .unwrap()
            .unwrap();
        assert_eq!(
            db.write(|tx| enqueue(tx, item(OutboxKind::ProductUpsert, "p1", "product:y"), 1))
                .unwrap(),
            None
        );
        let listed = waiting(&db, OutboxKind::ProductUpsert);
        assert_eq!(
            (
                listed.len(),
                listed[0].id.as_str(),
                listed[0].idempotency_key.as_str()
            ),
            (1, product.as_str(), "product:x"),
            "a product upsert reads the latest state when it is sent: the waiting row stays"
        );
    }

    #[test]
    fn transient_failures_back_off_and_refunds_wait_for_their_sale() {
        let db = Db::open_memory().unwrap();
        let sale = db
            .write(|tx| enqueue(tx, item(OutboxKind::PosSale, "d1", "sale:d1"), 0))
            .unwrap()
            .unwrap();
        let refund = NewItem {
            depends_on: Some(sale.clone()),
            ..item(OutboxKind::PosRefund, "r1", "refund:r1")
        };
        db.write(move |tx| enqueue(tx, refund, 0)).unwrap();
        assert!(
            db.read(|c| due(c, "141", OutboxKind::PosRefund, 0, 10))
                .unwrap()
                .is_empty(),
            "the sale first"
        );
        let s = sale.clone();
        db.write(move |tx| reschedule(tx, &s, 0, 1_000)).unwrap();
        assert!(
            db.read(|c| due(c, "141", OutboxKind::PosSale, 1_000 + backoff_ms(1) - 1, 10))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.read(|c| due(c, "141", OutboxKind::PosSale, 1_000 + backoff_ms(1), 10))
                .unwrap()
                .len(),
            1
        );
        db.write(move |tx| mark_sent(tx, &sale, None, 2_000))
            .unwrap();
        assert_eq!(
            db.read(|c| due(c, "141", OutboxKind::PosRefund, 2_000, 10))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(backoff_ms(0), 30_000);
        assert_eq!(backoff_ms(99), 3_600_000, "capped at an hour");
        let counts = db.read(|c| counts(c, "141")).unwrap();
        let sales = counts
            .iter()
            .find(|c| c.kind == OutboxKind::PosSale)
            .unwrap();
        assert_eq!((sales.pending, sales.last_sent_at), (0, Some(2_000)));
    }
}
