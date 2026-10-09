//! Minimum stock per product and location (STK-013/014): per-location minimums when set, else
//! the product's minimum against its storage total; the dashboard shows the count daily.

use super::{SHOP_LOCATION_ID, location};
use crate::audit::{self, Actor};
use crate::catalog::products;
use crate::error::AppError;
use crate::money::Qty;
use crate::time::now_ms;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocationMinimum {
    pub location_id: String,
    pub min_qty: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LowStockAt {
    pub product_id: String,
    pub reference: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub location_id: String,
    pub on_hand: Qty,
    pub min_qty: Qty,
    pub missing: Qty,
}

pub fn get(conn: &Connection, product_id: &str) -> Result<Vec<LocationMinimum>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT location_id, min_qty FROM location_minimums WHERE product_id = ?1 ORDER BY location_id",
    )?;
    let rows = stmt
        .query_map([product_id], |r| {
            Ok(LocationMinimum {
                location_id: r.get(0)?,
                min_qty: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    minimums: Vec<LocationMinimum>,
) -> Result<Vec<LocationMinimum>, AppError> {
    let p = products::get(tx, product_id)?;
    for (i, m) in minimums.iter().enumerate() {
        location(tx, &m.location_id).map_err(|_| {
            AppError::validation(
                &format!("minimums.{i}.locationId"),
                "stock_location_not_found",
            )
        })?;
        if m.min_qty.is_negative() || !m.min_qty.fits_decimals(p.unit_decimals) {
            return Err(AppError::validation(
                &format!("minimums.{i}.minQty"),
                "qty_invalid",
            ));
        }
    }
    let before = get(tx, product_id)?;
    tx.execute(
        "DELETE FROM location_minimums WHERE product_id = ?1",
        [product_id],
    )?;
    let now = now_ms();
    for m in minimums.iter().filter(|m| m.min_qty.is_positive()) {
        tx.execute(
            "INSERT INTO location_minimums (product_id, location_id, min_qty, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![product_id, m.location_id, m.min_qty, now],
        )?;
    }
    let after = get(tx, product_id)?;
    audit::append(
        tx,
        &actor.event(
            "stock.minimums_set",
            "product",
            Some(product_id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

/// Products under their minimum, the biggest shortfall first.
pub fn below(
    conn: &Connection,
    location_id: Option<&str>,
    limit: u32,
) -> Result<Vec<LowStockAt>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT * FROM (
           SELECT p.id, p.reference, p.name_ar, p.name_fr, p.name_en, m.location_id,
                  COALESCE((SELECT l.qty FROM stock_levels l WHERE l.product_id = p.id AND l.location_id = m.location_id), 0) AS on_hand,
                  m.min_qty
           FROM location_minimums m JOIN products p ON p.id = m.product_id
           WHERE p.archived_at IS NULL AND (?1 IS NULL OR m.location_id = ?1)
           UNION ALL
           SELECT p.id, p.reference, p.name_ar, p.name_fr, p.name_en, ?2,
                  COALESCE((SELECT SUM(l.qty) FROM stock_levels l JOIN stock_locations s ON s.id = l.location_id
                            WHERE l.product_id = p.id AND s.kind = 'storage'), 0),
                  p.min_stock
           FROM products p
           WHERE p.archived_at IS NULL AND p.track_stock = 1 AND p.min_stock > 0
             AND NOT EXISTS (SELECT 1 FROM location_minimums m WHERE m.product_id = p.id)
             AND (?1 IS NULL OR ?1 = ?2))
         WHERE on_hand < min_qty
         ORDER BY on_hand - min_qty, name_fr LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(
            params![location_id, SHOP_LOCATION_ID, limit.clamp(1, 1_000)],
            |r| {
                let on_hand: Qty = r.get(6)?;
                let min_qty: Qty = r.get(7)?;
                Ok(LowStockAt {
                    product_id: r.get(0)?,
                    reference: r.get(1)?,
                    name_ar: r.get(2)?,
                    name_fr: r.get(3)?,
                    name_en: r.get(4)?,
                    location_id: r.get(5)?,
                    on_hand,
                    min_qty,
                    missing: min_qty.checked_sub(on_hand).unwrap_or(Qty::ZERO),
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::testkit::shop;

    #[test]
    fn minimums_per_location_and_the_product_minimum_raise_alerts() {
        let shop = shop();
        let flour = shop.product("Farine 25 kg", Money(250_000), &shop.tva9);
        shop.stock_in(&flour.id, 3_000);
        let actor = shop.owner.actor();
        let id = flour.id.clone();
        shop.db
            .write(move |tx| {
                set(
                    tx,
                    &actor,
                    &id,
                    vec![LocationMinimum {
                        location_id: SHOP_LOCATION_ID.into(),
                        min_qty: Qty(5_000),
                    }],
                )
            })
            .unwrap();
        let mut input = crate::catalog::products::ProductInput::simple(
            "Semoule",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva9,
            Money(120_000),
        );
        input.min_stock = Qty(10_000);
        let semolina = shop.product_with(input);
        shop.stock_in(&semolina.id, 2_000);
        let low = shop.db.read(|c| below(c, None, 50)).unwrap();
        assert_eq!(
            low.iter()
                .map(|l| (l.name_fr.as_str(), l.missing.0))
                .collect::<Vec<_>>(),
            vec![("Semoule", 8_000), ("Farine 25 kg", 2_000)]
        );
    }
}
