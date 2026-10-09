//! Wholesale and distribution (master spec §30 WHS/DIS): prices agreed with a customer for a
//! product (on top of M2's price levels, quantity breaks and standing discounts), minimum order
//! quantities, pack products sold by the carton, bag or pallet from the unit's stock (PRD-033),
//! the "BL valant facture" authorisation (CFG-003) and delivery rounds (`rounds`).
//!
//! None of these tables carries a balance; the rounds' collections become
//! M2 versements, written by the host like every other versement.

#[cfg(test)]
mod close_preview_tests;
pub mod rounds;
#[cfg(test)]
pub(crate) mod tests;

use crate::audit::{self, Actor};
use crate::catalog::products::{self, Barcode, Component, Product, ProductInput, ProductKind};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// A price agreed with a customer for a product (PRC-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerPrice {
    pub customer_id: String,
    pub product_id: String,
    pub product_name: String,
    pub reference: Option<String>,
    pub price: Money,
    /// The product's default-level price, for comparison.
    pub list_price: Money,
    pub updated_at: i64,
}

/// Agrees `price` with the customer for the product; `None` removes the agreed price.
pub fn set_price(
    tx: &Transaction<'_>,
    actor: &Actor,
    customer_id: &str,
    product_id: &str,
    price: Option<Money>,
) -> Result<Option<CustomerPrice>, AppError> {
    packs::require(tx, Module::Wholesale)?;
    crate::customers::selectable(tx, customer_id)?;
    let product = products::get(tx, product_id)?;
    if matches!(product.kind, ProductKind::VariantParent) {
        return Err(AppError::validation("productId", "customer_price_invalid"));
    }
    match price {
        Some(p) if !p.is_positive() => {
            return Err(AppError::validation("price", "customer_price_invalid"));
        }
        Some(p) => {
            tx.execute(
                "INSERT INTO customer_prices (customer_id, product_id, price, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (customer_id, product_id) DO UPDATE SET price = excluded.price, updated_at = excluded.updated_at",
                params![customer_id, product_id, p, now_ms()],
            )?;
        }
        None => {
            tx.execute(
                "DELETE FROM customer_prices WHERE customer_id = ?1 AND product_id = ?2",
                params![customer_id, product_id],
            )?;
        }
    }
    audit::append(
        tx,
        &actor.event(
            "wholesale.price_set",
            "customer",
            Some(customer_id),
            json!({ "productId": product_id, "price": price }),
        ),
    )?;
    Ok(prices_of(tx, customer_id, Lang::Fr)?
        .into_iter()
        .find(|p| p.product_id == product_id))
}

