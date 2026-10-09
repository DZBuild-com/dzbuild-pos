//! LBL-007: labels whose shelf price no longer matches the till — the price, the running
//! promotion, the name or the barcode changed since the last print (or dismissal). A label is
//! compared on what it shows: the old price only when its template prints one.

use super::build::{self, LabelBatch};
use super::template::{self, LabelLang, LabelTemplate};
use crate::audit::{self, Actor};
use crate::catalog::products::{self, Product};
use crate::error::AppError;
use crate::money::Money;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StaleReason {
    Price,
    Promotion,
    Name,
    Barcode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StaleLabel {
    pub product_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub printed_at: i64,
    pub printed_price: Money,
    pub printed_old_price: Option<Money>,
    pub price: Money,
    pub old_price: Option<Money>,
    pub reasons: Vec<StaleReason>,
}

struct Printed {
    product_id: String,
    printed_at: i64,
    template_id: String,
    lang: String,
    price: Money,
    old_price: Option<Money>,
    name: String,
    barcode: Option<String>,
}

/// Whether labels of `template_id` print the old price; a template since deleted is assumed to.
fn shows_old_price(templates: &[LabelTemplate], template_id: &str) -> bool {
    templates
        .iter()
        .find(|t| t.id == template_id)
        .is_none_or(|t| t.fields.promo)
}

/// The codes a label of `product` may carry: its barcodes and its reference (Code 128).
fn is_current_code(product: &Product, code: &str) -> bool {
    product.barcodes.iter().any(|b| b.barcode == code) || product.reference.as_deref() == Some(code)
}

pub fn stale(conn: &Connection, day: &str, limit: u32) -> Result<Vec<StaleLabel>, AppError> {
    let company = build::company_of(conn)?;
    let active = crate::promotions::active_on(conn, day)?;
    let templates = template::templates(conn)?;
    let mut stmt = conn.prepare(
        "SELECT lp.product_id, lp.printed_at, lp.template_id, lp.lang, lp.price, lp.old_price,
                lp.name, lp.barcode
         FROM label_prints lp JOIN products p ON p.id = lp.product_id
         WHERE p.archived_at IS NULL ORDER BY lp.printed_at, lp.product_id",
    )?;
    let printed = stmt
        .query_map([], |r| {
            Ok(Printed {
                product_id: r.get(0)?,
                printed_at: r.get(1)?,
                template_id: r.get(2)?,
                lang: r.get(3)?,
                price: r.get(4)?,
                old_price: r.get(5)?,
                name: r.get(6)?,
                barcode: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    for p in printed {
        if out.len() >= limit as usize {
            break;
        }
        let product = products::get(conn, &p.product_id)?;
        let lang = LabelLang::parse(&p.lang).unwrap_or(LabelLang::Fr).primary();
        let (price, old_price) = build::price_now(company.price_mode, &active, &product, lang)?;
        let mut reasons = Vec::new();
        if price != p.price {
            reasons.push(StaleReason::Price);
        }
        if shows_old_price(&templates, &p.template_id)
            && old_price.is_some() != p.old_price.is_some()
        {
            reasons.push(StaleReason::Promotion);
        }
        if product.name_in(lang) != p.name {
            reasons.push(StaleReason::Name);
        }
        if p.barcode
            .as_deref()
            .is_some_and(|b| !is_current_code(&product, b))
        {
            reasons.push(StaleReason::Barcode);
        }
        if !reasons.is_empty() {
            out.push(StaleLabel {
                product_id: product.id.clone(),
                name_ar: product.name_ar.clone(),
                name_fr: product.name_fr.clone(),
                name_en: product.name_en.clone(),
                printed_at: p.printed_at,
                printed_price: p.price,
                printed_old_price: p.old_price,
                price,
                old_price,
                reasons,
            });
        }
    }
    Ok(out)
}

/// Marks the labels of `product_ids` up to date without printing (the shop wrote the new price
/// by hand): today's values replace the printed ones with `copies = 0`.
pub fn dismiss(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_ids: &[String],
    day: &str,
    now: i64,
) -> Result<u32, AppError> {
    let company = build::company_of(tx)?;
    let active = crate::promotions::active_on(tx, day)?;
    let templates = template::templates(tx)?;
    let mut done = 0_u32;
    for id in product_ids {
        let printed: Option<(String, String)> = tx
            .query_row(
                "SELECT lang, template_id FROM label_prints WHERE product_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((lang, template_id)) = printed else {
            continue;
        };
        let Some(lang) = LabelLang::parse(&lang) else {
            continue;
        };
        let product = products::get(tx, id)?;
        let (price, old_price) =
            build::price_now(company.price_mode, &active, &product, lang.primary())?;
        let old_price = old_price.filter(|_| shows_old_price(&templates, &template_id));
        tx.execute(
            "UPDATE label_prints SET printed_at = ?2, user_id = ?3, price = ?4, old_price = ?5,
               name = ?6,
               barcode = (SELECT barcode FROM product_barcodes WHERE product_id = ?1
                          ORDER BY rowid LIMIT 1),
               copies = 0
             WHERE product_id = ?1",
            params![
                id,
                now,
                actor.user_id,
                price,
                old_price,
                product.name_in(lang.primary())
            ],
        )?;
        done += 1;
    }
    audit::append(
        tx,
        &actor.event(
            "labels.dismissed",
            "labels",
            None,
            json!({ "products": done }),
        ),
    )?;
    Ok(done)
}

/// For callers that print the queue: the same products as `stale`, as a batch.
pub fn stale_batch(conn: &Connection, day: &str) -> Result<LabelBatch, AppError> {
    build::build(
        conn,
        &build::LabelRequest {
            source: build::LabelSource::Stale,
            template_id: None,
            lang: None,
        },
        day,
    )
}
