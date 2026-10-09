//! Append-only, hash-chained audit journal (spec §5.3). Every write command appends one row
//! inside its own transaction.

use crate::error::AppError;
use crate::ids::new_id;
use crate::time::now_ms;
use rusqlite::{Connection, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use specta::Type;

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Every action code written in M0 and M1. The UI translates each one
/// (`settings:audit.actions.*`).
pub const KNOWN_ACTIONS: &[&str] = &[
    "setup.completed",
    "auth.login",
    "auth.login_failed",
    "auth.logout",
    "auth.override",
    "auth.override_failed",
    "company.updated",
    "company.logo_changed",
    "settings.updated",
    "user.created",
    "user.updated",
    "user.pin_changed",
    "user.deactivated",
    "user.language_changed",
    "role.created",
    "role.updated",
    "role.archived",
    "backup.created",
    "backup.restore_scheduled",
    "backup.restored",
    "db.unclean_shutdown",
    "db.integrity_checked",
    // M1 catalogue
    "unit.created",
    "unit.updated",
    "tax_rate.synced",
    "tax_rate.updated",
    "price_level.created",
    "price_level.updated",
    "price_level.archived",
    "category.created",
    "category.updated",
    "category.archived",
    // M1 products
    "product.created",
    "product.updated",
    "product.price_changed",
    "product.archived",
    "product.restored",
    // M1 stock
    "stock_location.created",
    "stock.adjusted",
    "stock.levels_rebuilt",
    "stock.count_started",
    "stock.count_closed",
    "stock.count_cancelled",
    "stock.transferred",
    "stock.minimums_set",
    // M1 documents
    "document.cancelled",
    "document.reprinted",
    // M1 cash sessions
    "cash.session_opened",
    "cash.paid_in",
    "cash.paid_out",
    "cash.dropped",
    "cash.expense",
    "cash.close_refused",
    // M1 sales
    "sale.created",
    // M1 held tickets and cart events
    "sale.held",
    "sale.recalled",
    "sale.held_discarded",
    "sale.line_voided",
    "sale.cancelled",
    "sale.qty_reduced",
    // M1 returns
    "return.created",
    // M1 cash closing
    "cash.session_closed",
    // M1b hardware settings and the no-sale drawer
    "hardware.updated",
    "drawer.opened",
    // M2 ledgers
    "ledger.balances_rebuilt",
    // M2 customers and kridi
    "customer.created",
    "customer.updated",
    "customer.blocked",
    "customer.unblocked",
    "customer.archived",
    "customer.restored",
    "credit.opening_set",
    "credit.adjusted",
    "credit.collected",
    "credit.receipt_reprinted",
    // M2 product extras
    "product.extras_updated",
    "product.price_breaks_changed",
    "product.variants_generated",
    "product.prices_bulk_updated",
    "product.bulk_edited",
    "product.image_changed",
    // M2 promotions
    "promotion.created",
    "promotion.updated",
    "promotion.archived",
    // M2 lots and serials
    "serial.sold",
    "serial.returned",
    "serial.registered",
    // M2 suppliers
    "supplier.created",
    "supplier.updated",
    "supplier.archived",
    "supplier.restored",
    "supplier.opening_set",
    "supplier.adjusted",
    "supplier.paid",
    // M2 purchasing
    "purchase_order.saved",
    "purchase_order.validated",
    "goods_receipt.validated",
    "supplier_invoice.validated",
    "supplier_return.validated",
    // M2 expenses
    "expense.recorded",
    "expense.reversed",
    "expense_category.created",
    "expense_category.updated",
    "expense_category.archived",
    // M2 imports, data export and backup files
    "import.applied",
    "import.undone",
    "data.exported",
    "backup.exported",
    "backup.imported",
    // M3 documents
    "document.printed",
    "document.exported",
    "document.draft_saved",
    "quote.validated",
    "proforma.validated",
    "delivery_note.validated",
    "invoice.validated",
    "document.converted",
    "invoice.from_ticket",
    "credit_note.created",
    // M3 fiscal archive
    "fiscal.archive_exported",
    // M4 labels and scales
    "labels.printed",
    "labels.dismissed",
    "labels.barcodes_assigned",
    "scale.item_set",
    // M7 DZBuild link
    "cloud.linked",
    // loi 18-07: a run of the buyers' details purge, with its count (PDP-003)
    "cloud.personal_data_purged",
    "cloud.unlinked",
    "cloud.relink_required",
    "cloud.revoked",
    "cloud.order_claimed",
    "cloud.order_released",
    "cloud.order_prepared",
    "cloud.order_fulfilled",
    "cloud.order_refused",
    "cloud.pulled",
    "cloud.publish_changed",
    // the online backup of 0.7–0.9, removed in 1.0.0 (free edition): never recorded any more,
    // kept so the journals of older versions still read with their labels
    "cloud.backup_key_created",
    "cloud.backup_uploaded",
    "cloud.backup_downloaded",
    // M5 LAN
    "lan.reconciled",
    "lan.notice_acknowledged",
    "lan.replayed",
    "lan.pairing_code_issued",
    "lan.till_paired",
    "lan.till_revoked",
    "lan.paired",
    "lan.host_enabled",
    "lan.host_disabled",
    "lan.promoted",
    "lan.left",
    "lan.clock_skew",
    // a refused till item, on the main PC and on the till; the owner's retry of it
    "lan.replay_rejected",
    "lan.item_rejected",
    "lan.replay_retried",
    // a till's journal chain started on a main PC that held nothing of it (LAN-010)
    "lan.journal_adopted",
    // M6 backups: the second copy, per-PC settings
    "backup.mirrored",
    "backup.mirror_failed",
    "local.updated",
    // M6 updates
    "app.updated",
    // M6 start-up health and the diagnostic zip
    "clock.moved_back",
    "diagnostics.exported",
    // M6 the updater
    "update.downloaded",
    "update.install_started",
    // M6 the demo shop
    "demo.entered",
    "demo.left",
    "demo.erased",
    // M6 review: a till's damaged copy set aside (journaled once it is a till again, or set up
    // as a new shop), and a new shop created although backups were there
    "lan.till_rejoined",
    "recovery.set_aside",
    "recovery.fresh_start",
    // M8 packs
    "packs.template_applied",
    "packs.modules_set",
    "packs.till_mode_set",
    // M8 superette and kiosque
    "product.regulation_set",
    "department_key.created",
    "department_key.updated",
    "department_key.archived",
    // M8 restaurant: floor, open tickets, modifiers and the daily menu
    "dining.room_saved",
    "dining.room_archived",
    "dining.table_saved",
    "dining.table_archived",
    "dining.ticket_opened",
    "dining.line_added",
    "dining.line_changed",
    "dining.line_removed",
    "dining.ticket_updated",
    "dining.ticket_transferred",
    "dining.tickets_merged",
    "dining.menu_set",
    "modifier_group.saved",
    "modifier_group.archived",
    "product.modifiers_set",
    // M8 restaurant: kitchen tickets, voids and comps, the addition, splits and the payment
    "dining.kitchen_sent",
    "dining.kitchen_printed",
    "dining.line_voided",
    "dining.line_comped",
    "dining.ticket_voided",
    "dining.bill_printed",
    "dining.ticket_split",
    "dining.ticket_paid",
    "kitchen_station.saved",
    "kitchen_station.archived",
    // M8 pharmacy
    "pharmacy.product_set",
    "pharmacy.register_recorded",
    "pharmacy.chifa_applied",
    // M8 final review: the main PC books a till's registers and ledgers
    "registers.host_booked",
    // M8 butcher
    "butcher.yield_template_saved",
    "butcher.yield_template_archived",
    "butcher.yield_run",
    // M8 flexy top-ups
    "flexy.float_purchased",
    "flexy.operators_seeded",
    // M8 deposits and service tickets
    "deposit.taken",
    "deposit.refunded",
    "service.intake",
    "service.status_set",
    "service.assigned",
    "service.items_set",
    "service.delivered",
    // M8 instalment sales
    "instalments.signed",
    "instalments.guarantee_added",
    "instalments.reminder_noted",
    "instalments.level_seeded",
    // M8 wholesale and delivery rounds
    "wholesale.price_set",
    "wholesale.min_order_set",
    "wholesale.pack_created",
    "rounds.created",
    "rounds.stops_set",
    "rounds.started",
    "rounds.stop_reported",
    "rounds.closed",
    "rounds.van_created",
    // M8 customer orders, layaway and appointments
    "orders.created",
    "orders.ready",
    "orders.fulfilled",
    "orders.delivered",
    "orders.closed",
    "appointments.booked",
    "appointments.updated",
    "appointments.status_set",
    // M8 loyalty points, gift, exchange and loyalty vouchers
    "loyalty.enrolled",
    "loyalty.card_blocked",
    "loyalty.redeemed",
    "loyalty.adjusted",
    "loyalty.expired",
    "vouchers.sold",
    "vouchers.expired",
    // M8 jewellery
    "jewellery.piece_set",
    "jewellery.metal_price_set",
    "jewellery.repriced",
    // M8 employees lite and zakat
    "staff.profile_set",
    "staff.advance_given",
    "staff.advance_repaid",
    "staff.salary_deducted",
    "zakat.saved",
    // M8 MVP completion: custom payment types, role and user templates
    "payment_type.created",
    "payment_type.updated",
    "payment_type.archived",
    "role.duplicated",
    "user.duplicated",
    // M8 Task 17: signed regulatory packs, personal data, till keys
    "datapack.installed",
    "privacy.consent_set",
    "privacy.accessed",
    "privacy.exported",
    "privacy.anonymised",
    "till.key_map_set",
    // M8 Task 20 review: a till records in its drawer the money a host service booked for it
    "cash.host_share",
    // M9 licensing (pos-license's engine; entity type `license`)
    "license.activated",
    "license.expired",
    "license.removed",
    "license.tampered",
    "license.repaired",
];

/// Who does something, from which till.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Actor {
    pub user_id: Option<String>,
    pub terminal_id: Option<String>,
}

