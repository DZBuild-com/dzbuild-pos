//! Units of sale and stock (spec §5.1 `units`): pieces with 0 decimals, kg/l/m with 3.

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
pub struct Unit {
    pub id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    /// Decimals a quantity may have: 0 (pieces) to 3 (grams of a kg).
    pub decimals: u8,
    pub archived: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnitInput {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub decimals: u8,
}

const SELECT: &str =
    "SELECT id, code, name_ar, name_fr, name_en, decimals, archived_at IS NOT NULL FROM units";

fn map(row: &Row<'_>) -> rusqlite::Result<Unit> {
    Ok(Unit {
        id: row.get(0)?,
        code: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        decimals: row.get(5)?,
        archived: row.get(6)?,
    })
}

/// Every unit, active ones first, in creation order.
pub fn list(conn: &Connection) -> Result<Vec<Unit>, AppError> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY archived_at IS NOT NULL, rowid"))?;
    let units = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(units)
}

pub fn get(conn: &Connection, id: &str) -> Result<Unit, AppError> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("unit"))
}

fn clean(input: &UnitInput) -> Result<UnitInput, AppError> {
    let (name_ar, name_fr, name_en) = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    if input.decimals > 3 {
        return Err(AppError::validation("decimals", "value_invalid"));
    }
    Ok(UnitInput {
        code: clean_code(&input.code, "code")?,
        name_ar,
        name_fr,
        name_en,
        decimals: input.decimals,
    })
}

fn ensure_code_free(conn: &Connection, code: &str, except: Option<&str>) -> Result<(), AppError> {
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM units WHERE code = ?1 AND id IS NOT ?2)",
        params![code, except],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    Ok(())
}

pub fn create(tx: &Transaction<'_>, actor: &Actor, input: UnitInput) -> Result<Unit, AppError> {
    let u = clean(&input)?;
    ensure_code_free(tx, &u.code, None)?;
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO units (id, code, name_ar, name_fr, name_en, decimals, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![id, u.code, u.name_ar, u.name_fr, u.name_en, u.decimals, now],
    )?;
    let unit = get(tx, &id)?;
    audit::append(
        tx,
        &actor.event("unit.created", "unit", Some(&id), json!({ "after": unit })),
    )?;
    Ok(unit)
}

/// Renames a unit or changes its decimals. Fewer decimals are refused while a product uses
/// the unit (`unit_in_use`): their stored quantities could no longer be expressed.
pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: UnitInput,
) -> Result<Unit, AppError> {
    let before = get(tx, id)?;
    let u = clean(&input)?;
    ensure_code_free(tx, &u.code, Some(id))?;
    if u.decimals < before.decimals {
        let used: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM products WHERE unit_id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if used {
            return Err(AppError::conflict("unit_in_use"));
        }
    }
    tx.execute(
        "UPDATE units SET code = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5, decimals = ?6,
                updated_at = ?7 WHERE id = ?1",
        params![
            id,
            u.code,
            u.name_ar,
            u.name_fr,
            u.name_en,
            u.decimals,
            now_ms()
        ],
    )?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "unit.updated",
            "unit",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID};
    use crate::db::Db;

    fn input(code: &str, decimals: u8) -> UnitInput {
        UnitInput {
            code: code.into(),
            name_fr: "Sachet".into(),
            decimals,
            ..UnitInput::default()
        }
    }

    #[test]
    fn seeded_units_are_listed_in_order() {
        let db = Db::open_memory().unwrap();
        let units = db.read(list).unwrap();
        assert_eq!(units.len(), 14, "M1's eight and M8's six");
        assert_eq!(
            (units[0].id.as_str(), units[0].decimals),
            (UNIT_PIECE_ID, 0)
        );
        let kg = db.read(|c| get(c, UNIT_KG_ID)).unwrap();
        assert_eq!(
            (kg.code.as_str(), kg.name_ar.as_str(), kg.decimals),
            ("kg", "كلغ", 3)
        );
    }

    #[test]
    fn create_update_and_audit() {
        let db = Db::open_memory().unwrap();
        let unit = db
            .write(|tx| create(tx, &Actor::default(), input(" barquette ", 0)))
            .unwrap();
        assert_eq!(unit.code, "barquette");
        let id = unit.id.clone();
        let renamed = db
            .write(move |tx| update(tx, &Actor::default(), &id, input("sac", 2)))
            .unwrap();
        assert_eq!((renamed.code.as_str(), renamed.decimals), ("sac", 2));
        let actions: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT group_concat(action) FROM (SELECT action FROM audit_log ORDER BY seq)",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(actions, "unit.created,unit.updated");
    }

    #[test]
    fn codes_are_unique_ignoring_case_and_decimals_are_bounded() {
        let db = Db::open_memory().unwrap();
        let dup = db
            .write(|tx| create(tx, &Actor::default(), input("KG", 3)))
            .unwrap_err();
        assert_eq!(
            (dup.code(), dup.payload().field),
            ("code_taken".into(), Some("code".into()))
        );
        let bad = db
            .write(|tx| create(tx, &Actor::default(), input("x", 4)))
            .unwrap_err();
        assert_eq!(bad.code(), "value_invalid");
        assert_eq!(
            db.read(|c| get(c, "nope")).unwrap_err().code(),
            "unit_not_found"
        );
    }

    #[test]
    fn fewer_decimals_are_refused_while_a_product_uses_the_unit() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            tx.execute_batch(
                "INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at)
                   VALUES ('00000000-0000-7000-8000-000000000401', 'T', 0, 0, 0);
                 INSERT INTO products (id, name_fr, unit_id, tax_rate_id, created_at, updated_at)
                   VALUES ('00000000-0000-7000-8000-00000000a001', 'Riz', '00000000-0000-7000-8000-000000000102',
                           '00000000-0000-7000-8000-000000000401', 0, 0);",
            )?;
            Ok(())
        })
        .unwrap();
        let err = db
            .write(|tx| update(tx, &Actor::default(), UNIT_KG_ID, input("kg", 0)))
            .unwrap_err();
        assert_eq!(err.code(), "unit_in_use");
    }
}
