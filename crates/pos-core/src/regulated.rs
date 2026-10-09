//! Regulated staples (PRC-012, SAL-030, VRT-001): a product is linked to a code of the data
//! pack's price ceilings; the till's pricing flags its line ("subventionné"), and warns or refuses
//! a unit price above the ceiling as `sales.regulated_policy` says. The ceilings are data, never
//! code: a new decree is a new pack. A ceiling is a consumer price TTC for one unit, so every
//! comparison is made on the TTC of one unit (`tax::tva::unit_ttc`), whatever the price mode.

use crate::audit::{self, Actor};
use crate::catalog::products::{self, Product, ProductInput};
use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID, tax_rates};
use crate::company::{self, PriceMode};
use crate::datapack::{Pack, RegulatedItem};
use crate::error::AppError;
use crate::money::{Money, Rate};
use crate::packs::{self, Module};
use crate::tax::tva;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// `sales.regulated_policy`: what the till does with a regulated staple priced above its
/// ceiling (SAL-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RegulatedPolicy {
    Warn,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RegulatedFlag {
    pub code: String,
    pub ceiling: Money,
    /// The unit price is above the ceiling (only possible under `warn`).
    pub above: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RegulatedProduct {
    pub product_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub code: String,
    pub ceiling: Money,
    /// The price at the level, in the company's price mode (HT in an HT shop).
    pub price: Money,
    /// What one unit costs the consumer at `price`, TTC: the figure held to the ceiling.
    pub price_ttc: Money,
    pub above: bool,
}

pub fn item<'a>(pack: &'a Pack, code: &str) -> Option<&'a RegulatedItem> {
    pack.regulated.items.iter().find(|i| i.code == code)
}

fn price_mode(conn: &Connection) -> Result<PriceMode, AppError> {
    Ok(company::get(conn)?
        .ok_or_else(|| AppError::conflict("setup_required"))?
        .price_mode)
}

/// One unit of `product` at `price` (in `mode`), TTC at the product's rate.
fn product_ttc(
    conn: &Connection,
    mode: PriceMode,
    product: &Product,
    price: Money,
) -> Result<Money, AppError> {
    let rate = tax_rates::get(conn, &product.tax_rate_id)?.rate_bp;
    tva::unit_ttc(mode, price, rate)
}

pub fn code_of(conn: &Connection, product_id: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .prepare_cached("SELECT code FROM product_regulations WHERE product_id = ?1")?
        .query_row([product_id], |r| r.get(0))
        .optional()?)
}

/// Links `product_id` to `code` (or removes the link with `None`); journaled. Refused while the
/// `regulated_prices` module is off.
pub fn set(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    product_id: &str,
    code: Option<&str>,
) -> Result<Option<RegulatedFlag>, AppError> {
    packs::require(tx, Module::RegulatedPrices)?;
    let product = products::get(tx, product_id)?;
    let before = code_of(tx, product_id)?;
    let flag = match code {
        Some(code) => {
            let item = item(pack, code)
                .ok_or_else(|| AppError::validation("code", "regulated_code_unknown"))?;
            tx.execute(
                "INSERT INTO product_regulations (product_id, code, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (product_id) DO UPDATE SET code = excluded.code,
                     updated_at = excluded.updated_at",
                params![product_id, code, now_ms()],
            )?;
            let ttc = product_ttc(tx, price_mode(tx)?, &product, product.sell_price)?;
            Some(RegulatedFlag {
                code: code.to_owned(),
                ceiling: item.ceiling,
                above: ttc > item.ceiling,
            })
        }
        None => {
            tx.execute(
                "DELETE FROM product_regulations WHERE product_id = ?1",
                [product_id],
            )?;
            None
        }
    };
    audit::append(
        tx,
        &actor.event(
            "product.regulation_set",
            "product",
            Some(product_id),
            json!({ "before": before, "after": code }),
        ),
    )?;
    Ok(flag)
}

/// The flag of a line of `product_id` sold at `unit_price` (in `mode`, at `rate`), `None` for an
/// unregulated product. The ceiling is held against the TTC of one unit.
pub fn check(
    conn: &Connection,
    pack: &Pack,
    product_id: &str,
    mode: PriceMode,
    unit_price: Money,
    rate: Rate,
) -> Result<Option<RegulatedFlag>, AppError> {
    let Some(code) = code_of(conn, product_id)? else {
        return Ok(None);
    };
    let Some(item) = item(pack, &code) else {
        // A code the current pack no longer lists: nothing to compare with.
        return Ok(None);
    };
    Ok(Some(RegulatedFlag {
        code,
        ceiling: item.ceiling,
        above: tva::unit_ttc(mode, unit_price, rate)? > item.ceiling,
    }))
}