impl Actor {
    pub fn system(terminal_id: Option<String>) -> Actor {
        Actor {
            user_id: None,
            terminal_id,
        }
    }

    pub fn event(
        &self,
        action: &str,
        entity_type: &str,
        entity_id: Option<&str>,
        payload: Value,
    ) -> AuditEvent {
        AuditEvent {
            user_id: self.user_id.clone(),
            terminal_id: self.terminal_id.clone(),
            action: action.to_owned(),
            entity_type: entity_type.to_owned(),
            entity_id: entity_id.map(str::to_owned),
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub user_id: Option<String>,
    pub terminal_id: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditRow {
    pub seq: i64,
    pub id: String,
    pub at: i64,
    pub user_id: Option<String>,
    pub terminal_id: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    #[specta(type = crate::json::JsonValue)]
    pub payload: Value,
    pub prev_hash: String,
    pub hash: String,
    /// Display name of `user_id`, filled by `list` only.
    pub user_name: Option<String>,
}

/// What [`verify_chain`] found. `last_seq` and `last_hash` name the last row that verified;
/// recorded outside the database (Z-report, backup, cloud), they anchor the journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChainReport {
    /// Rows present in the journal.
    pub rows: i64,
    /// `None` when the chain is intact. `Some(n)` means the chain breaks at or before `n`: the
    /// edited row for a plain edit; the next row for a deleted row or for an edit whose own
    /// hash was recomputed; `last_seq + 1` when rows were deleted at the end (the
    /// `sqlite_sequence` high-water mark is above the last row).
    pub first_broken_seq: Option<i64>,
    /// `seq` of the last row that verified, 0 when none did.
    pub last_seq: i64,
    /// `hash` of that row, [`GENESIS_HASH`] when none did.
    pub last_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    pub before_seq: Option<i64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditPage {
    pub rows: Vec<AuditRow>,
    pub next_before_seq: Option<i64>,
}

/// The hashed fields, in spec order.
#[derive(Debug, Clone, Copy)]
pub struct ChainFields<'a> {
    pub prev_hash: &'a str,
    pub seq: i64,
    pub at: i64,
    pub user_id: Option<&'a str>,
    pub terminal_id: Option<&'a str>,
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<&'a str>,
    pub payload: &'a str,
}

impl ChainFields<'_> {
    /// SHA-256 over the fields joined by U+001F (NULL = empty), lower-case hex.
    pub fn hash(&self) -> String {
        let seq = self.seq.to_string();
        let at = self.at.to_string();
        let parts = [
            self.prev_hash,
            seq.as_str(),
            at.as_str(),
            self.user_id.unwrap_or(""),
            self.terminal_id.unwrap_or(""),
            self.action,
            self.entity_type,
            self.entity_id.unwrap_or(""),
            self.payload,
        ];
        let mut hasher = Sha256::new();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                hasher.update([0x1f_u8]);
            }
            hasher.update(part.as_bytes());
        }
        hex::encode(hasher.finalize().as_slice())
    }
}

