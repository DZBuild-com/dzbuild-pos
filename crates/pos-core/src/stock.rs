//! Stock (spec §5.1, STK-001/002/003/007/008/013/015/016/017): an append-only movement ledger;
//! `stock_levels` is a cache moved by the ledger's insert trigger in the same statement and
//! rebuildable from the ledger; negative-stock policy; composite products take their
//! components; adjustments with reasons; weighted average cost (CMUP) on stock received.

#[cfg(test)]
mod count_lots_tests;
pub mod counts;
pub mod lots;
pub mod minimums;
pub mod serials;
pub mod transfers;
pub mod views;

use crate::audit::{self, Actor};
use crate::catalog::clean_code;
use crate::catalog::products::{self, Product, ProductKind};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty};
use crate::sqlenum::sql_text_enum;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;

/// Locations seeded by migration 0002.
pub const SHOP_LOCATION_ID: &str = "00000000-0000-7000-8000-000000000301";
pub const DAMAGED_LOCATION_ID: &str = "00000000-0000-7000-8000-000000000302";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MovementReason {
    Sale,
    Return,
    Purchase,
    Adjustment,
    Count,
    TransferIn,
    TransferOut,
    Loss,
    Production,
    Opening,
}

impl MovementReason {
    pub const ALL: [MovementReason; 10] = [
        MovementReason::Sale,
        MovementReason::Return,
        MovementReason::Purchase,
        MovementReason::Adjustment,
        MovementReason::Count,
        MovementReason::TransferIn,
        MovementReason::TransferOut,
        MovementReason::Loss,
        MovementReason::Production,
        MovementReason::Opening,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            MovementReason::Sale => "sale",
            MovementReason::Return => "return",
            MovementReason::Purchase => "purchase",
            MovementReason::Adjustment => "adjustment",
            MovementReason::Count => "count",
            MovementReason::TransferIn => "transfer_in",
            MovementReason::TransferOut => "transfer_out",
            MovementReason::Loss => "loss",
            MovementReason::Production => "production",
            MovementReason::Opening => "opening",
        }
    }

    pub fn parse(s: &str) -> Option<MovementReason> {
        MovementReason::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

sql_text_enum!(MovementReason);

/// Why a manual adjustment was made (STK-007/008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AdjustReason {
    /// Stock present when the shop starts using the app (positive, with a cost).
    Opening,
    /// A counting or typing error fixed by hand (either sign).
    Correction,
    Damage,
    Loss,
    Theft,
    Expiry,
    /// Taken for the shop's own use (STK-008).
    OwnUse,
    Sample,
}

impl AdjustReason {
    pub const ALL: [AdjustReason; 8] = [
        AdjustReason::Opening,
        AdjustReason::Correction,
        AdjustReason::Damage,
        AdjustReason::Loss,
        AdjustReason::Theft,
        AdjustReason::Expiry,
        AdjustReason::OwnUse,
        AdjustReason::Sample,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            AdjustReason::Opening => "opening",
            AdjustReason::Correction => "correction",
            AdjustReason::Damage => "damage",
            AdjustReason::Loss => "loss",
            AdjustReason::Theft => "theft",
            AdjustReason::Expiry => "expiry",
            AdjustReason::OwnUse => "own_use",
            AdjustReason::Sample => "sample",
        }
    }

    pub fn parse(s: &str) -> Option<AdjustReason> {
        AdjustReason::ALL.into_iter().find(|r| r.as_str() == s)
    }

    /// The ledger reason: opening, adjustment (correction) or loss (every stock-out reason).
    pub const fn movement_reason(self) -> MovementReason {
        match self {
            AdjustReason::Opening => MovementReason::Opening,
            AdjustReason::Correction => MovementReason::Adjustment,
            _ => MovementReason::Loss,
        }
    }
}

sql_text_enum!(AdjustReason);

/// Setting `stock.negative_policy` (STK-017); a product with `allow_negative_stock` is always
/// allowed below zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NegativePolicy {
    Allow,
    /// Sell, and report the shortage to the cashier.
    #[default]
    Warn,
    /// Refuse the sale (`stock_insufficient`).
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LocationKind {
    Storage,
    /// Returned goods written off (RET-006), never counted as sellable stock.
    Damaged,
}

