//! Named sale price levels (PRC-001): Détail (default), Demi-gros, Gros, Revendeur, plus any the
//! owner adds. A product's price at a level is `product_prices`, else its `sell_price`.

use super::{clean_code, clean_names};
use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceLevel {
    pub id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub is_default: bool,
    pub sort_order: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceLevelInput {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub sort_order: i64,
}

const SELECT: &str = "SELECT id, code, name_ar, name_fr, name_en, is_default, sort_order,
        archived_at IS NOT NULL FROM price_levels";

fn map(row: &Row<'_>) -> rusqlite::Result<PriceLevel> {
    Ok(PriceLevel {
        id: row.get(0)?,
        code: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        is_default: row.get(5)?,
        sort_order: row.get(6)?,
        archived: row.get(7)?,
    })
}

/// Active levels in display order.
pub fn list(conn: &Connection) -> Result<Vec<PriceLevel>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE archived_at IS NULL ORDER BY sort_order, rowid"
    ))?;
    let levels = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(levels)
}

pub fn get(conn: &Connection, id: &str) -> Result<PriceLevel, AppError> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("price_level"))
}

/// The level used when a sale names none (Détail unless the owner changed it).
pub fn default_level(conn: &Connection) -> Result<PriceLevel, AppError> {
    conn.query_row(&format!("{SELECT} WHERE is_default = 1"), [], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("price_level"))
}

/// An active level, for pricing: archived and unknown levels are `price_level_not_found`.
pub fn active(conn: &Connection, id: &str) -> Result<PriceLevel, AppError> {
    let level = get(conn, id)?;
    if level.archived {
        return Err(AppError::not_found("price_level"));
    }
    Ok(level)
}

fn clean(input: &PriceLevelInput) -> Result<PriceLevelInput, AppError> {
    let (name_ar, name_fr, name_en) = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    Ok(PriceLevelInput {
        code: clean_code(&input.code, "code")?,
        name_ar,
        name_fr,
        name_en,
        sort_order: input.sort_order,
    })
}

fn ensure_code_free(conn: &Connection, code: &str, except: Option<&str>) -> Result<(), AppError> {
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM price_levels WHERE code = ?1 AND id IS NOT ?2)",
        params![code, except],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    Ok(())
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: PriceLevelInput,
) -> Result<PriceLevel, AppError> {
    let l = clean(&input)?;
    ensure_code_free(tx, &l.code, None)?;
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO price_levels (id, code, name_ar, name_fr, name_en, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![id, l.code, l.name_ar, l.name_fr, l.name_en, l.sort_order, now],
    )?;
    let level = get(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "price_level.created",
            "price_level",
            Some(&id),
            json!({ "after": level }),
        ),
    )?;
    Ok(level)
}

pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: PriceLevelInput,
) -> Result<PriceLevel, AppError> {
    let before = active(tx, id)?;
    let l = clean(&input)?;
    ensure_code_free(tx, &l.code, Some(id))?;
    tx.execute(
        "UPDATE price_levels SET code = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5, sort_order = ?6,
                updated_at = ?7 WHERE id = ?1",
        params![id, l.code, l.name_ar, l.name_fr, l.name_en, l.sort_order, now_ms()],
    )?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "price_level.updated",
            "price_level",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

/// Makes `id` the default level (the previous default stays as an ordinary level).
///
/// The default level's price is `products.sell_price` and never a `product_prices` row, so the
/// prices move with it and every product keeps the same price at every level: the old default's
/// price becomes a row, and the new default's row becomes `sell_price`.
pub fn set_default(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<PriceLevel, AppError> {
    let before = default_level(tx)?;
    active(tx, id)?;
    let now = now_ms();
    let mut moved = 0;
    if before.id != id {
        tx.execute(
            "INSERT INTO product_prices (product_id, price_level_id, price, updated_at)
             SELECT id, ?1, sell_price, ?2 FROM products WHERE true
             ON CONFLICT (product_id, price_level_id) DO NOTHING",
            params![before.id, now],
        )?;
        moved = tx.execute(
            "UPDATE products SET updated_at = ?2,
                 sell_price = (SELECT price FROM product_prices
                               WHERE product_id = products.id AND price_level_id = ?1)
             WHERE id IN (SELECT product_id FROM product_prices WHERE price_level_id = ?1)",
            params![id, now],
        )?;
        tx.execute("DELETE FROM product_prices WHERE price_level_id = ?1", [id])?;
    }
    tx.execute(
        "UPDATE price_levels SET is_default = 0, updated_at = ?1 WHERE is_default = 1",
        [now],
    )?;
    tx.execute(
        "UPDATE price_levels SET is_default = 1, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    audit::append(
        tx,
        &actor.event(
            "price_level.updated",
            "price_level",
            Some(id),
            json!({ "defaultBefore": before.id, "defaultAfter": id, "sellPricesMoved": moved }),
        ),
    )?;
    get(tx, id)
}

/// Archives a level that is not the default; its product prices stay for old documents.
pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let level = active(tx, id)?;
    if level.is_default {
        return Err(AppError::conflict("price_level_is_default"));
    }
    tx.execute(
        "UPDATE price_levels SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "price_level.archived",
            "price_level",
            Some(id),
            json!({ "code": level.code }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{RETAIL_LEVEL_ID, WHOLESALE_LEVEL_ID};
    use crate::db::Db;

    fn input(code: &str) -> PriceLevelInput {
        PriceLevelInput {
            code: code.into(),
            name_fr: "Garage".into(),
            sort_order: 5,
            ..PriceLevelInput::default()
        }
    }

    #[test]
    fn four_levels_are_seeded_with_retail_as_default() {
        let db = Db::open_memory().unwrap();
        let levels = db.read(list).unwrap();
        let names: Vec<&str> = levels.iter().map(|l| l.name_fr.as_str()).collect();
        assert_eq!(names, ["Détail", "Demi-gros", "Gros", "Revendeur"]);
        assert_eq!(db.read(default_level).unwrap().id, RETAIL_LEVEL_ID);
    }

    #[test]
    fn owner_adds_levels_and_moves_the_default() {
        let db = Db::open_memory().unwrap();
        let garage = db
            .write(|tx| create(tx, &Actor::default(), input("garage")))
            .unwrap();
        assert_eq!(db.read(list).unwrap().len(), 5);
        let dup = db
            .write(|tx| create(tx, &Actor::default(), input("GARAGE")))
            .unwrap_err();
        assert_eq!(dup.code(), "code_taken");
        db.write(|tx| set_default(tx, &Actor::default(), WHOLESALE_LEVEL_ID))
            .unwrap();
        assert_eq!(db.read(default_level).unwrap().id, WHOLESALE_LEVEL_ID);
        let err = db
            .write(|tx| archive(tx, &Actor::default(), WHOLESALE_LEVEL_ID))
            .unwrap_err();
        assert_eq!(err.code(), "price_level_is_default");
        let id = garage.id.clone();
        db.write(move |tx| archive(tx, &Actor::default(), &id))
            .unwrap();
        let id = garage.id.clone();
        assert_eq!(
            db.read(move |c| active(c, &id)).unwrap_err().code(),
            "price_level_not_found"
        );
        let retail = db.read(|c| get(c, RETAIL_LEVEL_ID)).unwrap();
        assert!(!retail.is_default);
    }
}