/// JSON with object keys sorted at every level and no whitespace.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let body: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("{}:{}", Value::String(k.clone()), canonical_json(v)))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(canonical_json)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        other => other.to_string(),
    }
}

/// Appends one row inside the caller's transaction and links it to the previous row.
/// A control character (U+0000–U+001F) in a text field is `value_invalid`; an action missing
/// from [`KNOWN_ACTIONS`] fails a debug assertion.
pub fn append(tx: &Transaction<'_>, event: &AuditEvent) -> Result<AuditRow, AppError> {
    append_at(tx, event, now_ms())
}

pub(crate) fn append_at(
    tx: &Transaction<'_>,
    event: &AuditEvent,
    at: i64,
) -> Result<AuditRow, AppError> {
    check_text_fields(event)?;
    debug_assert!(
        KNOWN_ACTIONS.contains(&event.action.as_str()),
        "unknown audit action {:?}",
        event.action
    );
    // The next seq follows the AUTOINCREMENT high-water mark, not just the last row: after
    // rows are deleted at the end, the gap stays visible to `verify_chain` instead of being
    // filled again by new rows.
    let (seq, prev_hash): (i64, String) = tx.query_row(
        "SELECT MAX(COALESCE(MAX(seq), 0),
                    COALESCE((SELECT seq FROM sqlite_sequence WHERE name = 'audit_log'), 0)) + 1,
                COALESCE((SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1), ?1)
         FROM audit_log",
        [GENESIS_HASH],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let payload = canonical_json(&event.payload);
    let hash = ChainFields {
        prev_hash: &prev_hash,
        seq,
        at,
        user_id: event.user_id.as_deref(),
        terminal_id: event.terminal_id.as_deref(),
        action: &event.action,
        entity_type: &event.entity_type,
        entity_id: event.entity_id.as_deref(),
        payload: &payload,
    }
    .hash();
    let id = new_id();
    tx.execute(
        "INSERT INTO audit_log (seq, id, at, user_id, terminal_id, action, entity_type, entity_id,
                                payload, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            seq,
            id,
            at,
            event.user_id,
            event.terminal_id,
            event.action,
            event.entity_type,
            event.entity_id,
            payload,
            prev_hash,
            hash
        ],
    )?;
    Ok(AuditRow {
        seq,
        id,
        at,
        user_id: event.user_id.clone(),
        terminal_id: event.terminal_id.clone(),
        action: event.action.clone(),
        entity_type: event.entity_type.clone(),
        entity_id: event.entity_id.clone(),
        payload: event.payload.clone(),
        prev_hash,
        hash,
        user_name: None,
    })
}

