//! Stock reports: levels, valuation, low stock, expiry, the movement log and count variances.
//! "As of" reports read the stock as it stood at the end of the `to` business day: the cached
//! levels (the movements' sums, kept by the triggers: M1a `stock::rebuild_levels`, Task 8
//! `lot_levels`) minus the movements made after that day, so a past day reads the ledger as it
//! stood then and today costs nothing more. Quantities are those of `to`; the prices and
//! minimums they are valued and compared at are today's.

use super::{Built, ChartKind, Scope, add, day_of, ints, name_of, rows, value_of, value_sql};
use crate::error::AppError;
use crate::table::{Cell, ColumnKind as K, Table, TagDomain};
use chrono::NaiveDate;
use rusqlite::{Connection, ToSql};

/// The cache `cache` (`stock_levels` keyed by `product_id`, `lot_levels` by `lot_id`) as it stood
/// at the end of the `:day_to` business day (Algiers): same key, `location_id` and `qty` columns.
fn as_of(cache: &str, key: &str) -> String {
    format!(
        "(SELECT c.{key}, c.location_id, c.qty - COALESCE(later.qty, 0) AS qty
          FROM {cache} c
          LEFT JOIN (SELECT {key}, location_id, SUM(qty_delta) AS qty FROM stock_movements
                     WHERE {key} IS NOT NULL
                       AND created_at >= unixepoch(:day_to, '+1 day') * 1000 - {offset}
                     GROUP BY {key}, location_id) later
            ON later.{key} = c.{key} AND later.location_id = c.location_id)",
        offset = crate::time::ALGIERS_OFFSET_MS,
    )
}

