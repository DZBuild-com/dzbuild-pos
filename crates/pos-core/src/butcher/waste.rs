//! The daily waste log (STK-026): losses, spoiled goods, invendus and own use typed together at
//! the end of the day, each line an M2 stock adjustment with its reason (journaled there), read
//! back per day with its value at cost.

use crate::auth::Permission;
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::sqlenum::sql_text_enum;
use crate::stock::{self, AdjustBatchInput, AdjustBatchLine, AdjustReason, StockMovement};
use crate::till::TillContext;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum WasteReason {
    Damage,
    Expiry,
    Loss,
    OwnUse,
}

impl WasteReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            WasteReason::Damage => "damage",
            WasteReason::Expiry => "expiry",
            WasteReason::Loss => "loss",
            WasteReason::OwnUse => "own_use",
        }
    }

    pub fn parse(s: &str) -> Option<WasteReason> {
        [
            WasteReason::Damage,
            WasteReason::Expiry,
            WasteReason::Loss,
            WasteReason::OwnUse,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }

    pub const fn adjust_reason(self) -> AdjustReason {
        match self {
            WasteReason::Damage => AdjustReason::Damage,
            WasteReason::Expiry => AdjustReason::Expiry,
            WasteReason::Loss => AdjustReason::Loss,
            WasteReason::OwnUse => AdjustReason::OwnUse,
        }
    }
}

sql_text_enum!(WasteReason);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WasteLine {
    pub product_id: String,
    /// Positive: what leaves the stock.
    pub qty: Qty,
    pub reason: WasteReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WasteInput {
    pub location_id: Option<String>,
    pub lines: Vec<WasteLine>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WasteRow {
    pub day: String,
    pub product_id: String,
    pub name: String,
    pub qty: Qty,
    pub reason: WasteReason,
    pub cost: Money,
}

/// The waste of a day and its value at cost, summed here: the screen never adds money up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WasteDay {
    pub rows: Vec<WasteRow>,
    pub total_cost: Money,
}

pub fn record(
    tx: &rusqlite::Transaction<'_>,
    ctx: &TillContext,
    input: WasteInput,
) -> Result<Vec<StockMovement>, AppError> {
    packs::require(tx, Module::Waste)?;
    if !ctx.allows(Permission::StockAdjust) {
        return Err(AppError::Permission {
            permission: Permission::StockAdjust.as_str().to_owned(),
        });
    }
    if input.lines.is_empty() {
        return Err(AppError::validation("lines", "required"));
    }
    let mut out = Vec::new();
    for reason in [
        WasteReason::Damage,
        WasteReason::Expiry,
        WasteReason::Loss,
        WasteReason::OwnUse,
    ] {
        let lines: Vec<AdjustBatchLine> = input
            .lines
            .iter()
            .filter(|l| l.reason == reason)
            .map(|l| {
                Ok(AdjustBatchLine {
                    product_id: l.product_id.clone(),
                    qty_delta: l
                        .qty
                        .checked_neg()
                        .filter(|q| q.is_negative())
                        .ok_or_else(|| AppError::validation("lines", "qty_invalid"))?,
                    unit_cost: None,
                })
            })
            .collect::<Result<_, AppError>>()?;
        if lines.is_empty() {
            continue;
        }
        out.extend(stock::adjust_batch(
            tx,
            &ctx.actor(),
            AdjustBatchInput {
                location_id: input.location_id.clone(),
                reason: reason.adjust_reason(),
                lines,
                note: input.note.clone(),
            },
            ctx.now,
        )?);
    }
    Ok(out)
}

/// The waste of one business day, per product and reason, valued at the movements' cost.
pub fn day(conn: &Connection, day: &str, lang: Lang) -> Result<Vec<WasteRow>, AppError> {
    let name = crate::reports::name_of("p", lang);
    let day_expr = crate::reports::day_of("m.created_at");
    let cost = crate::reports::value_sql("-m.qty_delta", "COALESCE(m.unit_cost, 0)");
    let mut stmt = conn.prepare(&format!(
        "SELECT {day_expr}, m.product_id, {name}, -SUM(m.qty_delta), m.reason_code, SUM({cost})
         FROM stock_movements m JOIN products p ON p.id = m.product_id
         WHERE m.reason = 'loss' AND m.reason_code IN ('damage', 'expiry', 'loss', 'own_use') AND {day_expr} = ?1
         GROUP BY m.product_id, m.reason_code ORDER BY 3"
    ))?;
    let rows = stmt
        .query_map([day], |r| {
            Ok(WasteRow {
                day: r.get(0)?,
                product_id: r.get(1)?,
                name: r.get(2)?,
                qty: r.get(3)?,
                reason: r.get(4)?,
                cost: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// [`day`] with the total of its costs.
pub fn day_summary(conn: &Connection, day_of: &str, lang: Lang) -> Result<WasteDay, AppError> {
    let rows = day(conn, day_of, lang)?;
    let total_cost = rows
        .iter()
        .try_fold(Money::ZERO, |sum, r| sum.checked_add(r.cost))
        .ok_or_else(|| AppError::validation("cost", "amount_invalid"))?;
    Ok(WasteDay { rows, total_cost })
}