/// `append_at` for the shell's tests (a row dated by the test, e.g. a clock set back).
#[doc(hidden)]
pub fn append_for_tests(
    tx: &Transaction<'_>,
    event: &AuditEvent,
    at: i64,
) -> Result<AuditRow, AppError> {
    append_at(tx, event, at)
}

/// The hash joins fields with U+001F, so a control character inside one could move a field
/// boundary and let two different rows hash alike.
fn check_text_fields(event: &AuditEvent) -> Result<(), AppError> {
    let fields = [
        ("userId", event.user_id.as_deref()),
        ("terminalId", event.terminal_id.as_deref()),
        ("action", Some(event.action.as_str())),
        ("entityType", Some(event.entity_type.as_str())),
        ("entityId", event.entity_id.as_deref()),
    ];
    for (field, value) in fields {
        if value.is_some_and(|v| v.chars().any(|c| c <= '\u{1f}')) {
            return Err(AppError::validation(field, "value_invalid"));
        }
    }
    Ok(())
}

/// Recomputes every hash in order and reports the first row that does not match, and rows
/// deleted at the end (see [`ChainReport::first_broken_seq`]).
pub fn verify_chain(conn: &Connection) -> Result<ChainReport, AppError> {
    verify_chain_from(conn, 0, GENESIS_HASH)
}

/// [`verify_chain`] from an anchor recorded earlier (a Z report): only the rows after
/// `from_seq` are read, and the first of them must link to `from_hash`. `rows` counts the rows
/// checked. A Z close verifies just the day's rows this way.
pub fn verify_chain_from(
    conn: &Connection,
    from_seq: i64,
    from_hash: &str,
) -> Result<ChainReport, AppError> {
    // Read before the rows: the mark only grows, so rows appended in between cannot make an
    // intact journal look truncated.
    let high_water: i64 = conn.query_row(
        "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name = 'audit_log'), 0)",
        [],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT seq, at, user_id, terminal_id, action, entity_type, entity_id, payload, prev_hash, hash
         FROM audit_log WHERE seq > ?1 ORDER BY seq",
    )?;
    let mut rows = stmt.query([from_seq])?;
    let mut last_seq = from_seq;
    let mut last_hash = from_hash.to_owned();
    let mut count = 0_i64;
    let mut broken = None;
    while let Some(row) = rows.next()? {
        count += 1;
        if broken.is_some() {
            continue;
        }
        let seq: i64 = row.get(0)?;
        let user_id: Option<String> = row.get(2)?;
        let terminal_id: Option<String> = row.get(3)?;
        let action: String = row.get(4)?;
        let entity_type: String = row.get(5)?;
        let entity_id: Option<String> = row.get(6)?;
        let payload: String = row.get(7)?;
        let prev_hash: String = row.get(8)?;
        let hash: String = row.get(9)?;
        let recomputed = ChainFields {
            prev_hash: &prev_hash,
            seq,
            at: row.get(1)?,
            user_id: user_id.as_deref(),
            terminal_id: terminal_id.as_deref(),
            action: &action,
            entity_type: &entity_type,
            entity_id: entity_id.as_deref(),
            payload: &payload,
        }
        .hash();
        if seq != last_seq + 1 || prev_hash != last_hash || recomputed != hash {
            broken = Some(seq);
            continue;
        }
        last_seq = seq;
        last_hash = hash;
    }
    if broken.is_none() && high_water > last_seq {
        broken = Some(last_seq + 1);
    }
    Ok(ChainReport {
        rows: count,
        first_broken_seq: broken,
        last_seq,
        last_hash,
    })
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<AuditRow> {
    let seq: i64 = row.get(0)?;
    let payload_text: String = row.get(8)?;
    // One unreadable payload must not hide the whole page: show it as text.
    let payload = match serde_json::from_str(&payload_text) {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(seq, error = %e, "audit payload is not JSON; listed as text");
            Value::String(payload_text)
        }
    };
    Ok(AuditRow {
        seq,
        id: row.get(1)?,
        at: row.get(2)?,
        user_id: row.get(3)?,
        terminal_id: row.get(4)?,
        action: row.get(5)?,
        entity_type: row.get(6)?,
        entity_id: row.get(7)?,
        payload,
        prev_hash: row.get(9)?,
        hash: row.get(10)?,
        user_name: row.get(11)?,
    })
}

