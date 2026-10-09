//! The daily menu ("menu du jour"): products offered on a business day, shown first at the till.

use crate::audit::{self, Actor};
use crate::catalog::products;
use crate::error::AppError;
use crate::party::clean_day;
use rusqlite::{Connection, Transaction, params};
use serde_json::json;

pub fn set_day(
    tx: &Transaction<'_>,
    actor: &Actor,
    day: &str,
    product_ids: &[String],
) -> Result<Vec<String>, AppError> {
    let day =
        clean_day(Some(day), "day")?.ok_or_else(|| AppError::validation("day", "required"))?;
    if product_ids.len() > 60 {
        return Err(AppError::validation("productIds", "value_invalid"));
    }
    tx.execute("DELETE FROM daily_menu WHERE business_date = ?1", [&day])?;
    for (i, id) in product_ids.iter().enumerate() {
        products::sellable(tx, id)
            .map_err(|_| AppError::validation(&format!("productIds.{i}"), "product_not_found"))?;
        tx.execute(
            "INSERT OR IGNORE INTO daily_menu (business_date, product_id, sort_order) VALUES (?1, ?2, ?3)",
            params![day, id, i64::try_from(i).unwrap_or(0)],
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "dining.menu_set",
            "daily_menu",
            Some(&day),
            json!({ "products": product_ids }),
        ),
    )?;
    of_day(tx, &day)
}

pub fn of_day(conn: &Connection, day: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT m.product_id FROM daily_menu m JOIN products p ON p.id = m.product_id
         WHERE m.business_date = ?1 AND p.archived_at IS NULL ORDER BY m.sort_order",
    )?;
    let rows = stmt
        .query_map([day], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
