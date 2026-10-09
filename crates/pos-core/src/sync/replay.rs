//! The main PC records a till's outbox item (spec §4.6): one host transaction per item, the
//! host's own triggers on (a counter only steps by one, a validated document is final, a drawer
//! movement needs an open session), so the till's rows are checked by the same rules that made
//! them. The till is the authority of its rows: they are stored as sent, except what depends on
//! the other tills — a kridi entry's balance and allocations — and the link of a validated
//! document when the host's chain of that series differs (after a restore on one side only).
//! Those differences go back as adjustments; nothing is deleted. What the till could not know
//! offline (stock gone, a limit passed) is recorded with a notice: the sale happened.

use super::capture::{Column, columns, primary_key};
use super::catalogue::{self, SyncClass, TableSpec, owners_of_image, quote};
use super::changelog;
use super::feed::Op;
use super::notices::{self, NoticeKind};
use super::outbox::{Adjustment, ItemResult, ItemStatus, WireItem};
use super::{SKEW_WARN_MS, journal};
use crate::audit::{self, Actor};
use crate::db::Db;
use crate::error::AppError;
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};

/// The till an item comes from, as its token names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sender {
    pub terminal_id: String,
    pub code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Received {
    /// The host's clock when the item arrived.
    pub at: i64,
    /// The till's clock offset (host minus till) it measured last, when it did.
    pub skew_ms: Option<i64>,
}

/// Codes that refuse an item for good: the till keeps it as `rejected` and the line stops.
const REFUSALS: [&str; 5] = [
    "replay_forbidden_row",
    "replay_invalid",
    "replay_conflict",
    "lan_version_mismatch",
    "constraint_violation",
];

/// The code an item is refused with for good, or `None` for an error another attempt may not
/// meet (the main PC busy or closing, a disk error, an unknown or revoked till). Besides
/// [`REFUSALS`], an item that names a row the main PC does not have or carries a value it
/// refuses gets the same answer at every attempt: it is `replay_invalid`, not an error the
/// till would retry forever.
fn refusal(error: &AppError) -> Option<String> {
    let code = error.code();
    if code == "constraint_violation" || code == DOCUMENT_MISMATCH {
        return Some("replay_invalid".to_owned());
    }
    if REFUSALS.contains(&code.as_str()) {
        return Some(code);
    }
    matches!(
        error,
        AppError::NotFound { .. } | AppError::Validation { .. }
    )
    .then(|| "replay_invalid".to_owned())
}

/// A till's validated document whose content does not hash to the hash the till sent.
pub const DOCUMENT_MISMATCH: &str = "replay_document_mismatch";

fn forbidden() -> AppError {
    AppError::conflict("replay_forbidden_row")
}

fn invalid() -> AppError {
    AppError::conflict("replay_invalid")
}

fn to_sql(value: &Value, col: &Column) -> Result<SqlValue, AppError> {
    Ok(match value {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
        Value::Number(n) => SqlValue::Integer(n.as_i64().ok_or_else(invalid)?),
        Value::String(s) if col.is_blob() => SqlValue::Blob(hex::decode(s).map_err(|_| invalid())?),
        Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    })
}

fn from_sql(value: SqlValue) -> Value {
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => json!(i),
        SqlValue::Text(t) => Value::String(t),
        SqlValue::Blob(b) => Value::String(hex::encode_upper(b)),
        SqlValue::Real(r) => json!(r.to_string()),
    }
}

/// The host's copy of the row an image names, as an image of the same columns.
fn host_row(
    tx: &Transaction<'_>,
    spec: &TableSpec,
    cols: &[Column],
    image: &Map<String, Value>,
) -> Result<Option<Map<String, Value>>, AppError> {
    let pk = primary_key(cols);
    let wanted: Vec<&Column> = cols
        .iter()
        .filter(|c| image.contains_key(&c.name))
        .collect();
    let clause = pk
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{} = ?{}", quote(&c.name), i + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!(
        "SELECT {} FROM {} WHERE {clause}",
        wanted
            .iter()
            .map(|c| quote(&c.name))
            .collect::<Vec<_>>()
            .join(", "),
        quote(spec.name)
    );
    let keys = pk
        .iter()
        .map(|c| to_sql(image.get(&c.name).unwrap_or(&Value::Null), c))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tx
        .query_row(&sql, params_from_iter(keys), |r| {
            let mut out = Map::new();
            for (i, c) in wanted.iter().enumerate() {
                out.insert(c.name.clone(), from_sql(r.get::<_, SqlValue>(i)?));
            }
            Ok(out)
        })
        .optional()?)
}

fn same(host: &Map<String, Value>, image: &Map<String, Value>, ignore: &[&str]) -> bool {
    image
        .iter()
        .filter(|(k, _)| !ignore.contains(&k.as_str()))
        .all(|(k, v)| host.get(k) == Some(v))
}

