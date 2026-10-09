//! Products (spec §5.1 `products`, PRD-001..074 MVP subset): kinds, names in three languages,
//! several barcodes with pack quantities, one price per level, composite components, weighed
//! and PLU items, archive. Every write is audited with before/after snapshots (AUD-002).

use super::{CODE_MAX, clean_names, clean_opt, normalize, price_levels, tax_rates, units};
use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::sqlenum::sql_text_enum;
use crate::table::Table;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    /// A stocked item.
    #[default]
    Simple,
    /// The model of a variant family (size × colour); never sold itself.
    VariantParent,
    /// One size/colour of a variant family, stocked on its own.
    Variant,
    /// A kit whose stock comes from its components (PRD-040).
    Composite,
    /// Labour, photocopies, pressing: no stock.
    Service,
}

impl ProductKind {
    pub const ALL: [ProductKind; 5] = [
        ProductKind::Simple,
        ProductKind::VariantParent,
        ProductKind::Variant,
        ProductKind::Composite,
        ProductKind::Service,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ProductKind::Simple => "simple",
            ProductKind::VariantParent => "variant_parent",
            ProductKind::Variant => "variant",
            ProductKind::Composite => "composite",
            ProductKind::Service => "service",
        }
    }

    pub fn parse(s: &str) -> Option<ProductKind> {
        ProductKind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// Kinds that have their own stock ledger.
    pub const fn holds_stock(self) -> bool {
        matches!(self, ProductKind::Simple | ProductKind::Variant)
    }

    /// Kinds that can be a line of a sale.
    pub const fn sellable(self) -> bool {
        !matches!(self, ProductKind::VariantParent)
    }
}