/// The prices agreed with a customer, by product name in `lang`.
pub fn prices_of(
    conn: &Connection,
    customer_id: &str,
    lang: Lang,
) -> Result<Vec<CustomerPrice>, AppError> {
    let name = crate::reports::name_of("p", lang);
    let mut stmt = conn.prepare(&format!(
        "SELECT cp.customer_id, cp.product_id, {name}, p.reference, cp.price, p.sell_price, cp.updated_at
         FROM customer_prices cp JOIN products p ON p.id = cp.product_id
         WHERE cp.customer_id = ?1 ORDER BY 3, cp.product_id"
    ))?;
    let rows = stmt
        .query_map([customer_id], |r| {
            Ok(CustomerPrice {
                customer_id: r.get(0)?,
                product_id: r.get(1)?,
                product_name: r.get(2)?,
                reference: r.get(3)?,
                price: r.get(4)?,
                list_price: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The price agreed with the customer for the product, if any (the till's pricing reads it).
pub(crate) fn price_for(
    conn: &Connection,
    customer_id: &str,
    product_id: &str,
) -> Result<Option<Money>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT price FROM customer_prices WHERE customer_id = ?1 AND product_id = ?2",
        )?
        .query_row(params![customer_id, product_id], |r| r.get(0))
        .optional()?)
}

/// Sets the product's minimum order quantity; `None` removes it.
pub fn set_min_order(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    min: Option<Qty>,
) -> Result<Option<Qty>, AppError> {
    packs::require(tx, Module::Wholesale)?;
    let product = products::get(tx, product_id)?;
    if min.is_some_and(|q| !q.is_positive() || !q.fits_decimals(product.unit_decimals)) {
        return Err(AppError::validation("minOrderQty", "qty_invalid"));
    }
    tx.execute(
        "INSERT INTO product_wholesale (product_id, min_order_qty, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (product_id) DO UPDATE SET min_order_qty = excluded.min_order_qty, updated_at = excluded.updated_at",
        params![product_id, min, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "wholesale.min_order_set",
            "product",
            Some(product_id),
            json!({ "minOrderQty": min }),
        ),
    )?;
    Ok(min)
}

/// The product's minimum order quantity, if any.
pub fn min_order(conn: &Connection, product_id: &str) -> Result<Option<Qty>, AppError> {
    Ok(conn
        .prepare_cached("SELECT min_order_qty FROM product_wholesale WHERE product_id = ?1")?
        .query_row([product_id], |r| r.get::<_, Option<Qty>>(0))
        .optional()?
        .flatten())
}

/// Setting `wholesale.min_order_policy`: a line below its minimum warns or is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MinOrderPolicy {
    Warn,
    Block,
}

pub fn min_order_policy(conn: &Connection) -> Result<MinOrderPolicy, AppError> {
    crate::settings::get(conn, "wholesale.min_order_policy")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackInput {
    pub unit_product_id: String,
    /// Units of the unit product in one pack (6 bottles = 6000).
    pub units_per_pack: Qty,
    /// The pack's unit: carton (…0108), bag (…0111), pallet (…0112), box (…0107), lot (…0114).
    pub unit_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub price: Money,
    pub barcode: Option<String>,
}

/// PRD-033: the pack is a composite product of one component — its stock is the unit's, its
/// price and barcode its own, its tax rate and category the unit's.
pub fn create_pack(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: PackInput,
) -> Result<Product, AppError> {
    packs::require(tx, Module::Wholesale)?;
    let unit = products::get(tx, &input.unit_product_id)
        .map_err(|_| AppError::validation("unitProductId", "product_not_found"))?;
    if !matches!(unit.kind, ProductKind::Simple | ProductKind::Variant)
        || unit.archived
        || input.units_per_pack.0 % Qty::ONE.0 != 0
        || input.units_per_pack.0 < 2 * Qty::ONE.0
    {
        return Err(AppError::validation("unitsPerPack", "pack_units_invalid"));
    }
    let product = products::create(
        tx,
        actor,
        ProductInput {
            name_ar: input.name_ar,
            name_fr: input.name_fr,
            name_en: input.name_en,
            category_id: unit.category_id.clone(),
            kind: ProductKind::Composite,
            barcodes: input
                .barcode
                .into_iter()
                .map(|b| Barcode {
                    barcode: b,
                    pack_qty: Qty::ONE,
                    label: None,
                })
                .collect(),
            components: vec![Component {
                component_id: unit.id.clone(),
                qty: input.units_per_pack,
            }],
            ..ProductInput::simple("", &input.unit_id, &unit.tax_rate_id, input.price)
        },
    )?;
    audit::append(
        tx,
        &actor.event(
            "wholesale.pack_created",
            "product",
            Some(&product.id),
            json!({ "unitProductId": unit.id, "unitsPerPack": input.units_per_pack }),
        ),
    )?;
    Ok(product)
}

/// The decision that lets the shop's delivery notes stand for invoices (CFG-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BlInvoiceAuthorisation {
    pub number: String,
    /// `YYYY-MM-DD`.
    pub date: String,
}

/// Setting `documents.bl_as_invoice`: the authorisation, or `None` when the mode is off.
pub fn bl_invoice_authorisation(
    conn: &Connection,
) -> Result<Option<BlInvoiceAuthorisation>, AppError> {
    let value = crate::settings::get_value(conn, "documents.bl_as_invoice")?;
    Ok(
        serde_json::from_value::<Option<BlInvoiceAuthorisation>>(value)
            .ok()
            .flatten()
            .map(|a| BlInvoiceAuthorisation {
                number: a.number.trim().to_owned(),
                date: a.date,
            }),
    )
}

#[cfg(test)]
impl PackInput {
    /// A carton of 6 at 220 DA without barcode.
    pub(crate) fn sample(unit_product_id: &str) -> PackInput {
        PackInput {
            unit_product_id: unit_product_id.into(),
            units_per_pack: Qty(6_000),
            unit_id: crate::catalog::UNIT_CARTON_ID.into(),
            name_ar: String::new(),
            name_fr: "Pack de 6".into(),
            name_en: String::new(),
            price: Money(22_000),
            barcode: None,
        }
    }
}