impl LocationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            LocationKind::Storage => "storage",
            LocationKind::Damaged => "damaged",
        }
    }

    pub fn parse(s: &str) -> Option<LocationKind> {
        [LocationKind::Storage, LocationKind::Damaged]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

sql_text_enum!(LocationKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StockLocation {
    pub id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub kind: LocationKind,
    pub is_default: bool,
    pub archived: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocationInput {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StockMovement {
    pub id: String,
    pub product_id: String,
    pub location_id: String,
    pub qty_delta: Qty,
    pub reason: MovementReason,
    pub reason_code: Option<AdjustReason>,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub unit_cost: Option<Money>,
    pub created_at: i64,
    pub user_id: Option<String>,
    pub terminal_id: Option<String>,
    pub note: Option<String>,
}

/// A ledger row to append. `qty_delta` is signed: negative leaves the location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMovement {
    pub product_id: String,
    pub location_id: String,
    pub qty_delta: Qty,
    pub reason: MovementReason,
    pub reason_code: Option<AdjustReason>,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub unit_cost: Option<Money>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StockLevel {
    pub location_id: String,
    pub location_code: String,
    pub kind: LocationKind,
    pub qty: Qty,
}

/// Stock that must leave a location for a sale line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockNeed {
    pub product_id: String,
    pub location_id: String,
    pub qty: Qty,
}

/// A product that would go below zero (sale warning or refusal details).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Shortage {
    pub product_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub location_id: String,
    pub available: Qty,
    pub needed: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdjustInput {
    pub product_id: String,
    /// Default location when absent.
    pub location_id: Option<String>,
    pub qty_delta: Qty,
    pub reason: AdjustReason,
    /// HT cost of what comes in (opening stock, found goods): updates the CMUP.
    pub unit_cost: Option<Money>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LowStock {
    pub product_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub on_hand: Qty,
    pub min_stock: Qty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RebuildReport {
    /// (product, location) pairs in the ledger.
    pub levels: i64,
    /// Cached levels that disagreed with the ledger and were corrected.
    pub corrected: i64,
}

const LOCATION_SELECT: &str = "SELECT id, code, name_ar, name_fr, name_en, kind, is_default,
        archived_at IS NOT NULL FROM stock_locations";

fn map_location(row: &Row<'_>) -> rusqlite::Result<StockLocation> {
    Ok(StockLocation {
        id: row.get(0)?,
        code: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        kind: row.get(5)?,
        is_default: row.get(6)?,
        archived: row.get(7)?,
    })
}

/// Active locations: the default first, damaged goods last.
pub fn locations(conn: &Connection) -> Result<Vec<StockLocation>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{LOCATION_SELECT} WHERE archived_at IS NULL ORDER BY is_default DESC, kind = 'damaged', rowid"
    ))?;
    let rows = stmt
        .query_map([], map_location)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn location(conn: &Connection, id: &str) -> Result<StockLocation, AppError> {
    conn.query_row(
        &format!("{LOCATION_SELECT} WHERE id = ?1"),
        [id],
        map_location,
    )
    .optional()?
    .filter(|l| !l.archived)
    .ok_or_else(|| AppError::not_found("stock_location"))
}

/// Where sales take stock from and restocked returns go.
pub fn default_location(conn: &Connection) -> Result<StockLocation, AppError> {
    conn.query_row(
        &format!("{LOCATION_SELECT} WHERE is_default = 1"),
        [],
        map_location,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("stock_location"))
}

/// Where returned goods written off go (RET-006).
pub fn damaged_location(conn: &Connection) -> Result<StockLocation, AppError> {
    conn.query_row(
        &format!("{LOCATION_SELECT} WHERE kind = 'damaged' AND archived_at IS NULL ORDER BY rowid LIMIT 1"),
        [],
        map_location,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("stock_location"))
}

/// A new storage location (dépôt, van: STK-003).
pub fn create_location(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: LocationInput,
) -> Result<StockLocation, AppError> {
    let code = clean_code(&input.code, "code")?;
    let (name_ar, name_fr, name_en) =
        crate::catalog::clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM stock_locations WHERE code = ?1)",
        [&code],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO stock_locations (id, code, name_ar, name_fr, name_en, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, code, name_ar, name_fr, name_en, now],
    )?;
    let created = location(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "stock_location.created",
            "stock_location",
            Some(&id),
            json!({ "after": created }),
        ),
    )?;
    Ok(created)
}

/// Appends one ledger row without a lot (every M1 caller).
pub fn append(
    tx: &Transaction<'_>,
    actor: &Actor,
    m: &NewMovement,
    now: i64,
) -> Result<String, AppError> {
    append_with_lot(tx, actor, m, None, now)
}

/// Appends one ledger row, in `lot_id` when given; the insert triggers move `stock_levels` (and
/// `lot_levels` for a lot) in the same statement. No audit row: the business operation that
/// moves stock (sale, return, adjustment, receipt) audits itself.
pub fn append_with_lot(
    tx: &Transaction<'_>,
    actor: &Actor,
    m: &NewMovement,
    lot_id: Option<&str>,
    now: i64,
) -> Result<String, AppError> {
    if m.qty_delta == Qty::ZERO {
        return Err(AppError::validation("qtyDelta", "qty_invalid"));
    }
    let id = new_id();
    tx.prepare_cached(
        "INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, reason_code,
             ref_type, ref_id, unit_cost, created_at, user_id, terminal_id, note, lot_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?
    .execute(params![
        id,
        m.product_id,
        m.location_id,
        m.qty_delta,
        m.reason,
        m.reason_code,
        m.ref_type,
        m.ref_id,
        m.unit_cost,
        now,
        actor.user_id,
        actor.terminal_id,
        m.note,
        lot_id
    ])?;
    Ok(id)
}

pub fn level(conn: &Connection, product_id: &str, location_id: &str) -> Result<Qty, AppError> {
    let qty: Option<Qty> = conn
        .prepare_cached("SELECT qty FROM stock_levels WHERE product_id = ?1 AND location_id = ?2")?
        .query_row(params![product_id, location_id], |r| r.get(0))
        .optional()?;
    Ok(qty.unwrap_or(Qty::ZERO))
}

/// Sellable stock: every storage location, damaged goods excluded.
pub fn on_hand(conn: &Connection, product_id: &str) -> Result<Qty, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(l.qty), 0) FROM stock_levels l
         JOIN stock_locations s ON s.id = l.location_id
         WHERE l.product_id = ?1 AND s.kind = 'storage'",
        [product_id],
        |r| r.get(0),
    )?)
}