/// Every non-zero level: the stock an archived (or no longer tracked) product still holds is
/// listed and valued too, flagged `archived`, never dropped (it would read as hidden stock).
pub(super) fn levels(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let (name, category, place) = (
        name_of("p", s.lang),
        name_of("c", s.lang),
        name_of("loc", s.lang),
    );
    let (cost_value, retail_value) = (
        value_sql("sl.qty", "p.cost_price"),
        value_sql("sl.qty", "p.sell_price"),
    );
    let levels = as_of("stock_levels", "product_id");
    let sql = format!(
        "SELECT p.reference, {name}, {category}, {place}, u.code, sl.qty,
                COALESCE(lm.min_qty, CASE WHEN loc.is_default = 1 AND p.min_stock > 0 THEN p.min_stock END),
                p.cost_price, {cost_value}, p.sell_price, {retail_value},
                p.archived_at IS NOT NULL
         FROM {levels} sl
         JOIN products p ON p.id = sl.product_id
         JOIN stock_locations loc ON loc.id = sl.location_id
         JOIN units u ON u.id = p.unit_id
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN location_minimums lm ON lm.product_id = sl.product_id AND lm.location_id = sl.location_id
         WHERE sl.qty <> 0
           AND (:location_id IS NULL OR sl.location_id = :location_id)
           AND (:category_id IS NULL OR p.category_id = :category_id)
         ORDER BY 2, 4
         LIMIT :row_limit + 1"
    );
    let found = rows(conn, &sql, s, |r| {
        let mut row = vec![
            Cell::opt_text(r.get(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::opt_text(r.get(2)?),
            Cell::opt_text(r.get(3)?),
            Cell::text(r.get::<_, String>(4)?),
            Cell::int(r.get(5)?),
            r.get::<_, Option<i64>>(6)?.map_or(Cell::NULL, Cell::int),
        ];
        row.extend(ints::<4>(r, 7)?.map(Cell::int));
        row.push(if r.get::<_, bool>(11)? {
            Cell::text("yes")
        } else {
            Cell::NULL
        });
        Ok(row)
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("category", K::Text),
        ("location", K::Text),
        ("unit", K::Code),
        ("qty", K::Qty),
        ("minQty", K::Qty),
        ("costPrice", K::Money),
        ("costValue", K::Money),
        ("sellPrice", K::Money),
        ("retailValue", K::Money),
        ("archived", K::Tag),
    ])
    .tag("archived", TagDomain::YesNo);
    for row in found {
        table.push(row)?;
    }
    if !truncated {
        let mut totals = vec![Cell::NULL; 8];
        totals.push(Cell::money(table.money_total("costValue")));
        totals.push(Cell::NULL);
        totals.push(Cell::money(table.money_total("retailValue")));
        totals.push(Cell::NULL);
        table.set_totals(totals)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

/// Value of the sellable stock per category (all storage locations, or the chosen one), the stock
/// an archived or untracked product still holds included.
pub(super) fn valuation(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let category = name_of("c", s.lang);
    let (cost_value, retail_value) = (
        value_sql("sl.qty", "p.cost_price"),
        value_sql("sl.qty", "p.sell_price"),
    );
    let levels = as_of("stock_levels", "product_id");
    let sql = format!(
        "SELECT MAX({category}), COUNT(DISTINCT p.id), SUM({cost_value}), SUM({retail_value})
         FROM {levels} sl
         JOIN products p ON p.id = sl.product_id
         JOIN stock_locations loc ON loc.id = sl.location_id
         LEFT JOIN categories c ON c.id = p.category_id
         WHERE sl.qty <> 0
           AND ((:location_id IS NULL AND loc.kind = 'storage') OR sl.location_id = :location_id)
         GROUP BY p.category_id
         ORDER BY SUM({cost_value}) DESC"
    );
    let mut table = Table::new(&[
        ("category", K::Text),
        ("products", K::Int),
        ("costValue", K::Money),
        ("retailValue", K::Money),
    ]);
    let mut sum = [0_i64; 3];
    for (category, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, Option<String>>(0)?, ints::<3>(r, 1)?))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::opt_text(category)];
        row.extend(v.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::charted(
        table,
        ChartKind::HBar,
        "category",
        "costValue",
    ))
}

/// Below the product minimum (all storage locations together) or a location's own minimum.
pub(super) fn low_stock(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let (name, place) = (name_of("p", s.lang), name_of("loc", s.lang));
    let levels = as_of("stock_levels", "product_id");
    let sql = format!(
        "SELECT p.reference, {name} AS product, NULL AS place, COALESCE(oh.qty, 0), p.min_stock, su.name
         FROM products p
         LEFT JOIN (SELECT sl.product_id, SUM(sl.qty) AS qty
                    FROM {levels} sl JOIN stock_locations loc ON loc.id = sl.location_id
                    WHERE loc.kind = 'storage' GROUP BY sl.product_id) oh ON oh.product_id = p.id
         LEFT JOIN product_extras x ON x.product_id = p.id
         LEFT JOIN suppliers su ON su.id = x.preferred_supplier_id
         WHERE :location_id IS NULL AND p.archived_at IS NULL AND p.track_stock = 1 AND p.min_stock > 0
           AND COALESCE(oh.qty, 0) < p.min_stock
         UNION ALL
         SELECT p.reference, {name}, {place}, COALESCE(sl.qty, 0), lm.min_qty, su.name
         FROM location_minimums lm
         JOIN products p ON p.id = lm.product_id
         JOIN stock_locations loc ON loc.id = lm.location_id
         LEFT JOIN {levels} sl ON sl.product_id = lm.product_id AND sl.location_id = lm.location_id
         LEFT JOIN product_extras x ON x.product_id = p.id
         LEFT JOIN suppliers su ON su.id = x.preferred_supplier_id
         WHERE p.archived_at IS NULL AND p.track_stock = 1 AND lm.min_qty > 0 AND COALESCE(sl.qty, 0) < lm.min_qty
           AND (:location_id IS NULL OR lm.location_id = :location_id)
         ORDER BY product, place"
    );
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("location", K::Text),
        ("qty", K::Qty),
        ("minQty", K::Qty),
        ("missing", K::Qty),
        ("supplier", K::Text),
    ]);
    let found = rows(conn, &sql, s, |r| {
        let [qty, min] = ints::<2>(r, 3)?;
        Ok(vec![
            Cell::opt_text(r.get(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::opt_text(r.get(2)?),
            Cell::int(qty),
            Cell::int(min),
            Cell::int(min.saturating_sub(qty)),
            Cell::opt_text(r.get(5)?),
        ])
    })?;
    for row in found {
        table.push(row)?;
    }
    Ok(Built::plain(table))
}

/// Lots that held stock at the end of the `to` day and were expired or expired within
/// `stock.expiry_alert_days` of it.
pub(super) fn expiry(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let today = NaiveDate::parse_from_str(&s.to, "%Y-%m-%d")
        .map_err(|_| AppError::validation("to", "date_invalid"))?;
    let days: i64 = crate::settings::get(conn, "stock.expiry_alert_days")?;
    // Checked like `stock::lots::expiry_list`: a horizon past chrono's calendar is refused.
    let limit = chrono::TimeDelta::try_days(days)
        .and_then(|d| today.checked_add_signed(d))
        .ok_or_else(|| AppError::validation("to", "date_invalid"))?
        .format("%Y-%m-%d")
        .to_string();
    let (name, place) = (name_of("p", s.lang), name_of("loc", s.lang));
    let lots = as_of("lot_levels", "lot_id");
    let sql = format!(
        "SELECT p.reference, {name}, lt.lot_number, lt.expiry_date, {place}, ll.qty
         FROM {lots} ll
         JOIN lots lt ON lt.id = ll.lot_id
         JOIN products p ON p.id = lt.product_id
         JOIN stock_locations loc ON loc.id = ll.location_id
         WHERE ll.qty > 0 AND lt.expiry_date IS NOT NULL AND lt.expiry_date <= :horizon
           AND (:location_id IS NULL OR ll.location_id = :location_id)
         ORDER BY lt.expiry_date, 2"
    );
    let named: [(&str, &dyn ToSql); 3] = [
        (":horizon", &limit),
        (":location_id", &s.req.location_id),
        (":day_to", &s.to),
    ];
    let mut stmt = conn.prepare(&sql)?;
    let found = stmt
        .query_map(named.as_slice(), |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("lot", K::Code),
        ("expiryDate", K::Date),
        ("location", K::Text),
        ("qty", K::Qty),
        ("daysLeft", K::Int),
        ("status", K::Tag),
    ])
    .tag("status", TagDomain::ExpiryStatus);
    for (reference, name, lot, expiry, place, qty) in found {
        let left = NaiveDate::parse_from_str(&expiry, "%Y-%m-%d")
            .map(|d| (d - today).num_days())
            .map_err(|_| AppError::Internal(format!("lot expiry {expiry} is not a day")))?;
        table.push(vec![
            Cell::opt_text(reference),
            Cell::opt_text(name),
            Cell::text(lot),
            Cell::text(expiry),
            Cell::opt_text(place),
            Cell::int(qty),
            Cell::int(left),
            Cell::text(if left < 0 { "expired" } else { "soon" }),
        ])?;
    }
    Ok(Built::plain(table))
}

/// The movement log (STK-021), oldest first, by business day of the movement's instant.
pub(super) fn movements(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let (name, place) = (name_of("p", s.lang), name_of("loc", s.lang));
    let moved_on = day_of("m.created_at");
    let sql = format!(
        "SELECT m.created_at, p.reference, {name}, {place}, m.reason, m.qty_delta, m.unit_cost,
                rd.number, lt.lot_number, COALESCE(u.display_name, m.user_id)
         FROM stock_movements m
         JOIN products p ON p.id = m.product_id
         JOIN stock_locations loc ON loc.id = m.location_id
         LEFT JOIN documents rd ON m.ref_type = 'document' AND rd.id = m.ref_id
         LEFT JOIN lots lt ON lt.id = m.lot_id
         LEFT JOIN users u ON u.id = m.user_id
         WHERE m.created_at >= unixepoch(:day_from) * 1000 - {offset}
           AND m.created_at < unixepoch(:day_to, '+1 day') * 1000 - {offset}
           AND {moved_on} BETWEEN :day_from AND :day_to
           AND (:location_id IS NULL OR m.location_id = :location_id)
           AND (:product_id IS NULL OR m.product_id = :product_id)
           AND (:category_id IS NULL OR p.category_id = :category_id)
         ORDER BY m.created_at, m.id
         LIMIT :row_limit + 1",
        offset = crate::time::ALGIERS_OFFSET_MS,
    );
    let found = rows(conn, &sql, s, |r| {
        Ok(vec![
            Cell::int(r.get(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::opt_text(r.get(2)?),
            Cell::opt_text(r.get(3)?),
            Cell::text(r.get::<_, String>(4)?),
            Cell::int(r.get(5)?),
            r.get::<_, Option<i64>>(6)?.map_or(Cell::NULL, Cell::int),
            Cell::opt_text(r.get(7)?),
            Cell::opt_text(r.get(8)?),
            Cell::opt_text(r.get(9)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("at", K::DateTime),
        ("reference", K::Code),
        ("name", K::Text),
        ("location", K::Text),
        ("reason", K::Tag),
        ("qty", K::Qty),
        ("unitCost", K::Money),
        ("document", K::Code),
        ("lot", K::Code),
        ("user", K::Text),
    ])
    .tag("reason", TagDomain::MovementReason);
    for row in found {
        table.push(row)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

/// The lines of one count: expected, counted, the difference and its value at the cost frozen on
/// the line. Like the count screen (`stock::counts`), a counted line of an open count expects the
/// level when it was counted (what its close posts against), an uncounted one the live level at the
/// count's location; a closed count shows what was posted.
pub(super) fn count_variance(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let name = name_of("p", s.lang);
    let sql = format!(
        "SELECT p.reference, {name},
                CASE WHEN c.status = 'open' AND cl.counted_at IS NULL
                     THEN COALESCE((SELECT sl.qty FROM stock_levels sl
                                    WHERE sl.product_id = cl.product_id AND sl.location_id = c.location_id), 0)
                     ELSE cl.expected_qty END,
                cl.counted_qty, cl.posted_delta, cl.unit_cost
         FROM stock_count_lines cl
         JOIN stock_counts c ON c.id = cl.count_id
         JOIN products p ON p.id = cl.product_id
         WHERE cl.count_id = :count_id
         ORDER BY cl.counted_qty IS NULL, 2"
    );
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("expected", K::Qty),
        ("counted", K::Qty),
        ("difference", K::Qty),
        ("unitCost", K::Money),
        ("costValue", K::Money),
    ]);
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let mut value_total = 0_i64;
    for (reference, name, expected, counted, posted, unit_cost) in found {
        let difference = posted.or_else(|| counted.and_then(|c| c.checked_sub(expected)));
        let value = difference.map(|d| value_of(d, unit_cost));
        value_total = value_total.saturating_add(value.unwrap_or(0));
        table.push(vec![
            Cell::opt_text(reference),
            Cell::opt_text(name),
            Cell::int(expected),
            counted.map_or(Cell::NULL, Cell::int),
            difference.map_or(Cell::NULL, Cell::int),
            Cell::int(unit_cost),
            value.map_or(Cell::NULL, Cell::int),
        ])?;
    }
    let mut totals = vec![Cell::NULL; 6];
    totals.push(Cell::int(value_total));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}
