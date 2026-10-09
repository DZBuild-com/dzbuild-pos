//! Read models of the stock screens and reports (STK-002, STK-015, RPT-023/024/027): levels
//! per location with their cost and retail value, and the movement ledger with document numbers.

use super::{AdjustReason, MovementReason};
use crate::catalog::normalize;
use crate::error::AppError;
use crate::money::{Money, Qty};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LevelQuery {
    pub location_id: Option<String>,
    pub category_id: Option<String>,
    pub text: String,
    pub only_below_min: bool,
    pub include_zero: bool,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LevelRow {
    pub product_id: String,
    pub reference: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub unit_code: String,
    pub location_id: String,
    pub qty: Qty,
    pub min_qty: Qty,
    pub cost_price: Money,
    pub cost_value: Money,
    pub sell_price: Money,
    pub retail_value: Money,
    /// An archived (or no longer tracked) product that still holds stock here: listed, never
    /// dropped, so no stock is out of sight.
    pub archived: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MovementQuery {
    pub product_id: Option<String>,
    pub location_id: Option<String>,
    pub reason: Option<MovementReason>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Keyset paging: movements created before this instant.
    pub before: Option<i64>,
    /// With `before`, the id of the last row of the previous page: the movements of that same
    /// instant that come after it are on this page (a count or a transfer appends many at once).
    pub before_id: Option<String>,
    pub limit: Option<u32>,
    /// Words each found in the product (names, reference, barcodes), the document number, the lot
    /// or the note: the whole ledger is searched, not the page shown.
    #[serde(default)]
    #[specta(optional)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MovementRow {
    pub id: String,
    pub created_at: i64,
    pub product_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub location_id: String,
    pub qty_delta: Qty,
    pub reason: MovementReason,
    pub reason_code: Option<AdjustReason>,
    pub unit_cost: Option<Money>,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub ref_number: Option<String>,
    pub lot_number: Option<String>,
    pub user_id: Option<String>,
    pub note: Option<String>,
}

fn like(words: &[String], i: usize) -> String {
    words.get(i).map(|w| format!("%{w}%")).unwrap_or_default()
}

/// Levels per product and location. `only_below_min` keeps the rows under their minimum, out of
/// stock ones included (it implies `include_zero`), filtered before the limit. An archived or
/// untracked product appears where it still holds stock (`archived`).
pub fn levels(conn: &Connection, q: &LevelQuery) -> Result<Vec<LevelRow>, AppError> {
    let words = normalize::words(&q.text);
    let mut stmt = conn.prepare_cached(
        "WITH RECURSIVE tree(id) AS (SELECT ?2 WHERE ?2 IS NOT NULL
                                     UNION SELECT c.id FROM categories c JOIN tree t ON c.parent_id = t.id)
         SELECT p.id, p.reference, p.name_ar, p.name_fr, p.name_en, u.code, s.id,
                COALESCE(l.qty, 0),
                COALESCE((SELECT m.min_qty FROM location_minimums m WHERE m.product_id = p.id AND m.location_id = s.id),
                         CASE WHEN s.is_default = 1 THEN p.min_stock ELSE 0 END),
                p.cost_price, p.sell_price, p.archived_at IS NOT NULL OR p.track_stock = 0
         FROM products p JOIN units u ON u.id = p.unit_id
         JOIN stock_locations s ON s.archived_at IS NULL AND (?1 IS NULL OR s.id = ?1)
         LEFT JOIN stock_levels l ON l.product_id = p.id AND l.location_id = s.id
         WHERE ((p.track_stock = 1 AND p.archived_at IS NULL) OR COALESCE(l.qty, 0) <> 0)
           AND (?2 IS NULL OR p.category_id IN (SELECT id FROM tree))
           AND (?3 = '' OR p.search_text LIKE ?3) AND (?4 = '' OR p.search_text LIKE ?4) AND (?5 = '' OR p.search_text LIKE ?5)
           AND (?6 = 1 OR ?8 = 1 OR COALESCE(l.qty, 0) <> 0)
           AND (?8 = 0 OR COALESCE(l.qty, 0) <
                COALESCE((SELECT m.min_qty FROM location_minimums m WHERE m.product_id = p.id AND m.location_id = s.id),
                         CASE WHEN s.is_default = 1 THEN p.min_stock ELSE 0 END))
         ORDER BY p.name_fr, s.is_default DESC LIMIT ?7",
    )?;
    let rows = stmt
        .query_map(
            params![
                q.location_id,
                q.category_id,
                like(&words, 0),
                like(&words, 1),
                like(&words, 2),
                q.include_zero,
                q.limit.unwrap_or(2_000).clamp(1, 20_000),
                q.only_below_min
            ],
            |r| {
                let qty: Qty = r.get(7)?;
                let cost: Money = r.get(9)?;
                let sell: Money = r.get(10)?;
                Ok(LevelRow {
                    product_id: r.get(0)?,
                    reference: r.get(1)?,
                    name_ar: r.get(2)?,
                    name_fr: r.get(3)?,
                    name_en: r.get(4)?,
                    unit_code: r.get(5)?,
                    location_id: r.get(6)?,
                    qty,
                    min_qty: r.get(8)?,
                    cost_price: cost,
                    cost_value: cost.mul_qty(qty).unwrap_or(Money::ZERO),
                    sell_price: sell,
                    retail_value: sell.mul_qty(qty).unwrap_or(Money::ZERO),
                    archived: r.get(11)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The ledger, newest first. Pages chain on `(before, before_id)` = the `created_at` and `id` of
/// the previous page's last row, so movements sharing an instant are never skipped. `text` keeps
/// the movements where each of its first three words is found in the product's searchable text,
/// the document number, the lot number or the note.
pub fn movement_log(conn: &Connection, q: &MovementQuery) -> Result<Vec<MovementRow>, AppError> {
    let bound = |day: &Option<String>, end: bool| -> Result<Option<i64>, AppError> {
        match day {
            None => Ok(None),
            Some(d) => {
                let day = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                    .map_err(|_| AppError::validation("from", "date_invalid"))?;
                let day = if end {
                    day.succ_opt().unwrap_or(day)
                } else {
                    day
                };
                let midnight = day
                    .and_hms_opt(0, 0, 0)
                    .map(|t| t.and_utc().timestamp_millis())
                    .unwrap_or(0);
                Ok(Some(midnight - crate::time::ALGIERS_OFFSET_MS))
            }
        }
    };
    let from = bound(&q.from, false)?;
    let to = bound(&q.to, true)?;
    let mut stmt = conn.prepare_cached(
        "SELECT m.id, m.created_at, m.product_id, p.name_ar, p.name_fr, p.name_en, m.location_id, m.qty_delta,
                m.reason, m.reason_code, m.unit_cost, m.ref_type, m.ref_id,
                (SELECT d.number FROM documents d WHERE d.id = m.ref_id),
                (SELECT lt.lot_number FROM lots lt WHERE lt.id = m.lot_id), m.user_id, m.note
         FROM stock_movements m JOIN products p ON p.id = m.product_id
         WHERE (?1 IS NULL OR m.product_id = ?1) AND (?2 IS NULL OR m.location_id = ?2)
           AND (?3 IS NULL OR m.reason = ?3) AND (?4 IS NULL OR m.created_at >= ?4)
           AND (?5 IS NULL OR m.created_at < ?5)
           AND (?6 IS NULL OR m.created_at < ?6 OR (m.created_at = ?6 AND m.id < ?8))
           AND (?9 = '' OR p.search_text LIKE ?9 OR m.note LIKE ?9
                OR EXISTS (SELECT 1 FROM documents d WHERE d.id = m.ref_id AND d.number LIKE ?9)
                OR EXISTS (SELECT 1 FROM lots lt WHERE lt.id = m.lot_id AND lt.lot_number LIKE ?9))
           AND (?10 = '' OR p.search_text LIKE ?10 OR m.note LIKE ?10
                OR EXISTS (SELECT 1 FROM documents d WHERE d.id = m.ref_id AND d.number LIKE ?10)
                OR EXISTS (SELECT 1 FROM lots lt WHERE lt.id = m.lot_id AND lt.lot_number LIKE ?10))
           AND (?11 = '' OR p.search_text LIKE ?11 OR m.note LIKE ?11
                OR EXISTS (SELECT 1 FROM documents d WHERE d.id = m.ref_id AND d.number LIKE ?11)
                OR EXISTS (SELECT 1 FROM lots lt WHERE lt.id = m.lot_id AND lt.lot_number LIKE ?11))
         ORDER BY m.created_at DESC, m.id DESC LIMIT ?7",
    )?;
    let words = normalize::words(q.text.as_deref().unwrap_or_default());
    let rows = stmt
        .query_map(
            params![
                q.product_id,
                q.location_id,
                q.reason,
                from,
                to,
                q.before,
                q.limit.unwrap_or(500).clamp(1, 5_000),
                q.before_id,
                like(&words, 0),
                like(&words, 1),
                like(&words, 2)
            ],
            |r| {
                Ok(MovementRow {
                    id: r.get(0)?,
                    created_at: r.get(1)?,
                    product_id: r.get(2)?,
                    name_ar: r.get(3)?,
                    name_fr: r.get(4)?,
                    name_en: r.get(5)?,
                    location_id: r.get(6)?,
                    qty_delta: r.get(7)?,
                    reason: r.get(8)?,
                    reason_code: r.get(9)?,
                    unit_cost: r.get(10)?,
                    ref_type: r.get(11)?,
                    ref_id: r.get(12)?,
                    ref_number: r.get(13)?,
                    lot_number: r.get(14)?,
                    user_id: r.get(15)?,
                    note: r.get(16)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn levels_are_valued_and_movements_are_listed_newest_first() {
        let shop = shop();
        let p = shop.product("Huile Elio 5 L", Money(65_000), &shop.tva19);
        let actor = shop.owner.actor();
        let input = crate::stock::AdjustInput {
            product_id: p.id.clone(),
            location_id: None,
            qty_delta: Qty(4_000),
            reason: crate::stock::AdjustReason::Opening,
            unit_cost: Some(Money(52_000)),
            note: None,
        };
        shop.db
            .write(move |tx| crate::stock::adjust(tx, &actor, input, 1))
            .unwrap();
        let rows = shop
            .db
            .read(|c| {
                levels(
                    c,
                    &LevelQuery {
                        text: "elio".into(),
                        ..LevelQuery::default()
                    },
                )
            })
            .unwrap();
        assert_eq!(
            (rows[0].qty, rows[0].cost_value, rows[0].retail_value),
            (Qty(4_000), Money(208_000), Money(260_000))
        );
        let moves = shop
            .db
            .read(move |c| {
                movement_log(
                    c,
                    &MovementQuery {
                        product_id: Some(p.id.clone()),
                        ..MovementQuery::default()
                    },
                )
            })
            .unwrap();
        assert_eq!((moves.len(), moves[0].reason), (1, MovementReason::Opening));
    }

    #[test]
    fn below_minimum_finds_empty_shelves_beyond_the_first_page() {
        let shop = shop();
        let product = |name: &str, held: i64, min: i64| {
            let mut input = crate::catalog::products::ProductInput::simple(
                name,
                crate::catalog::UNIT_PIECE_ID,
                &shop.tva9,
                Money(10_000),
            );
            input.min_stock = Qty(min);
            let p = shop.product_with(input);
            if held != 0 {
                shop.stock_in(&p.id, held);
            }
            p
        };
        product("Biscuits Bimo", 20_000, 0);
        product("Café Bonal", 20_000, 5_000);
        product("Farine 25 kg", 0, 5_000);
        product("Semoule", 2_000, 10_000);
        product("Thé Chiraz", 1_000, 0);
        let low = |limit: u32| {
            shop.db
                .read(move |c| {
                    levels(
                        c,
                        &LevelQuery {
                            only_below_min: true,
                            limit: Some(limit),
                            ..LevelQuery::default()
                        },
                    )
                })
                .unwrap()
                .into_iter()
                .map(|r| (r.name_fr, r.qty.0, r.min_qty.0))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            low(2),
            vec![
                ("Farine 25 kg".to_owned(), 0, 5_000),
                ("Semoule".to_owned(), 2_000, 10_000)
            ],
            "the out-of-stock product is the most urgent, and a small page still finds them"
        );
        assert_eq!(low(1).len(), 1);
    }

    /// The search runs over the whole ledger (product, document number, lot, note), not over
    /// the page the screen holds.
    #[test]
    fn the_movement_search_reads_the_whole_ledger() {
        let shop = shop();
        for name in ["Café Bonal", "Thé Chiraz", "Sucre"] {
            let p = shop.product(name, Money(10_000), &shop.tva9);
            shop.stock_in(&p.id, 1_000);
        }
        let find = |text: &str, limit: u32| {
            let q = MovementQuery {
                text: Some(text.to_owned()),
                limit: Some(limit),
                ..MovementQuery::default()
            };
            shop.db
                .read(move |c| movement_log(c, &q))
                .unwrap()
                .into_iter()
                .map(|m| m.name_fr)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            find("cafe", 1),
            ["Café Bonal"],
            "the oldest movement, found on a page of one"
        );
        assert_eq!(find("the chiraz", 5), ["Thé Chiraz"]);
        assert!(find("farine", 5).is_empty());
        assert_eq!(find("", 5).len(), 3);
    }

    #[test]
    fn movement_pages_lose_nothing_at_one_instant() {
        let shop = shop();
        for name in ["Café Bonal", "Thé Chiraz", "Sucre"] {
            let p = shop.product(name, Money(10_000), &shop.tva9);
            let actor = shop.owner.actor();
            let input = crate::stock::AdjustInput {
                product_id: p.id.clone(),
                location_id: None,
                qty_delta: Qty(1_000),
                reason: crate::stock::AdjustReason::Opening,
                unit_cost: None,
                note: None,
            };
            // A count close or a transfer appends all its movements at the same instant.
            shop.db
                .write(move |tx| crate::stock::adjust(tx, &actor, input, 5))
                .unwrap();
        }
        let page = |before: Option<&MovementRow>| {
            let q = MovementQuery {
                before: before.map(|m| m.created_at),
                before_id: before.map(|m| m.id.clone()),
                limit: Some(2),
                ..MovementQuery::default()
            };
            shop.db.read(move |c| movement_log(c, &q)).unwrap()
        };
        let first = page(None);
        assert_eq!(first.len(), 2);
        let second = page(first.last());
        assert_eq!(second.len(), 1, "the third movement of that instant");
        let ids: Vec<&str> = first.iter().chain(&second).map(|m| m.id.as_str()).collect();
        let mut newest_first = ids.clone();
        newest_first.sort_unstable_by(|a, b| b.cmp(a));
        newest_first.dedup();
        assert_eq!(ids, newest_first, "three distinct rows, newest first");
        assert!(page(second.last()).is_empty());
    }
}