/// Stock per location (STK-002).
pub fn levels(conn: &Connection, product_id: &str) -> Result<Vec<StockLevel>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT l.location_id, s.code, s.kind, l.qty FROM stock_levels l
         JOIN stock_locations s ON s.id = l.location_id
         WHERE l.product_id = ?1 ORDER BY s.is_default DESC, s.rowid",
    )?;
    let rows = stmt
        .query_map([product_id], |r| {
            Ok(StockLevel {
                location_id: r.get(0)?,
                location_code: r.get(1)?,
                kind: r.get(2)?,
                qty: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

const MOVEMENT_SELECT: &str = "SELECT id, product_id, location_id, qty_delta, reason, reason_code,
        ref_type, ref_id, unit_cost, created_at, user_id, terminal_id, note FROM stock_movements";

fn map_movement(r: &Row<'_>) -> rusqlite::Result<StockMovement> {
    Ok(StockMovement {
        id: r.get(0)?,
        product_id: r.get(1)?,
        location_id: r.get(2)?,
        qty_delta: r.get(3)?,
        reason: r.get(4)?,
        reason_code: r.get(5)?,
        ref_type: r.get(6)?,
        ref_id: r.get(7)?,
        unit_cost: r.get(8)?,
        created_at: r.get(9)?,
        user_id: r.get(10)?,
        terminal_id: r.get(11)?,
        note: r.get(12)?,
    })
}

/// The ledger of a product, newest first (STK-015: who, when, why, which document).
pub fn movements(
    conn: &Connection,
    product_id: &str,
    limit: u32,
) -> Result<Vec<StockMovement>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{MOVEMENT_SELECT} WHERE product_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(
            params![product_id, i64::from(limit.clamp(1, 500))],
            map_movement,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// One ledger row by id, `None` when there is no such movement.
pub fn movement(conn: &Connection, id: &str) -> Result<Option<StockMovement>, AppError> {
    Ok(conn
        .query_row(
            &format!("{MOVEMENT_SELECT} WHERE id = ?1"),
            [id],
            map_movement,
        )
        .optional()?)
}

/// Recomputes every cached level from the ledger, the per-lot cache `lot_levels` included, and
/// reports the corrections (stock and lot rows together in `corrected`, the lot part in the
/// audit payload); audited.
pub fn rebuild_levels(tx: &Transaction<'_>, actor: &Actor) -> Result<RebuildReport, AppError> {
    let count = |sql: &str| -> Result<i64, AppError> { Ok(tx.query_row(sql, [], |r| r.get(0))?) };
    let corrected = count(
        "SELECT COUNT(*) FROM (
             SELECT m.product_id, m.location_id, SUM(m.qty_delta) AS total FROM stock_movements m
             GROUP BY m.product_id, m.location_id) t
         LEFT JOIN stock_levels l ON l.product_id = t.product_id AND l.location_id = t.location_id
         WHERE l.qty IS NOT t.total",
    )?;
    let orphans = count(
        "SELECT COUNT(*) FROM stock_levels l WHERE NOT EXISTS (SELECT 1 FROM stock_movements m
             WHERE m.product_id = l.product_id AND m.location_id = l.location_id) AND l.qty <> 0",
    )?;
    let lot_corrected = count(
        "SELECT COUNT(*) FROM (
             SELECT m.lot_id, m.location_id, SUM(m.qty_delta) AS total FROM stock_movements m
             WHERE m.lot_id IS NOT NULL GROUP BY m.lot_id, m.location_id) t
         LEFT JOIN lot_levels l ON l.lot_id = t.lot_id AND l.location_id = t.location_id
         WHERE l.qty IS NOT t.total",
    )?;
    let lot_orphans = count(
        "SELECT COUNT(*) FROM lot_levels l WHERE NOT EXISTS (SELECT 1 FROM stock_movements m
             WHERE m.lot_id = l.lot_id AND m.location_id = l.location_id) AND l.qty <> 0",
    )?;
    let now = now_ms();
    tx.execute("DELETE FROM stock_levels", [])?;
    let levels = tx.execute(
        "INSERT INTO stock_levels (product_id, location_id, qty, updated_at)
         SELECT product_id, location_id, SUM(qty_delta), ?1 FROM stock_movements
         GROUP BY product_id, location_id",
        [now],
    )?;
    tx.execute("DELETE FROM lot_levels", [])?;
    let lot_levels = tx.execute(
        "INSERT INTO lot_levels (lot_id, location_id, qty, updated_at)
         SELECT lot_id, location_id, SUM(qty_delta), ?1 FROM stock_movements
         WHERE lot_id IS NOT NULL GROUP BY lot_id, location_id",
        [now],
    )?;
    let lot_corrected = lot_corrected + lot_orphans;
    let report = RebuildReport {
        levels: i64::try_from(levels).unwrap_or(i64::MAX),
        corrected: corrected + orphans + lot_corrected,
    };
    audit::append(
        tx,
        &actor.event(
            "stock.levels_rebuilt",
            "stock",
            None,
            json!({
                "levels": report.levels,
                "corrected": report.corrected,
                "lotLevels": i64::try_from(lot_levels).unwrap_or(i64::MAX),
                "lotCorrected": lot_corrected,
            }),
        ),
    )?;
    Ok(report)
}

fn policy(conn: &Connection) -> Result<NegativePolicy, AppError> {
    crate::settings::get(conn, "stock.negative_policy")
}

/// Checks that `needs` can leave their locations. Products allowed below zero, untracked
/// products and the `allow` policy never produce a shortage; under `warn` the shortages are
/// returned for the cashier; under `block` they refuse the operation with `stock_insufficient`
/// and the list in `details.shortages`.
pub fn check_available(conn: &Connection, needs: &[StockNeed]) -> Result<Vec<Shortage>, AppError> {
    let policy = policy(conn)?;
    if policy == NegativePolicy::Allow {
        return Ok(Vec::new());
    }
    let mut totals: BTreeMap<(&str, &str), Qty> = BTreeMap::new();
    for need in needs {
        let slot = totals
            .entry((&need.product_id, &need.location_id))
            .or_insert(Qty::ZERO);
        *slot = slot
            .checked_add(need.qty)
            .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?;
    }
    let mut shortages = Vec::new();
    for ((product_id, location_id), needed) in totals {
        let product = products::get(conn, product_id)?;
        if !product.track_stock || product.allow_negative_stock {
            continue;
        }
        // Stock reserved for a customer order is not sold to someone else (ORD-003); the order's
        // own sale or delivery releases its reservations first.
        let available = level(conn, product_id, location_id)?
            .checked_sub(crate::orders::reserved(conn, product_id, location_id)?)
            .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?;
        if available < needed {
            shortages.push(Shortage {
                product_id: product.id,
                name_ar: product.name_ar,
                name_fr: product.name_fr,
                name_en: product.name_en,
                location_id: location_id.to_owned(),
                available,
                needed,
            });
        }
    }
    if policy == NegativePolicy::Block && !shortages.is_empty() {
        return Err(AppError::rejected(
            json!({ "shortages": shortages }),
            "stock_insufficient",
        ));
    }
    Ok(shortages)
}

/// The stocked products and quantities that selling `qty` of `product` takes: itself when it
/// holds stock, its components (× qty) when it is a composite, nothing for a service or an
/// untracked product.
pub fn expand(
    conn: &Connection,
    product: &Product,
    qty: Qty,
) -> Result<Vec<(Product, Qty)>, AppError> {
    match product.kind {
        ProductKind::Composite => product
            .components
            .iter()
            .map(|c| {
                let component = products::get(conn, &c.component_id)?;
                let total = c
                    .qty
                    .mul_qty(qty)
                    .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?;
                Ok((component, total))
            })
            .collect(),
        _ if product.track_stock => Ok(vec![(product.clone(), qty)]),
        _ => Ok(Vec::new()),
    }
}

/// Weighted average cost after `qty` comes in at `unit_cost` (STK-016):
/// `(max(on_hand, 0) × cost + qty × unit_cost) / (max(on_hand, 0) + qty)`, rounded once, half
/// away from zero. Writes the product's cost and one `cost_history` row when it moves.
/// Returns (before, after). Call it before appending the incoming movement.
pub(crate) fn apply_cmup(
    tx: &Transaction<'_>,
    product: &Product,
    qty: Qty,
    unit_cost: Money,
    source_type: &str,
    source_id: Option<&str>,
    now: i64,
) -> Result<(Money, Money), AppError> {
    let invalid = || AppError::validation("unitCost", "amount_invalid");
    if unit_cost.is_negative() || !qty.is_positive() {
        return Err(invalid());
    }
    let before: Money = tx.query_row(
        "SELECT cost_price FROM products WHERE id = ?1",
        [&product.id],
        |r| r.get(0),
    )?;
    let held = on_hand(tx, &product.id)?.max(Qty::ZERO);
    let total = held.checked_add(qty).ok_or_else(invalid)?;
    let value =
        i128::from(held.0) * i128::from(before.0) + i128::from(qty.0) * i128::from(unit_cost.0);
    let after = i64::try_from((value + i128::from(total.0) / 2) / i128::from(total.0))
        .map(Money)
        .map_err(|_| invalid())?;
    if after != before {
        tx.execute(
            "UPDATE products SET cost_price = ?2, updated_at = ?3 WHERE id = ?1",
            params![product.id, after, now],
        )?;
        tx.execute(
            "INSERT INTO cost_history (id, product_id, cost_before, cost_after, source_type, source_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), product.id, before, after, source_type, source_id, now],
        )?;
    }
    Ok((before, after))
}

/// One received line (goods receipts, opening stock with lots): what comes in, at which HT cost,
/// in which lot, with which serial numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveLine {
    pub product_id: String,
    pub location_id: String,
    pub qty: Qty,
    pub unit_cost: Money,
    pub lot: Option<lots::LotInput>,
    pub serials: Vec<String>,
    pub supplier_id: Option<String>,
    pub ref_type: String,
    pub ref_id: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Received {
    pub movement_id: String,
    pub lot_id: Option<String>,
    pub cost_before: Money,
    pub cost_after: Money,
}

/// Stock in (PUR-003/004, STK-016): CMUP, lot, serials, `purchase` movement. No audit: the
/// receiving document audits.
pub fn receive(
    tx: &Transaction<'_>,
    actor: &Actor,
    line: &ReceiveLine,
    now: i64,
) -> Result<Received, AppError> {
    let product = products::get(tx, &line.product_id)?;
    if !product.track_stock {
        return Err(AppError::validation("productId", "product_not_stocked"));
    }
    if !line.qty.is_positive() || !line.qty.fits_decimals(product.unit_decimals) {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    let location_id = location(tx, &line.location_id)
        .map_err(|_| AppError::validation("locationId", "stock_location_not_found"))?
        .id;
    let extras = crate::catalog::extras::get(tx, &product.id)?;
    let lot = match (&line.lot, extras.track_lots) {
        (Some(lot), _) => Some(lots::get_or_create(
            tx,
            &product.id,
            lot,
            line.supplier_id.as_deref(),
            now,
        )?),
        (None, true) => return Err(AppError::validation("lot", "lot_required")),
        (None, false) => None,
    };
    if extras.track_serials {
        let pieces = usize::try_from(line.qty.0 / 1_000).unwrap_or(usize::MAX);
        if line.qty.0 % 1_000 != 0 || line.serials.len() != pieces {
            return Err(AppError::validation("serials", "serials_count_mismatch"));
        }
    }
    let (cost_before, cost_after) = apply_cmup(
        tx,
        &product,
        line.qty,
        line.unit_cost,
        &line.ref_type,
        Some(&line.ref_id),
        now,
    )?;
    let movement = NewMovement {
        product_id: product.id.clone(),
        location_id: location_id.clone(),
        qty_delta: line.qty,
        reason: MovementReason::Purchase,
        reason_code: None,
        ref_type: Some(line.ref_type.clone()),
        ref_id: Some(line.ref_id.clone()),
        unit_cost: Some(line.unit_cost),
        note: crate::catalog::clean_opt(line.note.as_deref(), "note", 200)?,
    };
    let movement_id = append_with_lot(
        tx,
        actor,
        &movement,
        lot.as_ref().map(|l| l.id.as_str()),
        now,
    )?;
    let received_document = (line.ref_type == "document").then_some(line.ref_id.as_str());
    for serial in &line.serials {
        serials::register(
            tx,
            &product.id,
            serial,
            Some(&location_id),
            received_document,
            now,
        )?;
    }
    Ok(Received {
        movement_id,
        lot_id: lot.map(|l| l.id),
        cost_before,
        cost_after,
    })
}

/// A manual adjustment (STK-007/008): opening stock, correction, or a stock-out reason. Stock
/// coming in with a `unit_cost` updates the product's CMUP (STK-016) through [`apply_cmup`].
///
/// For a product tracked by lot, stock going out leaves through [`lots::take`], first-expired
/// first-out and never refused (an expiry write-off empties the expired lots first), one ledger
/// row per lot, then stock without a lot. Stock coming in through an adjustment carries no lot:
/// it is stock without a lot (lots come in through [`receive`]). Returns the first ledger row;
/// the audit payload lists every row (`movementIds`) and lot (`lots`).
pub fn adjust(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: AdjustInput,
    now: i64,
) -> Result<StockMovement, AppError> {
    let product = products::get(tx, &input.product_id)?;
    if !product.track_stock {
        return Err(AppError::validation("productId", "product_not_stocked"));
    }
    // An archived product's stock is out of sight: restore it before stocking or correcting it.
    if product.archived {
        return Err(AppError::validation("productId", "product_archived"));
    }
    let location_id = match &input.location_id {
        Some(id) => {
            location(tx, id)
                .map_err(|_| AppError::validation("locationId", "stock_location_not_found"))?
                .id
        }
        None => default_location(tx)?.id,
    };
    let qty = input.qty_delta;
    let sign_ok = match input.reason {
        AdjustReason::Opening => qty.is_positive(),
        AdjustReason::Correction => qty != Qty::ZERO,
        _ => qty.is_negative(),
    };
    if !sign_ok || !qty.fits_decimals(product.unit_decimals) {
        return Err(AppError::validation("qtyDelta", "qty_invalid"));
    }
    if input.unit_cost.is_some_and(Money::is_negative) {
        return Err(AppError::validation("unitCost", "amount_invalid"));
    }
    let note = crate::catalog::clean_opt(input.note.as_deref(), "note", 200)?;
    let before_cost = product.cost_price;
    if let (Some(unit_cost), true) = (input.unit_cost, qty.is_positive()) {
        apply_cmup(tx, &product, qty, unit_cost, "adjustment", None, now)?;
    }
    let movement = NewMovement {
        product_id: product.id.clone(),
        location_id,
        qty_delta: qty,
        reason: input.reason.movement_reason(),
        reason_code: Some(input.reason).filter(|r| !matches!(r, AdjustReason::Opening)),
        ref_type: None,
        ref_id: None,
        unit_cost: input.unit_cost.or(Some(before_cost)),
        note,
    };
    let (lot_takes, ids) = if qty.is_negative() {
        let today = crate::time::business_date(now)
            .format("%Y-%m-%d")
            .to_string();
        lots::take_rows(tx, actor, &movement, &today, now)?
    } else {
        (Vec::new(), vec![append(tx, actor, &movement, now)?])
    };
    let id = ids
        .first()
        .cloned()
        .ok_or_else(|| AppError::Internal("adjustment appended no movement".into()))?;
    let cost_after: Money = tx.query_row(
        "SELECT cost_price FROM products WHERE id = ?1",
        [&product.id],
        |r| r.get(0),
    )?;
    audit::append(
        tx,
        &actor.event(
            "stock.adjusted",
            "product",
            Some(&product.id),
            json!({
                "movementId": id,
                "movementIds": ids,
                "lots": lot_takes,
                "locationId": movement.location_id,
                "qtyDelta": qty,
                "reason": input.reason,
                "costBefore": before_cost,
                "costAfter": cost_after,
                "note": movement.note,
            }),
        ),
    )?;
    // By id, not "newest first": a later-dated row (clock set back, replica) must not be returned.
    self::movement(tx, &id)?
        .ok_or_else(|| AppError::Internal("movement missing after insert".into()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdjustBatchLine {
    pub product_id: String,
    pub qty_delta: Qty,
    pub unit_cost: Option<Money>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdjustBatchInput {
    pub location_id: Option<String>,
    pub reason: AdjustReason,
    pub lines: Vec<AdjustBatchLine>,
    pub note: Option<String>,
}

/// A multi-line adjustment (STK-006): one reason, several products, every line or none; each
/// line is an M1 adjustment with its own audit row.
pub fn adjust_batch(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: AdjustBatchInput,
    now: i64,
) -> Result<Vec<StockMovement>, AppError> {
    if input.lines.is_empty() {
        return Err(AppError::validation("lines", "required"));
    }
    input
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            adjust(
                tx,
                actor,
                AdjustInput {
                    product_id: l.product_id.clone(),
                    location_id: input.location_id.clone(),
                    qty_delta: l.qty_delta,
                    reason: input.reason,
                    unit_cost: l.unit_cost,
                    note: input.note.clone(),
                },
                now,
            )
            .map_err(|e| match e {
                AppError::Validation { field, code } => {
                    AppError::validation(&format!("lines.{i}.{field}"), &code)
                }
                other => other,
            })
        })
        .collect()
}

/// Tracked products whose sellable stock is below their minimum (STK-013/014).
pub fn below_minimum(conn: &Connection, limit: u32) -> Result<Vec<LowStock>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name_ar, p.name_fr, p.name_en,
                COALESCE((SELECT SUM(l.qty) FROM stock_levels l JOIN stock_locations s ON s.id = l.location_id
                          WHERE l.product_id = p.id AND s.kind = 'storage'), 0) AS on_hand,
                p.min_stock
         FROM products p
         WHERE p.track_stock = 1 AND p.archived_at IS NULL AND p.min_stock > 0 AND on_hand < p.min_stock
         ORDER BY on_hand - p.min_stock, p.name_fr LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([i64::from(limit.clamp(1, 1_000))], |r| {
            Ok(LowStock {
                product_id: r.get(0)?,
                name_ar: r.get(1)?,
                name_fr: r.get(2)?,
                name_en: r.get(3)?,
                on_hand: r.get(4)?,
                min_stock: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::products::{Component, ProductInput};
    use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID};
    use crate::testkit::{NOON, shop};
    use proptest::prelude::*;
    use serde_json::json;

    /// Stock never goes out of sight: a product holding stock is neither archived nor untracked
    /// (`stock_not_zero`), an archived one takes no opening stock, adjustment or transfer
    /// (`product_archived`), and stock an archived product still holds stays listed, flagged.
    #[test]
    fn a_product_holding_stock_is_neither_archived_nor_untracked() {
        let shop = shop();
        let oil = shop.product("Huile Elio 5 L", Money(65_000), &shop.tva19);
        shop.stock_in(&oil.id, 3_000);
        let actor = shop.owner.actor();
        let id = oil.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| products::archive(tx, &actor, &id))
                .unwrap_err(),
            AppError::conflict("stock_not_zero")
        );
        let mut untracked = ProductInput::from(&oil);
        untracked.track_stock = false;
        let (actor, id) = (shop.owner.actor(), oil.id.clone());
        assert_eq!(
            shop.db
                .write(move |tx| products::update(tx, &actor, &id, untracked))
                .unwrap_err(),
            AppError::conflict("stock_not_zero")
        );
        // Written off to zero, it can be archived; archived, it takes no stock.
        adjust_by(&shop, &oil.id, -3_000, AdjustReason::Damage, None).unwrap();
        let (actor, id) = (shop.owner.actor(), oil.id.clone());
        shop.db
            .write(move |tx| products::archive(tx, &actor, &id))
            .unwrap();
        let input = AdjustInput {
            product_id: oil.id.clone(),
            location_id: None,
            qty_delta: Qty(1_000),
            reason: AdjustReason::Opening,
            unit_cost: None,
            note: None,
        };
        let actor = shop.owner.actor();
        assert_eq!(
            shop.db
                .write(move |tx| adjust(tx, &actor, input, NOON))
                .unwrap_err(),
            AppError::validation("productId", "product_archived")
        );
        // Stock an archived product holds anyway (data from before this rule) stays listed.
        let (actor, id) = (shop.owner.actor(), oil.id.clone());
        shop.db
            .write(move |tx| products::restore(tx, &actor, &id))
            .unwrap();
        shop.stock_in(&oil.id, 2_000);
        let id = oil.id.clone();
        shop.db
            .write(move |tx| {
                tx.execute("UPDATE products SET archived_at = 1 WHERE id = ?1", [&id])?;
                Ok(())
            })
            .unwrap();
        let rows = shop
            .db
            .read(|c| views::levels(c, &views::LevelQuery::default()))
            .unwrap();
        let row = rows.iter().find(|r| r.product_id == oil.id).unwrap();
        assert_eq!((row.qty, row.archived), (Qty(2_000), true));
    }

    fn adjust_by(
        shop: &crate::testkit::Shop,
        product_id: &str,
        qty: i64,
        reason: AdjustReason,
        cost: Option<i64>,
    ) -> Result<StockMovement, AppError> {
        let input = AdjustInput {
            product_id: product_id.into(),
            location_id: None,
            qty_delta: Qty(qty),
            reason,
            unit_cost: cost.map(Money),
            note: Some("test".into()),
        };
        let actor = shop.owner.actor();
        shop.db.write(move |tx| adjust(tx, &actor, input, NOON))
    }

    #[test]
    fn adjustments_move_the_ledger_and_the_cache_together() {
        let shop = shop();
        let p = shop.product("Lait", Money(11_000), &shop.tva19);
        adjust_by(&shop, &p.id, 24_000, AdjustReason::Opening, Some(8_000)).unwrap();
        let out = adjust_by(&shop, &p.id, -2_000, AdjustReason::Damage, None).unwrap();
        assert_eq!(
            (out.reason, out.reason_code),
            (MovementReason::Loss, Some(AdjustReason::Damage))
        );
        assert_eq!(out.user_id.as_deref(), Some(shop.owner.user_id.as_str()));
        let id = p.id.clone();
        assert_eq!(shop.db.read(move |c| on_hand(c, &id)).unwrap(), Qty(22_000));
        let id = p.id.clone();
        let per_location = shop.db.read(move |c| levels(c, &id)).unwrap();
        assert_eq!(
            (per_location[0].location_id.as_str(), per_location[0].qty),
            (SHOP_LOCATION_ID, Qty(22_000))
        );
        let id = p.id.clone();
        let ledger = shop.db.read(move |c| movements(c, &id, 10)).unwrap();
        assert_eq!(ledger.len(), 2);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'stock.adjusted'"),
            2
        );
    }

    #[test]
    fn adjust_returns_its_own_movement_even_behind_a_later_one() {
        // A clock set back after a power cut, or a replica row stamped by a host clock running
        // ahead, leaves a newer movement in the ledger: `adjust` must still return its own row.
        let shop = shop();
        let p = shop.product("Lait", Money(11_000), &shop.tva19);
        let actor = shop.owner.actor();
        let later = NewMovement {
            product_id: p.id.clone(),
            location_id: SHOP_LOCATION_ID.into(),
            qty_delta: Qty(5_000),
            reason: MovementReason::Adjustment,
            reason_code: Some(AdjustReason::Correction),
            ref_type: None,
            ref_id: None,
            unit_cost: None,
            note: None,
        };
        let later_id = shop
            .db
            .write(move |tx| append(tx, &actor, &later, NOON + 86_400_000))
            .unwrap();
        let out = adjust_by(&shop, &p.id, -2_000, AdjustReason::Damage, None).unwrap();
        let id = p.id.clone();
        let inserted: String = shop
            .db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT id FROM stock_movements WHERE product_id = ?1 AND created_at = ?2",
                    params![id, NOON],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_ne!(out.id, later_id);
        assert_eq!(out.id, inserted);
        assert_eq!(
            (out.qty_delta, out.created_at, out.reason_code),
            (Qty(-2_000), NOON, Some(AdjustReason::Damage))
        );
        let found = shop.db.read(move |c| movement(c, &later_id)).unwrap();
        assert_eq!(found.map(|m| m.qty_delta), Some(Qty(5_000)));
        let unknown = new_id();
        assert!(
            shop.db
                .read(move |c| movement(c, &unknown))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn adjustment_signs_follow_the_reason() {
        let shop = shop();
        let p = shop.product("Lait", Money(11_000), &shop.tva19);
        for (qty, reason) in [
            (-1_000, AdjustReason::Opening),
            (1_000, AdjustReason::Theft),
            (0, AdjustReason::Correction),
            (500, AdjustReason::Correction),
        ] {
            let err = adjust_by(&shop, &p.id, qty, reason, None).unwrap_err();
            assert_eq!(err.code(), "qty_invalid", "{qty} {reason:?}");
        }
        let mut service =
            ProductInput::simple("Photocopie", UNIT_PIECE_ID, &shop.tva19, Money(1_000));
        service.kind = ProductKind::Service;
        let service = shop.product_with(service);
        let err = adjust_by(&shop, &service.id, 1_000, AdjustReason::Opening, None).unwrap_err();
        assert_eq!(err.code(), "product_not_stocked");
    }

    #[test]
    fn stock_received_at_a_cost_updates_the_average_cost() {
        let shop = shop();
        let p = shop.product("Huile 5 L", Money(90_000), &shop.tva19);
        adjust_by(&shop, &p.id, 10_000, AdjustReason::Opening, Some(70_000)).unwrap();
        adjust_by(&shop, &p.id, 30_000, AdjustReason::Opening, Some(74_000)).unwrap();
        // (10 × 700 + 30 × 740) / 40 = 730.00 DA
        let id = p.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| products::get(c, &id))
                .unwrap()
                .cost_price,
            Money(73_000)
        );
    }

    #[test]
    fn each_receipt_moves_the_cmup_and_writes_its_history() {
        let shop = crate::testkit::shop();
        let p = shop.product("Huile Elio 5 L", Money(65_000), &shop.tva19);
        let line = |qty: i64, cost: i64| ReceiveLine {
            product_id: p.id.clone(),
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(qty),
            unit_cost: Money(cost),
            lot: None,
            serials: Vec::new(),
            supplier_id: None,
            ref_type: "document".into(),
            ref_id: "rc1".into(),
            note: None,
        };
        let actor = shop.owner.actor();
        let (a, b) = (line(10_000, 50_000), line(30_000, 54_000));
        let first = shop.db.write(move |tx| receive(tx, &actor, &a, 1)).unwrap();
        assert_eq!(
            (first.cost_before, first.cost_after),
            (Money::ZERO, Money(50_000))
        );
        let actor = shop.owner.actor();
        let second = shop.db.write(move |tx| receive(tx, &actor, &b, 2)).unwrap();
        // (10 × 500 + 30 × 540) / 40 = 530 DA.
        assert_eq!(second.cost_after, Money(53_000));
        assert_eq!(shop.count("SELECT COUNT(*) FROM cost_history"), 2);
        assert_eq!(shop.count("SELECT SUM(qty) FROM stock_levels"), 40_000);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE reason = 'purchase'"),
            2
        );
    }

    #[test]
    fn negative_policy_warns_blocks_or_allows() {
        let shop = shop();
        let p = shop.product("Lait", Money(11_000), &shop.tva19);
        adjust_by(&shop, &p.id, 1_000, AdjustReason::Opening, None).unwrap();
        let need = vec![StockNeed {
            product_id: p.id.clone(),
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(3_000),
        }];
        let n = need.clone();
        let warned = shop.db.read(move |c| check_available(c, &n)).unwrap();
        assert_eq!(
            (warned.len(), warned[0].available, warned[0].needed),
            (1, Qty(1_000), Qty(3_000))
        );
        shop.set("stock.negative_policy", json!("block"));
        let n = need.clone();
        let err = shop.db.read(move |c| check_available(c, &n)).unwrap_err();
        let payload = err.payload();
        assert_eq!(payload.code, "stock_insufficient");
        assert_eq!(
            payload.details.unwrap()["shortages"][0]["productId"],
            json!(p.id)
        );
        shop.set("stock.negative_policy", json!("allow"));
        let n = need.clone();
        assert!(
            shop.db
                .read(move |c| check_available(c, &n))
                .unwrap()
                .is_empty()
        );
        // A product allowed below zero never blocks.
        shop.set("stock.negative_policy", json!("block"));
        let mut loose = ProductInput::simple("Pain", UNIT_PIECE_ID, &shop.tva9, Money(1_000));
        loose.allow_negative_stock = true;
        let bread = shop.product_with(loose);
        let n = vec![StockNeed {
            product_id: bread.id,
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(50_000),
        }];
        assert!(
            shop.db
                .read(move |c| check_available(c, &n))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn composites_take_their_components() {
        let shop = shop();
        let pen = shop.product("Stylo", Money(5_000), &shop.tva19);
        let book = shop.product("Cahier", Money(8_000), &shop.tva19);
        let mut kit = ProductInput::simple("Kit", UNIT_PIECE_ID, &shop.tva19, Money(20_000));
        kit.kind = ProductKind::Composite;
        kit.components = vec![
            Component {
                component_id: pen.id.clone(),
                qty: Qty(2_000),
            },
            Component {
                component_id: book.id.clone(),
                qty: Qty(3_000),
            },
        ];
        let kit = shop.product_with(kit);
        let expanded = shop.db.read(move |c| expand(c, &kit, Qty(2_000))).unwrap();
        let summary: Vec<(&str, Qty)> = expanded
            .iter()
            .map(|(p, q)| (p.name_fr.as_str(), *q))
            .collect();
        assert_eq!(summary, [("Stylo", Qty(4_000)), ("Cahier", Qty(6_000))]);
        let mut weighed = ProductInput::simple("Tomate", UNIT_KG_ID, &shop.tva9, Money(12_000));
        weighed.is_weighed = true;
        let tomato = shop.product_with(weighed);
        let own = shop
            .db
            .read(move |c| expand(c, &tomato, Qty(1_250)))
            .unwrap();
        assert_eq!(own[0].1, Qty(1_250));
    }

    #[test]
    fn locations_and_low_stock() {
        let shop = shop();
        let actor = shop.owner.actor();
        let depot = LocationInput {
            code: "DEPOT".into(),
            name_fr: "Dépôt".into(),
            ..LocationInput::default()
        };
        let depot = shop
            .db
            .write(move |tx| create_location(tx, &actor, depot))
            .unwrap();
        let all = shop.db.read(locations).unwrap();
        let codes: Vec<&str> = all.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["SHOP", "DEPOT", "DAMAGED"]);
        assert_eq!(
            shop.db.read(damaged_location).unwrap().id,
            DAMAGED_LOCATION_ID
        );
        let mut input = ProductInput::simple("Sucre", UNIT_PIECE_ID, &shop.tva9, Money(9_000));
        input.min_stock = Qty(10_000);
        let sugar = shop.product_with(input);
        shop.stock_in(&sugar.id, 4_000);
        let low = shop.db.read(|c| below_minimum(c, 50)).unwrap();
        assert_eq!(
            (low.len(), low[0].on_hand, low[0].min_stock),
            (1, Qty(4_000), Qty(10_000))
        );
        assert_eq!(depot.kind, LocationKind::Storage);
    }

    #[test]
    fn a_batch_adjustment_posts_every_line_or_none() {
        let shop = crate::testkit::shop();
        let a = shop.product("Yaourt", Money(3_000), &shop.tva9);
        let b = shop.product("Fromage", Money(12_000), &shop.tva9);
        shop.stock_in(&a.id, 10_000);
        shop.stock_in(&b.id, 10_000);
        let input = |qty_b: i64| AdjustBatchInput {
            location_id: None,
            reason: AdjustReason::Expiry,
            lines: vec![
                AdjustBatchLine {
                    product_id: a.id.clone(),
                    qty_delta: Qty(-2_000),
                    unit_cost: None,
                },
                AdjustBatchLine {
                    product_id: b.id.clone(),
                    qty_delta: Qty(qty_b),
                    unit_cost: None,
                },
            ],
            note: Some("DLC dépassée".into()),
        };
        let actor = shop.owner.actor();
        let bad = input(3_000);
        assert_eq!(
            shop.db
                .write(move |tx| adjust_batch(tx, &actor, bad, 1))
                .unwrap_err(),
            AppError::validation("lines.1.qtyDelta", "qty_invalid")
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE reason = 'loss'"),
            0
        );
        let actor = shop.owner.actor();
        let good = input(-1_000);
        assert_eq!(
            shop.db
                .write(move |tx| adjust_batch(tx, &actor, good, 1))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn rebuild_repairs_a_damaged_cache() {
        let shop = shop();
        let p = shop.product("Lait", Money(11_000), &shop.tva19);
        adjust_by(&shop, &p.id, 5_000, AdjustReason::Opening, None).unwrap();
        shop.db
            .write(|tx| {
                tx.execute("UPDATE stock_levels SET qty = 999", [])?;
                Ok(())
            })
            .unwrap();
        let actor = shop.owner.actor();
        let report = shop.db.write(move |tx| rebuild_levels(tx, &actor)).unwrap();
        assert_eq!(
            report,
            RebuildReport {
                levels: 1,
                corrected: 1
            }
        );
        let id = p.id.clone();
        assert_eq!(shop.db.read(move |c| on_hand(c, &id)).unwrap(), Qty(5_000));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]
        #[test]
        fn levels_always_equal_the_sum_of_the_ledger(deltas in prop::collection::vec((0usize..2, -50_000i64..50_000), 1..40)) {
            let shop = shop();
            let a = shop.product("A", Money(100), &shop.tva19);
            let b = shop.product("B", Money(100), &shop.tva19);
            let ids = [a.id.clone(), b.id.clone()];
            let actor = shop.owner.actor();
            let moves: Vec<(String, i64)> = deltas.iter().filter(|(_, d)| *d != 0).map(|(i, d)| (ids[*i].clone(), *d)).collect();
            shop.db.write(move |tx| {
                for (product_id, delta) in &moves {
                    let m = NewMovement {
                        product_id: product_id.clone(),
                        location_id: SHOP_LOCATION_ID.into(),
                        qty_delta: Qty(*delta),
                        reason: MovementReason::Adjustment,
                        reason_code: Some(AdjustReason::Correction),
                        ref_type: None,
                        ref_id: None,
                        unit_cost: None,
                        note: None,
                    };
                    append(tx, &actor, &m, NOON)?;
                }
                Ok(())
            }).unwrap();
            for id in ids {
                let expected: i64 = deltas.iter().filter(|(i, _)| [a.id.as_str(), b.id.as_str()][*i] == id).map(|(_, d)| d).sum();
                let got = shop.db.read(move |c| level(c, &id, SHOP_LOCATION_ID)).unwrap();
                prop_assert_eq!(got, Qty(expected));
            }
            let actor = shop.owner.actor();
            let report = shop.db.write(move |tx| rebuild_levels(tx, &actor)).unwrap();
            prop_assert_eq!(report.corrected, 0);
        }
    }
}