/// The host holds the row in a state its image leads to: the item was applied by an earlier main
/// PC and reached this one through that main PC's feed before this one was promoted (LAN-010),
/// so a till joining it again resends it. A counter past the image's number, a document no
/// longer a draft (validated once, final), a closed drawer session.
fn host_is_later(spec: &TableSpec, host: &Map<String, Value>, image: &Map<String, Value>) -> bool {
    let int = |m: &Map<String, Value>, k: &str| m.get(k).and_then(Value::as_i64);
    let text = |m: &Map<String, Value>, k: &str| -> Option<String> {
        m.get(k).and_then(Value::as_str).map(str::to_owned)
    };
    match spec.name {
        "counters" => matches!(
            (int(host, "last_number"), int(image, "last_number")),
            (Some(h), Some(i)) if h >= i
        ),
        "documents" => {
            let held = text(host, "status");
            held.as_deref().is_some_and(|s| s != "draft")
                && (text(image, "status").as_deref() == Some("draft")
                    || text(image, "status") == held)
        }
        "cash_sessions" => text(host, "status").as_deref() == Some("closed"),
        _ => false,
    }
}

/// Takes a `Connection` so that a `Savepoint` can run it too (`check_chain`).
fn write_row(
    conn: &rusqlite::Connection,
    spec: &TableSpec,
    cols: &[Column],
    image: &Map<String, Value>,
    op: Op,
) -> Result<(), AppError> {
    let present: Vec<&Column> = cols
        .iter()
        .filter(|c| image.contains_key(&c.name))
        .collect();
    let values = present
        .iter()
        .map(|c| to_sql(&image[&c.name], c))
        .collect::<Result<Vec<_>, _>>()?;
    let sql = match op {
        Op::Insert => format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quote(spec.name),
            present
                .iter()
                .map(|c| quote(&c.name))
                .collect::<Vec<_>>()
                .join(", "),
            (1..=present.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Op::Update => {
            let set = present
                .iter()
                .enumerate()
                .filter(|(_, c)| c.pk == 0)
                .map(|(i, c)| format!("{} = ?{}", quote(&c.name), i + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let key = present
                .iter()
                .enumerate()
                .filter(|(_, c)| c.pk > 0)
                .map(|(i, c)| format!("{} = ?{}", quote(&c.name), i + 1))
                .collect::<Vec<_>>()
                .join(" AND ");
            format!("UPDATE {} SET {set} WHERE {key}", quote(spec.name))
        }
        Op::Delete => return Err(forbidden()),
    };
    conn.execute(&sql, params_from_iter(values))?;
    Ok(())
}

struct Effects {
    adjustments: Vec<Adjustment>,
    products: BTreeSet<String>,
    customers: BTreeSet<String>,
    overdrawn: BTreeSet<String>,
    closures: Vec<Map<String, Value>>,
}

/// `balance_after` from the host's balance: the till could not see the other tills' entries.
fn rebase_ledger(
    tx: &Transaction<'_>,
    image: &mut Map<String, Value>,
    fx: &mut Effects,
) -> Result<(), AppError> {
    let customer = image
        .get("customer_id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
        .to_owned();
    let amount = image
        .get("amount")
        .and_then(Value::as_i64)
        .ok_or_else(invalid)?;
    let balance: i64 = tx.query_row(
        "SELECT balance FROM customers WHERE id = ?1",
        [&customer],
        |r| r.get(0),
    )?;
    let rebased = balance.checked_add(amount).ok_or_else(invalid)?;
    let till_value = image
        .get("balance_after")
        .and_then(Value::as_i64)
        .ok_or_else(invalid)?;
    if rebased != till_value {
        let id = image
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?
            .to_owned();
        fx.adjustments.push(Adjustment {
            table: "customer_ledger".into(),
            row_id: id,
            fields: json!({ "balance_after": rebased })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        });
        if image.get("kind").and_then(Value::as_str) == Some("voucher_redeemed")
            && till_value <= 0
            && rebased > 0
        {
            fx.overdrawn.insert(customer.clone());
        }
        image.insert("balance_after".into(), json!(rebased));
    }
    fx.customers.insert(customer);
    Ok(())
}

/// Whether both entries of an allocation still have `amount` unallocated on the host.
fn allocation_fits(tx: &Transaction<'_>, image: &Map<String, Value>) -> Result<bool, AppError> {
    let amount = image
        .get("amount")
        .and_then(Value::as_i64)
        .ok_or_else(invalid)?;
    let entry = |side: &str| image.get(side).and_then(Value::as_str).ok_or_else(invalid);
    let free: (Option<i64>, Option<i64>) = tx.query_row(
        "SELECT (SELECT ABS(l.amount) - COALESCE((SELECT SUM(a.amount) FROM customer_allocations a
                                                   WHERE a.debit_id = l.id), 0)
                 FROM customer_ledger l WHERE l.id = ?1),
                (SELECT ABS(l.amount) - COALESCE((SELECT SUM(a.amount) FROM customer_allocations a
                                                   WHERE a.credit_id = l.id), 0)
                 FROM customer_ledger l WHERE l.id = ?2)",
        params![entry("debit_id")?, entry("credit_id")?],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok([free.0, free.1]
        .into_iter()
        .all(|f| f.is_some_and(|f| f >= amount)))
}

/// A ticket's validation, checked against the host's anchor of its series (in a savepoint that
/// is rolled back); rebased when the host's chain differs.
fn check_chain(
    tx: &mut Transaction<'_>,
    spec: &TableSpec,
    cols: &[Column],
    image: &mut Map<String, Value>,
    fx: &mut Effects,
    sender: &Sender,
    now: i64,
) -> Result<(), AppError> {
    let doc_type = image
        .get("doc_type")
        .and_then(Value::as_str)
        .and_then(crate::numbering::DocType::parse)
        .ok_or_else(invalid)?;
    let series = image
        .get("series")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
        .to_owned();
    let id = image
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
        .to_owned();
    let (anchor_seq, anchor_hash) = crate::documents::anchor(tx, doc_type, &series)?;
    let as_sent = (
        image.get("chain_seq").and_then(Value::as_i64),
        image
            .get("prev_hash")
            .and_then(Value::as_str)
            .map(str::to_owned),
        image.get("hash").and_then(Value::as_str).map(str::to_owned),
    );
    let (expected, till_link) = {
        let sp = tx.savepoint()?;
        write_row(&sp, spec, cols, image, Op::Update)?;
        let doc = crate::documents::get(&sp, &id)?;
        (
            crate::documents::document_hash(&anchor_hash, &doc),
            as_sent
                .1
                .as_deref()
                .map(|prev| crate::documents::document_hash(prev, &doc)),
        )
        // dropped here: the savepoint rolls back
    };
    // Only the host's anchor may differ: the content received must be what the till hashed.
    // A ticket whose lines, payments or totals no longer match its own hash is refused, never
    // given a fresh hash (`refusal` answers `replay_invalid`, the notice names the cause).
    if as_sent.2.is_none() || till_link != as_sent.2 {
        return Err(AppError::rejected(
            json!({ "number": image.get("number"), "document": id }),
            DOCUMENT_MISMATCH,
        ));
    }
    if as_sent
        == (
            Some(anchor_seq + 1),
            Some(anchor_hash.clone()),
            Some(expected.clone()),
        )
    {
        return Ok(());
    }
    let fields = json!({ "chain_seq": anchor_seq + 1, "prev_hash": anchor_hash, "hash": expected });
    notices::raise(
        tx,
        NoticeKind::ChainRebased,
        Some(&sender.terminal_id),
        &id,
        json!({ "number": image.get("number"), "tillHash": as_sent.2, "hostHash": fields["hash"] }),
        now,
    )?;
    let fields = fields.as_object().cloned().unwrap_or_default();
    for (k, v) in &fields {
        image.insert(k.clone(), v.clone());
    }
    fx.adjustments.push(Adjustment {
        table: "documents".into(),
        row_id: id,
        fields,
    });
    Ok(())
}

/// Records one item of `sender` in the caller's transaction (see the module doc). An item the
/// host already recorded answers `Duplicate` with the answer it gave then.
pub fn apply(
    tx: &mut Transaction<'_>,
    sender: &Sender,
    item: &WireItem,
    received: &Received,
) -> Result<ItemResult, AppError> {
    let bundle = &item.bundle;
    if let Some(stored) = tx
        .query_row(
            "SELECT result FROM lan_inbox WHERE idempotency_key = ?1",
            [&bundle.key],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        let mut result: ItemResult = serde_json::from_str(&stored)?;
        result.status = ItemStatus::Duplicate;
        return Ok(result);
    }
    if bundle.terminal_id != sender.terminal_id {
        return Err(forbidden());
    }
    let last: Option<i64> = tx
        .query_row(
            "SELECT last_replayed_seq FROM lan_devices WHERE terminal_id = ?1 AND revoked_at IS NULL",
            [&sender.terminal_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::auth("lan_token_invalid"))?;
    if let Some(last) = last
        && item.local_seq > last + 1
    {
        return Err(AppError::rejected(
            json!({ "expected": last + 1 }),
            "replay_out_of_order",
        ));
    }
    let now = received.at;
    let mut fx = Effects {
        adjustments: Vec::new(),
        products: BTreeSet::new(),
        customers: BTreeSet::new(),
        overdrawn: BTreeSet::new(),
        closures: Vec::new(),
    };
    let mut schemas: HashMap<String, Vec<Column>> = HashMap::new();
    for row in &bundle.rows {
        if schemas.contains_key(&row.table) {
            continue;
        }
        let spec = catalogue::spec(&row.table)
            .filter(|s| s.class == SyncClass::TillOwned)
            .ok_or_else(forbidden)?;
        if !catalogue::present(tx, spec.name)? {
            return Err(AppError::conflict("lan_version_mismatch"));
        }
        schemas.insert(row.table.clone(), columns(tx, spec.name)?);
    }
    // A row may appear several times in one item (a ticket inserted as a draft, then validated):
    // a resend compares the host's row with the row's last image in the item.
    let key_of = |table: &str, image: &Map<String, Value>| -> String {
        schemas
            .get(table)
            .map(|cols| {
                primary_key(cols)
                    .iter()
                    .map(|c| image.get(&c.name).map(Value::to_string).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .unwrap_or_default()
    };
    let mut last_of: HashMap<(String, String), usize> = HashMap::new();
    for (i, row) in bundle.rows.iter().enumerate() {
        last_of.insert((row.table.clone(), key_of(&row.table, &row.image)), i);
    }
    // Rows written or newly handled here: none means the host already held every row of the
    // item as sent or later (a resend to a main PC that learnt them from its predecessor).
    let mut handled = 0_usize;
    for (i, row) in bundle.rows.iter().enumerate() {
        let spec = catalogue::spec(&row.table).ok_or_else(forbidden)?;
        let cols = schemas.get(&row.table).cloned().ok_or_else(forbidden)?;
        if row.image.keys().any(|k| !cols.iter().any(|c| &c.name == k)) {
            return Err(AppError::conflict("lan_version_mismatch"));
        }
        let last = last_of
            .get(&(row.table.clone(), key_of(&row.table, &row.image)))
            .copied()
            .unwrap_or(i);
        let final_image = &bundle.rows[last].image;
        if let Some((column, values)) = spec.till_only
            && !row
                .image
                .get(column)
                .and_then(Value::as_str)
                .is_some_and(|v| values.contains(&v))
        {
            return Err(forbidden());
        }
        if row.op == Op::Delete {
            return Err(forbidden());
        }
        let mut image = row.image.clone();
        let existing = host_row(tx, spec, &cols, &image)?;
        // As the till's guard: the row must be the sender's as written (NEW) and, when the host
        // has it, as the host holds it (OLD). An allocation is the sender's when either of its
        // entries is (the till's guard_sql).
        let mut images = vec![&image];
        if let Some(host) = &existing {
            images.push(host);
        }
        for owned in images {
            if !owners_of_image(tx, spec, owned)?
                .iter()
                .any(|o| o == &sender.terminal_id)
            {
                return Err(forbidden());
            }
        }
        let ignore: &[&str] = if spec.name == "customer_ledger" {
            &["balance_after"]
        } else {
            &[]
        };
        match (row.op, existing) {
            // A resend: the host holds the row as sent, or a later state of it.
            (_, Some(host))
                if same(&host, final_image, ignore) || host_is_later(spec, &host, final_image) =>
            {
                continue;
            }
            (Op::Insert, Some(_)) => return Err(AppError::conflict("replay_conflict")),
            (Op::Update, None) => return Err(invalid()),
            (Op::Insert, None) => {
                match spec.name {
                    "customer_ledger" => rebase_ledger(tx, &mut image, &mut fx)?,
                    "customer_allocations" if !allocation_fits(tx, &image)? => {
                        let subject = image
                            .get("debit_id")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_owned();
                        notices::raise(
                            tx,
                            NoticeKind::AllocationSkipped,
                            Some(&sender.terminal_id),
                            &subject,
                            Value::Object(image.clone()),
                            now,
                        )?;
                        handled += 1;
                        continue;
                    }
                    "cash_closures" => fx.closures.push(image.clone()),
                    _ => {}
                }
                if spec.name == "stock_movements"
                    && let Some(p) = image.get("product_id").and_then(Value::as_str)
                {
                    fx.products.insert(p.to_owned());
                }
                write_row(tx, spec, &cols, &image, Op::Insert)?;
                handled += 1;
            }
            (Op::Update, Some(host)) => {
                let validating = spec.name == "documents"
                    && host.get("status").and_then(Value::as_str) == Some("draft")
                    && image.get("status").and_then(Value::as_str) == Some("validated");
                if validating {
                    check_chain(tx, spec, &cols, &mut image, &mut fx, sender, now)?;
                }
                write_row(tx, spec, &cols, &image, Op::Update)?;
                handled += 1;
            }
            (Op::Delete, _) => return Err(forbidden()),
        }
    }
    let resend = !bundle.rows.is_empty() && handled == 0;
    let mut broken_journal = false;
    // Rows over a seq the host already holds for the till (it went back to an older copy):
    // not stored, so what they were goes into this item's `lan.replayed` row.
    let mut collisions = Vec::new();
    for row in &bundle.audit {
        let appended = journal::append(tx, &sender.terminal_id, row, now)?;
        broken_journal |= !appended.intact && !appended.duplicate;
        if appended.collided {
            collisions.push(json!({ "seq": row.seq, "id": row.id, "hash": row.hash }));
        }
    }
    if broken_journal {
        let mut details = json!({ "key": bundle.key });
        if !collisions.is_empty() {
            details["collisions"] = json!(collisions.len());
        }
        notices::raise(
            tx,
            NoticeKind::JournalBroken,
            Some(&sender.terminal_id),
            &sender.terminal_id,
            details,
            now,
        )?;
    }
    for closure in &fx.closures {
        let terminal = closure
            .get("terminal_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let z_seq = closure.get("z_seq").and_then(Value::as_i64).unwrap_or(0);
        // The previous Z of the terminal, as closing links it (a restore may have skipped numbers).
        let prev: Option<String> = tx
            .query_row(
                "SELECT hash FROM cash_closures WHERE terminal_id = ?1 AND z_seq < ?2
                 ORDER BY z_seq DESC LIMIT 1",
                params![terminal, z_seq],
                |r| r.get(0),
            )
            .optional()?;
        let expected = prev.unwrap_or_else(|| audit::GENESIS_HASH.to_owned());
        let subject = closure
            .get("z_number")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned();
        if closure.get("prev_hash").and_then(Value::as_str) != Some(expected.as_str()) {
            notices::raise(
                tx,
                NoticeKind::ClosureUnlinked,
                Some(&sender.terminal_id),
                &subject,
                json!({}),
                now,
            )?;
        }
        let report: Value = closure
            .get("report")
            .and_then(Value::as_str)
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or(Value::Null);
        if let Some(pending) = report
            .get("offlinePending")
            .and_then(Value::as_i64)
            .filter(|p| *p > 0)
        {
            notices::raise(
                tx,
                NoticeKind::OfflineZ,
                Some(&sender.terminal_id),
                &subject,
                json!({ "pending": pending }),
                now,
            )?;
        }
    }
    let policy: String = crate::settings::get(tx, "stock.negative_policy")?;
    let mut oversold = Vec::new();
    for product in &fx.products {
        let allowed: bool = tx.query_row(
            "SELECT allow_negative_stock FROM products WHERE id = ?1",
            [product],
            |r| r.get(0),
        )?;
        let level = crate::stock::on_hand(tx, product)?;
        if policy == "block" && !allowed && level.0 < 0 {
            notices::raise(
                tx,
                NoticeKind::Oversold,
                Some(&sender.terminal_id),
                product,
                json!({ "onHand": level.0, "key": bundle.key }),
                now,
            )?;
            oversold.push(product.clone());
        }
    }
    for customer in &fx.customers {
        let (balance, limit): (i64, Option<i64>) = tx.query_row(
            "SELECT balance, credit_limit FROM customers WHERE id = ?1",
            [customer],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if limit.is_some_and(|l| balance > l) {
            notices::raise(
                tx,
                NoticeKind::CreditOverLimit,
                Some(&sender.terminal_id),
                customer,
                json!({ "balance": balance, "limit": limit }),
                now,
            )?;
        }
    }
    for customer in &fx.overdrawn {
        notices::raise(
            tx,
            NoticeKind::StoreCreditOverdrawn,
            Some(&sender.terminal_id),
            customer,
            json!({ "key": bundle.key }),
            now,
        )?;
    }
    if let Some(skew) = received.skew_ms
        && skew.abs() > SKEW_WARN_MS
    {
        notices::raise(
            tx,
            NoticeKind::ClockSkew,
            Some(&sender.terminal_id),
            &sender.terminal_id,
            json!({ "skewMs": skew }),
            now,
        )?;
    }
    let first_user = bundle.audit.first().and_then(|a| a.user_id.clone());
    let mut replayed = json!({
        "terminal": sender.code,
        "localSeq": item.local_seq,
        "kind": bundle.kind,
        "rows": bundle.rows.len(),
        "auditSeqs": [bundle.audit.first().map(|a| a.seq), bundle.audit.last().map(|a| a.seq)],
        "adjustments": fx.adjustments.len(),
        "oversold": oversold,
        "clientCreatedAt": bundle.created_at,
        "receivedAt": now,
        "skewMs": received.skew_ms,
    });
    if !collisions.is_empty() {
        replayed["journalCollisions"] = Value::Array(collisions);
    }
    if resend {
        replayed["resend"] = json!(true);
    }
    audit::append(
        tx,
        &Actor {
            user_id: first_user,
            terminal_id: Some(sender.terminal_id.clone()),
        }
        .event("lan.replayed", "lan_item", Some(&bundle.key), replayed),
    )?;
    let result = ItemResult {
        key: bundle.key.clone(),
        local_seq: item.local_seq,
        status: ItemStatus::Applied,
        code: None,
        adjustments: fx.adjustments,
        host_seq: changelog::max_seq(tx)?,
    };
    tx.execute(
        "INSERT INTO lan_inbox (idempotency_key, terminal_id, local_seq, kind, client_created_at, received_at,
                                skew_ms, result, host_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            bundle.key,
            sender.terminal_id,
            item.local_seq,
            bundle.kind,
            bundle.created_at,
            now,
            received.skew_ms,
            serde_json::to_string(&result)?,
            result.host_seq
        ],
    )?;
    // The till's row says the clock offset this item came with, like the notice above.
    tx.execute(
        "UPDATE lan_devices SET last_replayed_seq = MAX(COALESCE(last_replayed_seq, 0), ?2), last_seen_at = ?3,
                skew_ms = COALESCE(?4, skew_ms)
         WHERE terminal_id = ?1",
        params![sender.terminal_id, item.local_seq, now, received.skew_ms],
    )?;
    Ok(result)
}

/// Items in order, one transaction each; stops after the first refused one ([`refusal`]), which
/// also raises a `replay_rejected` notice in its own transaction (`details.cause` names the
/// error when the code says less). Another error on the first item (out of order, an unknown
/// till, the main PC busy) is returned as the error, for the till to try again.
pub fn apply_batch(
    db: &Db,
    sender: &Sender,
    items: Vec<WireItem>,
    received: &Received,
) -> Result<Vec<ItemResult>, AppError> {
    let mut results = Vec::with_capacity(items.len());
    for item in items {
        let (s, r, it) = (sender.clone(), *received, item.clone());
        let error = match db.write(move |tx| apply(tx, &s, &it, &r)) {
            Ok(result) => {
                results.push(result);
                continue;
            }
            Err(error) => error,
        };
        let Some(code) = refusal(&error) else {
            if results.is_empty() {
                return Err(error);
            }
            tracing::warn!(code = %error.code(), "replay stopped after {} items", results.len());
            break;
        };
        let mut details = json!({ "code": code });
        if error.code() != code {
            details["cause"] = json!(error.code());
        }
        if let AppError::Rejected { details: extra, .. } = &error
            && let Some(number) = extra.get("number").filter(|n| !n.is_null())
        {
            details["documentNumber"] = number.clone();
        }
        let (till, key) = (sender.clone(), item.bundle.key.clone());
        let (at, local_seq) = (received.at, item.local_seq);
        // The refusal is journaled with its notice: `lan_notices` is neither chained nor
        // exported, and a retry or a promotion later forgets the reason on the till.
        db.write(move |tx| {
            notices::raise(
                tx,
                NoticeKind::ReplayRejected,
                Some(&till.terminal_id),
                &key,
                details.clone(),
                at,
            )?;
            audit::append(
                tx,
                &Actor::system(Some(till.terminal_id.clone())).event(
                    "lan.replay_rejected",
                    "lan_item",
                    Some(&key),
                    json!({
                        "terminal": till.code,
                        "localSeq": local_seq,
                        "code": details["code"],
                        "cause": details.get("cause"),
                        "number": details.get("documentNumber"),
                    }),
                ),
            )?;
            Ok(())
        })?;
        results.push(ItemResult {
            key: item.bundle.key.clone(),
            local_seq: item.local_seq,
            status: ItemStatus::Rejected,
            code: Some(code),
            adjustments: Vec::new(),
            host_seq: 0,
        });
        break;
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::{self, DocType};
    use crate::money::{Money, Qty};
    use crate::payments::{PaymentInput, PaymentMethod};
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::sync::outbox::{self, ItemStatus};
    use crate::sync::testkit::{LanPair, lan_pair};
    use crate::testkit::NOON;
    use serde_json::json;

    fn milk(lan: &LanPair, stock: i64) -> String {
        let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
        if stock > 0 {
            lan.host.stock_in(&milk.id, stock);
        }
        lan.pull();
        milk.id
    }

    #[test]
    fn a_till_sale_is_recorded_on_the_main_pc_as_it_was_made() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        let sale = lan.sell_on_till(&milk, 1_000, 10_500);
        let results = lan.push();
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|r| r.status == ItemStatus::Applied && r.adjustments.is_empty())
        );
        let id = sale.document.id.clone();
        let on_host = lan.host.db.read(move |c| documents::get(c, &id)).unwrap();
        assert_eq!(on_host.number.as_deref(), Some("TK2026-C2-000001"));
        assert_eq!(
            on_host.hash, sale.document.hash,
            "the till's hash, recomputed identical"
        );
        assert_eq!(on_host.lines.len(), sale.document.lines.len());
        assert_eq!(
            lan.count_host("SELECT qty FROM stock_levels"),
            9_000,
            "the host's triggers moved its stock"
        );
        let drawer = "SELECT COALESCE(SUM(amount), 0) FROM cash_movements";
        assert_eq!(
            (lan.count_host(drawer), lan.count_till(drawer)),
            (10_000, 10_000),
            "the till's drawer (105.00 tendered, 5.00 given back), on the host"
        );
        let journal = lan
            .host
            .db
            .read({
                let t = lan.till_terminal.id.clone();
                move |c| crate::sync::journal::verify(c, &t)
            })
            .unwrap();
        assert_eq!(
            (journal.rows, journal.broken),
            (2, 0),
            "session opened, sale created"
        );
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM audit_log WHERE action = 'lan.replayed'"),
            2
        );
        assert!(
            lan.till
                .read(|c| outbox::pending(c, 10))
                .unwrap()
                .is_empty(),
            "sent"
        );
        let chains = lan.host.db.read(documents::verify_all).unwrap();
        assert!(
            chains.iter().all(|c| c.first_broken_seq.is_none()),
            "{chains:?}"
        );
    }

    #[test]
    fn the_host_chain_equals_the_till_chain() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        for _ in 0..3 {
            lan.sell_on_till(&milk, 1_000, 10_500);
        }
        lan.push();
        let chain = |db: &crate::db::Db| {
            db.read(|c| documents::verify_chain(c, DocType::Ticket, "C2"))
                .unwrap()
        };
        let (on_till, on_host) = (chain(&lan.till), chain(&lan.host.db));
        assert_eq!((on_host.rows, on_host.first_broken_seq), (3, None));
        assert_eq!(
            (on_host.last_seq, &on_host.last_hash),
            (on_till.last_seq, &on_till.last_hash)
        );
    }

    #[test]
    fn a_bundle_replayed_twice_is_applied_once() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        lan.sell_on_till(&milk, 1_000, 10_500);
        let items = lan.till.read(|c| outbox::pending(c, 10)).unwrap();
        let first = apply_batch(
            &lan.host.db,
            &lan.sender(),
            items.clone(),
            &Received {
                at: NOON,
                skew_ms: None,
            },
        )
        .unwrap();
        let again = apply_batch(
            &lan.host.db,
            &lan.sender(),
            items,
            &Received {
                at: NOON + 5,
                skew_ms: None,
            },
        )
        .unwrap();
        assert!(again.iter().all(|r| r.status == ItemStatus::Duplicate));
        assert_eq!(again[1].host_seq, first[1].host_seq, "the stored answer");
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM documents WHERE series = 'C2'"),
            1
        );
        assert_eq!(
            lan.count_host("SELECT qty FROM stock_levels"),
            9_000,
            "stock taken once"
        );
        assert_eq!(lan.count_host("SELECT COUNT(*) FROM lan_journal"), 2);
    }

    #[test]
    fn a_lost_answer_is_given_again_from_the_inbox() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        lan.sell_on_till(&milk, 1_000, 10_500);
        let items = lan.till.read(|c| outbox::pending(c, 10)).unwrap();
        apply_batch(
            &lan.host.db,
            &lan.sender(),
            items,
            &Received {
                at: NOON,
                skew_ms: None,
            },
        )
        .unwrap();
        // The till lost power before recording the answers: it sends again.
        let results = lan.push();
        assert!(results.iter().all(|r| r.status == ItemStatus::Duplicate));
        assert!(
            lan.till
                .read(|c| outbox::pending(c, 10))
                .unwrap()
                .is_empty()
        );
        assert_eq!(lan.count_host("SELECT COUNT(*) FROM lan_inbox"), 2);
    }

    #[test]
    fn an_item_out_of_order_is_refused_with_the_expected_seq() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        lan.sell_on_till(&milk, 1_000, 10_500);
        lan.sell_on_till(&milk, 1_000, 10_500);
        let items = lan.till.read(|c| outbox::pending(c, 10)).unwrap();
        apply_batch(
            &lan.host.db,
            &lan.sender(),
            vec![items[0].clone()],
            &Received {
                at: NOON,
                skew_ms: None,
            },
        )
        .unwrap();
        let err = apply_batch(
            &lan.host.db,
            &lan.sender(),
            vec![items[2].clone()],
            &Received {
                at: NOON,
                skew_ms: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "replay_out_of_order");
        assert_eq!(
            err.payload().details.unwrap()["expected"],
            items[1].local_seq
        );
    }

    #[test]
    fn a_new_host_skips_rows_it_already_has() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        lan.sell_on_till(&milk, 1_000, 10_500);
        lan.push();
        let mut resent = lan.till.read(|c| outbox::kept_since(c, 0)).unwrap();
        // A main PC that has these rows but never saw these keys (a promoted till, a re-pairing).
        lan.host
            .db
            .write(|tx| Ok(tx.execute("UPDATE lan_devices SET last_replayed_seq = NULL", [])?))
            .unwrap();
        for item in &mut resent {
            item.bundle.key = format!("tx:{}", crate::ids::new_id());
        }
        let results = apply_batch(
            &lan.host.db,
            &lan.sender(),
            resent,
            &Received {
                at: NOON,
                skew_ms: None,
            },
        )
        .unwrap();
        assert!(
            results.iter().all(|r| r.status == ItemStatus::Applied),
            "{results:?}"
        );
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM documents WHERE series = 'C2'"),
            1
        );
        assert_eq!(lan.count_host("SELECT qty FROM stock_levels"), 9_000);
    }

    #[test]
    fn another_tills_rows_are_refused_and_the_line_stops() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        lan.sell_on_till(&milk, 1_000, 10_500);
        let mut items = lan.till.read(|c| outbox::pending(c, 10)).unwrap();
        let c1 = lan.host.terminal.id.clone();
        for row in &mut items[1].bundle.rows {
            if row.table == "stock_movements" {
                row.image.insert("terminal_id".into(), json!(c1));
            }
        }
        let results = apply_batch(
            &lan.host.db,
            &lan.sender(),
            items,
            &Received {
                at: NOON,
                skew_ms: None,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[1].status, ItemStatus::Rejected);
        assert_eq!(results[1].code.as_deref(), Some("replay_forbidden_row"));
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM documents WHERE series = 'C2'"),
            0,
            "rolled back whole"
        );
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM lan_notices WHERE kind = 'replay_rejected' AND acknowledged_at IS NULL"),
            1
        );
    }

    #[test]
    fn an_offline_sale_that_took_the_last_piece_twice_is_recorded_with_a_notice() {
        let lan = lan_pair();
        lan.host.set("stock.negative_policy", json!("block"));
        let milk = milk(&lan, 1_000);
        lan.open_till_session(0);
        lan.sell_on_till(&milk, 1_000, 10_500); // offline: the till still sees one piece
        lan.host.open_till(0);
        let ctx = lan.host.till();
        let input = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(&milk, Qty(1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(10_500))],
        };
        lan.host
            .db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap();
        let results = lan.push();
        assert!(
            results.iter().all(|r| r.status == ItemStatus::Applied),
            "the sale happened: it is recorded"
        );
        assert_eq!(lan.count_host("SELECT qty FROM stock_levels"), -1_000);
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM lan_notices WHERE kind = 'oversold'"),
            1
        );
        let payload: String = lan
            .host
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'lan.replayed' ORDER BY seq DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["oversold"][0], milk.as_str());
    }

    #[test]
    fn a_kridi_entry_takes_the_host_balance_and_the_till_is_corrected() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        let karim = lan.host.customer("Karim");
        lan.pull();
        lan.open_till_session(0);
        let on_credit = |db: &crate::db::Db, ctx: crate::till::TillContext| {
            let mut cart = Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(&milk, Qty(1_000))],
            );
            cart.customer_id = Some(karim.id.clone());
            let input = SaleInput {
                cart,
                payments: vec![PaymentInput {
                    method: PaymentMethod::Credit,
                    ..PaymentInput::cash(Money(10_000))
                }],
            };
            db.write(move |tx| create_sale(tx, &ctx, input)).unwrap()
        };
        let owner_ctx =
            crate::till::TillContext::new(lan.till_owner.clone(), lan.host.pack.clone(), NOON);
        on_credit(&lan.till, owner_ctx); // the till thinks: 0 → 100.00
        lan.host.open_till(0);
        on_credit(&lan.host.db, lan.host.owner_till()); // meanwhile on the main PC: 0 → 100.00
        let results = lan.push();
        let adjusted: Vec<&outbox::Adjustment> =
            results.iter().flat_map(|r| &r.adjustments).collect();
        assert_eq!(adjusted.len(), 1);
        assert_eq!(adjusted[0].table, "customer_ledger");
        assert_eq!(adjusted[0].fields["balance_after"], 20_000);
        let id = karim.id.clone();
        let host_balance: i64 = lan
            .host
            .db
            .read(move |c| {
                Ok(
                    c.query_row("SELECT balance FROM customers WHERE id = ?1", [&id], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(host_balance, 20_000);
        lan.pull();
        let id = karim.id.clone();
        let till_balance: i64 = lan
            .till
            .read(move |c| {
                Ok(
                    c.query_row("SELECT balance FROM customers WHERE id = ?1", [&id], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(
            till_balance, 20_000,
            "the till's cache follows the host after the next page"
        );
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM audit_log WHERE action = 'lan.reconciled'"),
            1
        );
    }

    #[test]
    fn the_host_records_its_reception_time_and_the_skew_but_keeps_the_ticket_time() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        let sale = lan.sell_on_till(&milk, 1_000, 10_500);
        let two_hours = 7_200_000;
        lan.push_with(Received {
            at: NOON + two_hours,
            skew_ms: Some(-two_hours),
        });
        let id = sale.document.id.clone();
        let on_host = lan.host.db.read(move |c| documents::get(c, &id)).unwrap();
        assert_eq!(
            on_host.validated_at, sale.document.validated_at,
            "the printed time stays"
        );
        assert_eq!(on_host.business_date, sale.document.business_date);
        let (received, skew): (i64, i64) = lan
            .host
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT received_at, skew_ms FROM lan_inbox WHERE idempotency_key LIKE 'sale:%'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!((received, skew), (NOON + two_hours, -two_hours));
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM lan_notices WHERE kind = 'clock_skew'"),
            1
        );
    }

    #[test]
    fn a_chain_that_differs_on_the_host_is_rebased_and_the_till_corrected() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        lan.sell_on_till(&milk, 1_000, 10_500);
        lan.push();
        // The main PC's copy of ticket 1 was restored from an older state: its hash differs.
        lan.host
            .db
            .write_replica(|tx| {
                Ok(tx.execute(
                    "UPDATE documents SET hash = ?1 WHERE series = 'C2'",
                    [&"c".repeat(64)],
                )?)
            })
            .unwrap();
        let second = lan.sell_on_till(&milk, 1_000, 10_500);
        let results = lan.push();
        let chain: Vec<&outbox::Adjustment> = results
            .iter()
            .flat_map(|r| &r.adjustments)
            .filter(|a| a.table == "documents")
            .collect();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].fields["prev_hash"], "c".repeat(64));
        let id = second.document.id.clone();
        let (till_hash, host_hash) = (
            lan.till
                .read({
                    let id = id.clone();
                    move |c| {
                        Ok(
                            c.query_row("SELECT hash FROM documents WHERE id = ?1", [&id], |r| {
                                r.get::<_, String>(0)
                            })?,
                        )
                    }
                })
                .unwrap(),
            lan.host
                .db
                .read(move |c| {
                    Ok(
                        c.query_row("SELECT hash FROM documents WHERE id = ?1", [&id], |r| {
                            r.get::<_, String>(0)
                        })?,
                    )
                })
                .unwrap(),
        );
        assert_eq!(till_hash, host_hash, "the till took the host's link");
        assert_eq!(
            lan.count_host("SELECT COUNT(*) FROM lan_notices WHERE kind = 'chain_rebased'"),
            1
        );
    }

    /// A till restored from an older backup moved its ticket counter past the numbers the
    /// replaced copy had printed, and its `backup.restored` row lists that skip. On the main PC
    /// that row lives in `lan_journal`, where numbering verification finds it.
    #[test]
    fn a_skip_journaled_by_a_restored_till_keeps_its_series_whole_on_the_host() {
        let lan = lan_pair();
        let milk = milk(&lan, 10_000);
        lan.open_till_session(0);
        let restore = crate::ids::new_id();
        lan.till
            .write(move |tx| {
                let skip =
                    crate::numbering::fast_forward(tx, "ticket", "C2", 2026, 3, &restore, NOON)?
                        .unwrap();
                crate::audit::append(
                    tx,
                    &crate::audit::Actor::system(None).event(
                        "backup.restored",
                        "backup",
                        Some(&restore),
                        json!({ "skipped": [skip] }),
                    ),
                )
                .map(drop)
            })
            .unwrap();
        let sale = lan.sell_on_till(&milk, 1_000, 10_500);
        assert_eq!(sale.document.number.as_deref(), Some("TK2026-C2-000004"));
        let results = lan.push();
        assert!(
            results.iter().all(|r| r.status == ItemStatus::Applied),
            "{results:?}"
        );
        let chain = |db: &crate::db::Db| {
            db.read(|c| documents::verify_chain(c, DocType::Ticket, "C2"))
                .unwrap()
        };
        assert_eq!(
            chain(&lan.till).first_broken_seq,
            None,
            "the till's own journal lists the skip"
        );
        let on_host = chain(&lan.host.db);
        assert_eq!(
            (on_host.rows, on_host.first_broken_seq),
            (1, None),
            "the till's journal on the host lists it too"
        );
    }
}
