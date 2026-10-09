//! VAT rates as data (TAX-001): copied from the regulatory data pack, then switched on/off and
//! end-dated by the owner. A document line stores the rate in force when it was sold.

use crate::audit::{self, Actor};
use crate::datapack::Pack;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Rate;
use crate::time::now_ms;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxRate {
    pub id: String,
    pub code: String,
    pub rate_bp: Rate,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub active: bool,
    /// First business date the rate applies (`YYYY-MM-DD`), inclusive.
    pub valid_from: Option<String>,
    /// Last business date the rate applies, inclusive (end-dated exemptions).
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxRateUpdate {
    pub active: bool,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

impl TaxRate {
    /// True when the rate may be used on a document of `business_date` (`YYYY-MM-DD`).
    pub fn usable_on(&self, business_date: &str) -> bool {
        self.active
            && self
                .valid_from
                .as_deref()
                .is_none_or(|from| from <= business_date)
            && self
                .valid_to
                .as_deref()
                .is_none_or(|to| business_date <= to)
    }
}

const SELECT: &str = "SELECT id, code, rate_bp, name_ar, name_fr, name_en, active, valid_from, valid_to FROM tax_rates";

fn map(row: &Row<'_>) -> rusqlite::Result<TaxRate> {
    Ok(TaxRate {
        id: row.get(0)?,
        code: row.get(1)?,
        rate_bp: row.get(2)?,
        name_ar: row.get(3)?,
        name_fr: row.get(4)?,
        name_en: row.get(5)?,
        active: row.get(6)?,
        valid_from: row.get(7)?,
        valid_to: row.get(8)?,
    })
}

/// Every rate, highest first.
pub fn list(conn: &Connection) -> Result<Vec<TaxRate>, AppError> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY rate_bp DESC, code"))?;
    let rates = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rates)
}

pub fn get(conn: &Connection, id: &str) -> Result<TaxRate, AppError> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("tax_rate"))
}

pub fn by_code(conn: &Connection, code: &str) -> Result<Option<TaxRate>, AppError> {
    Ok(conn
        .query_row(&format!("{SELECT} WHERE code = ?1"), [code], map)
        .optional()?)
}

/// Inserts the pack's rates that are missing (by code) and aligns the rate and names of the
/// ones that changed in a new pack; audits `tax_rate.synced` when anything changed. Returns the
/// number of rows inserted or changed. Never deactivates a rate the owner still uses.
pub fn sync_from_pack(tx: &Transaction<'_>, pack: &Pack) -> Result<usize, AppError> {
    let now = now_ms();
    let mut changes = Vec::new();
    for rate in &pack.vat.rates {
        match by_code(tx, &rate.code)? {
            None => {
                tx.execute(
                    "INSERT INTO tax_rates (id, code, rate_bp, name_ar, name_fr, name_en, source,
                                            created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        new_id(),
                        rate.code,
                        rate.rate_bp,
                        rate.name_ar,
                        rate.name_fr,
                        rate.name_en,
                        rate.source,
                        now
                    ],
                )?;
                changes.push(json!({ "code": rate.code, "rateBp": rate.rate_bp, "before": null }));
            }
            Some(existing)
                if existing.rate_bp.0 != rate.rate_bp
                    || existing.name_ar != rate.name_ar
                    || existing.name_fr != rate.name_fr
                    || existing.name_en != rate.name_en =>
            {
                tx.execute(
                    "UPDATE tax_rates SET rate_bp = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5,
                            source = ?6, updated_at = ?7 WHERE id = ?1",
                    params![
                        existing.id,
                        rate.rate_bp,
                        rate.name_ar,
                        rate.name_fr,
                        rate.name_en,
                        rate.source,
                        now
                    ],
                )?;
                changes.push(
                    json!({ "code": rate.code, "rateBp": rate.rate_bp, "before": existing.rate_bp }),
                );
            }
            Some(_) => {}
        }
    }
    if !changes.is_empty() {
        audit::append(
            tx,
            &Actor::system(None).event(
                "tax_rate.synced",
                "tax_rate",
                None,
                json!({ "pack": pack.id, "changes": changes }),
            ),
        )?;
    }
    Ok(changes.len())
}

