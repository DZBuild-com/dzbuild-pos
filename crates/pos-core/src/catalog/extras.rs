//! What M1's product row does not hold (PRD-005, 011, 050, 051, 061): brand, supplier and
//! alternative references (searchable), tags, free attributes (size, colour, compatibility),
//! a floor price, lot/serial tracking and the preferred supplier.

use crate::audit::{self, Actor};
use crate::catalog::clean_opt;
use crate::catalog::products;
use crate::error::AppError;
use crate::money::Money;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProductExtras {
    pub product_id: String,
    pub brand: Option<String>,
    pub supplier_ref: Option<String>,
    /// Other references (manufacturer, old codes), comma-separated, searchable.
    pub alt_refs: Option<String>,
    pub tags: Option<String>,
    pub attributes: BTreeMap<String, String>,
    /// Floor price (PRD-061), in the company price mode.
    pub min_price: Option<Money>,
    pub track_lots: bool,
    pub track_serials: bool,
    pub preferred_supplier_id: Option<String>,
    /// Warranty the shop gives on the product (DOC-012 certificate), 1 to 120 months.
    pub warranty_months: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProductExtrasInput {
    pub brand: Option<String>,
    pub supplier_ref: Option<String>,
    pub alt_refs: Option<String>,
    pub tags: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub min_price: Option<Money>,
    pub track_lots: bool,
    pub track_serials: bool,
    pub preferred_supplier_id: Option<String>,
    #[serde(default)]
    pub warranty_months: Option<i64>,
}

impl From<&ProductExtras> for ProductExtrasInput {
    fn from(e: &ProductExtras) -> ProductExtrasInput {
        ProductExtrasInput {
            brand: e.brand.clone(),
            supplier_ref: e.supplier_ref.clone(),
            alt_refs: e.alt_refs.clone(),
            tags: e.tags.clone(),
            attributes: e.attributes.clone(),
            min_price: e.min_price,
            track_lots: e.track_lots,
            track_serials: e.track_serials,
            preferred_supplier_id: e.preferred_supplier_id.clone(),
            warranty_months: e.warranty_months,
        }
    }
}

pub fn get(conn: &Connection, product_id: &str) -> Result<ProductExtras, AppError> {
    let row = conn
        .prepare_cached(
            "SELECT brand, supplier_ref, alt_refs, tags, attributes, min_price, track_lots,
                    track_serials, preferred_supplier_id, warranty_months
             FROM product_extras WHERE product_id = ?1",
        )?
        .query_row([product_id], |r| {
            Ok((
                ProductExtras {
                    product_id: product_id.to_owned(),
                    brand: r.get(0)?,
                    supplier_ref: r.get(1)?,
                    alt_refs: r.get(2)?,
                    tags: r.get(3)?,
                    attributes: BTreeMap::new(),
                    min_price: r.get(5)?,
                    track_lots: r.get(6)?,
                    track_serials: r.get(7)?,
                    preferred_supplier_id: r.get(8)?,
                    warranty_months: r.get(9)?,
                },
                r.get::<_, String>(4)?,
            ))
        })
        .optional()?;
    match row {
        Some((mut extras, attributes)) => {
            extras.attributes = serde_json::from_str(&attributes)?;
            Ok(extras)
        }
        None => Ok(ProductExtras {
            product_id: product_id.to_owned(),
            ..ProductExtras::default()
        }),
    }
}

fn clean(
    tx: &Transaction<'_>,
    product_id: &str,
    input: ProductExtrasInput,
) -> Result<ProductExtrasInput, AppError> {
    let product = products::get(tx, product_id)?;
    if (input.track_lots || input.track_serials) && !product.track_stock {
        let field = if input.track_lots {
            "trackLots"
        } else {
            "trackSerials"
        };
        return Err(AppError::validation(field, "product_not_stocked"));
    }
    if input.min_price.is_some_and(Money::is_negative) {
        return Err(AppError::validation("minPrice", "amount_invalid"));
    }
    if input
        .warranty_months
        .is_some_and(|m| !(1..=120).contains(&m))
    {
        return Err(AppError::validation("warrantyMonths", "value_invalid"));
    }
    if input.attributes.len() > 12 {
        return Err(AppError::validation("attributes", "value_invalid"));
    }
    let mut attributes = BTreeMap::new();
    for (key, value) in input.attributes {
        let key = clean_opt(Some(key.as_str()), "attributes", 40)?
            .ok_or_else(|| AppError::validation("attributes", "value_invalid"))?;
        if let Some(value) = clean_opt(Some(value.as_str()), "attributes", 80)? {
            attributes.insert(key, value);
        }
    }
    if let Some(supplier) = &input.preferred_supplier_id {
        let known: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM suppliers WHERE id = ?1)",
            [supplier],
            |r| r.get(0),
        )?;
        if !known {
            return Err(AppError::validation(
                "preferredSupplierId",
                "supplier_not_found",
            ));
        }
    }
    Ok(ProductExtrasInput {
        brand: clean_opt(input.brand.as_deref(), "brand", 60)?,
        supplier_ref: clean_opt(input.supplier_ref.as_deref(), "supplierRef", 48)?,
        alt_refs: clean_opt(input.alt_refs.as_deref(), "altRefs", 200)?,
        tags: clean_opt(input.tags.as_deref(), "tags", 120)?,
        attributes,
        min_price: input.min_price,
        track_lots: input.track_lots,
        track_serials: input.track_serials,
        preferred_supplier_id: input.preferred_supplier_id,
        warranty_months: input.warranty_months,
    })
}

