//! Department keys (SAL-019, VRT-001, KSK): the "calculatrice" mode of shops that replace a cash
//! register — a tile per department ("Légumes", "Pain", "Divers") whose price the cashier types.
//! Each key is backed by an open-price product (no stock, its own tax rate and category), so the
//! sale, the reports and the Z see ordinary lines, and the typed price is never an override.
//! Keys are written only while the `department_keys` module is on, and archived, never deleted.

use super::products::{self, ProductInput, ProductKind};
use super::{UNIT_KG_ID, UNIT_PIECE_ID, categories, clean_opt, tax_rates, units};
use crate::audit::{self, Actor};
use crate::company::ShopActivity;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty};
use crate::packs::templates::{SeedName, n};
use crate::packs::{self, Module};
use crate::sales::CartLine;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DepartmentKey {
    pub id: String,
    pub product_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub category_id: Option<String>,
    pub tax_rate_id: String,
    pub unit_id: String,
    pub color: Option<String>,
    pub sort_order: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DepartmentKeyInput {
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub category_id: Option<String>,
    pub tax_rate_id: String,
    /// The piece when absent; kilogram for goods sold by weight.
    pub unit_id: Option<String>,
    pub color: Option<String>,
    pub sort_order: i64,
}

const SELECT: &str = "SELECT k.id, k.product_id, p.name_ar, p.name_fr, p.name_en, p.category_id,
        p.tax_rate_id, p.unit_id, k.color, k.sort_order, k.archived_at IS NOT NULL
     FROM department_keys k JOIN products p ON p.id = k.product_id";

fn map(row: &Row<'_>) -> rusqlite::Result<DepartmentKey> {
    Ok(DepartmentKey {
        id: row.get(0)?,
        product_id: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        category_id: row.get(5)?,
        tax_rate_id: row.get(6)?,
        unit_id: row.get(7)?,
        color: row.get(8)?,
        sort_order: row.get(9)?,
        archived: row.get(10)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<DepartmentKey, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE k.id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("department_key"))
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<DepartmentKey>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE (?1 OR k.archived_at IS NULL) ORDER BY k.sort_order, p.name_fr"
    ))?;
    let rows = stmt
        .query_map([include_archived], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// A product priced when it is sold (a department key, a top-up, an open-price service line).
pub fn is_open_price(conn: &Connection, product_id: &str) -> Result<bool, AppError> {
    Ok(conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM open_price_products WHERE product_id = ?1)")?
        .query_row([product_id], |r| r.get(0))?)
}

/// `#RRGGBB` or nothing.
fn color(value: Option<&str>) -> Result<Option<String>, AppError> {
    let c = clean_opt(value, "color", 7)?;
    if c.as_deref().is_some_and(|c| {
        c.len() != 7 || !c.starts_with('#') || !c[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        return Err(AppError::validation("color", "value_invalid"));
    }
    Ok(c)
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: DepartmentKeyInput,
) -> Result<DepartmentKey, AppError> {
    packs::require(tx, Module::DepartmentKeys)?;
    let color = color(input.color.as_deref())?;
    let unit = input
        .unit_id
        .clone()
        .unwrap_or_else(|| UNIT_PIECE_ID.to_owned());
    let mut p = ProductInput::simple(&input.name_fr, &unit, &input.tax_rate_id, Money::ZERO);
    p.name_ar = input.name_ar.clone();
    p.name_en = input.name_en.clone();
    p.category_id = input.category_id.clone();
    p.kind = ProductKind::Simple;
    p.track_stock = false;
    let product = products::create(tx, actor, p)?;
    let now = now_ms();
    tx.execute(
        "INSERT INTO open_price_products (product_id, created_at) VALUES (?1, ?2)",
        params![product.id, now],
    )?;
    let id = new_id();
    tx.execute(
        "INSERT INTO department_keys (id, product_id, color, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, product.id, color, input.sort_order, now],
    )?;
    audit::append(
        tx,
        &actor.event(
            "department_key.created",
            "department_key",
            Some(&id),
            json!({ "productId": product.id, "nameFr": product.name_fr }),
        ),
    )?;
    get(tx, &id)
}

/// The key's names, category, tax rate, unit, colour and place; its product follows.
pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: DepartmentKeyInput,
) -> Result<DepartmentKey, AppError> {
    packs::require(tx, Module::DepartmentKeys)?;
    let before = get(tx, id)?;
    let color = color(input.color.as_deref())?;
    let (name_ar, name_fr, name_en) =
        super::clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    let rate = tax_rates::get(tx, &input.tax_rate_id)
        .map_err(|_| AppError::validation("taxRateId", "tax_rate_not_found"))?;
    if !rate.active {
        return Err(AppError::validation("taxRateId", "tax_rate_inactive"));
    }
    let unit = input
        .unit_id
        .clone()
        .unwrap_or_else(|| before.unit_id.clone());
    if !units::get(tx, &unit).is_ok_and(|u| !u.archived) {
        return Err(AppError::validation("unitId", "unit_not_found"));
    }
    let category_id = clean_opt(input.category_id.as_deref(), "categoryId", 36)?;
    if let Some(c) = &category_id
        && !categories::get(tx, c).is_ok_and(|c| !c.archived)
    {
        return Err(AppError::validation("categoryId", "category_not_found"));
    }
    let now = now_ms();
    tx.execute(
        "UPDATE products SET name_ar = ?2, name_fr = ?3, name_en = ?4, category_id = ?5,
                             tax_rate_id = ?6, unit_id = ?7, updated_at = ?8 WHERE id = ?1",
        params![
            before.product_id,
            name_ar,
            name_fr,
            name_en,
            category_id,
            rate.id,
            unit,
            now
        ],
    )?;
    products::refresh_search_text(tx, &before.product_id)?;
    tx.execute(
        "UPDATE department_keys SET color = ?2, sort_order = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, color, input.sort_order, now],
    )?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "department_key.updated",
            "department_key",
            Some(id),
            json!({
                "before": before.name_fr,
                "after": after.name_fr,
                "taxRateId": after.tax_rate_id,
            }),
        ),
    )?;
    Ok(after)
}