fn clean_date(value: Option<&str>, field: &str) -> Result<Option<String>, AppError> {
    let Some(text) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map(|d| Some(d.format("%Y-%m-%d").to_string()))
        .map_err(|_| AppError::validation(field, "value_invalid"))
}

/// Switches a rate on or off and sets its validity dates (AUD-007: audited before/after).
pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: TaxRateUpdate,
) -> Result<TaxRate, AppError> {
    let before = get(tx, id)?;
    let valid_from = clean_date(input.valid_from.as_deref(), "validFrom")?;
    let valid_to = clean_date(input.valid_to.as_deref(), "validTo")?;
    if let (Some(from), Some(to)) = (&valid_from, &valid_to)
        && to < from
    {
        return Err(AppError::validation("validTo", "value_invalid"));
    }
    tx.execute(
        "UPDATE tax_rates SET active = ?2, valid_from = ?3, valid_to = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, input.active, valid_from, valid_to, now_ms()],
    )?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "tax_rate.updated",
            "tax_rate",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn synced() -> Db {
        let db = Db::open_memory().unwrap();
        let pack = crate::datapack::bundled().unwrap();
        db.write(move |tx| sync_from_pack(tx, &pack)).unwrap();
        db
    }

    #[test]
    fn the_pack_rates_are_copied_once() {
        let db = synced();
        let rates = db.read(list).unwrap();
        let summary: Vec<(&str, i32, bool)> = rates
            .iter()
            .map(|r| (r.code.as_str(), r.rate_bp.0, r.active))
            .collect();
        assert_eq!(
            summary,
            [
                ("TVA19", 1_900, true),
                ("TVA9", 900, true),
                ("EXO", 0, true)
            ]
        );
        assert_eq!(rates[0].name_fr, "TVA 19 %");
        let pack = crate::datapack::bundled().unwrap();
        assert_eq!(db.write(move |tx| sync_from_pack(tx, &pack)).unwrap(), 0);
        let synced: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'tax_rate.synced'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(synced, 1);
    }

    #[test]
    fn a_new_pack_changes_the_rate_for_future_sales() {
        let db = synced();
        let mut pack = crate::datapack::bundled().unwrap();
        pack.vat.rates[1].rate_bp = 700;
        assert_eq!(db.write(move |tx| sync_from_pack(tx, &pack)).unwrap(), 1);
        let reduced = db.read(|c| by_code(c, "TVA9")).unwrap().unwrap();
        assert_eq!(reduced.rate_bp, Rate(700));
    }

    #[test]
    fn end_dated_rates_are_usable_inside_their_window_only() {
        let db = synced();
        let exo = db.read(|c| by_code(c, "EXO")).unwrap().unwrap();
        let id = exo.id.clone();
        let update_input = TaxRateUpdate {
            active: true,
            valid_from: None,
            valid_to: Some("2026-12-31".into()),
        };
        let exo = db
            .write(move |tx| update(tx, &Actor::default(), &id, update_input))
            .unwrap();
        assert!(exo.usable_on("2026-12-31"));
        assert!(!exo.usable_on("2027-01-01"));
        let id = exo.id.clone();
        let off = TaxRateUpdate {
            active: false,
            ..TaxRateUpdate::default()
        };
        let exo = db
            .write(move |tx| update(tx, &Actor::default(), &id, off))
            .unwrap();
        assert!(!exo.usable_on("2026-06-01"));
        let id = exo.id.clone();
        let backwards = TaxRateUpdate {
            active: true,
            valid_from: Some("2026-12-31".into()),
            valid_to: Some("2026-01-01".into()),
        };
        let err = db
            .write(move |tx| update(tx, &Actor::default(), &id, backwards))
            .unwrap_err();
        assert_eq!(err.payload().field.as_deref(), Some("validTo"));
        let id = exo.id.clone();
        let garbage = TaxRateUpdate {
            active: true,
            valid_from: Some("31/12/2026".into()),
            valid_to: None,
        };
        let err = db
            .write(move |tx| update(tx, &Actor::default(), &id, garbage))
            .unwrap_err();
        assert_eq!(err.code(), "value_invalid");
    }
}
