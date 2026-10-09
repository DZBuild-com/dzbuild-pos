//! Stock counts (STK-009/010/011/012): a full, category or product-list count of one location,
//! blind or not; lines counted by typing, by scanning (+1 pack of the code read) or from a data
//! collector file (quantities in packs of the code read). Each time a line is counted it keeps
//! the level of that moment (`expected_qty`); at the close every counted line posts
//! `counted − level when counted` as a `count` movement (FEFO when stock goes down) on top of the
//! current level, so the sales, receipts and transfers made after the line was counted survive a
//! count done during opening hours. Uncounted lines are ignored or zeroed (against the level at
//! the close). One open count per location.

use super::lots;
use super::{
    MovementReason, NewMovement, append, append_with_lot, default_location, level, location,
};
use crate::audit;
use crate::catalog::products;
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::sqlenum::sql_text_enum;
use crate::table::{Cell, ColumnKind, Table};
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CountScope {
    Full,
    Category,
    Products,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CountStatus {
    Open,
    Closed,
    Cancelled,
}

impl CountScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            CountScope::Full => "full",
            CountScope::Category => "category",
            CountScope::Products => "products",
        }
    }

    pub fn parse(s: &str) -> Option<CountScope> {
        [CountScope::Full, CountScope::Category, CountScope::Products]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

impl CountStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            CountStatus::Open => "open",
            CountStatus::Closed => "closed",
            CountStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<CountStatus> {
        [
            CountStatus::Open,
            CountStatus::Closed,
            CountStatus::Cancelled,
        ]
        .into_iter()
        .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(CountScope);
sql_text_enum!(CountStatus);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CountInput {
    pub location_id: Option<String>,
    pub scope: CountScope,
    pub category_id: Option<String>,
    pub product_ids: Vec<String>,
    pub blind: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StockCount {
    pub id: String,
    pub location_id: String,
    pub scope: CountScope,
    pub category_id: Option<String>,
    pub blind: bool,
    pub status: CountStatus,
    pub note: Option<String>,
    pub started_at: i64,
    pub closed_at: Option<i64>,
    pub user_id: String,
    pub lines: i64,
    pub counted: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CountLineView {
    pub product_id: String,
    pub reference: Option<String>,
    pub barcode: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub unit_code: String,
    pub unit_decimals: u8,
    pub expected_qty: Option<Qty>,
    pub counted_qty: Option<Qty>,
    pub variance: Option<Qty>,
    pub unit_cost: Option<Money>,
    pub variance_value: Option<Money>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CountMode {
    /// The typed quantity replaces the counted one.
    Set,
    /// Added to the counted quantity (a scan adds one pack of the code read, a collector file
    /// adds its lines).
    Add,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Uncounted {
    /// A partial (cycle) count: uncounted products keep their stock.
    Ignore,
    /// A full count: what was not counted is not there.
    Zero,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CountResult {
    pub count: StockCount,
    pub adjusted: u32,
    pub variance_value: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedCount {
    pub matched: u32,
    pub unknown: Vec<String>,
}

/// What one scan of a code adds to a count: its product and one pack of it (a carton barcode
/// counts its 12 pieces; a unit barcode, a reference or a PLU counts one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CountScan {
    pub product_id: String,
    pub pack_qty: Qty,
}

const SELECT: &str = "SELECT c.id, c.location_id, c.scope, c.category_id, c.blind, c.status, c.note, c.started_at,
        c.closed_at, c.user_id,
        (SELECT COUNT(*) FROM stock_count_lines l WHERE l.count_id = c.id),
        (SELECT COUNT(*) FROM stock_count_lines l WHERE l.count_id = c.id AND l.counted_qty IS NOT NULL)
     FROM stock_counts c";

fn map(r: &Row<'_>) -> rusqlite::Result<StockCount> {
    Ok(StockCount {
        id: r.get(0)?,
        location_id: r.get(1)?,
        scope: r.get(2)?,
        category_id: r.get(3)?,
        blind: r.get(4)?,
        status: r.get(5)?,
        note: r.get(6)?,
        started_at: r.get(7)?,
        closed_at: r.get(8)?,
        user_id: r.get(9)?,
        lines: r.get(10)?,
        counted: r.get(11)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<StockCount, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE c.id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("count"))
}

pub fn list(conn: &Connection, limit: u32) -> Result<Vec<StockCount>, AppError> {
    let mut stmt = conn.prepare_cached(&format!("{SELECT} ORDER BY c.started_at DESC LIMIT ?1"))?;
    let rows = stmt
        .query_map([limit.clamp(1, 500)], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn open(conn: &Connection, id: &str) -> Result<StockCount, AppError> {
    let count = get(conn, id)?;
    if count.status != CountStatus::Open {
        return Err(AppError::conflict("count_closed"));
    }
    Ok(count)
}

fn add_line(
    tx: &Transaction<'_>,
    count_id: &str,
    location_id: &str,
    product_id: &str,
) -> Result<(), AppError> {
    let p = products::get(tx, product_id)?;
    let expected = level(tx, product_id, location_id)?;
    tx.execute(
        "INSERT OR IGNORE INTO stock_count_lines (count_id, product_id, expected_qty, unit_cost)
         VALUES (?1, ?2, ?3, ?4)",
        params![count_id, product_id, expected, p.cost_price],
    )?;
    Ok(())
}

pub fn start(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: CountInput,
) -> Result<StockCount, AppError> {
    let location_id = match &input.location_id {
        Some(id) => {
            location(tx, id)
                .map_err(|_| AppError::validation("locationId", "stock_location_not_found"))?
                .id
        }
        None => default_location(tx)?.id,
    };
    let taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM stock_counts WHERE location_id = ?1 AND status = 'open')",
        [&location_id],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::conflict("count_open_exists"));
    }
    let product_ids: Vec<String> = match input.scope {
        CountScope::Full => {
            let mut stmt = tx.prepare(
                "SELECT id FROM products WHERE track_stock = 1 AND archived_at IS NULL ORDER BY name_fr",
            )?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
        }
        CountScope::Category => {
            let category = input
                .category_id
                .clone()
                .ok_or_else(|| AppError::validation("categoryId", "required"))?;
            let mut stmt = tx.prepare(
                "WITH RECURSIVE tree(id) AS (SELECT ?1 UNION SELECT c.id FROM categories c JOIN tree t ON c.parent_id = t.id)
                 SELECT id FROM products WHERE track_stock = 1 AND archived_at IS NULL AND category_id IN (SELECT id FROM tree)
                 ORDER BY name_fr",
            )?;
            stmt.query_map([category], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
        }
        CountScope::Products => {
            if input.product_ids.is_empty() {
                return Err(AppError::validation("productIds", "required"));
            }
            input.product_ids.clone()
        }
    };
    let id = new_id();
    tx.execute(
        "INSERT INTO stock_counts (id, location_id, scope, category_id, blind, status, note, started_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6, ?7, ?8)",
        params![
            id,
            location_id,
            input.scope,
            input.category_id,
            input.blind,
            crate::catalog::clean_opt(input.note.as_deref(), "note", 200)?,
            ctx.now,
            ctx.session.user_id
        ],
    )?;
    for product_id in &product_ids {
        add_line(tx, &id, &location_id, product_id)?;
    }
    let count = get(tx, &id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "stock.count_started",
            "stock_count",
            Some(&id),
            json!({ "locationId": location_id, "scope": input.scope, "lines": count.lines, "blind": input.blind }),
        ),
    )?;
    Ok(count)
}

/// A counted line's expected quantity is the level at the moment it was counted, the one the
/// close posts against; an uncounted line of an open count shows the live level at the count's
/// location; once closed, the level each line was posted against (written by `close`).
const LINES: &str = "SELECT l.product_id, p.reference,
        (SELECT b.barcode FROM product_barcodes b WHERE b.product_id = p.id ORDER BY b.rowid LIMIT 1),
        p.name_ar, p.name_fr, p.name_en, u.code, u.decimals,
        CASE WHEN c.status = 'open' AND l.counted_at IS NULL
             THEN COALESCE((SELECT s.qty FROM stock_levels s
                            WHERE s.product_id = l.product_id AND s.location_id = c.location_id), 0)
             ELSE l.expected_qty END,
        l.counted_qty, l.unit_cost, l.posted_delta
     FROM stock_count_lines l JOIN stock_counts c ON c.id = l.count_id
     JOIN products p ON p.id = l.product_id JOIN units u ON u.id = p.unit_id";

fn view(r: &Row<'_>, reveal_expected: bool, reveal_cost: bool) -> rusqlite::Result<CountLineView> {
    let expected: Qty = r.get(8)?;
    let counted: Option<Qty> = r.get(9)?;
    let cost: Money = r.get(10)?;
    let posted: Option<Qty> = r.get(11)?;
    // A closed line shows what reached the ledger; an open one what the close would post now.
    let variance = posted.or_else(|| counted.and_then(|c| c.checked_sub(expected)));
    Ok(CountLineView {
        product_id: r.get(0)?,
        reference: r.get(1)?,
        barcode: r.get(2)?,
        name_ar: r.get(3)?,
        name_fr: r.get(4)?,
        name_en: r.get(5)?,
        unit_code: r.get(6)?,
        unit_decimals: r.get(7)?,
        expected_qty: reveal_expected.then_some(expected),
        counted_qty: counted,
        variance: variance.filter(|_| reveal_expected),
        unit_cost: reveal_cost.then_some(cost),
        variance_value: variance
            .and_then(|v| cost.mul_qty(v))
            .filter(|_| reveal_expected && reveal_cost),
    })
}

/// The count sheet: expected quantities only when `reveal_expected` (not blind, or a manager),
/// costs only when `reveal_cost` (`reports.profit`). A counted line shows the level when it was
/// counted, so the variance approved is the one `close` posts; an uncounted line of an open count
/// the live level; a closed count shows the posted variance.
pub fn lines(
    conn: &Connection,
    id: &str,
    reveal_expected: bool,
    reveal_cost: bool,
) -> Result<Vec<CountLineView>, AppError> {
    get(conn, id)?;
    let mut stmt =
        conn.prepare_cached(&format!("{LINES} WHERE l.count_id = ?1 ORDER BY p.name_fr"))?;
    let rows = stmt
        .query_map([id], |r| view(r, reveal_expected, reveal_cost))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_counted(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    product_id: &str,
    qty: Qty,
    mode: CountMode,
) -> Result<CountLineView, AppError> {
    let count = open(tx, id)?;
    let p = products::get(tx, product_id)?;
    if !p.track_stock {
        return Err(AppError::validation("productId", "product_not_stocked"));
    }
    if qty.is_negative() || !qty.fits_decimals(p.unit_decimals) {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    if counted_by_lot(tx, id, product_id)? {
        return Err(AppError::validation("lotNumber", "count_lot_required"));
    }
    add_line(tx, id, &count.location_id, product_id)?;
    // The level at this moment: what the counted quantity is compared with at the close, so the
    // stock that moves after the shelf was counted is not undone by the count.
    let held = level(tx, product_id, &count.location_id)?;
    let sql = match mode {
        CountMode::Set => {
            "UPDATE stock_count_lines SET counted_qty = ?3, counted_at = ?4, counted_by = ?5, expected_qty = ?6 WHERE count_id = ?1 AND product_id = ?2"
        }
        CountMode::Add => {
            "UPDATE stock_count_lines SET counted_qty = COALESCE(counted_qty, 0) + ?3, counted_at = ?4, counted_by = ?5, expected_qty = ?6 WHERE count_id = ?1 AND product_id = ?2"
        }
    };
    tx.execute(
        sql,
        params![id, product_id, qty, ctx.now, ctx.session.user_id, held],
    )?;
    let reveal = !count.blind;
    tx.prepare_cached(&format!(
        "{LINES} WHERE l.count_id = ?1 AND l.product_id = ?2"
    ))?
    .query_row(params![id, product_id], |r| view(r, reveal, false))
    .map_err(AppError::from)
}

/// The product has lot lines in this count (STK-004): it is counted lot by lot only.
fn counted_by_lot(conn: &Connection, id: &str, product_id: &str) -> Result<bool, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM stock_count_lot_lines WHERE count_id = ?1 AND product_id = ?2)",
        )?
        .query_row(params![id, product_id], |r| r.get(0))?)
}

/// A lot's level at a location (0 where it never was).
fn lot_level(conn: &Connection, lot_id: &str, location_id: &str) -> Result<Qty, AppError> {
    let qty: Option<Qty> = conn
        .prepare_cached("SELECT qty FROM lot_levels WHERE lot_id = ?1 AND location_id = ?2")?
        .query_row(params![lot_id, location_id], |r| r.get(0))
        .optional()?;
    Ok(qty.unwrap_or(Qty::ZERO))
}

/// What the product's lots hold at a location; the rest of its level is stock without a lot.
fn lots_level(conn: &Connection, product_id: &str, location_id: &str) -> Result<Qty, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT COALESCE(SUM(ll.qty), 0) FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
             WHERE l.product_id = ?1 AND ll.location_id = ?2",
        )?
        .query_row(params![product_id, location_id], |r| r.get(0))?)
}

/// One lot of a count (STK-004): a lot line, or, while the count is open, a lot in stock at the
/// count's location that nobody counted yet (`counted_qty` empty). Expected quantities only when
/// `reveal_expected`, as on the product lines; a closed lot line shows what it posted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CountLotLineView {
    pub product_id: String,
    pub lot_id: String,
    pub lot_number: String,
    pub expiry_date: Option<String>,
    pub expected_qty: Option<Qty>,
    pub counted_qty: Option<Qty>,
    pub variance: Option<Qty>,
}

/// STK-004: counts one lot of a lot-tracked product (`Set` replaces, `Add` adds, as
/// [`set_counted`]). A lot number the books do not know is a lot found on the shelf: it is
/// created, without an expiry date. The lot line keeps the lot's level of this moment, which the
/// close posts against; the product's line holds the sum of its lot lines, compared with the
/// counted lots at their level when counted plus the rest of the books (lots nobody counted,
/// stock without a lot) at its level now: what the close takes to zero. Returns the product line.
pub fn set_counted_lot(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    product_id: &str,
    lot_number: &str,
    qty: Qty,
    mode: CountMode,
) -> Result<CountLineView, AppError> {
    let count = open(tx, id)?;
    let p = products::get(tx, product_id)?;
    if !p.track_stock {
        return Err(AppError::validation("productId", "product_not_stocked"));
    }
    if !crate::catalog::extras::get(tx, product_id)?.track_lots {
        return Err(AppError::validation("lotNumber", "product_not_lot_tracked"));
    }
    if qty.is_negative() || !qty.fits_decimals(p.unit_decimals) {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    let lot = lots::get_or_create(
        tx,
        product_id,
        &lots::LotInput {
            lot_number: lot_number.to_owned(),
            expiry_date: None,
        },
        None,
        ctx.now,
    )?;
    add_line(tx, id, &count.location_id, product_id)?;
    let held = lot_level(tx, &lot.id, &count.location_id)?;
    let sql = match mode {
        CountMode::Set => {
            "INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at, counted_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (count_id, lot_id) DO UPDATE SET expected_qty = excluded.expected_qty,
                 counted_qty = excluded.counted_qty, counted_at = excluded.counted_at, counted_by = excluded.counted_by"
        }
        CountMode::Add => {
            "INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at, counted_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (count_id, lot_id) DO UPDATE SET expected_qty = excluded.expected_qty,
                 counted_qty = counted_qty + excluded.counted_qty, counted_at = excluded.counted_at,
                 counted_by = excluded.counted_by"
        }
    };
    tx.execute(
        sql,
        params![
            id,
            product_id,
            lot.id,
            held,
            qty,
            ctx.now,
            ctx.session.user_id
        ],
    )?;
    let (counted, lots_expected, counted_lots_now): (Qty, Qty, Qty) = tx.query_row(
        "SELECT COALESCE(SUM(k.counted_qty), 0), COALESCE(SUM(k.expected_qty), 0), COALESCE(SUM(ll.qty), 0)
         FROM stock_count_lot_lines k
         LEFT JOIN lot_levels ll ON ll.lot_id = k.lot_id AND ll.location_id = ?3
         WHERE k.count_id = ?1 AND k.product_id = ?2",
        params![id, product_id, count.location_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let invalid = || AppError::validation("qty", "qty_invalid");
    let rest = level(tx, product_id, &count.location_id)?
        .checked_sub(counted_lots_now)
        .ok_or_else(invalid)?;
    let expected = lots_expected.checked_add(rest).ok_or_else(invalid)?;
    tx.execute(
        "UPDATE stock_count_lines SET counted_qty = ?3, counted_at = ?4, counted_by = ?5, expected_qty = ?6
         WHERE count_id = ?1 AND product_id = ?2",
        params![
            id,
            product_id,
            counted,
            ctx.now,
            ctx.session.user_id,
            expected
        ],
    )?;
    let reveal = !count.blind;
    tx.prepare_cached(&format!(
        "{LINES} WHERE l.count_id = ?1 AND l.product_id = ?2"
    ))?
    .query_row(params![id, product_id], |r| view(r, reveal, false))
    .map_err(AppError::from)
}

/// The lots of a count (STK-004), product by product (as [`lines`]) then first expiry first:
/// its lot lines and, while it is open, the lots of its lot-tracked products still in stock at
/// its location that nobody counted yet, so the counter knows what to look for.
pub fn lot_lines(
    conn: &Connection,
    id: &str,
    reveal_expected: bool,
) -> Result<Vec<CountLotLineView>, AppError> {
    get(conn, id)?;
    let mut stmt = conn.prepare_cached(
        "SELECT x.product_id, x.lot_id, l.lot_number, l.expiry_date, x.expected_qty, x.counted_qty, x.posted_delta
         FROM (SELECT k.product_id, k.lot_id, k.expected_qty, k.counted_qty, k.posted_delta
               FROM stock_count_lot_lines k WHERE k.count_id = ?1
               UNION ALL
               SELECT s.product_id, ll.lot_id, ll.qty, NULL, NULL
               FROM stock_counts c
               JOIN stock_count_lines s ON s.count_id = c.id
               JOIN product_extras e ON e.product_id = s.product_id AND e.track_lots = 1
               JOIN lots lt ON lt.product_id = s.product_id
               JOIN lot_levels ll ON ll.lot_id = lt.id AND ll.location_id = c.location_id
               WHERE c.id = ?1 AND c.status = 'open' AND ll.qty <> 0
                 AND NOT EXISTS (SELECT 1 FROM stock_count_lot_lines k
                                 WHERE k.count_id = c.id AND k.lot_id = ll.lot_id)) x
         JOIN lots l ON l.id = x.lot_id JOIN products p ON p.id = x.product_id
         ORDER BY p.name_fr, x.product_id, l.expiry_date IS NULL, l.expiry_date, l.lot_number",
    )?;
    let rows = stmt
        .query_map([id], |r| {
            let expected: Qty = r.get(4)?;
            let counted: Option<Qty> = r.get(5)?;
            let posted: Option<Qty> = r.get(6)?;
            let variance = posted.or_else(|| counted.and_then(|c| c.checked_sub(expected)));
            Ok(CountLotLineView {
                product_id: r.get(0)?,
                lot_id: r.get(1)?,
                lot_number: r.get(2)?,
                expiry_date: r.get(3)?,
                expected_qty: reveal_expected.then_some(expected),
                counted_qty: counted,
                variance: variance.filter(|_| reveal_expected),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The product line was counted lot by lot (STK-004): the lots of the books nobody found are
/// counted 0 (a line each, against their level now); every lot line posts
/// `counted − level when counted` in its lot, never taking a lot below zero: a lot the sales
/// emptied since it was counted passes the rest of its difference to the other lots, first
/// expiry first (`lots::take`). The stock the books held without a lot was not on the shelf and
/// leaves too. Returns what the product's lines posted in all.
fn close_by_lot(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    count: &StockCount,
    product_id: &str,
    cost: Money,
) -> Result<Qty, AppError> {
    let (id, location) = (count.id.as_str(), count.location_id.as_str());
    tx.execute(
        "INSERT INTO stock_count_lot_lines (count_id, product_id, lot_id, expected_qty, counted_qty, counted_at, counted_by)
         SELECT ?1, ?2, ll.lot_id, ll.qty, 0, ?4, ?5
         FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
         WHERE l.product_id = ?2 AND ll.location_id = ?3 AND ll.qty <> 0
           AND NOT EXISTS (SELECT 1 FROM stock_count_lot_lines k WHERE k.count_id = ?1 AND k.lot_id = ll.lot_id)",
        params![id, product_id, location, ctx.now, ctx.session.user_id],
    )?;
    let actor = ctx.actor();
    let invalid = || AppError::validation("qty", "qty_invalid");
    let movement = |qty: Qty| NewMovement {
        product_id: product_id.to_owned(),
        location_id: location.to_owned(),
        qty_delta: qty,
        reason: MovementReason::Count,
        reason_code: None,
        ref_type: Some("stock_count".into()),
        ref_id: Some(id.to_owned()),
        unit_cost: Some(cost),
        note: None,
    };
    let unlotted = level(tx, product_id, location)?
        .checked_sub(lots_level(tx, product_id, location)?)
        .ok_or_else(invalid)?;
    let mut posted = Qty::ZERO;
    if unlotted != Qty::ZERO {
        let out = unlotted.checked_neg().ok_or_else(invalid)?;
        append(tx, &actor, &movement(out), ctx.now)?;
        posted = out;
    }
    let lines: Vec<(String, Qty, Qty)> = {
        let mut stmt = tx.prepare(
            "SELECT k.lot_id, k.expected_qty, k.counted_qty
             FROM stock_count_lot_lines k JOIN lots l ON l.id = k.lot_id
             WHERE k.count_id = ?1 AND k.product_id = ?2
             ORDER BY l.expiry_date IS NULL, l.expiry_date, l.created_at, l.lot_number",
        )?;
        stmt.query_map(params![id, product_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let mut short = Qty::ZERO;
    for (lot_id, expected, counted) in lines {
        let delta = counted.checked_sub(expected).ok_or_else(invalid)?;
        tx.execute(
            "UPDATE stock_count_lot_lines SET posted_delta = ?3 WHERE count_id = ?1 AND lot_id = ?2",
            params![id, lot_id, delta],
        )?;
        if delta == Qty::ZERO {
            continue;
        }
        posted = posted.checked_add(delta).ok_or_else(invalid)?;
        let here = if delta.is_negative() {
            let held = lot_level(tx, &lot_id, location)?.max(Qty::ZERO);
            delta.max(held.checked_neg().ok_or_else(invalid)?)
        } else {
            delta
        };
        if here != Qty::ZERO {
            append_with_lot(tx, &actor, &movement(here), Some(&lot_id), ctx.now)?;
        }
        short = short
            .checked_add(delta.checked_sub(here).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
    }
    if short.is_negative() {
        lots::take(tx, &actor, &movement(short), &ctx.business_day(), ctx.now)?;
    }
    Ok(posted)
}

/// The product a scanned or typed code names (barcode, reference, PLU: M1a's scan lookup) and
/// the pack one scan adds (`set_counted(…, scan.pack_qty, CountMode::Add)`).
pub fn line_for_code(conn: &Connection, code: &str) -> Result<Option<CountScan>, AppError> {
    Ok(
        crate::catalog::search::find_by_code(conn, code)?.map(|hit| CountScan {
            product_id: hit.product.id,
            pack_qty: hit.pack_qty,
        }),
    )
}

/// `qty` packs of `pack_qty` each, exactly, in milli-units of the product's unit: `None` when it
/// overflows or falls between two milli-units.
fn packs(qty: Qty, pack_qty: Qty) -> Option<Qty> {
    let exact = i128::from(qty.0) * i128::from(pack_qty.0);
    let scale = i128::from(Qty::ONE.0);
    if exact % scale != 0 {
        return None;
    }
    i64::try_from(exact / scale).ok().map(Qty)
}

/// STK-012: `barcode;qty` lines of a data collector, added to the counted quantities. The
/// quantity is in packs of the code read (a carton barcode read once counts its 12 pieces), and a
/// row that does not give a whole number of the unit is refused (`rows.{i}.qty`). A code naming
/// no product, or a scale label the shop cannot read (unknown PLU, product not sold by weight, no
/// price per kg), is listed in `unknown`; a label that cannot be read is refused (`rows.{i}.code`).
pub fn import_lines(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    rows: &[(String, Qty)],
) -> Result<ImportedCount, AppError> {
    open(tx, id)?;
    let mut matched = 0_u32;
    let mut unknown = Vec::new();
    for (i, (code, qty)) in rows.iter().enumerate() {
        let found = match crate::catalog::search::find_by_code(tx, code) {
            Err(AppError::Rejected { code: refusal, .. })
                if matches!(
                    refusal.as_str(),
                    "scale_plu_unknown" | "scale_product_not_weighed" | "scale_price_missing"
                ) =>
            {
                None
            }
            Err(AppError::Validation { field, code }) => {
                return Err(AppError::validation(&format!("rows.{i}.{field}"), &code));
            }
            other => other?,
        };
        let Some(hit) = found else {
            unknown.push(code.clone());
            continue;
        };
        let units = packs(*qty, hit.pack_qty)
            .ok_or_else(|| AppError::validation(&format!("rows.{i}.qty"), "qty_invalid"))?;
        set_counted(tx, ctx, id, &hit.product.id, units, CountMode::Add).map_err(|e| match e {
            AppError::Validation { field, code } => {
                AppError::validation(&format!("rows.{i}.{field}"), &code)
            }
            other => other,
        })?;
        matched += 1;
    }
    Ok(ImportedCount { matched, unknown })
}

pub fn close(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    uncounted: Uncounted,
) -> Result<CountResult, AppError> {
    let count = open(tx, id)?;
    let rows: Vec<(String, Option<Qty>, Qty, Money)> = {
        let mut stmt = tx.prepare(
            "SELECT product_id, counted_qty, expected_qty, unit_cost FROM stock_count_lines WHERE count_id = ?1",
        )?;
        stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let by_lot: std::collections::BTreeSet<String> = {
        let mut stmt = tx
            .prepare("SELECT DISTINCT product_id FROM stock_count_lot_lines WHERE count_id = ?1")?;
        stmt.query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    let actor = ctx.actor();
    let today = ctx.business_day();
    let mut adjusted = 0_u32;
    let mut value = Money::ZERO;
    for (product_id, counted, when_counted, cost) in rows {
        if by_lot.contains(&product_id) {
            let delta = close_by_lot(tx, ctx, &count, &product_id, cost)?;
            let counted = counted.unwrap_or(Qty::ZERO);
            let expected = counted
                .checked_sub(delta)
                .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?;
            tx.execute(
                "UPDATE stock_count_lines SET posted_delta = ?3, counted_qty = ?4, expected_qty = ?5
                 WHERE count_id = ?1 AND product_id = ?2",
                params![id, product_id, delta, counted, expected],
            )?;
            if delta != Qty::ZERO {
                value = value
                    .checked_add(cost.mul_qty(delta).unwrap_or(Money::ZERO))
                    .unwrap_or(value);
                adjusted += 1;
            }
            continue;
        }
        // A counted line is compared with the level when it was counted (`set_counted` kept it);
        // an uncounted line zeroed by a full count with the level now.
        let (counted, expected) = match (counted, uncounted) {
            (Some(c), _) => (c, when_counted),
            (None, Uncounted::Zero) => (Qty::ZERO, level(tx, &product_id, &count.location_id)?),
            (None, Uncounted::Ignore) => continue,
        };
        let delta = counted
            .checked_sub(expected)
            .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?;
        // The line keeps the level it was posted against (while the count is still open).
        tx.execute(
            "UPDATE stock_count_lines SET posted_delta = ?3, counted_qty = ?4, expected_qty = ?5
             WHERE count_id = ?1 AND product_id = ?2",
            params![id, product_id, delta, counted, expected],
        )?;
        if delta == Qty::ZERO {
            continue;
        }
        let movement = NewMovement {
            product_id: product_id.clone(),
            location_id: count.location_id.clone(),
            qty_delta: delta,
            reason: MovementReason::Count,
            reason_code: None,
            ref_type: Some("stock_count".into()),
            ref_id: Some(id.to_owned()),
            unit_cost: Some(cost),
            note: None,
        };
        if delta.is_negative() {
            lots::take(tx, &actor, &movement, &today, ctx.now)?;
        } else {
            append(tx, &actor, &movement, ctx.now)?;
        }
        value = value
            .checked_add(cost.mul_qty(delta).unwrap_or(Money::ZERO))
            .unwrap_or(value);
        adjusted += 1;
    }
    tx.execute(
        "UPDATE stock_counts SET status = 'closed', closed_at = ?2, closed_by = ?3 WHERE id = ?1",
        params![id, ctx.now, ctx.session.user_id],
    )?;
    audit::append(
        tx,
        &actor.event(
            "stock.count_closed",
            "stock_count",
            Some(id),
            json!({ "adjusted": adjusted, "varianceValue": value, "uncounted": uncounted, "approvals": ctx.approvers() }),
        ),
    )?;
    Ok(CountResult {
        count: get(tx, id)?,
        adjusted,
        variance_value: value,
    })
}

pub fn cancel(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<StockCount, AppError> {
    open(tx, id)?;
    tx.execute(
        "UPDATE stock_counts SET status = 'cancelled', closed_at = ?2, closed_by = ?3 WHERE id = ?1",
        params![id, ctx.now, ctx.session.user_id],
    )?;
    audit::append(
        tx,
        &ctx.actor()
            .event("stock.count_cancelled", "stock_count", Some(id), json!({})),
    )?;
    get(tx, id)
}

/// The printable count sheet (STK-010) as a table: blank counted column unless counted. A
/// lot-tracked product's line is followed by one row per lot (STK-004, `lot_lines`): the lots
/// counted and, while the count is open, those still in stock to look for.
pub fn sheet(
    conn: &Connection,
    id: &str,
    lang: Lang,
    reveal_expected: bool,
) -> Result<Table, AppError> {
    let mut table = Table::new(&[
        ("reference", ColumnKind::Code),
        ("barcode", ColumnKind::Code),
        ("product", ColumnKind::Text),
        ("lot", ColumnKind::Code),
        ("unit", ColumnKind::Text),
        ("expected", ColumnKind::Qty),
        ("counted", ColumnKind::Qty),
    ]);
    let mut lots_of: std::collections::BTreeMap<String, Vec<CountLotLineView>> =
        std::collections::BTreeMap::new();
    for lot in lot_lines(conn, id, reveal_expected)? {
        lots_of.entry(lot.product_id.clone()).or_default().push(lot);
    }
    for l in lines(conn, id, reveal_expected, false)? {
        let name = match lang {
            Lang::Ar if !l.name_ar.is_empty() => l.name_ar.clone(),
            Lang::En if !l.name_en.is_empty() => l.name_en.clone(),
            _ if !l.name_fr.is_empty() => l.name_fr.clone(),
            _ => l.name_ar.clone(),
        };
        table.push(vec![
            Cell::opt_text(l.reference.clone()),
            Cell::opt_text(l.barcode.clone()),
            Cell::text(name.clone()),
            Cell::NULL,
            Cell::text(l.unit_code.clone()),
            l.expected_qty.map_or(Cell::NULL, Cell::qty),
            l.counted_qty.map_or(Cell::NULL, Cell::qty),
        ])?;
        for lot in lots_of.remove(&l.product_id).unwrap_or_default() {
            table.push(vec![
                Cell::opt_text(l.reference.clone()),
                Cell::NULL,
                Cell::text(name.clone()),
                Cell::text(lot.lot_number),
                Cell::text(l.unit_code.clone()),
                lot.expected_qty.map_or(Cell::NULL, Cell::qty),
                lot.counted_qty.map_or(Cell::NULL, Cell::qty),
            ])?;
        }
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{Shop, shop};

    /// A product costing 100 DA HT, with `qty` milli-units in the shop.
    fn stocked(shop: &Shop, name: &str, qty: i64) -> String {
        let mut input = crate::catalog::products::ProductInput::simple(
            name,
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(15_000),
        );
        input.cost_price = Money(10_000);
        let p = shop.product_with(input);
        shop.stock_in(&p.id, qty);
        p.id
    }

    fn full() -> CountInput {
        CountInput {
            location_id: None,
            scope: CountScope::Full,
            category_id: None,
            product_ids: Vec::new(),
            blind: true,
            note: None,
        }
    }

    #[test]
    fn a_count_posts_the_difference_with_the_level_when_counted() {
        let shop = shop();
        let a = stocked(&shop, "Café Bonal", 10_000);
        let b = stocked(&shop, "Thé Chiraz", 5_000);
        let c = stocked(&shop, "Sucre", 3_000);
        let ctx = shop.owner_till();
        let count = shop.db.write(move |tx| start(tx, &ctx, full())).unwrap();
        assert_eq!((count.lines, count.status), (3, CountStatus::Open));
        let ctx = shop.owner_till();
        let other = shop
            .db
            .write(move |tx| start(tx, &ctx, full()))
            .unwrap_err();
        assert_eq!(other, AppError::conflict("count_open_exists"));
        let set = |product: &str, qty: i64, mode: CountMode| {
            let (ctx, id, product) = (shop.owner_till(), count.id.clone(), product.to_owned());
            shop.db
                .write(move |tx| set_counted(tx, &ctx, &id, &product, Qty(qty), mode))
                .unwrap()
        };
        set(&a, 8_000, CountMode::Set);
        set(&b, 1_000, CountMode::Add);
        set(&b, 5_000, CountMode::Add);
        // A receipt after the café was counted: the count posts against the level when counted,
        // so the piece received later is still there after the close.
        shop.stock_in(&a, 1_000);
        let blind = shop
            .db
            .read({
                let id = count.id.clone();
                move |c| lines(c, &id, false, false)
            })
            .unwrap();
        assert!(
            blind
                .iter()
                .all(|l| l.expected_qty.is_none() && l.unit_cost.is_none()),
            "blind count"
        );
        let ctx = shop.owner_till();
        let id = count.id.clone();
        let done = shop
            .db
            .write(move |tx| close(tx, &ctx, &id, Uncounted::Ignore))
            .unwrap();
        assert_eq!(
            done.adjusted, 2,
            "café −2 (8 counted, 10 held when counted), thé +1; sucre not counted"
        );
        assert_eq!(
            done.variance_value,
            Money(-10_000),
            "−2 + 1 pieces at the 100 DA cost"
        );
        let level = |p: &str| {
            let p = p.to_owned();
            shop.db.read(move |c| crate::stock::on_hand(c, &p)).unwrap()
        };
        assert_eq!(
            (level(&a), level(&b), level(&c)),
            (Qty(9_000), Qty(6_000), Qty(3_000)),
            "the 8 counted and the piece received after the count"
        );
        let ctx = shop.owner_till();
        let id = count.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| set_counted(tx, &ctx, &id, &a, Qty(1), CountMode::Set))
                .unwrap_err(),
            AppError::conflict("count_closed")
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE reason = 'count'"),
            2
        );
    }

    /// The usual cycle count of a shop that never closes: the shelf is counted, then the till
    /// sells from it before the manager closes the count. The sale is not undone.
    #[test]
    fn a_sale_after_the_shelf_was_counted_survives_the_close() {
        use crate::payments::PaymentInput;
        use crate::sales::cart::{Cart, CartLine};
        use crate::sales::sale::{SaleInput, create_sale};
        let shop = shop();
        let cafe = stocked(&shop, "Café Bonal", 10_000);
        let ctx = shop.owner_till();
        let count = shop.db.write(move |tx| start(tx, &ctx, full())).unwrap();
        let (ctx, id, product) = (shop.owner_till(), count.id.clone(), cafe.clone());
        shop.db
            .write(move |tx| set_counted(tx, &ctx, &id, &product, Qty(8_000), CountMode::Set))
            .unwrap();
        shop.open_till(0);
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&cafe, Qty(1_000))]);
        let ctx = shop.till();
        shop.db
            .write(move |tx| {
                create_sale(
                    tx,
                    &ctx,
                    SaleInput {
                        cart,
                        payments: vec![PaymentInput::cash(Money(15_000))],
                    },
                )
            })
            .unwrap();
        let (ctx, id) = (shop.owner_till(), count.id.clone());
        let done = shop
            .db
            .write(move |tx| close(tx, &ctx, &id, Uncounted::Ignore))
            .unwrap();
        assert_eq!(
            done.variance_value,
            Money(-20_000),
            "8 counted of the 10 held then"
        );
        let product = cafe.clone();
        assert_eq!(
            shop.db
                .read(move |c| crate::stock::on_hand(c, &product))
                .unwrap(),
            Qty(7_000),
            "the 8 counted less the one sold after the count"
        );
        let posted: i64 =
            shop.count("SELECT SUM(qty_delta) FROM stock_movements WHERE reason = 'count'");
        assert_eq!(posted, -2_000, "no false loss for the piece sold");
    }

    #[test]
    fn uncounted_lines_can_be_zeroed_and_collector_files_add_up() {
        let shop = shop();
        let mut input = crate::catalog::products::ProductInput::simple(
            "Savon",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(12_000),
        );
        input.barcodes = vec![crate::catalog::products::Barcode {
            barcode: "6130001000014".into(),
            pack_qty: Qty(1_000),
            label: None,
        }];
        let soap = shop.product_with(input);
        shop.stock_in(&soap.id, 7_000);
        let lost = stocked(&shop, "Stylos", 4_000);
        let ctx = shop.owner_till();
        let count = shop.db.write(move |tx| start(tx, &ctx, full())).unwrap();
        let rows = vec![
            ("6130001000014".to_owned(), Qty(2_000)),
            ("6130001000014".to_owned(), Qty(3_000)),
            ("999".to_owned(), Qty(1_000)),
        ];
        let ctx = shop.owner_till();
        let id = count.id.clone();
        let imported = shop
            .db
            .write(move |tx| import_lines(tx, &ctx, &id, &rows))
            .unwrap();
        assert_eq!(
            (imported.matched, imported.unknown),
            (2, vec!["999".to_owned()])
        );
        let ctx = shop.owner_till();
        let id = count.id.clone();
        shop.db
            .write(move |tx| close(tx, &ctx, &id, Uncounted::Zero))
            .unwrap();
        assert_eq!(
            shop.db
                .read(move |c| crate::stock::on_hand(c, &soap.id))
                .unwrap(),
            Qty(5_000)
        );
        assert_eq!(
            shop.db
                .read(move |c| crate::stock::on_hand(c, &lost))
                .unwrap(),
            Qty::ZERO
        );
    }

    #[test]
    fn the_reconciliation_shows_what_the_close_posts() {
        let shop = shop();
        let a = stocked(&shop, "Café Bonal", 10_000);
        let b = stocked(&shop, "Thé Chiraz", 5_000);
        stocked(&shop, "Sucre", 3_000);
        let ctx = shop.owner_till();
        let count = shop.db.write(move |tx| start(tx, &ctx, full())).unwrap();
        let set = |product: &str, qty: i64| {
            let (ctx, id, product) = (shop.owner_till(), count.id.clone(), product.to_owned());
            shop.db
                .write(move |tx| set_counted(tx, &ctx, &id, &product, Qty(qty), CountMode::Set))
                .unwrap()
        };
        set(&a, 8_000);
        set(&b, 6_000);
        // Stock moves after the café was counted: the manager approves against the level of
        // that moment, the variance the close posts.
        shop.stock_in(&a, 1_000);
        let managed = || {
            let id = count.id.clone();
            shop.db.read(move |c| lines(c, &id, true, true)).unwrap()
        };
        let line_of = |rows: &[CountLineView], product: &str| {
            rows.iter()
                .find(|l| l.product_id == product)
                .cloned()
                .unwrap()
        };
        let open = managed();
        let cafe = line_of(&open, &a);
        assert_eq!(
            (cafe.expected_qty, cafe.variance, cafe.variance_value),
            (Some(Qty(10_000)), Some(Qty(-2_000)), Some(Money(-20_000))),
            "8 counted against the 10 held when counted"
        );
        let sucre = open.iter().find(|l| l.counted_qty.is_none()).unwrap();
        assert_eq!(
            sucre.expected_qty,
            Some(Qty(3_000)),
            "uncounted: the live level"
        );
        let total = |rows: &[CountLineView]| {
            Money::checked_sum(rows.iter().filter_map(|l| l.variance_value)).unwrap()
        };
        assert_eq!(total(&open), Money(-10_000));
        let ctx = shop.owner_till();
        let id = count.id.clone();
        let done = shop
            .db
            .write(move |tx| close(tx, &ctx, &id, Uncounted::Ignore))
            .unwrap();
        let closed = managed();
        assert_eq!(total(&closed), done.variance_value);
        assert_eq!(done.variance_value, Money(-10_000));
        for l in &closed {
            let (id, product) = (count.id.clone(), l.product_id.clone());
            let posted: i64 = shop
                .db
                .read(move |c| {
                    Ok(c.query_row(
                        "SELECT COALESCE(SUM(qty_delta), 0) FROM stock_movements
                         WHERE ref_type = 'stock_count' AND ref_id = ?1 AND product_id = ?2",
                        params![id, product],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            assert_eq!(
                l.variance.map_or(0, |v| v.0),
                posted,
                "{} shows what reached the ledger",
                l.name_fr
            );
        }
        assert_eq!(line_of(&closed, &a).expected_qty, Some(Qty(10_000)));
        // Later stock moves do not change a closed count.
        shop.stock_in(&a, 5_000);
        assert_eq!(managed(), closed);
    }

    #[test]
    fn a_carton_barcode_counts_its_pieces() {
        let shop = shop();
        let mut input = crate::catalog::products::ProductInput::simple(
            "Eau Ifri 1,5 L",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva9,
            Money(5_000),
        );
        input.barcodes = vec![
            crate::catalog::products::Barcode {
                barcode: "6130002000011".into(),
                pack_qty: Qty(1_000),
                label: None,
            },
            crate::catalog::products::Barcode {
                barcode: "6130002000028".into(),
                pack_qty: Qty(12_000),
                label: Some("Pack 12".into()),
            },
        ];
        let water = shop.product_with(input);
        shop.stock_in(&water.id, 30_000);
        let ctx = shop.owner_till();
        let count = shop.db.write(move |tx| start(tx, &ctx, full())).unwrap();
        // One scan of the carton adds one pack: 12 pieces.
        let scan = shop
            .db
            .read(|c| line_for_code(c, "6130002000028"))
            .unwrap()
            .unwrap();
        assert_eq!(
            (scan.product_id.as_str(), scan.pack_qty),
            (water.id.as_str(), Qty(12_000))
        );
        let (ctx, id) = (shop.owner_till(), count.id.clone());
        shop.db
            .write(move |tx| {
                set_counted(
                    tx,
                    &ctx,
                    &id,
                    &scan.product_id,
                    scan.pack_qty,
                    CountMode::Add,
                )
            })
            .unwrap();
        let import = |rows: Vec<(&str, i64)>| {
            let rows: Vec<(String, Qty)> = rows
                .into_iter()
                .map(|(code, qty)| (code.to_owned(), Qty(qty)))
                .collect();
            let (ctx, id) = (shop.owner_till(), count.id.clone());
            shop.db.write(move |tx| import_lines(tx, &ctx, &id, &rows))
        };
        // A collector file: one carton and five bottles, then half a carton.
        let imported = import(vec![("6130002000028", 1_000), ("6130002000011", 5_000)]).unwrap();
        assert_eq!(imported.matched, 2);
        import(vec![("6130002000028", 500)]).unwrap();
        let counted = || {
            let id = count.id.clone();
            shop.db.read(move |c| lines(c, &id, true, false)).unwrap()[0].counted_qty
        };
        assert_eq!(counted(), Some(Qty(35_000)), "12 + 12 + 5 + 6 pieces");
        // A third of a carton is not a whole number of pieces; a huge one does not fit.
        assert_eq!(
            import(vec![("6130002000011", 1_000), ("6130002000028", 333)]).unwrap_err(),
            AppError::validation("rows.1.qty", "qty_invalid")
        );
        assert_eq!(
            import(vec![("6130002000028", i64::MAX)]).unwrap_err(),
            AppError::validation("rows.0.qty", "qty_invalid")
        );
        assert_eq!(counted(), Some(Qty(35_000)), "a refused file adds nothing");
        let (ctx, id) = (shop.owner_till(), count.id.clone());
        shop.db
            .write(move |tx| close(tx, &ctx, &id, Uncounted::Ignore))
            .unwrap();
        assert_eq!(
            shop.db
                .read(move |c| crate::stock::on_hand(c, &water.id))
                .unwrap(),
            Qty(35_000)
        );
    }

    #[test]
    fn collector_files_list_the_scale_codes_they_cannot_count() {
        use crate::catalog::products::{Barcode, ProductInput};
        let ean = |body: &str| {
            format!(
                "{body}{}",
                crate::catalog::codes::ean13_check_digit(body).unwrap()
            )
        };
        let shop = shop();
        let mut soap = ProductInput::simple(
            "Savon",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(12_000),
        );
        soap.barcodes = vec![Barcode {
            barcode: "6130001000014".into(),
            pack_qty: Qty::ONE,
            label: None,
        }];
        shop.product_with(soap);
        let mut eggs = ProductInput::simple(
            "Œufs x30",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva9,
            Money(90_000),
        );
        eggs.plu = Some(10013);
        shop.product_with(eggs);
        let mut lentils = ProductInput::simple(
            "Lentilles",
            crate::catalog::UNIT_KG_ID,
            &shop.tva9,
            Money(30_000),
        );
        lentils.is_weighed = true;
        lentils.plu = Some(555);
        shop.product_with(lentils);
        let ctx = shop.owner_till();
        let count = shop.db.write(move |tx| start(tx, &ctx, full())).unwrap();
        let import = |rows: Vec<(String, i64)>| {
            let rows: Vec<(String, Qty)> = rows
                .into_iter()
                .map(|(code, qty)| (code, Qty(qty)))
                .collect();
            let (ctx, id) = (shop.owner_till(), count.id.clone());
            shop.db.write(move |tx| import_lines(tx, &ctx, &id, &rows))
        };
        // A scale label of a PLU nobody has, and one of a product not sold by weight.
        let (no_plu, not_weighed) = (ean("211001201750"), ean("211001301750"));
        let imported = import(vec![
            (no_plu.clone(), 1_000),
            ("6130001000014".into(), 2_000),
            (not_weighed.clone(), 1_000),
        ])
        .unwrap();
        assert_eq!(
            (imported.matched, imported.unknown),
            (1, vec![no_plu, not_weighed])
        );
        // A label that weighs nothing is a bad row, named by its index.
        assert_eq!(
            import(vec![
                ("6130001000014".into(), 1_000),
                (ean("210055500000"), 1_000),
            ])
            .unwrap_err(),
            AppError::validation("rows.1.code", "value_invalid")
        );
    }
}