pub fn policy(conn: &Connection) -> Result<RegulatedPolicy, AppError> {
    crate::settings::get(conn, "sales.regulated_policy")
}

/// The regulated products with their price at `level_id` against the ceiling (the superette's
/// "prix plafonnés" board).
pub fn board(
    conn: &Connection,
    pack: &Pack,
    level_id: &str,
) -> Result<Vec<RegulatedProduct>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT r.product_id, r.code FROM product_regulations r
         JOIN products p ON p.id = r.product_id
         WHERE p.archived_at IS NULL ORDER BY r.code, p.name_fr",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mode = price_mode(conn)?;
    let mut out = Vec::with_capacity(rows.len());
    for (product_id, code) in rows {
        let Some(item) = item(pack, &code) else {
            continue;
        };
        let p = products::get(conn, &product_id)?;
        let price = p.price_for(level_id);
        let price_ttc = product_ttc(conn, mode, &p, price)?;
        out.push(RegulatedProduct {
            product_id,
            name_ar: p.name_ar,
            name_fr: p.name_fr,
            name_en: p.name_en,
            code,
            ceiling: item.ceiling,
            price,
            price_ttc,
            above: price_ttc > item.ceiling,
        });
    }
    Ok(out)
}

/// VRT-001: the superette template's regulated staples, one product per pack item, sold at the
/// ceiling (the shop may lower it), linked to their code. An HT shop gets the highest HT price
/// whose TTC stays within the ceiling. A code already linked, or whose `REG-…` product exists
/// (its link removed by the owner), is skipped; the number created.
pub fn seed_staples(tx: &Transaction<'_>, actor: &Actor, pack: &Pack) -> Result<u32, AppError> {
    packs::require(tx, Module::RegulatedPrices)?;
    let mode = price_mode(tx)?;
    let mut created = 0;
    for item in &pack.regulated.items {
        let reference = format!("REG-{}", item.code.to_uppercase());
        let present: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM product_regulations WHERE code = ?1)
                 OR EXISTS (SELECT 1 FROM products WHERE reference = ?2)",
            params![item.code, reference],
            |r| r.get(0),
        )?;
        if present {
            continue;
        }
        let rate = tax_rates::by_code(tx, &item.tax_code)?
            .ok_or_else(|| AppError::not_found("tax_rate"))?;
        let unit = if item.unit == "kg" {
            UNIT_KG_ID
        } else {
            UNIT_PIECE_ID
        };
        let price = match mode {
            PriceMode::Ttc => item.ceiling,
            PriceMode::Ht => tva::ht_within_ttc(item.ceiling, rate.rate_bp)?,
        };
        let mut input = ProductInput::simple(&item.name_fr, unit, &rate.id, price);
        input.name_ar = item.name_ar.clone();
        input.name_en = item.name_en.clone();
        input.reference = Some(reference);
        input.is_weighed = item.unit == "kg";
        let product = products::create(tx, actor, input)?;
        tx.execute(
            "INSERT INTO product_regulations (product_id, code, updated_at) VALUES (?1, ?2, ?3)",
            params![product.id, item.code, now_ms()],
        )?;
        created += 1;
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::money::Qty;
    use crate::sales::{Cart, CartLine, cart::price_cart};
    use crate::testkit::{Shop, shop};
    use serde_json::json;

    fn oil(shop: &Shop, price: i64) -> String {
        let p = shop.product("Huile Elio 5 L", Money(price), &shop.tva9);
        let (actor, pack, id) = (shop.owner.actor(), shop.pack.clone(), p.id.clone());
        shop.db
            .write(move |tx| set(tx, &actor, &pack, &id, Some("oil_5l")))
            .unwrap();
        p.id
    }

    fn priced(shop: &Shop, product: &str) -> Result<crate::sales::PricedCart, AppError> {
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(1_000))]);
        shop.db.read(move |c| price_cart(c, &ctx, &cart))
    }

    #[test]
    fn a_price_above_the_ceiling_warns_or_blocks_as_set() {
        let shop = shop();
        let id = oil(&shop, 70_000);
        let warned = priced(&shop, &id).unwrap();
        let flag = warned.lines[0].regulated.clone().unwrap();
        assert_eq!(
            (flag.code.as_str(), flag.ceiling, flag.above),
            ("oil_5l", Money(65_000), true)
        );
        shop.set("sales.regulated_policy", json!("block"));
        let err = priced(&shop, &id).unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field.as_deref()),
            ("price_above_ceiling".into(), Some("lines.0.unitPrice"))
        );
    }

    #[test]
    fn a_price_at_or_below_the_ceiling_is_flagged_subsidised_only() {
        let shop = shop();
        shop.set("sales.regulated_policy", json!("block"));
        let id = oil(&shop, 65_000);
        let line = priced(&shop, &id).unwrap().lines[0].clone();
        assert_eq!(line.regulated.map(|f| f.above), Some(false));
        let plain = shop.product("Huile d'olive 1 L", Money(120_000), &shop.tva19);
        assert!(
            priced(&shop, &plain.id).unwrap().lines[0]
                .regulated
                .is_none()
        );
    }

    #[test]
    fn an_unknown_code_is_refused_and_a_code_can_be_removed() {
        let shop = shop();
        let p = shop.product("Sucre", Money(9_000), &shop.tva9);
        let (actor, pack, id) = (shop.owner.actor(), shop.pack.clone(), p.id.clone());
        let err = shop
            .db
            .write(move |tx| set(tx, &actor, &pack, &id, Some("caviar")))
            .unwrap_err();
        assert_eq!(err.code(), "regulated_code_unknown");
        let (actor, pack, id) = (shop.owner.actor(), shop.pack.clone(), p.id.clone());
        shop.db
            .write(move |tx| set(tx, &actor, &pack, &id, Some("sugar_bulk_kg")))
            .unwrap();
        let (actor, pack, id) = (shop.owner.actor(), shop.pack.clone(), p.id.clone());
        assert!(
            shop.db
                .write(move |tx| set(tx, &actor, &pack, &id, None))
                .unwrap()
                .is_none()
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM product_regulations"), 0);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'product.regulation_set'"),
            2
        );
    }

    #[test]
    fn the_superette_template_ships_the_fourteen_staples_at_their_ceilings() {
        let shop = shop();
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        let created = shop
            .db
            .write(move |tx| seed_staples(tx, &actor, &pack))
            .unwrap();
        assert_eq!(created, 14);
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        assert_eq!(
            shop.db
                .write(move |tx| seed_staples(tx, &actor, &pack))
                .unwrap(),
            0,
            "once"
        );
        let pack = shop.pack.clone();
        let board = shop
            .db
            .read(move |c| board(c, &pack, crate::catalog::RETAIL_LEVEL_ID))
            .unwrap();
        assert_eq!(board.len(), 14);
        assert!(
            board
                .iter()
                .all(|r| r.price == r.ceiling && r.price_ttc == r.ceiling && !r.above)
        );
        let milk = board.iter().find(|r| r.code == "milk_sachet_1l").unwrap();
        assert_eq!(milk.name_ar, "حليب مبستر في كيس 1 ل");
    }

    #[test]
    fn a_blocked_ceiling_still_values_a_void() {
        let shop = shop();
        let id = oil(&shop, 70_000);
        shop.set("sales.regulated_policy", json!("block"));
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&id, Qty(1_000))]);
        let valued = shop
            .db
            .read(move |c| crate::sales::cart::value_cart(c, &ctx, &cart))
            .unwrap();
        assert_eq!(
            valued.totals.total_ttc,
            Money(70_000),
            "the journal of a void values the line as it stands"
        );
    }

    #[test]
    fn a_code_is_linked_only_while_the_module_is_on() {
        // M8: a pack feature refuses its writes while its module is off; its data stays.
        let shop = shop();
        let id = oil(&shop, 60_000);
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| crate::packs::set_modules(tx, &actor, &[crate::packs::Module::Staff]))
            .unwrap();
        let (actor, pack, pid) = (shop.owner.actor(), shop.pack.clone(), id.clone());
        let err = shop
            .db
            .write(move |tx| set(tx, &actor, &pack, &pid, Some("oil_1l")))
            .unwrap_err();
        assert_eq!(err.code(), "module_disabled");
        assert_eq!(err.payload().details.unwrap()["module"], "regulated_prices");
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        let err = shop
            .db
            .write(move |tx| seed_staples(tx, &actor, &pack))
            .unwrap_err();
        assert_eq!(err.code(), "module_disabled");
        let code = shop.db.read(move |c| code_of(c, &id)).unwrap();
        assert_eq!(code.as_deref(), Some("oil_5l"), "the link stays");
    }

    fn ht_shop() -> Shop {
        crate::testkit::shop_with(
            crate::company::TaxRegime::Reel,
            crate::company::PriceMode::Ht,
        )
    }

    #[test]
    fn in_an_ht_shop_the_ceiling_is_held_against_the_consumer_price_ttc() {
        // PRC-012: a ceiling is a consumer price TTC. 620.00 HT at 9 % is 675.80 TTC, above 650.00.
        let shop = ht_shop();
        let id = oil(&shop, 62_000);
        let flag = priced(&shop, &id).unwrap().lines[0]
            .regulated
            .clone()
            .unwrap();
        assert!(flag.above, "675.80 TTC is above the 650.00 ceiling");
        shop.set("sales.regulated_policy", json!("block"));
        let err = priced(&shop, &id).unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field.as_deref()),
            ("price_above_ceiling".into(), Some("lines.0.unitPrice"))
        );
        // 596.33 HT + 53.67 TVA (53.6697) = 650.00 TTC: at the ceiling; a centime more is above.
        let at = oil(&shop, 59_633);
        let line = priced(&shop, &at).unwrap().lines[0].clone();
        assert_eq!(
            (line.amounts.total_ttc, line.regulated.map(|f| f.above)),
            (Money(65_000), Some(false))
        );
        let over = oil(&shop, 59_634);
        assert_eq!(
            priced(&shop, &over).unwrap_err().code(),
            "price_above_ceiling"
        );
    }

    #[test]
    fn in_an_ht_shop_the_link_and_the_board_compare_the_price_ttc() {
        let shop = ht_shop();
        let p = shop.product("Huile Elio 5 L", Money(62_000), &shop.tva9);
        let (actor, pack, id) = (shop.owner.actor(), shop.pack.clone(), p.id.clone());
        let flag = shop
            .db
            .write(move |tx| set(tx, &actor, &pack, &id, Some("oil_5l")))
            .unwrap()
            .unwrap();
        assert!(flag.above, "675.80 TTC is above the 650.00 ceiling");
        let pack = shop.pack.clone();
        let board = shop
            .db
            .read(move |c| board(c, &pack, crate::catalog::RETAIL_LEVEL_ID))
            .unwrap();
        assert_eq!(
            (board[0].price, board[0].price_ttc, board[0].above),
            (Money(62_000), Money(67_580), true),
            "the board shows the HT price and the TTC it is held to"
        );
    }

    #[test]
    fn an_ht_superette_ships_its_staples_at_the_ht_price_of_their_ceilings() {
        let shop = ht_shop();
        shop.set("sales.regulated_policy", json!("block"));
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        let created = shop
            .db
            .write(move |tx| seed_staples(tx, &actor, &pack))
            .unwrap();
        assert_eq!(created, 14);
        let pack = shop.pack.clone();
        let board = shop
            .db
            .read(move |c| board(c, &pack, crate::catalog::RETAIL_LEVEL_ID))
            .unwrap();
        assert_eq!(board.len(), 14);
        let price = |code: &str| board.iter().find(|r| r.code == code).unwrap().price;
        assert_eq!(price("oil_5l"), Money(59_633), "650.00 TTC at 9 %");
        assert_eq!(price("oil_1l"), Money(11_468), "125.00 TTC at 9 %");
        assert_eq!(price("milk_sachet_1l"), Money(2_500), "exempt: HT is TTC");
        for staple in &board {
            // The till sells one unit at no more than its ceiling TTC, never flagged above it.
            let line = priced(&shop, &staple.product_id).unwrap().lines[0].clone();
            assert!(line.amounts.total_ttc <= staple.ceiling, "{}", staple.code);
            assert_eq!(
                line.regulated.map(|f| f.above),
                Some(false),
                "{}",
                staple.code
            );
            assert_eq!(
                (staple.price_ttc, staple.above),
                (line.amounts.total_ttc, false),
                "{}",
                staple.code
            );
        }
    }
}