/// Hides the key and its product; both stay in the history (never deleted).
pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    packs::require(tx, Module::DepartmentKeys)?;
    let key = get(tx, id)?;
    let now = now_ms();
    tx.execute(
        "UPDATE department_keys SET archived_at = ?2, updated_at = ?2
         WHERE id = ?1 AND archived_at IS NULL",
        params![id, now],
    )?;
    tx.execute(
        "UPDATE products SET archived_at = ?2, updated_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
        params![key.product_id, now],
    )?;
    audit::append(
        tx,
        &actor.event(
            "department_key.archived",
            "department_key",
            Some(id),
            json!({ "productId": key.product_id }),
        ),
    )?;
    Ok(())
}

/// The cart line of `qty` on key `id` at `price` (the till's calculator keypad).
pub fn line(conn: &Connection, id: &str, price: Money, qty: Qty) -> Result<CartLine, AppError> {
    let key = get(conn, id)?;
    if key.archived {
        return Err(AppError::not_found("department_key"));
    }
    let mut line = CartLine::product(&key.product_id, qty);
    line.unit_price = Some(price);
    Ok(line)
}

/// A key a template creates: names, tax code, sold by the kilogram.
type Seed = (SeedName, &'static str, bool);

// Bread is exempt (CTCA art. 9); every other key starts at 19 % and the owner changes the rate on
// the key: a template never claims a rate it cannot verify.
const GENERAL: &[Seed] = &[
    (
        n("خضر وفواكه", "Fruits et légumes", "Fruit and vegetables"),
        "TVA19",
        true,
    ),
    (n("خبز", "Pain", "Bread"), "EXO", false),
    (n("متفرقات", "Divers", "Miscellaneous"), "TVA19", false),
];
const BUTCHER: &[Seed] = &[
    (n("لحم", "Viande", "Meat"), "TVA19", true),
    (
        n("خضر وفواكه", "Fruits et légumes", "Fruit and vegetables"),
        "TVA19",
        true,
    ),
];
const BAKERY: &[Seed] = &[
    (n("خبز", "Pain", "Bread"), "EXO", false),
    (
        n("حلويات بالوزن", "Gâteaux au poids", "Cakes by weight"),
        "TVA19",
        true,
    ),
];
const STATIONERY: &[Seed] = &[(
    n("قرطاسية متنوعة", "Papeterie divers", "Stationery sundries"),
    "TVA19",
    false,
)];
const KIOSK: &[Seed] = &[(n("متفرقات", "Divers", "Miscellaneous"), "TVA19", false)];

/// The department keys each template creates.
fn seeds_of(activity: ShopActivity) -> &'static [Seed] {
    match activity {
        ShopActivity::General => GENERAL,
        ShopActivity::Butcher => BUTCHER,
        ShopActivity::Bakery => BAKERY,
        ShopActivity::Stationery => STATIONERY,
        ShopActivity::Kiosk => KIOSK,
        _ => &[],
    }
}

