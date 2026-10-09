//! The Chifa reference list: the CNAS reference price ("tarif de référence") and the reimbursable
//! flag of medicines, imported from the file the pharmacist receives (xlsx/csv, read by M2's
//! readers in the command layer), checked first and applied only when every row is valid. Rows
//! are matched to products by barcode, then by reference. Informative: the till shows it; the
//! CNAS tiers payant itself is V2+ (VRT-032).

use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::imports::parse::{boolean, money};
use crate::money::Money;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The 0-based columns of the file that hold each field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChifaMapping {
    pub code: u32,
    pub name: u32,
    pub reference_price: u32,
    pub reimbursable: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChifaProblem {
    /// 1-based row of the data (after the header).
    pub row: u32,
    pub column: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChifaCheck {
    pub rows: u32,
    pub valid: u32,
    pub matched: u32,
    pub problems: Vec<ChifaProblem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChifaApplied {
    pub created: u32,
    pub updated: u32,
    pub matched: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChifaPrice {
    pub code: String,
    pub name: String,
    pub product_id: Option<String>,
    pub reference_price: Money,
    pub reimbursable: bool,
}

struct Parsed {
    code: String,
    name: String,
    price: Money,
    reimbursable: bool,
}

fn cell(row: &[String], i: u32) -> &str {
    row.get(i as usize).map_or("", |s| s.trim())
}

fn parse_row(
    row: &[String],
    m: &ChifaMapping,
    n: u32,
    problems: &mut Vec<ChifaProblem>,
) -> Option<Parsed> {
    let mut problem = |column: &str, code: &str| {
        problems.push(ChifaProblem {
            row: n,
            column: column.into(),
            code: code.into(),
        });
    };
    let code = cell(row, m.code);
    let code_ok = !code.is_empty() && code.chars().count() <= 40;
    if code.is_empty() {
        problem("code", "required");
    } else if !code_ok {
        problem("code", "too_long");
    }
    let name = cell(row, m.name);
    let price = match money(cell(row, m.reference_price)) {
        Ok(Some(p)) if !p.is_negative() => Some(p),
        Ok(_) => {
            problem("referencePrice", "required");
            None
        }
        Err(code) => {
            problem("referencePrice", code);
            None
        }
    };
    let reimbursable = match m.reimbursable.map(|c| boolean(cell(row, c))) {
        None | Some(Ok(None)) => Some(true),
        Some(Ok(Some(b))) => Some(b),
        Some(Err(code)) => {
            problem("reimbursable", code);
            None
        }
    };
    match (code_ok, price, reimbursable) {
        (true, Some(price), Some(reimbursable)) => Some(Parsed {
            code: code.to_owned(),
            name: if name.is_empty() {
                code.to_owned()
            } else {
                name.chars().take(160).collect()
            },
            price,
            reimbursable,
        }),
        _ => None,
    }
}

/// The product a list code names: a barcode, else a live product's reference.
fn product_of(conn: &Connection, code: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT product_id FROM product_barcodes WHERE barcode = ?1
             UNION ALL SELECT id FROM products WHERE reference = ?1 AND archived_at IS NULL LIMIT 1",
        )?
        .query_row([code], |r| r.get(0))
        .optional()?)
}

/// The dry run of a list: every row parsed, its problems listed, its product matched.
pub fn check(
    conn: &Connection,
    rows: &[Vec<String>],
    m: &ChifaMapping,
) -> Result<ChifaCheck, AppError> {
    let mut problems = Vec::new();
    let (mut valid, mut matched) = (0, 0);
    for (i, row) in rows.iter().enumerate() {
        let n = u32::try_from(i + 1).unwrap_or(u32::MAX);
        if let Some(p) = parse_row(row, m, n, &mut problems) {
            valid += 1;
            if product_of(conn, &p.code)?.is_some() {
                matched += 1;
            }
        }
    }
    Ok(ChifaCheck {
        rows: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        valid,
        matched,
        problems,
    })
}

/// Writes the list (a code already known gets its new price and flag), refused as a whole when
/// any row has a problem.
pub fn apply(
    tx: &Transaction<'_>,
    actor: &Actor,
    rows: &[Vec<String>],
    m: &ChifaMapping,
    file_name: &str,
) -> Result<ChifaApplied, AppError> {
    crate::packs::require(tx, crate::packs::Module::Pharmacy)?;
    let checked = check(tx, rows, m)?;
    if !checked.problems.is_empty() {
        return Err(AppError::rejected(
            json!({ "problems": checked.problems.len() }),
            "chifa_list_invalid",
        ));
    }
    let mut out = ChifaApplied {
        created: 0,
        updated: 0,
        matched: 0,
    };
    let mut sink = Vec::new();
    let now = now_ms();
    for (i, row) in rows.iter().enumerate() {
        let n = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let Some(p) = parse_row(row, m, n, &mut sink) else {
            continue;
        };
        let product = product_of(tx, &p.code)?;
        out.matched += u32::from(product.is_some());
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM chifa_prices WHERE code = ?1)",
            [&p.code],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO chifa_prices (id, code, name, product_id, reference_price, reimbursable, source_file, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT (code) DO UPDATE SET name = excluded.name, product_id = excluded.product_id,
                 reference_price = excluded.reference_price, reimbursable = excluded.reimbursable,
                 source_file = excluded.source_file, updated_at = excluded.updated_at",
            params![
                new_id(),
                p.code,
                p.name,
                product,
                p.price,
                p.reimbursable,
                file_name,
                now
            ],
        )?;
        if exists {
            out.updated += 1;
        } else {
            out.created += 1;
        }
    }
    audit::append(
        tx,
        &actor.event(
            "pharmacy.chifa_applied",
            "chifa_prices",
            None,
            json!({ "file": file_name, "created": out.created, "updated": out.updated, "matched": out.matched }),
        ),
    )?;
    Ok(out)
}

pub fn of_product(conn: &Connection, product_id: &str) -> Result<Option<ChifaPrice>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT code, name, product_id, reference_price, reimbursable FROM chifa_prices
             WHERE product_id = ?1 LIMIT 1",
        )?
        .query_row([product_id], |r| {
            Ok(ChifaPrice {
                code: r.get(0)?,
                name: r.get(1)?,
                product_id: r.get(2)?,
                reference_price: r.get(3)?,
                reimbursable: r.get(4)?,
            })
        })
        .optional()?)
}

pub fn list(conn: &Connection, text: &str, limit: u32) -> Result<Vec<ChifaPrice>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT code, name, product_id, reference_price, reimbursable FROM chifa_prices
         WHERE ?1 = '' OR code LIKE '%' || ?1 || '%' OR name LIKE '%' || ?1 || '%' ORDER BY name LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![text.trim(), limit.clamp(1, 5_000)], |r| {
            Ok(ChifaPrice {
                code: r.get(0)?,
                name: r.get(1)?,
                product_id: r.get(2)?,
                reference_price: r.get(3)?,
                reimbursable: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
