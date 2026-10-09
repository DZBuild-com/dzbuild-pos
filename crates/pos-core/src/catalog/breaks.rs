//! Quantity breaks (PRC-004): from `min_qty` on, a lower unit price at a price level (a carton of
//! water cheaper per bottle). The cart takes the best break for the line quantity (Task 7).

use crate::audit::{self, Actor};
use crate::catalog::{price_levels, products};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty};
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceBreak {
    pub id: String,
    pub price_level_id: String,
    pub min_qty: Qty,
    pub price: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceBreakInput {
    pub price_level_id: String,
    pub min_qty: Qty,
    pub price: Money,
}

pub fn list(conn: &Connection, product_id: &str) -> Result<Vec<PriceBreak>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, price_level_id, min_qty, price FROM price_breaks WHERE product_id = ?1
         ORDER BY price_level_id, min_qty",
    )?;
    let rows = stmt
        .query_map([product_id], |r| {
            Ok(PriceBreak {
                id: r.get(0)?,
                price_level_id: r.get(1)?,
                min_qty: r.get(2)?,
                price: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn replace(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    breaks: Vec<PriceBreakInput>,
) -> Result<Vec<PriceBreak>, AppError> {
    let product = products::get(tx, product_id)?;
    let mut seen = BTreeSet::new();
    for (i, b) in breaks.iter().enumerate() {
        let f = |name: &str| format!("breaks.{i}.{name}");
        price_levels::active(tx, &b.price_level_id)
            .map_err(|_| AppError::validation(&f("priceLevelId"), "price_level_not_found"))?;
        if !b.min_qty.is_positive() || !b.min_qty.fits_decimals(product.unit_decimals) {
            return Err(AppError::validation(&f("minQty"), "qty_invalid"));
        }
        if b.price.is_negative() {
            return Err(AppError::validation(&f("price"), "amount_invalid"));
        }
        if !seen.insert((b.price_level_id.clone(), b.min_qty.0)) {
            return Err(AppError::validation(&f("minQty"), "value_invalid"));
        }
    }
    let before = list(tx, product_id)?;
    tx.execute(
        "DELETE FROM price_breaks WHERE product_id = ?1",
        [product_id],
    )?;
    let now = now_ms();
    for b in &breaks {
        tx.execute(
            "INSERT INTO price_breaks (id, product_id, price_level_id, min_qty, price, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                new_id(),
                product_id,
                b.price_level_id,
                b.min_qty,
                b.price,
                now
            ],
        )?;
    }
    let after = list(tx, product_id)?;
    audit::append(
        tx,
        &actor.event(
            "product.price_breaks_changed",
            "product",
            Some(product_id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

/// The lowest break price of `level_id` whose minimum `qty` reaches.
pub fn best_price(
    conn: &Connection,
    product_id: &str,
    level_id: &str,
    qty: Qty,
) -> Result<Option<Money>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT MIN(price) FROM price_breaks
             WHERE product_id = ?1 AND price_level_id = ?2 AND min_qty <= ?3",
        )?
        .query_row(params![product_id, level_id, qty], |r| {
            r.get::<_, Option<Money>>(0)
        })
        .optional()?
        .flatten())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{RETAIL_LEVEL_ID, WHOLESALE_LEVEL_ID};
    use crate::testkit::shop;

    #[test]
    fn the_best_break_for_the_quantity_and_level_wins() {
        let shop = shop();
        let water = shop.product("Eau Ifri 1,5 L", Money(5_000), &shop.tva9);
        let actor = shop.owner.actor();
        let id = water.id.clone();
        shop.db
            .write(move |tx| {
                replace(
                    tx,
                    &actor,
                    &id,
                    vec![
                        PriceBreakInput {
                            price_level_id: RETAIL_LEVEL_ID.into(),
                            min_qty: Qty(6_000),
                            price: Money(4_500),
                        },
                        PriceBreakInput {
                            price_level_id: RETAIL_LEVEL_ID.into(),
                            min_qty: Qty(24_000),
                            price: Money(4_000),
                        },
                        PriceBreakInput {
                            price_level_id: WHOLESALE_LEVEL_ID.into(),
                            min_qty: Qty(24_000),
                            price: Money(3_800),
                        },
                    ],
                )
            })
            .unwrap();
        let best = |level: &'static str, qty: i64| {
            let id = water.id.clone();
            shop.db
                .read(move |c| best_price(c, &id, level, Qty(qty)))
                .unwrap()
        };
        assert_eq!(best(RETAIL_LEVEL_ID, 5_000), None);
        assert_eq!(best(RETAIL_LEVEL_ID, 6_000), Some(Money(4_500)));
        assert_eq!(best(RETAIL_LEVEL_ID, 30_000), Some(Money(4_000)));
        assert_eq!(best(WHOLESALE_LEVEL_ID, 24_000), Some(Money(3_800)));
        let actor = shop.owner.actor();
        let id = water.id.clone();
        let twice = shop.db.write(move |tx| {
            replace(
                tx,
                &actor,
                &id,
                vec![
                    PriceBreakInput {
                        price_level_id: RETAIL_LEVEL_ID.into(),
                        min_qty: Qty(6_000),
                        price: Money(4_500),
                    },
                    PriceBreakInput {
                        price_level_id: RETAIL_LEVEL_ID.into(),
                        min_qty: Qty(6_000),
                        price: Money(4_400),
                    },
                ],
            )
        });
        assert_eq!(
            twice.unwrap_err(),
            AppError::validation("breaks.1.minQty", "value_invalid")
        );
    }
}