/// The template's keys that do not exist yet (by French name); the number created.
pub fn seed(tx: &Transaction<'_>, actor: &Actor, activity: ShopActivity) -> Result<u32, AppError> {
    let mut created = 0;
    for (i, (name, tax_code, by_kg)) in seeds_of(activity).iter().enumerate() {
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM department_keys k JOIN products p ON p.id = k.product_id
                            WHERE k.archived_at IS NULL AND lower(p.name_fr) = lower(?1))",
            [name.fr],
            |r| r.get(0),
        )?;
        if exists {
            continue;
        }
        let rate =
            tax_rates::by_code(tx, tax_code)?.ok_or_else(|| AppError::not_found("tax_rate"))?;
        create(
            tx,
            actor,
            DepartmentKeyInput {
                name_ar: name.ar.to_owned(),
                name_fr: name.fr.to_owned(),
                name_en: name.en.to_owned(),
                category_id: None,
                tax_rate_id: rate.id,
                unit_id: by_kg.then(|| UNIT_KG_ID.to_owned()),
                color: None,
                sort_order: i64::try_from(i).unwrap_or(0) + 1,
            },
        )?;
        created += 1;
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::payments::PaymentInput;
    use crate::sales::{Cart, SaleInput, create_sale};
    use crate::testkit::shop;

    fn key(shop: &crate::testkit::Shop, fr: &str) -> DepartmentKey {
        let input = DepartmentKeyInput {
            name_ar: "خضر".into(),
            name_fr: fr.into(),
            name_en: "Vegetables".into(),
            category_id: None,
            tax_rate_id: shop.exempt.clone(),
            unit_id: None,
            color: Some("#1B7F3B".into()),
            sort_order: 1,
        };
        let actor = shop.owner.actor();
        shop.db.write(move |tx| create(tx, &actor, input)).unwrap()
    }

    #[test]
    fn a_cashier_sells_a_department_key_at_the_typed_price_without_override_rights() {
        let shop = shop();
        let legumes = key(&shop, "Légumes");
        shop.open_till(100_000);
        let id = legumes.id.clone();
        let line = shop
            .db
            .read(move |c| line(c, &id, Money(23_500), Qty(1_000)))
            .unwrap();
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![line]),
            payments: vec![PaymentInput::cash(Money(23_500))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap();
        let l = &sale.document.lines[0];
        assert_eq!(
            (l.description.as_str(), l.total_ttc, l.price_overridden),
            ("Légumes", Money(23_500), false)
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements"),
            0,
            "no stock for a department"
        );
    }

    #[test]
    fn a_department_line_needs_a_price() {
        let shop = shop();
        let pain = key(&shop, "Pain");
        let ctx = shop.till();
        let cart = Cart::new(
            Lang::Fr,
            vec![crate::sales::CartLine::product(
                &pain.product_id,
                Qty(1_000),
            )],
        );
        let err = shop
            .db
            .read(move |c| crate::sales::cart::price_cart(c, &ctx, &cart))
            .unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field.as_deref()),
            ("required".into(), Some("lines.0.unitPrice"))
        );
    }

    #[test]
    fn keys_are_edited_and_archived_never_deleted() {
        let shop = shop();
        let k = key(&shop, "Divers");
        let actor = shop.owner.actor();
        let id = k.id.clone();
        let input = DepartmentKeyInput {
            name_ar: "متفرقات".into(),
            name_fr: "Divers".into(),
            name_en: "Miscellaneous".into(),
            category_id: None,
            tax_rate_id: shop.tva19.clone(),
            unit_id: None,
            color: None,
            sort_order: 3,
        };
        let edited = shop
            .db
            .write(move |tx| update(tx, &actor, &id, input))
            .unwrap();
        assert_eq!(
            (edited.name_en.as_str(), edited.tax_rate_id.as_str()),
            ("Miscellaneous", shop.tva19.as_str())
        );
        let actor = shop.owner.actor();
        let id = k.id.clone();
        shop.db.write(move |tx| archive(tx, &actor, &id)).unwrap();
        assert!(shop.db.read(|c| list(c, false)).unwrap().is_empty());
        assert_eq!(shop.db.read(|c| list(c, true)).unwrap().len(), 1);
        assert!(
            shop.db
                .write(|tx| Ok(tx.execute_batch("DELETE FROM department_keys")?))
                .is_err()
        );
    }

    #[test]
    fn templates_seed_their_keys_once() {
        let shop = shop();
        let actor = shop.owner.actor();
        let n = shop
            .db
            .write(move |tx| seed(tx, &actor, crate::company::ShopActivity::General))
            .unwrap();
        assert_eq!(n, 3);
        let actor = shop.owner.actor();
        assert_eq!(
            shop.db
                .write(move |tx| seed(tx, &actor, crate::company::ShopActivity::General))
                .unwrap(),
            0
        );
        let names: Vec<String> = shop
            .db
            .read(|c| list(c, false))
            .unwrap()
            .into_iter()
            .map(|k| k.name_fr)
            .collect();
        assert_eq!(names, ["Fruits et légumes", "Pain", "Divers"]);
    }

    #[test]
    fn a_typed_price_is_never_held_to_a_floor() {
        // An open-price product sells at the typed price: no override right, no floor (SAL-019).
        let shop = shop();
        let divers = key(&shop, "Divers");
        let (actor, pid) = (shop.owner.actor(), divers.product_id.clone());
        shop.db
            .write(move |tx| {
                let input = crate::catalog::extras::ProductExtrasInput {
                    min_price: Some(Money(10_000)),
                    ..Default::default()
                };
                crate::catalog::extras::set(tx, &actor, &pid, input)
            })
            .unwrap();
        let id = divers.id.clone();
        let line = shop
            .db
            .read(move |c| line(c, &id, Money(500), Qty(1_000)))
            .unwrap();
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![line]);
        let priced = shop
            .db
            .read(move |c| crate::sales::cart::price_cart(c, &ctx, &cart))
            .unwrap();
        let l = &priced.lines[0];
        assert_eq!(
            (l.unit_price, l.catalogue_price, l.price_overridden),
            (Money(500), Some(Money(500)), false)
        );
    }

    #[test]
    fn keys_are_written_only_while_the_module_is_on_and_an_archived_key_is_not_sold() {
        let shop = shop();
        let k = key(&shop, "Divers");
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| crate::packs::set_modules(tx, &actor, &[crate::packs::Module::Staff]))
            .unwrap();
        let actor = shop.owner.actor();
        let err = shop
            .db
            .write(move |tx| {
                let input = DepartmentKeyInput {
                    name_ar: String::new(),
                    name_fr: "Pain".into(),
                    name_en: String::new(),
                    category_id: None,
                    tax_rate_id: String::new(),
                    unit_id: None,
                    color: None,
                    sort_order: 0,
                };
                create(tx, &actor, input)
            })
            .unwrap_err();
        assert_eq!(err.code(), "module_disabled");
        let (actor, id) = (shop.owner.actor(), k.id.clone());
        let err = shop
            .db
            .write(move |tx| archive(tx, &actor, &id))
            .unwrap_err();
        assert_eq!(err.code(), "module_disabled");
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| crate::packs::reset_modules(tx, &actor))
            .unwrap();
        let (actor, id) = (shop.owner.actor(), k.id.clone());
        shop.db.write(move |tx| archive(tx, &actor, &id)).unwrap();
        let id = k.id.clone();
        let err = shop
            .db
            .read(move |c| line(c, &id, Money(1_000), Qty(1_000)))
            .unwrap_err();
        assert_eq!(err.code(), "department_key_not_found");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action LIKE 'department_key.%'"),
            2
        );
    }
}