/// Seeks on the `seq` primary key: the `COALESCE` bound is usable by the index, where
/// `?1 IS NULL OR a.seq < ?1` would scan the whole journal.
const LIST_SQL: &str =
    "SELECT a.seq, a.id, a.at, a.user_id, a.terminal_id, a.action, a.entity_type,
        a.entity_id, a.payload, a.prev_hash, a.hash, u.display_name
 FROM audit_log a LEFT JOIN users u ON u.id = a.user_id
 WHERE a.seq < COALESCE(?1, 9223372036854775807)
 ORDER BY a.seq DESC LIMIT ?2";

/// Newest first, keyset-paginated on `seq`.
pub fn list(conn: &Connection, query: &AuditQuery) -> Result<AuditPage, AppError> {
    let limit = i64::from(query.limit.unwrap_or(50).clamp(1, 200));
    let mut stmt = conn.prepare(LIST_SQL)?;
    let mut rows = stmt
        .query_map(params![query.before_seq, limit + 1], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    let next_before_seq = if has_more {
        rows.last().map(|r| r.seq)
    } else {
        None
    };
    Ok(AuditPage {
        rows,
        next_before_seq,
    })
}

/// One record's history (AUD-002: who, when, before → after) or one family of actions
/// (`sale.`, `cash.`, `drawer.`…), newest first, keyset-paginated on `seq`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditFilter {
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    /// `a-z`, `_` and `.` only, e.g. `sale.` (compared with `substr`, no wildcards).
    pub action_prefix: Option<String>,
    pub before_seq: Option<i64>,
    /// 1..=200, default 50.
    pub limit: Option<u32>,
}

const FILTER_SQL: &str =
    "SELECT a.seq, a.id, a.at, a.user_id, a.terminal_id, a.action, a.entity_type,
        a.entity_id, a.payload, a.prev_hash, a.hash, u.display_name
 FROM audit_log a LEFT JOIN users u ON u.id = a.user_id
 WHERE a.seq < COALESCE(?1, 9223372036854775807)
   AND (?3 IS NULL OR a.entity_type = ?3)
   AND (?4 IS NULL OR a.entity_id = ?4)
   AND (?5 IS NULL OR substr(a.action, 1, length(?5)) = ?5)
 ORDER BY a.seq DESC LIMIT ?2";

