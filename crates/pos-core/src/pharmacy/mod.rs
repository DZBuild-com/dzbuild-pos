//! Pharmacy and parapharmacy (master spec §30 VRT-030, PRD-051…053, SAL-011; the officine's CNAS
//! tiers payant is V2+): medicines carry their DCI (searchable, and the key of the generics shown
//! as a substitution notice), a "controlled" flag with its schedule, and an optional Chifa
//! reference price. The controlled-medicine register (`register`) is the only place of the whole
//! product that holds health data (loi 18-07 art. 18): the pharmacy module must be on and the user
//! must hold `pharmacy.register`.

#[cfg(test)]
mod board_tests;
pub mod chifa;
pub mod register;
#[cfg(test)]
pub(crate) mod tests;

use crate::audit::{self, Actor};
use crate::catalog::normalize::normalize;
use crate::catalog::{clean_opt, products};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::stock::lots::{ExpiryRow, ExpiryStatus, expiry_list};
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PharmacyProduct {
    pub product_id: String,
    pub dci: Option<String>,
    pub controlled: bool,
    pub schedule: Option<String>,
    pub prescription_required: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PharmacyProductInput {
    pub dci: Option<String>,
    pub controlled: bool,
    pub schedule: Option<String>,
    pub prescription_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Equivalent {
    pub product_id: String,
    pub name: String,
    pub price: Money,
    pub on_hand: Qty,
    /// Cheaper than the product asked for (the substitution notice shows these first).
    pub cheaper: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpiryBoard {
    pub expired: Vec<BoardLot>,
    pub soon: Vec<BoardLot>,
    /// Value at CMUP of what can no longer be sold (the sum of its lots).
    pub expired_cost: Money,
    pub soon_cost: Money,
}

/// A lot of the expiry board with its value at the product's CMUP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoardLot {
    pub lot: ExpiryRow,
    pub cost: Money,
}

pub fn get(conn: &Connection, product_id: &str) -> Result<Option<PharmacyProduct>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT product_id, dci, controlled, schedule, prescription_required FROM pharmacy_products WHERE product_id = ?1",
        )?
        .query_row([product_id], |r| {
            Ok(PharmacyProduct {
                product_id: r.get(0)?,
                dci: r.get(1)?,
                controlled: r.get(2)?,
                schedule: r.get(3)?,
                prescription_required: r.get(4)?,
            })
        })
        .optional()?)
}

pub fn set(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    input: PharmacyProductInput,
) -> Result<PharmacyProduct, AppError> {
    packs::require(tx, Module::Pharmacy)?;
    products::get(tx, product_id)?;
    let dci = clean_opt(input.dci.as_deref(), "dci", 120)?;
    let schedule = clean_opt(input.schedule.as_deref(), "schedule", 40)?;
    let key = dci.as_deref().map(normalize);
    tx.execute(
        "INSERT INTO pharmacy_products (product_id, dci, dci_key, controlled, schedule, prescription_required, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (product_id) DO UPDATE SET dci = excluded.dci, dci_key = excluded.dci_key,
             controlled = excluded.controlled, schedule = excluded.schedule,
             prescription_required = excluded.prescription_required, updated_at = excluded.updated_at",
        params![
            product_id,
            dci,
            key,
            input.controlled,
            schedule,
            input.prescription_required,
            now_ms()
        ],
    )?;
    products::refresh_search_text(tx, product_id)?;
    audit::append(
        tx,
        &actor.event(
            "pharmacy.product_set",
            "product",
            Some(product_id),
            json!({ "dci": dci, "controlled": input.controlled, "schedule": schedule }),
        ),
    )?;
    get(tx, product_id)?.ok_or_else(|| AppError::not_found("product"))
}

/// The other live products with the same DCI, cheapest first (the substitution notice).
pub fn equivalents(
    conn: &Connection,
    product_id: &str,
    lang: Lang,
    level_id: &str,
) -> Result<Vec<Equivalent>, AppError> {
    let Some(key) = conn
        .query_row(
            "SELECT dci_key FROM pharmacy_products WHERE product_id = ?1",
            [product_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
    else {
        return Ok(Vec::new());
    };
    let asked = products::get(conn, product_id)?.price_for(level_id);
    let mut stmt = conn.prepare_cached(
        "SELECT pp.product_id FROM pharmacy_products pp JOIN products p ON p.id = pp.product_id
         WHERE pp.dci_key = ?1 AND pp.product_id <> ?2 AND p.archived_at IS NULL",
    )?;
    let ids = stmt
        .query_map(params![key, product_id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let p = products::get(conn, &id)?;
        let price = p.price_for(level_id);
        out.push(Equivalent {
            on_hand: crate::stock::on_hand(conn, &id)?,
            name: p.name_in(lang).to_owned(),
            cheaper: price < asked,
            price,
            product_id: id,
        });
    }
    out.sort_by(|a, b| a.price.cmp(&b.price).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// PRD-053: lots already expired and lots expiring within `within_days`, each with its value at
/// the product's CMUP, and the value of each table.
pub fn expiry_board(
    conn: &Connection,
    today: &str,
    within_days: i64,
) -> Result<ExpiryBoard, AppError> {
    let rows = expiry_list(conn, today, within_days)?;
    let mut board = ExpiryBoard {
        expired: Vec::new(),
        soon: Vec::new(),
        expired_cost: Money::ZERO,
        soon_cost: Money::ZERO,
    };
    for row in rows {
        let cost = products::get(conn, &row.product_id)?
            .cost_price
            .mul_qty(row.qty)
            .unwrap_or(Money::ZERO);
        let expired = row.status == ExpiryStatus::Expired;
        let lot = BoardLot { lot: row, cost };
        if expired {
            board.expired_cost = board
                .expired_cost
                .checked_add(cost)
                .unwrap_or(board.expired_cost);
            board.expired.push(lot);
        } else {
            board.soon_cost = board.soon_cost.checked_add(cost).unwrap_or(board.soon_cost);
            board.soon.push(lot);
        }
    }
    Ok(board)
}