sql_text_enum!(ProductKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Barcode {
    pub barcode: String,
    /// Units added by one scan: 1 piece = 1000; a carton of 24 = 24000 (PRD-032).
    pub pack_qty: Qty,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LevelPrice {
    pub price_level_id: String,
    pub price: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub component_id: String,
    /// Quantity of the component in one kit.
    pub qty: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: String,
    pub reference: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub category_id: Option<String>,
    pub unit_id: String,
    pub unit_code: String,
    pub unit_decimals: u8,
    pub tax_rate_id: String,
    pub tax_rate_bp: Rate,
    pub kind: ProductKind,
    pub parent_id: Option<String>,
    /// Price at the default level, in the company's price mode (TTC or HT).
    pub sell_price: Money,
    /// Weighted average cost (CMUP), HT.
    pub cost_price: Money,
    pub min_stock: Qty,
    pub track_stock: bool,
    pub allow_negative_stock: bool,
    pub is_weighed: bool,
    pub available_at_till: bool,
    pub plu: Option<u32>,
    pub image: Option<String>,
    pub archived: bool,
    pub barcodes: Vec<Barcode>,
    /// Prices at the other active levels; a level without a row sells at `sell_price`.
    pub prices: Vec<LevelPrice>,
    pub components: Vec<Component>,
    pub updated_at: i64,
}

impl Product {
    /// The unit price at `price_level_id`: its own price when set, else `sell_price`.
    pub fn price_for(&self, price_level_id: &str) -> Money {
        self.prices
            .iter()
            .find(|p| p.price_level_id == price_level_id)
            .map_or(self.sell_price, |p| p.price)
    }

    /// The name printed on a document in `lang`, with the spec §5 fallback fr → ar → en.
    pub fn name_in(&self, lang: crate::lang::Lang) -> &str {
        use crate::lang::Lang;
        let preferred = match lang {
            Lang::Ar => &self.name_ar,
            Lang::Fr => &self.name_fr,
            Lang::En => &self.name_en,
        };
        [preferred, &self.name_fr, &self.name_ar, &self.name_en]
            .into_iter()
            .find(|n| !n.is_empty())
            .map_or("", String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProductInput {
    pub reference: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub category_id: Option<String>,
    pub unit_id: String,
    pub tax_rate_id: String,
    pub kind: ProductKind,
    pub parent_id: Option<String>,
    pub sell_price: Money,
    pub cost_price: Money,
    pub min_stock: Qty,
    pub track_stock: bool,
    pub allow_negative_stock: bool,
    pub is_weighed: bool,
    pub available_at_till: bool,
    pub plu: Option<u32>,
    pub image: Option<String>,
    pub barcodes: Vec<Barcode>,
    pub prices: Vec<LevelPrice>,
    pub components: Vec<Component>,
}

impl ProductInput {
    /// A stocked piece sold at `sell_price` (quick add, tests).
    pub fn simple(
        name_fr: &str,
        unit_id: &str,
        tax_rate_id: &str,
        sell_price: Money,
    ) -> ProductInput {
        ProductInput {
            reference: None,
            name_ar: String::new(),
            name_fr: name_fr.to_owned(),
            name_en: String::new(),
            category_id: None,
            unit_id: unit_id.to_owned(),
            tax_rate_id: tax_rate_id.to_owned(),
            kind: ProductKind::Simple,
            parent_id: None,
            sell_price,
            cost_price: Money::ZERO,
            min_stock: Qty::ZERO,
            track_stock: true,
            allow_negative_stock: false,
            is_weighed: false,
            available_at_till: true,
            plu: None,
            image: None,
            barcodes: Vec::new(),
            prices: Vec::new(),
            components: Vec::new(),
        }
    }
}

impl From<&Product> for ProductInput {
    fn from(p: &Product) -> ProductInput {
        ProductInput {
            reference: p.reference.clone(),
            name_ar: p.name_ar.clone(),
            name_fr: p.name_fr.clone(),
            name_en: p.name_en.clone(),
            category_id: p.category_id.clone(),
            unit_id: p.unit_id.clone(),
            tax_rate_id: p.tax_rate_id.clone(),
            kind: p.kind,
            parent_id: p.parent_id.clone(),
            sell_price: p.sell_price,
            cost_price: p.cost_price,
            min_stock: p.min_stock,
            track_stock: p.track_stock,
            allow_negative_stock: p.allow_negative_stock,
            is_weighed: p.is_weighed,
            available_at_till: p.available_at_till,
            plu: p.plu,
            image: p.image.clone(),
            barcodes: p.barcodes.clone(),
            prices: p.prices.clone(),
            components: p.components.clone(),
        }
    }
}

const SELECT: &str = "SELECT p.id, p.reference, p.name_ar, p.name_fr, p.name_en, p.category_id,
        p.unit_id, u.code, u.decimals, p.tax_rate_id, t.rate_bp, p.kind, p.parent_id, p.sell_price,
        p.cost_price, p.min_stock, p.track_stock, p.allow_negative_stock, p.is_weighed,
        p.available_at_till, p.plu, p.image, p.archived_at IS NOT NULL, p.updated_at
     FROM products p JOIN units u ON u.id = p.unit_id JOIN tax_rates t ON t.id = p.tax_rate_id";

fn map(row: &Row<'_>) -> rusqlite::Result<Product> {
    Ok(Product {
        id: row.get(0)?,
        reference: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        category_id: row.get(5)?,
        unit_id: row.get(6)?,
        unit_code: row.get(7)?,
        unit_decimals: row.get(8)?,
        tax_rate_id: row.get(9)?,
        tax_rate_bp: row.get(10)?,
        kind: row.get(11)?,
        parent_id: row.get(12)?,
        sell_price: row.get(13)?,
        cost_price: row.get(14)?,
        min_stock: row.get(15)?,
        track_stock: row.get(16)?,
        allow_negative_stock: row.get(17)?,
        is_weighed: row.get(18)?,
        available_at_till: row.get(19)?,
        plu: row.get(20)?,
        image: row.get(21)?,
        archived: row.get(22)?,
        updated_at: row.get(23)?,
        barcodes: Vec::new(),
        prices: Vec::new(),
        components: Vec::new(),
    })
}

fn load_children(conn: &Connection, product: &mut Product) -> Result<(), AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT barcode, pack_qty, label FROM product_barcodes WHERE product_id = ?1 ORDER BY rowid",
    )?;
    product.barcodes = stmt
        .query_map([&product.id], |r| {
            Ok(Barcode {
                barcode: r.get(0)?,
                pack_qty: r.get(1)?,
                label: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // Prices of archived levels stay in the table as history but are not the product's any more.
    let mut stmt = conn.prepare_cached(
        "SELECT pp.price_level_id, pp.price FROM product_prices pp
         JOIN price_levels l ON l.id = pp.price_level_id
         WHERE pp.product_id = ?1 AND l.archived_at IS NULL ORDER BY l.sort_order, l.rowid",
    )?;
    product.prices = stmt
        .query_map([&product.id], |r| {
            Ok(LevelPrice {
                price_level_id: r.get(0)?,
                price: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT component_id, qty FROM product_components WHERE product_id = ?1 ORDER BY rowid",
    )?;
    product.components = stmt
        .query_map([&product.id], |r| {
            Ok(Component {
                component_id: r.get(0)?,
                qty: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

/// A product with its barcodes, prices and components; archived ones included.
pub fn get(conn: &Connection, id: &str) -> Result<Product, AppError> {
    let mut product = conn
        .query_row(&format!("{SELECT} WHERE p.id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("product"))?;
    load_children(conn, &mut product)?;
    Ok(product)
}

/// A product that can be put on a sale: exists, not archived, available at the till, not a
/// variant model (`product_not_found`, `product_not_sellable`).
pub fn sellable(conn: &Connection, id: &str) -> Result<Product, AppError> {
    let product = get(conn, id)?;
    if product.archived {
        return Err(AppError::not_found("product"));
    }
    if !product.available_at_till || !product.kind.sellable() {
        return Err(AppError::conflict("product_not_sellable"));
    }
    Ok(product)
}

struct Clean {
    input: ProductInput,
    unit_decimals: u8,
}

fn exists(conn: &Connection, sql: &str, p: impl rusqlite::Params) -> Result<bool, AppError> {
    Ok(conn.query_row(sql, p, |r| r.get(0))?)
}

fn clean(conn: &Connection, id: Option<&str>, input: &ProductInput) -> Result<Clean, AppError> {
    let (name_ar, name_fr, name_en) = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    let reference = clean_opt(input.reference.as_deref(), "reference", CODE_MAX)?;
    if let Some(r) = &reference
        && exists(
            conn,
            "SELECT EXISTS (SELECT 1 FROM products WHERE reference = ?1 AND id IS NOT ?2)",
            params![r, id],
        )?
    {
        return Err(AppError::validation("reference", "reference_taken"));
    }
    let unit = units::get(conn, &input.unit_id)
        .ok()
        .filter(|u| !u.archived)
        .ok_or_else(|| AppError::validation("unitId", "unit_not_found"))?;
    let rate = tax_rates::get(conn, &input.tax_rate_id)
        .map_err(|_| AppError::validation("taxRateId", "tax_rate_not_found"))?;
    if !rate.active {
        return Err(AppError::validation("taxRateId", "tax_rate_inactive"));
    }
    let category_id = clean_opt(input.category_id.as_deref(), "categoryId", 36)?;
    if let Some(c) = &category_id
        && !exists(
            conn,
            "SELECT EXISTS (SELECT 1 FROM categories WHERE id = ?1 AND archived_at IS NULL)",
            [c],
        )?
    {
        return Err(AppError::validation("categoryId", "category_not_found"));
    }
    let parent_id = clean_opt(input.parent_id.as_deref(), "parentId", 36)?;
    match (input.kind, &parent_id) {
        (ProductKind::Variant, Some(parent)) => {
            let ok = exists(
                conn,
                "SELECT EXISTS (SELECT 1 FROM products WHERE id = ?1 AND kind = 'variant_parent'
                                AND archived_at IS NULL)",
                [parent],
            )?;
            if !ok {
                return Err(AppError::validation("parentId", "variant_parent_required"));
            }
        }
        (ProductKind::Variant, None) => {
            return Err(AppError::validation("parentId", "variant_parent_required"));
        }
        (_, Some(_)) => return Err(AppError::validation("parentId", "value_invalid")),
        (_, None) => {}
    }
    for (field, amount) in [
        ("sellPrice", input.sell_price),
        ("costPrice", input.cost_price),
    ] {
        if amount.is_negative() {
            return Err(AppError::validation(field, "amount_invalid"));
        }
    }
    if input.min_stock.is_negative() || !input.min_stock.fits_decimals(unit.decimals) {
        return Err(AppError::validation("minStock", "qty_invalid"));
    }
    if input.is_weighed && unit.decimals == 0 {
        return Err(AppError::validation("isWeighed", "unit_not_weighable"));
    }
    if let Some(plu) = input.plu {
        if !(1..=99_999).contains(&plu) {
            return Err(AppError::validation("plu", "value_invalid"));
        }
        if exists(
            conn,
            "SELECT EXISTS (SELECT 1 FROM products WHERE plu = ?1 AND id IS NOT ?2)",
            params![plu, id],
        )? {
            return Err(AppError::validation("plu", "plu_taken"));
        }
    }
    let image = clean_opt(input.image.as_deref(), "image", 260)?;
    if image
        .as_deref()
        .is_some_and(|i| !super::images::is_valid_name(i))
    {
        return Err(AppError::validation("image", "value_invalid"));
    }
    let barcodes = clean_barcodes(conn, id, &input.barcodes, unit.decimals)?;
    let prices = clean_prices(conn, &input.prices)?;
    let components = clean_components(conn, id, input.kind, &input.components)?;
    Ok(Clean {
        input: ProductInput {
            reference,
            name_ar,
            name_fr,
            name_en,
            category_id,
            unit_id: unit.id,
            tax_rate_id: rate.id,
            kind: input.kind,
            parent_id,
            sell_price: input.sell_price,
            cost_price: input.cost_price,
            min_stock: input.min_stock,
            track_stock: input.kind.holds_stock() && input.track_stock,
            allow_negative_stock: input.allow_negative_stock,
            is_weighed: input.is_weighed,
            available_at_till: input.available_at_till,
            plu: input.plu,
            image,
            barcodes,
            prices,
            components,
        },
        unit_decimals: unit.decimals,
    })
}

fn clean_barcodes(
    conn: &Connection,
    id: Option<&str>,
    barcodes: &[Barcode],
    decimals: u8,
) -> Result<Vec<Barcode>, AppError> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(barcodes.len());
    for (i, b) in barcodes.iter().enumerate() {
        let code = b.barcode.trim().to_owned();
        let field = format!("barcodes.{i}.barcode");
        if code.is_empty()
            || code.chars().count() > CODE_MAX
            || !code.chars().all(|c| c.is_ascii_graphic())
        {
            return Err(AppError::validation(&field, "barcode_invalid"));
        }
        if !b.pack_qty.is_positive() || !b.pack_qty.fits_decimals(decimals) {
            return Err(AppError::validation(
                &format!("barcodes.{i}.packQty"),
                "qty_invalid",
            ));
        }
        let taken_elsewhere = exists(
            conn,
            "SELECT EXISTS (SELECT 1 FROM product_barcodes WHERE barcode = ?1 AND product_id IS NOT ?2)",
            params![code, id],
        )?;
        if !seen.insert(code.clone()) || taken_elsewhere {
            return Err(AppError::validation(&field, "barcode_taken"));
        }
        out.push(Barcode {
            barcode: code,
            pack_qty: b.pack_qty,
            label: clean_opt(b.label.as_deref(), &format!("barcodes.{i}.label"), CODE_MAX)?,
        });
    }
    Ok(out)
}

fn clean_prices(conn: &Connection, prices: &[LevelPrice]) -> Result<Vec<LevelPrice>, AppError> {
    let default_level = price_levels::default_level(conn)?;
    let mut seen = BTreeSet::new();
    for (i, p) in prices.iter().enumerate() {
        let field = format!("prices.{i}.priceLevelId");
        let level = price_levels::active(conn, &p.price_level_id)
            .map_err(|_| AppError::validation(&field, "price_level_not_found"))?;
        // The default level's price is `sellPrice`; a second copy could disagree with it.
        if level.id == default_level.id || !seen.insert(level.id) {
            return Err(AppError::validation(&field, "value_invalid"));
        }
        if p.price.is_negative() {
            return Err(AppError::validation(
                &format!("prices.{i}.price"),
                "amount_invalid",
            ));
        }
    }
    Ok(prices.to_vec())
}

fn clean_components(
    conn: &Connection,
    id: Option<&str>,
    kind: ProductKind,
    components: &[Component],
) -> Result<Vec<Component>, AppError> {
    if kind != ProductKind::Composite {
        if components.is_empty() {
            return Ok(Vec::new());
        }
        return Err(AppError::validation("components", "value_invalid"));
    }
    if components.is_empty() {
        return Err(AppError::validation("components", "components_required"));
    }
    let mut seen = BTreeSet::new();
    for (i, c) in components.iter().enumerate() {
        let field = format!("components.{i}.componentId");
        let stocked = exists(
            conn,
            "SELECT EXISTS (SELECT 1 FROM products WHERE id = ?1 AND kind IN ('simple', 'variant')
                            AND archived_at IS NULL)",
            [&c.component_id],
        )?;
        if !stocked || Some(c.component_id.as_str()) == id || !seen.insert(c.component_id.clone()) {
            return Err(AppError::validation(&field, "component_invalid"));
        }
        if !c.qty.is_positive() {
            return Err(AppError::validation(
                &format!("components.{i}.qty"),
                "qty_invalid",
            ));
        }
    }
    Ok(components.to_vec())
}

fn search_text(p: &ProductInput) -> String {
    let mut parts: Vec<&str> = vec![&p.name_ar, &p.name_fr, &p.name_en];
    parts.extend(p.reference.as_deref());
    parts.extend(p.barcodes.iter().map(|b| b.barcode.as_str()));
    normalize::search_text(parts)
}

/// Rebuilds `search_text` from the names, reference, barcodes, the extras' brand, supplier
/// reference and other references, and a medicine's DCI (PRD-011, SAL-011); the FTS trigger
/// follows.
pub(crate) fn refresh_search_text(tx: &Transaction<'_>, id: &str) -> Result<(), AppError> {
    let p = get(tx, id)?;
    let extras = super::extras::get(tx, id)?;
    let mut parts: Vec<&str> = vec![&p.name_ar, &p.name_fr, &p.name_en];
    if let Some(reference) = &p.reference {
        parts.push(reference);
    }
    parts.extend(p.barcodes.iter().map(|b| b.barcode.as_str()));
    parts.extend(
        [&extras.brand, &extras.supplier_ref, &extras.alt_refs]
            .into_iter()
            .flatten()
            .map(String::as_str),
    );
    let dci: Option<String> = tx
        .query_row(
            "SELECT dci FROM pharmacy_products WHERE product_id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(dci) = &dci {
        parts.push(dci);
    }
    let text = normalize::search_text(parts);
    tx.execute(
        "UPDATE products SET search_text = ?2 WHERE id = ?1 AND search_text <> ?2",
        params![id, text],
    )?;
    Ok(())
}

fn write_children(
    tx: &Transaction<'_>,
    id: &str,
    p: &ProductInput,
    now: i64,
) -> Result<(), AppError> {
    tx.execute("DELETE FROM product_barcodes WHERE product_id = ?1", [id])?;
    tx.execute(
        "DELETE FROM product_prices WHERE product_id = ?1
             AND price_level_id IN (SELECT id FROM price_levels WHERE archived_at IS NULL)",
        [id],
    )?;
    tx.execute("DELETE FROM product_components WHERE product_id = ?1", [id])?;
    for b in &p.barcodes {
        tx.execute(
            "INSERT INTO product_barcodes (id, product_id, barcode, pack_qty, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![new_id(), id, b.barcode, b.pack_qty, b.label, now],
        )?;
    }
    for price in &p.prices {
        tx.execute(
            "INSERT INTO product_prices (product_id, price_level_id, price, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, price.price_level_id, price.price, now],
        )?;
    }
    for c in &p.components {
        tx.execute(
            "INSERT INTO product_components (product_id, component_id, qty) VALUES (?1, ?2, ?3)",
            params![id, c.component_id, c.qty],
        )?;
    }
    Ok(())
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: ProductInput,
) -> Result<Product, AppError> {
    let Clean { input: p, .. } = clean(tx, None, &input)?;
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO products (id, reference, name_ar, name_fr, name_en, search_text, category_id,
             unit_id, tax_rate_id, kind, parent_id, sell_price, cost_price, min_stock, track_stock,
             allow_negative_stock, is_weighed, available_at_till, plu, image, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?21)",
        params![
            id,
            p.reference,
            p.name_ar,
            p.name_fr,
            p.name_en,
            search_text(&p),
            p.category_id,
            p.unit_id,
            p.tax_rate_id,
            p.kind,
            p.parent_id,
            p.sell_price,
            p.cost_price,
            p.min_stock,
            p.track_stock,
            p.allow_negative_stock,
            p.is_weighed,
            p.available_at_till,
            p.plu,
            p.image,
            now
        ],
    )?;
    write_children(tx, &id, &p, now)?;
    let product = get(tx, &id)?;
    refresh_search_text(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "product.created",
            "product",
            Some(&id),
            json!({ "after": product }),
        ),
    )?;
    Ok(product)
}

/// Replaces every field, barcode, price and component (prices of archived levels stay as
/// history). The kind of a product that has stock movements or has been sold is locked
/// (`product_kind_locked`).
pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: ProductInput,
) -> Result<Product, AppError> {
    let before = get(tx, id)?;
    let Clean {
        input: p,
        unit_decimals,
    } = clean(tx, Some(id), &input)?;
    let used = exists(
        tx,
        "SELECT EXISTS (SELECT 1 FROM stock_movements WHERE product_id = ?1)
             OR EXISTS (SELECT 1 FROM document_lines WHERE product_id = ?1)",
        [id],
    )?;
    if used && (p.kind != before.kind || unit_decimals < before.unit_decimals) {
        return Err(AppError::conflict("product_kind_locked"));
    }
    // An untracked product's stock leaves every level and valuation: it must be zero first.
    if before.track_stock && !p.track_stock && holds_stock(tx, id)? {
        return Err(AppError::conflict("stock_not_zero"));
    }
    let now = now_ms();
    tx.execute(
        "UPDATE products SET reference = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5,
             search_text = ?6, category_id = ?7, unit_id = ?8, tax_rate_id = ?9, kind = ?10,
             parent_id = ?11, sell_price = ?12, cost_price = ?13, min_stock = ?14, track_stock = ?15,
             allow_negative_stock = ?16, is_weighed = ?17, available_at_till = ?18, plu = ?19,
             image = ?20, updated_at = ?21
         WHERE id = ?1",
        params![
            id,
            p.reference,
            p.name_ar,
            p.name_fr,
            p.name_en,
            search_text(&p),
            p.category_id,
            p.unit_id,
            p.tax_rate_id,
            p.kind,
            p.parent_id,
            p.sell_price,
            p.cost_price,
            p.min_stock,
            p.track_stock,
            p.allow_negative_stock,
            p.is_weighed,
            p.available_at_till,
            p.plu,
            p.image,
            now
        ],
    )?;
    write_children(tx, id, &p, now)?;
    let after = get(tx, id)?;
    refresh_search_text(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "product.updated",
            "product",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

/// Changes one price (`products.price` permission in the command): the default level's price
/// is `sell_price`, another level's is its `product_prices` row (AUD-007).
pub fn set_price(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    price_level_id: &str,
    price: Money,
) -> Result<Product, AppError> {
    let before = get(tx, id)?;
    if price.is_negative() {
        return Err(AppError::validation("price", "amount_invalid"));
    }
    let level = price_levels::active(tx, price_level_id)
        .map_err(|_| AppError::validation("priceLevelId", "price_level_not_found"))?;
    let now = now_ms();
    if level.is_default {
        tx.execute(
            "UPDATE products SET sell_price = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, price, now],
        )?;
    } else {
        tx.execute(
            "INSERT INTO product_prices (product_id, price_level_id, price, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (product_id, price_level_id) DO UPDATE SET price = excluded.price,
                 updated_at = excluded.updated_at",
            params![id, level.id, price, now],
        )?;
        tx.execute(
            "UPDATE products SET updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "product.price_changed",
            "product",
            Some(id),
            json!({ "priceLevelId": level.id, "before": before.price_for(&level.id), "after": price }),
        ),
    )?;
    get(tx, id)
}

fn set_archived(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    archived: bool,
) -> Result<(), AppError> {
    let product = get(tx, id)?;
    let now = now_ms();
    tx.execute(
        "UPDATE products SET archived_at = CASE WHEN ?2 THEN ?3 END, updated_at = ?3 WHERE id = ?1",
        params![id, archived, now],
    )?;
    let action = if archived {
        "product.archived"
    } else {
        "product.restored"
    };
    let names =
        json!({ "nameAr": product.name_ar, "nameFr": product.name_fr, "nameEn": product.name_en });
    audit::append(tx, &actor.event(action, "product", Some(id), names))?;
    Ok(())
}

/// Stock at any location, the damaged one included (`stock_levels` sums the ledger).
pub(crate) fn holds_stock(conn: &Connection, id: &str) -> Result<bool, AppError> {
    exists(
        conn,
        "SELECT EXISTS (SELECT 1 FROM stock_levels WHERE product_id = ?1 AND qty <> 0)",
        [id],
    )
}

/// Hides a product from the till and the search; its history stays (spec §5: never deleted).
/// A product holding stock is refused (`stock_not_zero`, like a customer's `balance_not_zero`):
/// its stock would leave the levels and the valuation, which reads as hidden stock. Count it out
/// (or write it off) first.
pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    if holds_stock(tx, id)? {
        return Err(AppError::conflict("stock_not_zero"));
    }
    set_archived(tx, actor, id, true)
}

pub fn restore(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    set_archived(tx, actor, id, false)
}

/// A copy of `id` without its unique codes (reference, barcodes, PLU) (PRD-071).
pub fn duplicate(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<Product, AppError> {
    let source = get(tx, id)?;
    let input = ProductInput {
        reference: None,
        name_ar: source.name_ar,
        name_fr: source.name_fr,
        name_en: source.name_en,
        category_id: source.category_id,
        unit_id: source.unit_id,
        tax_rate_id: source.tax_rate_id,
        kind: source.kind,
        parent_id: source.parent_id,
        sell_price: source.sell_price,
        cost_price: source.cost_price,
        min_stock: source.min_stock,
        track_stock: source.track_stock,
        allow_negative_stock: source.allow_negative_stock,
        is_weighed: source.is_weighed,
        available_at_till: source.available_at_till,
        plu: None,
        image: source.image,
        barcodes: Vec::new(),
        prices: source.prices,
        components: source.components,
    };
    create(tx, actor, input)
}

/// Every active product as the import reads it back (IMX-009): the columns are the import
/// field keys, plus the stock on hand, for reading only. The map names each `price.<CODE>`
/// column after its price level in `lang`.
pub fn export_table(
    conn: &Connection,
    lang: Lang,
) -> Result<(Table, BTreeMap<String, String>), AppError> {
    use crate::table::{Cell, ColumnKind as K, TagDomain};
    let levels: Vec<price_levels::PriceLevel> = price_levels::list(conn)?
        .into_iter()
        .filter(|l| !l.is_default && !l.archived)
        .collect();
    let mut columns: Vec<(String, K)> = [
        ("reference", K::Code),
        ("barcode", K::Code),
        ("nameFr", K::Text),
        ("nameAr", K::Text),
        ("nameEn", K::Text),
        ("category", K::Text),
        ("unit", K::Code),
        ("taxRate", K::Percent),
        ("sellPrice", K::Money),
        ("costPrice", K::Money),
        ("minStock", K::Qty),
        ("trackStock", K::Tag),
        ("brand", K::Text),
        ("supplierRef", K::Code),
    ]
    .iter()
    .map(|(k, kind)| ((*k).to_owned(), *kind))
    .collect();
    let mut labels = BTreeMap::new();
    for level in &levels {
        let key = format!("price.{}", level.code);
        let name = match lang {
            Lang::Ar => &level.name_ar,
            Lang::Fr => &level.name_fr,
            Lang::En => &level.name_en,
        };
        labels.insert(
            key.clone(),
            if name.is_empty() {
                level.code.clone()
            } else {
                name.clone()
            },
        );
        columns.push((key, K::Money));
    }
    columns.push(("qty".to_owned(), K::Qty));
    let spec: Vec<(&str, K)> = columns
        .iter()
        .map(|(k, kind)| (k.as_str(), *kind))
        .collect();
    let mut table = Table::new(&spec).tag("trackStock", TagDomain::YesNo);

    let mut barcodes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut stmt =
        conn.prepare("SELECT product_id, barcode FROM product_barcodes ORDER BY rowid")?;
    for pair in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (product, code) = pair?;
        barcodes.entry(product).or_default().push(code);
    }
    let mut prices: BTreeMap<(String, String), i64> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT product_id, price_level_id, price FROM product_prices")?;
    for triple in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (product, level, price) = triple?;
        prices.insert((product, level), price);
    }
    let category = match lang {
        Lang::Ar => "COALESCE(NULLIF(c.name_ar, ''), NULLIF(c.name_fr, ''), c.name_en)",
        Lang::Fr => "COALESCE(NULLIF(c.name_fr, ''), NULLIF(c.name_ar, ''), c.name_en)",
        Lang::En => "COALESCE(NULLIF(c.name_en, ''), NULLIF(c.name_fr, ''), c.name_ar)",
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT p.id, p.reference, p.name_fr, p.name_ar, p.name_en, {category}, u.code, t.rate_bp, p.sell_price,
                p.cost_price, p.min_stock, p.track_stock, x.brand, x.supplier_ref,
                COALESCE((SELECT SUM(l.qty) FROM stock_levels l JOIN stock_locations s ON s.id = l.location_id
                          WHERE l.product_id = p.id AND s.kind = 'storage'), 0)
         FROM products p
         JOIN units u ON u.id = p.unit_id
         JOIN tax_rates t ON t.id = p.tax_rate_id
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN product_extras x ON x.product_id = p.id
         WHERE p.archived_at IS NULL AND p.kind <> 'variant_parent'
         ORDER BY p.name_fr, p.id"
    ))?;
    let found = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let text = |i: usize| -> rusqlite::Result<Cell> {
                Ok(Cell::opt_text(
                    r.get::<_, Option<String>>(i)?.filter(|s| !s.is_empty()),
                ))
            };
            let mut row = vec![
                text(1)?,
                Cell::NULL,
                text(2)?,
                text(3)?,
                text(4)?,
                text(5)?,
                text(6)?,
            ];
            row.push(Cell::int(r.get(7)?));
            row.push(Cell::int(r.get(8)?));
            row.push(Cell::int(r.get(9)?));
            row.push(Cell::int(r.get(10)?));
            row.push(Cell::text(if r.get::<_, bool>(11)? { "yes" } else { "no" }));
            row.push(text(12)?);
            row.push(text(13)?);
            let on_hand: i64 = r.get(14)?;
            Ok((id, row, on_hand))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, mut row, on_hand) in found {
        row[1] = barcodes
            .get(&id)
            .map_or(Cell::NULL, |codes| Cell::text(codes.join("|")));
        for level in &levels {
            row.push(
                prices
                    .get(&(id.clone(), level.id.clone()))
                    .map_or(Cell::NULL, |p| Cell::int(*p)),
            );
        }
        row.push(Cell::int(on_hand));
        table.push(row)?;
    }
    Ok((table, labels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID, WHOLESALE_LEVEL_ID};
    use crate::db::Db;
    use crate::lang::Lang;

    fn catalog_db() -> (Db, String) {
        let db = Db::open_memory().unwrap();
        let pack = crate::datapack::bundled().unwrap();
        db.write(move |tx| crate::catalog::ensure_defaults(tx, &pack))
            .unwrap();
        let tva19 = db
            .read(|c| tax_rates::by_code(c, "TVA19"))
            .unwrap()
            .unwrap()
            .id;
        (db, tva19)
    }

    fn add(db: &Db, input: ProductInput) -> Result<Product, AppError> {
        db.write(move |tx| create(tx, &Actor::default(), input))
    }

    fn milk(tva: &str) -> ProductInput {
        ProductInput {
            name_ar: "حليب كانديا".into(),
            name_en: "Candia milk".into(),
            reference: Some("LT-001".into()),
            barcodes: vec![
                Barcode {
                    barcode: "6130000000017".into(),
                    pack_qty: Qty(1_000),
                    label: None,
                },
                Barcode {
                    barcode: "6130000000024".into(),
                    pack_qty: Qty(12_000),
                    label: Some("Pack 12".into()),
                },
            ],
            prices: vec![LevelPrice {
                price_level_id: WHOLESALE_LEVEL_ID.into(),
                price: Money(9_500),
            }],
            ..ProductInput::simple("Lait Candia 1 L", UNIT_PIECE_ID, tva, Money(11_000))
        }
    }

    #[test]
    fn create_reads_back_everything_and_audits() {
        let (db, tva) = catalog_db();
        let p = add(&db, milk(&tva)).unwrap();
        assert_eq!(
            (p.unit_code.as_str(), p.unit_decimals, p.tax_rate_bp),
            ("pc", 0, Rate(1_900))
        );
        assert_eq!(p.barcodes.len(), 2);
        assert_eq!(p.barcodes[1].pack_qty, Qty(12_000));
        assert_eq!(p.price_for(WHOLESALE_LEVEL_ID), Money(9_500));
        assert_eq!(p.price_for(crate::catalog::RETAIL_LEVEL_ID), Money(11_000));
        assert_eq!(p.name_in(Lang::Ar), "حليب كانديا");
        let st: String = db
            .read(|c| Ok(c.query_row("SELECT search_text FROM products", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(
            st,
            "حليب كانديا lait candia 1 l candia milk lt 001 6130000000017 6130000000024"
        );
        let action: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT action FROM audit_log ORDER BY seq DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(action, "product.created");
    }

    #[test]
    fn unique_codes_are_enforced_with_the_offending_field() {
        let (db, tva) = catalog_db();
        add(&db, milk(&tva)).unwrap();
        let field_code = |input: ProductInput| {
            let p = add(&db, input).unwrap_err().payload();
            (p.field.unwrap_or_default(), p.code)
        };
        let mut again = milk(&tva);
        again.reference = Some("lt-001".into());
        again.barcodes.clear();
        assert_eq!(
            field_code(again),
            ("reference".into(), "reference_taken".into())
        );
        let mut again = milk(&tva);
        again.reference = None;
        again.barcodes.remove(0);
        assert_eq!(
            field_code(again),
            ("barcodes.0.barcode".into(), "barcode_taken".into())
        );
        let mut twice = ProductInput::simple("X", UNIT_PIECE_ID, &tva, Money(1));
        twice.barcodes = vec![
            Barcode {
                barcode: "1".into(),
                pack_qty: Qty(1_000),
                label: None,
            },
            Barcode {
                barcode: "1".into(),
                pack_qty: Qty(1_000),
                label: None,
            },
        ];
        assert_eq!(
            field_code(twice),
            ("barcodes.1.barcode".into(), "barcode_taken".into())
        );
        let mut spaced = ProductInput::simple("X", UNIT_PIECE_ID, &tva, Money(1));
        spaced.barcodes = vec![Barcode {
            barcode: "12 34".into(),
            pack_qty: Qty(1_000),
            label: None,
        }];
        assert_eq!(
            field_code(spaced),
            ("barcodes.0.barcode".into(), "barcode_invalid".into())
        );
        let mut half = ProductInput::simple("X", UNIT_PIECE_ID, &tva, Money(1));
        half.barcodes = vec![Barcode {
            barcode: "99".into(),
            pack_qty: Qty(500),
            label: None,
        }];
        assert_eq!(
            field_code(half),
            ("barcodes.0.packQty".into(), "qty_invalid".into())
        );
        let mut plu = ProductInput::simple("Tomate", UNIT_KG_ID, &tva, Money(12_000));
        plu.plu = Some(12);
        plu.is_weighed = true;
        add(&db, plu.clone()).unwrap();
        assert_eq!(field_code(plu), ("plu".into(), "plu_taken".into()));
    }

    #[test]
    fn invalid_references_name_their_field() {
        let (db, tva) = catalog_db();
        let field_code = |input: ProductInput| {
            let p = add(&db, input).unwrap_err().payload();
            (p.field.unwrap_or_default(), p.code)
        };
        let base = ProductInput::simple("X", UNIT_PIECE_ID, &tva, Money(100));
        let with = |f: fn(&mut ProductInput)| {
            let mut i = base.clone();
            f(&mut i);
            i
        };
        assert_eq!(
            field_code(with(|i| i.unit_id = "u".into())),
            ("unitId".into(), "unit_not_found".into())
        );
        assert_eq!(
            field_code(with(|i| i.tax_rate_id = "t".into())),
            ("taxRateId".into(), "tax_rate_not_found".into())
        );
        assert_eq!(
            field_code(with(|i| i.category_id = Some("c".into()))),
            ("categoryId".into(), "category_not_found".into())
        );
        assert_eq!(
            field_code(with(|i| i.sell_price = Money(-1))),
            ("sellPrice".into(), "amount_invalid".into())
        );
        assert_eq!(
            field_code(with(|i| i.min_stock = Qty(500))),
            ("minStock".into(), "qty_invalid".into())
        );
        assert_eq!(
            field_code(with(|i| i.is_weighed = true)),
            ("isWeighed".into(), "unit_not_weighable".into())
        );
        assert_eq!(
            field_code(with(|i| i.plu = Some(100_000))),
            ("plu".into(), "value_invalid".into())
        );
        assert_eq!(
            field_code(with(|i| i.kind = ProductKind::Variant)),
            ("parentId".into(), "variant_parent_required".into())
        );
        assert_eq!(
            field_code(with(|i| i.kind = ProductKind::Composite)),
            ("components".into(), "components_required".into())
        );
        assert_eq!(
            field_code(with(|i| i.prices = vec![LevelPrice {
                price_level_id: crate::catalog::RETAIL_LEVEL_ID.into(),
                price: Money(1)
            }])),
            ("prices.0.priceLevelId".into(), "value_invalid".into())
        );
        assert_eq!(
            field_code(with(|i| {
                i.name_fr.clear();
            })),
            ("name".into(), "required".into())
        );
    }

    #[test]
    fn kinds_decide_stock_tracking_and_composites_list_stocked_components() {
        let (db, tva) = catalog_db();
        let pen = add(
            &db,
            ProductInput::simple("Stylo", UNIT_PIECE_ID, &tva, Money(5_000)),
        )
        .unwrap();
        let book = add(
            &db,
            ProductInput::simple("Cahier", UNIT_PIECE_ID, &tva, Money(8_000)),
        )
        .unwrap();
        let mut kit = ProductInput::simple("Kit scolaire", UNIT_PIECE_ID, &tva, Money(20_000));
        kit.kind = ProductKind::Composite;
        kit.components = vec![
            Component {
                component_id: pen.id.clone(),
                qty: Qty(2_000),
            },
            Component {
                component_id: book.id.clone(),
                qty: Qty(3_000),
            },
        ];
        let kit = add(&db, kit).unwrap();
        assert!(!kit.track_stock, "a kit's stock is its components'");
        assert_eq!(kit.components.len(), 2);
        let mut nested = ProductInput::simple("Méga kit", UNIT_PIECE_ID, &tva, Money(1));
        nested.kind = ProductKind::Composite;
        nested.components = vec![Component {
            component_id: kit.id.clone(),
            qty: Qty(1_000),
        }];
        let err = add(&db, nested).unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            (
                "component_invalid".into(),
                Some("components.0.componentId".into())
            )
        );
        let mut service = ProductInput::simple("Photocopie", UNIT_PIECE_ID, &tva, Money(1_000));
        service.kind = ProductKind::Service;
        assert!(!add(&db, service).unwrap().track_stock);
        let mut model = ProductInput::simple("T-shirt", UNIT_PIECE_ID, &tva, Money(150_000));
        model.kind = ProductKind::VariantParent;
        let model = add(&db, model).unwrap();
        let mut size_m = ProductInput::simple("T-shirt M", UNIT_PIECE_ID, &tva, Money(150_000));
        size_m.kind = ProductKind::Variant;
        size_m.parent_id = Some(model.id.clone());
        let size_m = add(&db, size_m).unwrap();
        assert!(size_m.track_stock);
        let model_id = model.id.clone();
        assert_eq!(
            db.read(move |c| sellable(c, &model_id)).unwrap_err().code(),
            "product_not_sellable"
        );
    }

    #[test]
    fn update_replaces_children_and_keeps_history() {
        let (db, tva) = catalog_db();
        let p = add(&db, milk(&tva)).unwrap();
        let mut input = milk(&tva);
        input.barcodes.truncate(1);
        input.prices.clear();
        input.sell_price = Money(12_000);
        let id = p.id.clone();
        let after = db
            .write(move |tx| update(tx, &Actor::default(), &id, input))
            .unwrap();
        assert_eq!(
            (after.barcodes.len(), after.prices.len(), after.sell_price),
            (1, 0, Money(12_000))
        );
        let payload: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'product.updated'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            (
                payload["before"]["sellPrice"].as_i64(),
                payload["after"]["sellPrice"].as_i64()
            ),
            (Some(11_000), Some(12_000))
        );
    }

    #[test]
    fn a_sold_product_keeps_its_kind() {
        let (db, tva) = catalog_db();
        let p = add(&db, milk(&tva)).unwrap();
        let id = p.id.clone();
        db.write(move |tx| {
            tx.execute(
                "INSERT INTO stock_movements (id, product_id, location_id, qty_delta, reason, created_at)
                 VALUES (?1, ?2, '00000000-0000-7000-8000-000000000301', 1000, 'opening', 0)",
                params![new_id(), id],
            )?;
            Ok(())
        })
        .unwrap();
        let mut service = milk(&tva);
        service.kind = ProductKind::Service;
        let id = p.id.clone();
        let err = db
            .write(move |tx| update(tx, &Actor::default(), &id, service))
            .unwrap_err();
        assert_eq!(err.code(), "product_kind_locked");
    }

    #[test]
    fn prices_archive_restore_and_duplicate() {
        let (db, tva) = catalog_db();
        let p = add(&db, milk(&tva)).unwrap();
        let id = p.id.clone();
        let p = db
            .write(move |tx| {
                set_price(
                    tx,
                    &Actor::default(),
                    &id,
                    crate::catalog::RETAIL_LEVEL_ID,
                    Money(11_500),
                )
            })
            .unwrap();
        assert_eq!(p.sell_price, Money(11_500));
        let id = p.id.clone();
        let p = db
            .write(move |tx| {
                set_price(tx, &Actor::default(), &id, WHOLESALE_LEVEL_ID, Money(9_900))
            })
            .unwrap();
        assert_eq!(p.price_for(WHOLESALE_LEVEL_ID), Money(9_900));
        let id = p.id.clone();
        db.write(move |tx| archive(tx, &Actor::default(), &id))
            .unwrap();
        let id = p.id.clone();
        assert_eq!(
            db.read(move |c| sellable(c, &id)).unwrap_err().code(),
            "product_not_found"
        );
        let id = p.id.clone();
        db.write(move |tx| restore(tx, &Actor::default(), &id))
            .unwrap();
        let id = p.id.clone();
        let copy = db
            .write(move |tx| duplicate(tx, &Actor::default(), &id))
            .unwrap();
        assert_ne!(copy.id, p.id);
        assert_eq!(
            (
                copy.reference.as_deref(),
                copy.barcodes.len(),
                copy.name_fr.as_str()
            ),
            (None, 0, "Lait Candia 1 L")
        );
        assert_eq!(copy.price_for(WHOLESALE_LEVEL_ID), Money(9_900));
        let actions: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT group_concat(action) FROM (SELECT action FROM audit_log WHERE entity_type = 'product' ORDER BY seq)",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            actions,
            "product.created,product.price_changed,product.price_changed,product.archived,product.restored,product.created"
        );
    }

    /// What the product form sends back after loading `p` unchanged.
    fn input_of(p: &Product) -> ProductInput {
        ProductInput {
            reference: p.reference.clone(),
            name_ar: p.name_ar.clone(),
            name_fr: p.name_fr.clone(),
            name_en: p.name_en.clone(),
            category_id: p.category_id.clone(),
            unit_id: p.unit_id.clone(),
            tax_rate_id: p.tax_rate_id.clone(),
            kind: p.kind,
            parent_id: p.parent_id.clone(),
            sell_price: p.sell_price,
            cost_price: p.cost_price,
            min_stock: p.min_stock,
            track_stock: p.track_stock,
            allow_negative_stock: p.allow_negative_stock,
            is_weighed: p.is_weighed,
            available_at_till: p.available_at_till,
            plu: p.plu,
            image: p.image.clone(),
            barcodes: p.barcodes.clone(),
            prices: p.prices.clone(),
            components: p.components.clone(),
        }
    }

    fn reread(db: &Db, id: &str) -> Product {
        let id = id.to_owned();
        db.read(move |c| get(c, &id)).unwrap()
    }

    #[test]
    fn moving_the_default_level_keeps_every_price_and_the_round_trip() {
        use crate::catalog::RETAIL_LEVEL_ID;
        let (db, tva) = catalog_db();
        let milk = add(&db, milk(&tva)).unwrap();
        let bread = add(
            &db,
            ProductInput::simple("Pain", UNIT_PIECE_ID, &tva, Money(1_500)),
        )
        .unwrap();
        db.write(|tx| price_levels::set_default(tx, &Actor::default(), WHOLESALE_LEVEL_ID))
            .unwrap();
        let moved = reread(&db, &milk.id);
        assert_eq!(
            (
                moved.price_for(RETAIL_LEVEL_ID),
                moved.price_for(WHOLESALE_LEVEL_ID),
                moved.sell_price
            ),
            (Money(11_000), Money(9_500), Money(9_500))
        );
        assert_eq!(
            moved.prices,
            vec![LevelPrice {
                price_level_id: RETAIL_LEVEL_ID.into(),
                price: Money(11_000)
            }],
            "no row is left for the default level"
        );
        let bread = reread(&db, &bread.id);
        assert_eq!(
            (
                bread.price_for(RETAIL_LEVEL_ID),
                bread.price_for(WHOLESALE_LEVEL_ID)
            ),
            (Money(1_500), Money(1_500))
        );
        let payload: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'price_level.updated'
                     ORDER BY seq DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["sellPricesMoved"].as_i64(), Some(1));
        // Setting the level that already is the default changes nothing.
        db.write(|tx| price_levels::set_default(tx, &Actor::default(), WHOLESALE_LEVEL_ID))
            .unwrap();
        assert_eq!(reread(&db, &milk.id).prices, moved.prices);

        let id = milk.id.clone();
        let repriced = db
            .write(move |tx| {
                set_price(
                    tx,
                    &Actor::default(),
                    &id,
                    WHOLESALE_LEVEL_ID,
                    Money(10_000),
                )
            })
            .unwrap();
        assert_eq!(
            (
                repriced.price_for(WHOLESALE_LEVEL_ID),
                repriced.price_for(RETAIL_LEVEL_ID)
            ),
            (Money(10_000), Money(11_000))
        );
        let id = milk.id.clone();
        let input = input_of(&repriced);
        let after = db
            .write(move |tx| update(tx, &Actor::default(), &id, input))
            .unwrap();
        assert_eq!(
            (after.sell_price, after.prices),
            (Money(10_000), repriced.prices)
        );
    }

    #[test]
    fn archived_level_prices_stay_as_history_out_of_the_product() {
        let (db, tva) = catalog_db();
        let milk = add(&db, milk(&tva)).unwrap();
        db.write(|tx| price_levels::archive(tx, &Actor::default(), WHOLESALE_LEVEL_ID))
            .unwrap();
        let current = reread(&db, &milk.id);
        assert!(current.prices.is_empty(), "{:?}", current.prices);
        let id = milk.id.clone();
        let copy = db
            .write(move |tx| duplicate(tx, &Actor::default(), &id))
            .unwrap();
        assert!(copy.prices.is_empty());
        let id = milk.id.clone();
        let input = input_of(&current);
        db.write(move |tx| update(tx, &Actor::default(), &id, input))
            .unwrap();
        let id = milk.id.clone();
        let kept: Money = db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT price FROM product_prices WHERE product_id = ?1 AND price_level_id = ?2",
                    params![id, WHOLESALE_LEVEL_ID],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(kept, Money(9_500), "the archived level's price is history");
    }
}