/// Saves the extras without an audit row (the caller's operation audits: variants, imports).
pub fn set_quiet(
    tx: &Transaction<'_>,
    product_id: &str,
    input: ProductExtrasInput,
) -> Result<ProductExtras, AppError> {
    let e = clean(tx, product_id, input)?;
    tx.execute(
        "INSERT INTO product_extras (product_id, brand, supplier_ref, alt_refs, tags, attributes,
             min_price, track_lots, track_serials, preferred_supplier_id, updated_at, warranty_months)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT (product_id) DO UPDATE SET brand = excluded.brand,
             supplier_ref = excluded.supplier_ref, alt_refs = excluded.alt_refs, tags = excluded.tags,
             attributes = excluded.attributes, min_price = excluded.min_price,
             track_lots = excluded.track_lots, track_serials = excluded.track_serials,
             preferred_supplier_id = excluded.preferred_supplier_id, updated_at = excluded.updated_at,
             warranty_months = excluded.warranty_months",
        params![
            product_id,
            e.brand,
            e.supplier_ref,
            e.alt_refs,
            e.tags,
            serde_json::to_string(&e.attributes)?,
            e.min_price,
            e.track_lots,
            e.track_serials,
            e.preferred_supplier_id,
            now_ms(),
            e.warranty_months
        ],
    )?;
    products::refresh_search_text(tx, product_id)?;
    get(tx, product_id)
}

pub fn set(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    input: ProductExtrasInput,
) -> Result<ProductExtras, AppError> {
    let before = get(tx, product_id)?;
    let after = set_quiet(tx, product_id, input)?;
    audit::append(
        tx,
        &actor.event(
            "product.extras_updated",
            "product",
            Some(product_id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::search::{SearchQuery, search};
    use crate::testkit::shop;

    #[test]
    fn extras_default_then_save_and_make_references_searchable() {
        let shop = shop();
        let filter = shop.product("Filtre à huile", Money(120_000), &shop.tva19);
        let got = shop
            .db
            .read({
                let id = filter.id.clone();
                move |c| get(c, &id)
            })
            .unwrap();
        assert_eq!(
            (got.brand, got.track_lots, got.min_price),
            (None, false, None)
        );
        let input = ProductExtrasInput {
            brand: Some("Bosch".into()),
            supplier_ref: Some("0 451 103 316".into()),
            alt_refs: Some("W712/75, OC 90".into()),
            min_price: Some(Money(100_000)),
            attributes: BTreeMap::from([("Compatibilité".into(), "Clio 2".into())]),
            ..ProductExtrasInput::default()
        };
        let actor = shop.owner.actor();
        let id = filter.id.clone();
        let saved = shop
            .db
            .write(move |tx| set(tx, &actor, &id, input))
            .unwrap();
        assert_eq!(
            saved.attributes.get("Compatibilité").map(String::as_str),
            Some("Clio 2")
        );
        let hits = |text: &str| {
            let q = SearchQuery {
                text: text.into(),
                ..SearchQuery::default()
            };
            shop.db.read(move |c| search(c, &q)).unwrap().len()
        };
        assert_eq!(hits("bosch"), 1);
        assert_eq!(
            hits("w712"),
            1,
            "an alternative reference finds the product"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'product.extras_updated'"),
            1
        );
    }

    #[test]
    fn warranty_months_are_between_one_and_one_hundred_twenty() {
        let shop = shop();
        let phone = shop.product("Redmi 13", Money(3_500_000), &shop.tva19);
        let id = phone.id.clone();
        let err = shop
            .db
            .write(move |tx| {
                let input = ProductExtrasInput {
                    warranty_months: Some(0),
                    ..ProductExtrasInput::default()
                };
                set_quiet(tx, &id, input)
            })
            .unwrap_err();
        assert_eq!(err.payload().field.as_deref(), Some("warrantyMonths"));
        let id = phone.id.clone();
        shop.db
            .write(move |tx| {
                let input = ProductExtrasInput {
                    warranty_months: Some(12),
                    ..ProductExtrasInput::default()
                };
                set_quiet(tx, &id, input)
            })
            .unwrap();
        let extras = shop.db.read(move |c| get(c, &phone.id)).unwrap();
        assert_eq!(extras.warranty_months, Some(12));
    }

    #[test]
    fn lots_and_serials_only_on_stocked_products() {
        let shop = shop();
        let service = shop.product_with({
            let mut i = crate::catalog::products::ProductInput::simple(
                "Photocopie",
                crate::catalog::UNIT_PIECE_ID,
                &shop.tva19,
                Money(1_000),
            );
            i.kind = crate::catalog::products::ProductKind::Service;
            i.track_stock = false;
            i
        });
        let input = ProductExtrasInput {
            track_lots: true,
            ..ProductExtrasInput::default()
        };
        let actor = shop.owner.actor();
        let err = shop
            .db
            .write(move |tx| set(tx, &actor, &service.id, input))
            .unwrap_err();
        assert_eq!(
            err,
            AppError::validation("trackLots", "product_not_stocked")
        );
    }
}
