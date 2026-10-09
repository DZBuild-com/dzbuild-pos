//! Bulk price changes (PRD-062) and the spreadsheet-like bulk edit (PRD-070). Each product goes
//! through `products::update`/`set_price` (so the per-product audit rows and checks stay), and
//! the whole operation adds one summary audit row.

use crate::audit::{self, Actor};
use crate::catalog::price_levels;
use crate::catalog::products::{self, ProductInput};
use crate::error::AppError;
use crate::money::{Money, Qty, Rate};
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PriceChange {
    /// Signed basis points: −1000 lowers by 10 %.
    Percent(Rate),
    /// Signed centimes added to each price.
    Amount(Money),
    /// The same price for all.
    Set(Money),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkPriceInput {
    /// When not empty, exactly these products; otherwise the filters below (all active products
    /// when none is set).
    pub product_ids: Vec<String>,
    pub category_id: Option<String>,
    pub brand: Option<String>,
    pub supplier_id: Option<String>,
    /// `None`: the sale price (default level).
    pub price_level_id: Option<String>,
    pub change: PriceChange,
    /// Round to the nearest multiple (0 = none; 100 = 1 DA, 500 = 5 DA, 1000 = 10 DA).
    pub round_to: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceChangeRow {
    pub product_id: String,
    pub reference: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub before: Money,
    pub after: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkEdit {
    pub product_id: String,
    pub sell_price: Option<Money>,
    pub cost_price: Option<Money>,
    pub min_stock: Option<Qty>,
    pub category_id: Option<String>,
    pub tax_rate_id: Option<String>,
    pub available_at_till: Option<bool>,
}

fn selected(conn: &Connection, input: &BulkPriceInput) -> Result<Vec<String>, AppError> {
    if !input.product_ids.is_empty() {
        return Ok(input.product_ids.clone());
    }
    let mut stmt = conn.prepare_cached(
        "WITH RECURSIVE tree(id) AS (
             SELECT ?1 WHERE ?1 IS NOT NULL
             UNION SELECT c.id FROM categories c JOIN tree t ON c.parent_id = t.id)
         SELECT p.id FROM products p LEFT JOIN product_extras e ON e.product_id = p.id
         WHERE p.archived_at IS NULL AND p.kind <> 'variant_parent'
           AND (?1 IS NULL OR p.category_id IN (SELECT id FROM tree))
           AND (?2 IS NULL OR e.brand = ?2 COLLATE NOCASE)
           AND (?3 IS NULL OR e.preferred_supplier_id = ?3)
         ORDER BY p.name_fr",
    )?;
    let ids = stmt
        .query_map(
            params![input.category_id, input.brand, input.supplier_id],
            |r| r.get(0),
        )?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(ids)
}

fn changed(before: Money, change: PriceChange, round_to: Money) -> Result<Money, AppError> {
    let invalid = || AppError::validation("change", "amount_invalid");
    let raw = match change {
        PriceChange::Percent(bp) => {
            if !(-9_000..=100_000).contains(&bp.0) {
                return Err(invalid());
            }
            before
                .mul_div(10_000 + i64::from(bp.0), 10_000)
                .ok_or_else(invalid)?
        }
        PriceChange::Amount(delta) => before.checked_add(delta).ok_or_else(invalid)?,
        PriceChange::Set(price) => price,
    };
    let rounded = if round_to.is_positive() {
        raw.mul_div(1, round_to.0)
            .and_then(|q| q.0.checked_mul(round_to.0))
            .map(Money)
            .ok_or_else(invalid)?
    } else {
        raw
    };
    if rounded.is_negative() {
        return Err(invalid());
    }
    Ok(rounded)
}

pub fn preview_prices(
    conn: &Connection,
    input: &BulkPriceInput,
) -> Result<Vec<PriceChangeRow>, AppError> {
    let default_level = price_levels::default_level(conn)?.id;
    let level = input.price_level_id.clone().unwrap_or(default_level);
    let mut rows = Vec::new();
    for id in selected(conn, input)? {
        let p = products::get(conn, &id)?;
        let before = p.price_for(&level);
        rows.push(PriceChangeRow {
            product_id: p.id,
            reference: p.reference,
            name_ar: p.name_ar,
            name_fr: p.name_fr,
            name_en: p.name_en,
            before,
            after: changed(before, input.change, input.round_to)?,
        });
    }
    Ok(rows)
}

/// Applies the change; the number of prices that moved.
pub fn apply_prices(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: &BulkPriceInput,
) -> Result<u32, AppError> {
    let default_level = price_levels::default_level(tx)?.id;
    let rows = preview_prices(tx, input)?;
    let mut count = 0_u32;
    for row in rows.iter().filter(|r| r.before != r.after) {
        match &input.price_level_id {
            Some(level) if *level != default_level => {
                products::set_price(tx, actor, &row.product_id, level, row.after)?;
            }
            _ => {
                let p = products::get(tx, &row.product_id)?;
                let mut update = ProductInput::from(&p);
                update.sell_price = row.after;
                products::update(tx, actor, &row.product_id, update)?;
            }
        }
        count += 1;
    }
    audit::append(
        tx,
        &actor.event(
            "product.prices_bulk_updated",
            "product",
            None,
            json!({ "change": input.change, "roundTo": input.round_to,
                    "priceLevelId": input.price_level_id, "changed": count }),
        ),
    )?;
    Ok(count)
}

/// PRD-070: several products edited at once. Costs only with `may_edit_costs`
/// (`reports.profit`, USR-012).
pub fn apply_edits(
    tx: &Transaction<'_>,
    actor: &Actor,
    edits: &[BulkEdit],
    may_edit_costs: bool,
) -> Result<u32, AppError> {
    if !may_edit_costs && edits.iter().any(|e| e.cost_price.is_some()) {
        return Err(AppError::Permission {
            permission: crate::auth::Permission::ReportsProfit.as_str().to_owned(),
        });
    }
    let mut count = 0_u32;
    for (i, e) in edits.iter().enumerate() {
        let p = products::get(tx, &e.product_id).map_err(|_| {
            AppError::validation(&format!("edits.{i}.productId"), "product_not_found")
        })?;
        let mut input = ProductInput::from(&p);
        if let Some(v) = e.sell_price {
            input.sell_price = v;
        }
        if let Some(v) = e.cost_price {
            input.cost_price = v;
        }
        if let Some(v) = e.min_stock {
            input.min_stock = v;
        }
        if let Some(v) = &e.category_id {
            input.category_id = Some(v.clone());
        }
        if let Some(v) = &e.tax_rate_id {
            input.tax_rate_id = v.clone();
        }
        if let Some(v) = e.available_at_till {
            input.available_at_till = v;
        }
        products::update(tx, actor, &e.product_id, input).map_err(|err| match err {
            AppError::Validation { field, code } => {
                AppError::validation(&format!("edits.{i}.{field}"), &code)
            }
            other => other,
        })?;
        count += 1;
    }
    audit::append(
        tx,
        &actor.event(
            "product.bulk_edited",
            "product",
            None,
            json!({ "products": count }),
        ),
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn a_percent_rise_is_previewed_rounded_then_applied() {
        let shop = shop();
        let a = shop.product("Café Bonal 250 g", Money(31_000), &shop.tva19);
        let b = shop.product("Thé Chiraz 500 g", Money(47_550), &shop.tva19);
        let input = BulkPriceInput {
            product_ids: vec![a.id.clone(), b.id.clone()],
            category_id: None,
            brand: None,
            supplier_id: None,
            price_level_id: None,
            change: PriceChange::Percent(Rate(750)),
            round_to: Money(500),
        };
        let preview = shop
            .db
            .read({
                let i = input.clone();
                move |c| preview_prices(c, &i)
            })
            .unwrap();
        // 310 DA + 7.5 % = 333.25 → 335 DA; 475.50 DA + 7.5 % = 511.16 → 510 DA (nearest 5 DA).
        assert_eq!(
            preview
                .iter()
                .map(|r| (r.before, r.after))
                .collect::<Vec<_>>(),
            vec![
                (Money(31_000), Money(33_500)),
                (Money(47_550), Money(51_000))
            ]
        );
        let actor = shop.owner.actor();
        assert_eq!(
            shop.db
                .write(move |tx| apply_prices(tx, &actor, &input))
                .unwrap(),
            2
        );
        let after = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &a.id))
            .unwrap();
        assert_eq!(after.sell_price, Money(33_500));
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'product.prices_bulk_updated'"
            ),
            1
        );
    }

    #[test]
    fn a_bulk_edit_keeps_costs_out_of_reach_without_the_right() {
        let shop = shop();
        let p = shop.product("Lait Candia 1 L", Money(15_000), &shop.tva9);
        let edit = BulkEdit {
            product_id: p.id.clone(),
            sell_price: Some(Money(16_000)),
            cost_price: Some(Money(12_000)),
            min_stock: Some(Qty(24_000)),
            category_id: None,
            tax_rate_id: None,
            available_at_till: None,
        };
        let actor = shop.owner.actor();
        let e = edit.clone();
        let err = shop
            .db
            .write(move |tx| apply_edits(tx, &actor, &[e], false))
            .unwrap_err();
        assert_eq!(err.code(), "insufficient_permission");
        let actor = shop.owner.actor();
        assert_eq!(
            shop.db
                .write(move |tx| apply_edits(tx, &actor, &[edit], true))
                .unwrap(),
            1
        );
        let got = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &p.id))
            .unwrap();
        assert_eq!(
            (got.sell_price, got.cost_price, got.min_stock),
            (Money(16_000), Money(12_000), Qty(24_000))
        );
    }
}