/// [`list`] narrowed by [`AuditFilter`]; an action prefix outside `a-z`, `_`, `.` (or empty, or
/// longer than 40) is refused with `value_invalid`.
pub fn list_filtered(conn: &Connection, filter: &AuditFilter) -> Result<AuditPage, AppError> {
    if let Some(prefix) = &filter.action_prefix
        && (prefix.is_empty()
            || prefix.len() > 40
            || !prefix
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'))
    {
        return Err(AppError::validation("actionPrefix", "value_invalid"));
    }
    let limit = i64::from(filter.limit.unwrap_or(50).clamp(1, 200));
    let mut stmt = conn.prepare(FILTER_SQL)?;
    let mut rows = stmt
        .query_map(
            params![
                filter.before_seq,
                limit + 1,
                filter.entity_type,
                filter.entity_id,
                filter.action_prefix
            ],
            map_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = i64::try_from(rows.len()).unwrap_or(i64::MAX) > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    let next_before_seq = if has_more {
        rows.last().map(|r| r.seq)
    } else {
        None
    };
    Ok(AuditPage {
        rows,
        next_before_seq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    fn journal(rows: usize) -> Db {
        let db = Db::open_memory().unwrap();
        for i in 0..rows {
            db.write(move |tx| {
                append(
                    tx,
                    &Actor::system(None).event(
                        "setup.completed",
                        "probe",
                        Some(&i.to_string()),
                        json!({ "i": i }),
                    ),
                )?;
                Ok(())
            })
            .unwrap();
        }
        db
    }

    #[test]
    fn known_answer_hashes() {
        let full = ChainFields {
            prev_hash: GENESIS_HASH,
            seq: 1,
            at: 1_790_000_000_000,
            user_id: Some("u1"),
            terminal_id: Some("t1"),
            action: "setup.completed",
            entity_type: "company",
            entity_id: Some("1"),
            payload: r#"{"a":1}"#,
        };
        assert_eq!(
            full.hash(),
            "21630d28d30e815d8099c4326f94d4ce20370721f94e857d254a0df4ef12c297"
        );
        let nulls = ChainFields {
            user_id: None,
            terminal_id: None,
            entity_id: None,
            payload: "{}",
            ..full
        };
        assert_eq!(
            nulls.hash(),
            "2a309c2564afe10231368da7637acd275136faf79940911232520f033e19f676"
        );
    }

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let value = json!({ "b": 1, "a": { "d": [2, { "z": true, "y": null }], "c": "é" } });
        assert_eq!(
            canonical_json(&value),
            r#"{"a":{"c":"é","d":[2,{"y":null,"z":true}]},"b":1}"#
        );
    }

    #[test]
    fn rows_chain_from_genesis() {
        let db = journal(3);
        let page = db
            .read(|c| {
                list(
                    c,
                    &AuditQuery {
                        before_seq: None,
                        limit: Some(10),
                    },
                )
            })
            .unwrap();
        let rows: Vec<AuditRow> = page.rows.into_iter().rev().collect();
        assert_eq!(rows[0].seq, 1);
        assert_eq!(rows[0].prev_hash, GENESIS_HASH);
        assert_eq!(rows[1].prev_hash, rows[0].hash);
        assert_eq!(rows[2].prev_hash, rows[1].hash);
    }

    fn page(db: &Db, before_seq: Option<i64>, limit: Option<u32>) -> AuditPage {
        db.read(move |c| list(c, &AuditQuery { before_seq, limit }))
            .unwrap()
    }

    fn seqs(page: &AuditPage) -> Vec<i64> {
        page.rows.iter().map(|r| r.seq).collect()
    }

    #[test]
    fn untouched_journal_verifies() {
        let db = journal(5);
        let report = db.read(verify_chain).unwrap();
        assert_eq!(report.rows, 5);
        assert_eq!(report.first_broken_seq, None);
        assert_eq!(report.last_seq, 5);
        assert_eq!(report.last_hash, page(&db, None, Some(1)).rows[0].hash);
    }

    #[test]
    fn empty_journal_report_starts_at_genesis() {
        let report = Db::open_memory().unwrap().read(verify_chain).unwrap();
        assert_eq!(
            report,
            ChainReport {
                rows: 0,
                first_broken_seq: None,
                last_seq: 0,
                last_hash: GENESIS_HASH.to_owned(),
            }
        );
    }

    /// A tamperer who recomputes the edited row's own hash keeps that row valid; the break
    /// shows at the next row, whose `prev_hash` still names the original hash.
    #[test]
    fn relinked_edit_is_detected_at_the_next_row() {
        let db = journal(5);
        db.write(|tx| {
            tx.execute_batch("DROP TRIGGER audit_log_no_update")?;
            let (at, action, entity_type, entity_id, prev_hash): (
                i64,
                String,
                String,
                Option<String>,
                String,
            ) = tx.query_row(
                "SELECT at, action, entity_type, entity_id, prev_hash FROM audit_log WHERE seq = 3",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            let payload = r#"{"i":99}"#;
            let hash = ChainFields {
                prev_hash: &prev_hash,
                seq: 3,
                at,
                user_id: None,
                terminal_id: None,
                action: &action,
                entity_type: &entity_type,
                entity_id: entity_id.as_deref(),
                payload,
            }
            .hash();
            tx.execute(
                "UPDATE audit_log SET payload = ?1, hash = ?2 WHERE seq = 3",
                params![payload, hash],
            )?;
            Ok(())
        })
        .unwrap();
        let report = db.read(verify_chain).unwrap();
        assert_eq!(report.first_broken_seq, Some(4));
        assert_eq!(report.last_seq, 3, "the relinked row itself still verifies");
    }

    #[test]
    fn rows_deleted_at_the_end_are_detected() {
        let db = journal(5);
        db.write(|tx| {
            tx.execute_batch("DROP TRIGGER audit_log_no_delete")?;
            tx.execute("DELETE FROM audit_log WHERE seq >= 4", [])?;
            Ok(())
        })
        .unwrap();
        let report = db.read(verify_chain).unwrap();
        assert_eq!(
            (report.rows, report.first_broken_seq, report.last_seq),
            (3, Some(4), 3)
        );
        // Later appends continue after the high-water mark, so the gap stays visible.
        for _ in 0..3 {
            db.write(|tx| {
                append(
                    tx,
                    &Actor::system(None).event("setup.completed", "probe", None, json!({})),
                )?;
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(seqs(&page(&db, None, None)), [8, 7, 6, 3, 2, 1]);
        assert_eq!(db.read(verify_chain).unwrap().first_broken_seq, Some(6));

        let emptied = journal(2);
        emptied
            .write(|tx| {
                tx.execute_batch("DROP TRIGGER audit_log_no_delete")?;
                tx.execute("DELETE FROM audit_log", [])?;
                Ok(())
            })
            .unwrap();
        let report = emptied.read(verify_chain).unwrap();
        assert_eq!(
            (report.rows, report.first_broken_seq, report.last_seq),
            (0, Some(1), 0)
        );
    }

    #[test]
    fn rolled_back_append_leaves_no_gap() {
        let db = journal(2);
        let failed = db.write(|tx| {
            append(
                tx,
                &Actor::system(None).event("setup.completed", "probe", None, json!({})),
            )?;
            Err::<(), _>(AppError::conflict("forced"))
        });
        assert_eq!(failed.unwrap_err().code(), "forced");
        let row = db
            .write(|tx| {
                append(
                    tx,
                    &Actor::system(None).event("setup.completed", "probe", None, json!({})),
                )
            })
            .unwrap();
        assert_eq!(row.seq, 3);
        let report = db.read(verify_chain).unwrap();
        assert_eq!(
            (report.rows, report.first_broken_seq, report.last_seq),
            (3, None, 3)
        );
    }

    #[test]
    fn control_characters_in_text_fields_are_rejected() {
        let ok = Actor {
            user_id: Some("u1".into()),
            terminal_id: Some("t1".into()),
        }
        .event("setup.completed", "probe", Some("e1"), json!({}));
        let cases = [
            (
                "userId",
                AuditEvent {
                    user_id: Some("u\u{1f}1".into()),
                    ..ok.clone()
                },
            ),
            (
                "terminalId",
                AuditEvent {
                    terminal_id: Some("t\u{0}".into()),
                    ..ok.clone()
                },
            ),
            (
                "action",
                AuditEvent {
                    action: "setup.completed\u{1f}".into(),
                    ..ok.clone()
                },
            ),
            (
                "entityType",
                AuditEvent {
                    entity_type: "pro\tbe".into(),
                    ..ok.clone()
                },
            ),
            (
                "entityId",
                AuditEvent {
                    entity_id: Some("\u{1f}".into()),
                    ..ok.clone()
                },
            ),
        ];
        let db = Db::open_memory().unwrap();
        for (field, event) in cases {
            let err = db.write(move |tx| append(tx, &event)).unwrap_err();
            let payload = err.payload();
            assert_eq!(
                (payload.code.as_str(), payload.field.as_deref()),
                ("value_invalid", Some(field))
            );
        }
        assert_eq!(db.read(verify_chain).unwrap().rows, 0);
        // Non-ASCII text is fine: only U+0000–U+001F could shift a field boundary.
        let arabic = AuditEvent {
            entity_id: Some("مخزن".into()),
            ..ok
        };
        assert_eq!(db.write(move |tx| append(tx, &arabic)).unwrap().seq, 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "unknown audit action")]
    fn unknown_action_trips_a_debug_assertion() {
        // A plain connection: a panic on the Db writer thread would not reach this test.
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::MIGRATIONS
            .to_latest(&mut conn)
            .unwrap();
        let tx = conn.transaction().unwrap();
        let _ = append(
            &tx,
            &Actor::system(None).event("made.up", "probe", None, json!({})),
        );
    }

    // Tampering goes around the append-only triggers the simplest way that stays inside the
    // test: the test's own write transaction drops the trigger on its private in-memory
    // database, then edits the row with plain SQL.
    #[test]
    fn tampered_payload_is_detected_at_its_seq() {
        let db = journal(5);
        db.write(|tx| {
            tx.execute_batch("DROP TRIGGER audit_log_no_update")?;
            tx.execute(
                "UPDATE audit_log SET payload = '{\"i\":99}' WHERE seq = 3",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(db.read(verify_chain).unwrap().first_broken_seq, Some(3));
    }

    #[test]
    fn deleted_row_is_detected() {
        let db = journal(5);
        db.write(|tx| {
            tx.execute_batch("DROP TRIGGER audit_log_no_delete")?;
            tx.execute("DELETE FROM audit_log WHERE seq = 2", [])?;
            Ok(())
        })
        .unwrap();
        assert_eq!(db.read(verify_chain).unwrap().first_broken_seq, Some(3));
    }

    #[test]
    fn list_pages_newest_first() {
        let db = journal(5);
        let first = db
            .read(|c| {
                list(
                    c,
                    &AuditQuery {
                        before_seq: None,
                        limit: Some(2),
                    },
                )
            })
            .unwrap();
        assert_eq!(first.rows.iter().map(|r| r.seq).collect::<Vec<_>>(), [5, 4]);
        assert_eq!(first.next_before_seq, Some(4));
        let rest = db
            .read(|c| {
                list(
                    c,
                    &AuditQuery {
                        before_seq: Some(4),
                        limit: Some(10),
                    },
                )
            })
            .unwrap();
        assert_eq!(
            rest.rows.iter().map(|r| r.seq).collect::<Vec<_>>(),
            [3, 2, 1]
        );
        assert_eq!(rest.next_before_seq, None);
    }

    #[test]
    fn list_limit_defaults_to_50_and_is_clamped() {
        let db = journal(201);
        let default = page(&db, None, None);
        assert_eq!(
            (default.rows.len(), default.next_before_seq),
            (50, Some(152))
        );
        let zero = page(&db, None, Some(0));
        assert_eq!(seqs(&zero), [201]);
        assert_eq!(zero.next_before_seq, Some(201));
        let huge = page(&db, None, Some(500));
        assert_eq!((huge.rows.len(), huge.next_before_seq), (200, Some(2)));
        let last = page(&db, Some(2), Some(500));
        assert_eq!((seqs(&last), last.next_before_seq), (vec![1], None));
    }

    #[test]
    fn list_seeks_on_seq() {
        let db = journal(3);
        let plan = db
            .read(|c| {
                let mut stmt = c.prepare(&["EXPLAIN QUERY PLAN ", LIST_SQL].concat())?;
                let details = stmt
                    .query_map(params![Some(2_i64), 10_i64], |r| r.get::<_, String>(3))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(details)
            })
            .unwrap();
        assert!(
            plan.iter()
                .any(|d| d.contains("USING INTEGER PRIMARY KEY (rowid<?)")),
            "{plan:?}"
        );
    }

    #[test]
    fn list_shows_an_unparsable_payload_as_text() {
        let db = journal(1);
        db.write(|tx| {
            // The CHECK (json_valid) would refuse it; a foreign tool or a repair might not.
            tx.execute_batch("PRAGMA ignore_check_constraints = ON")?;
            tx.execute(
                "INSERT INTO audit_log (seq, id, at, action, entity_type, payload, prev_hash, hash)
                 VALUES (2, ?1, 0, 'setup.completed', 'probe', 'not json', ?2, ?2)",
                params![new_id(), GENESIS_HASH],
            )?;
            Ok(())
        })
        .unwrap();
        let rows = page(&db, None, None).rows;
        assert_eq!(rows[0].payload, Value::String("not json".into()));
        assert_eq!(rows[1].payload, json!({ "i": 0 }));
    }

    #[test]
    fn canonical_json_escapes_strings_and_sorts_nested_keys() {
        let value = json!({
            "z": { "b": [{ "d": 1, "c": "say \"hi\"" }], "a": "C:\\pos" },
            "k\"ey": "unit\u{1f}sep",
        });
        assert_eq!(
            canonical_json(&value),
            r#"{"k\"ey":"unit\u001fsep","z":{"a":"C:\\pos","b":[{"c":"say \"hi\"","d":1}]}}"#
        );
    }

    #[test]
    fn verification_can_start_at_an_anchor() {
        let db = journal(5);
        let third = db
            .read(|c| {
                Ok(
                    c.query_row("SELECT hash FROM audit_log WHERE seq = 3", [], |r| {
                        r.get::<_, String>(0)
                    })?,
                )
            })
            .unwrap();
        let anchor = third.clone();
        let report = db.read(move |c| verify_chain_from(c, 3, &anchor)).unwrap();
        assert_eq!(
            (report.rows, report.first_broken_seq, report.last_seq),
            (2, None, 5)
        );
        let full = db.read(verify_chain).unwrap();
        assert_eq!(report.last_hash, full.last_hash);
        let wrong = db.read(|c| verify_chain_from(c, 3, GENESIS_HASH)).unwrap();
        assert_eq!(
            wrong.first_broken_seq,
            Some(4),
            "a wrong anchor breaks at the next row"
        );
    }

    #[test]
    fn known_actions_are_unique() {
        let unique: std::collections::BTreeSet<_> = KNOWN_ACTIONS.iter().collect();
        assert_eq!(unique.len(), KNOWN_ACTIONS.len());
    }

    #[test]
    fn list_filtered_finds_one_records_history_and_action_prefixes() {
        let db = journal(5); // five `setup.completed` rows on entity "probe" 0..4
        let one = db
            .read(|c| {
                list_filtered(
                    c,
                    &AuditFilter {
                        entity_type: Some("probe".into()),
                        entity_id: Some("2".into()),
                        ..AuditFilter::default()
                    },
                )
            })
            .unwrap();
        assert_eq!(one.rows.len(), 1);
        assert_eq!(one.rows[0].entity_id.as_deref(), Some("2"));
        let setup = db
            .read(|c| {
                list_filtered(
                    c,
                    &AuditFilter {
                        action_prefix: Some("setup.".into()),
                        limit: Some(2),
                        ..AuditFilter::default()
                    },
                )
            })
            .unwrap();
        assert_eq!(setup.rows.len(), 2);
        assert_eq!(setup.next_before_seq, Some(4));
        let none = db
            .read(|c| {
                list_filtered(
                    c,
                    &AuditFilter {
                        action_prefix: Some("auth.".into()),
                        ..AuditFilter::default()
                    },
                )
            })
            .unwrap();
        assert!(none.rows.is_empty());
        let refused = db.read(|c| {
            list_filtered(
                c,
                &AuditFilter {
                    action_prefix: Some("a%".into()),
                    ..AuditFilter::default()
                },
            )
        });
        assert_eq!(refused.unwrap_err().code(), "value_invalid");
    }
}
