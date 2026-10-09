//! Goods for the label scales (PRD-074, HW-030 PLU list): the PLU (`products.plu`), the scale
//! hotkey, the tare and the shelf life, and the PLU table the scales' PC tools import as xlsx or
//! csv (the vendors' direct protocols are a later milestone).

use crate::audit::{self, Actor};
use crate::catalog::products;
use crate::catalog::scale_codes::{self, ScaleValue};
use crate::company::PriceMode;
use crate::error::AppError;
use crate::money::Money;
use crate::table::{Cell, ColumnKind, Table};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleItem {
    pub product_id: String,
    pub plu: u32,
    pub reference: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub unit_code: String,
    pub is_weighed: bool,
    /// TTC per kg (or per piece).
    pub price: Money,
    pub hotkey: Option<u16>,
    pub tare_g: u32,
    pub shelf_life_days: Option<u16>,
    /// What the scale prints before the value: the prefix of the first weight (or count) layout
    /// whose PLU field holds the PLU, and the PLU, e.g. `2100012`.
    pub barcode_prefix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleItemInput {
    pub hotkey: Option<u16>,
    pub tare_g: u32,
    pub shelf_life_days: Option<u16>,
}

fn price_mode(conn: &Connection) -> Result<PriceMode, AppError> {
    Ok(crate::company::get(conn)?.map_or(PriceMode::Ttc, |c| c.price_mode))
}

fn item(conn: &Connection, mode: PriceMode, product_id: &str) -> Result<ScaleItem, AppError> {
    let p = products::get(conn, product_id)?;
    let plu = p
        .plu
        .ok_or_else(|| AppError::validation("plu", "scale_plu_required"))?;
    let extra: Option<(Option<u16>, u32, Option<u16>)> = conn
        .query_row(
            "SELECT hotkey, tare_g, shelf_life_days FROM scale_items WHERE product_id = ?1",
            [product_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (hotkey, tare_g, shelf_life_days) = extra.unwrap_or((None, 0, None));
    let wanted = if p.is_weighed {
        ScaleValue::Weight
    } else {
        ScaleValue::Count
    };
    // A layout whose PLU field cannot hold this PLU would print a wrong code (as `encode` refuses).
    let fits = |digits: u8| 10_u64.checked_pow(u32::from(digits)) > Some(u64::from(plu));
    let barcode_prefix = scale_codes::layouts(conn)?
        .into_iter()
        .find(|l| l.value == wanted && !l.verifier && l.problem().is_none() && fits(l.plu_digits))
        .map(|l| format!("{}{plu:0w$}", l.prefix, w = usize::from(l.plu_digits)));
    Ok(ScaleItem {
        product_id: p.id.clone(),
        plu,
        reference: p.reference.clone(),
        name_ar: p.name_ar.clone(),
        name_fr: p.name_fr.clone(),
        name_en: p.name_en.clone(),
        unit_code: p.unit_code.clone(),
        is_weighed: p.is_weighed,
        price: crate::labels::ttc(mode, p.sell_price, p.tax_rate_bp).unwrap_or(p.sell_price),
        hotkey,
        tare_g,
        shelf_life_days,
        barcode_prefix,
    })
}

/// Every unarchived product with a PLU, by PLU.
pub fn list(conn: &Connection) -> Result<Vec<ScaleItem>, AppError> {
    let mode = price_mode(conn)?;
    let ids: Vec<String> = conn
        .prepare(
            "SELECT id FROM products WHERE plu IS NOT NULL AND archived_at IS NULL ORDER BY plu",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    ids.iter().map(|id| item(conn, mode, id)).collect()
}

/// Saves the scale settings of a product that has a PLU (hotkey 1–999 unique across products,
/// tare 0–99 999 g, shelf life 0–3 650 days), audited as `scale.item_set`.
pub fn set(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    input: &ScaleItemInput,
    now: i64,
) -> Result<ScaleItem, AppError> {
    let product = products::get(tx, product_id)?;
    if product.plu.is_none() {
        return Err(AppError::validation("plu", "scale_plu_required"));
    }
    if input.hotkey.is_some_and(|h| !(1..=999).contains(&h)) {
        return Err(AppError::validation("hotkey", "value_invalid"));
    }
    if input.tare_g > 99_999 {
        return Err(AppError::validation("tareG", "value_invalid"));
    }
    if input.shelf_life_days.is_some_and(|d| d > 3_650) {
        return Err(AppError::validation("shelfLifeDays", "value_invalid"));
    }
    if let Some(hotkey) = input.hotkey {
        let taken: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM scale_items WHERE hotkey = ?1 AND product_id <> ?2)",
            params![hotkey, product_id],
            |r| r.get(0),
        )?;
        if taken {
            return Err(AppError::validation("hotkey", "scale_hotkey_taken"));
        }
    }
    tx.execute(
        "INSERT INTO scale_items (product_id, hotkey, tare_g, shelf_life_days, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (product_id) DO UPDATE SET hotkey = excluded.hotkey, tare_g = excluded.tare_g,
           shelf_life_days = excluded.shelf_life_days, updated_at = excluded.updated_at",
        params![
            product_id,
            input.hotkey,
            input.tare_g,
            input.shelf_life_days,
            now
        ],
    )?;
    audit::append(
        tx,
        &actor.event(
            "scale.item_set",
            "product",
            Some(product_id),
            json!({ "after": input }),
        ),
    )?;
    item(tx, price_mode(tx)?, product_id)
}

/// The PLU list the scale makers' PC tools import (Rongta, ACLAS, Dibal all read xlsx/csv with a
/// column mapping step): one row per scale item, names in Latin and Arabic, the TTC price in
/// centimes (the writer shows DA), 1 for goods sold by weight, 0 for pieces.
pub fn plu_table(conn: &Connection) -> Result<Table, AppError> {
    let mut table = Table::new(&[
        ("scalePlu", ColumnKind::Int),
        ("scaleHotkey", ColumnKind::Int),
        ("scaleName", ColumnKind::Text),
        ("scaleNameAr", ColumnKind::Text),
        ("scaleWeighed", ColumnKind::Int),
        ("scaleUnit", ColumnKind::Text),
        ("scalePrice", ColumnKind::Money),
        ("scaleTare", ColumnKind::Int),
        ("scaleShelfLife", ColumnKind::Int),
        ("scaleBarcode", ColumnKind::Code),
    ]);
    for i in list(conn)? {
        let latin = [&i.name_fr, &i.name_en, &i.name_ar]
            .into_iter()
            .find(|n| !n.is_empty())
            .cloned()
            .unwrap_or_default();
        table.push(vec![
            Cell::int(i64::from(i.plu)),
            i.hotkey.map_or(Cell::NULL, |h| Cell::int(i64::from(h))),
            Cell::text(latin),
            Cell::text(i.name_ar),
            Cell::int(i64::from(i.is_weighed)),
            Cell::text(i.unit_code),
            Cell::money(i.price),
            Cell::int(i64::from(i.tare_g)),
            i.shelf_life_days
                .map_or(Cell::NULL, |d| Cell::int(i64::from(d))),
            Cell::opt_text(i.barcode_prefix),
        ])?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::products::ProductInput;
    use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID};
    use crate::money::Money;
    use crate::table::Cell;
    use crate::testkit::{NOON, Shop, shop};

    fn weighed(shop: &Shop, name: &str, plu: u32, price: i64) -> String {
        let mut input = ProductInput::simple(name, UNIT_KG_ID, &shop.tva9, Money(price));
        input.is_weighed = true;
        input.plu = Some(plu);
        shop.product_with(input).id
    }

    fn set_item(shop: &Shop, id: &str, input: ScaleItemInput) -> Result<ScaleItem, AppError> {
        let actor = shop.owner.actor();
        let id = id.to_owned();
        shop.db.write(move |tx| set(tx, &actor, &id, &input, NOON))
    }

    #[test]
    fn weighed_goods_with_a_plu_are_the_scale_items() {
        let shop = shop();
        let tomato = weighed(&shop, "Tomates", 12, 12_000);
        weighed(&shop, "Courgettes", 7, 9_000);
        shop.product("Lait Candia 1 L", Money(15_000), &shop.tva9); // no PLU: not a scale item
        let items = shop.db.read(list).unwrap();
        let plus: Vec<u32> = items.iter().map(|i| i.plu).collect();
        assert_eq!(plus, [7, 12]);
        let saved = set_item(
            &shop,
            &tomato,
            ScaleItemInput {
                hotkey: Some(3),
                tare_g: 120,
                shelf_life_days: Some(3),
            },
        )
        .unwrap();
        assert_eq!(
            (saved.hotkey, saved.tare_g, saved.shelf_life_days),
            (Some(3), 120, Some(3))
        );
        assert_eq!(
            saved.barcode_prefix.as_deref(),
            Some("2100012"),
            "prefix 21 + PLU on 5 digits"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'scale.item_set'"),
            1
        );
    }

    #[test]
    fn hotkeys_are_unique_and_a_plu_is_needed() {
        let shop = shop();
        let tomato = weighed(&shop, "Tomates", 12, 12_000);
        let zucchini = weighed(&shop, "Courgettes", 7, 9_000);
        set_item(
            &shop,
            &tomato,
            ScaleItemInput {
                hotkey: Some(3),
                tare_g: 0,
                shelf_life_days: None,
            },
        )
        .unwrap();
        let taken = set_item(
            &shop,
            &zucchini,
            ScaleItemInput {
                hotkey: Some(3),
                tare_g: 0,
                shelf_life_days: None,
            },
        );
        assert_eq!(taken.unwrap_err().payload().code, "scale_hotkey_taken");
        let bad = set_item(
            &shop,
            &zucchini,
            ScaleItemInput {
                hotkey: Some(1_000),
                tare_g: 0,
                shelf_life_days: None,
            },
        );
        assert_eq!(bad.unwrap_err().payload().field.as_deref(), Some("hotkey"));
        let mut eggs = ProductInput::simple("Œufs", UNIT_PIECE_ID, &shop.tva9, Money(2_500));
        eggs.plu = None;
        let eggs = shop.product_with(eggs).id;
        let no_plu = set_item(
            &shop,
            &eggs,
            ScaleItemInput {
                hotkey: None,
                tare_g: 0,
                shelf_life_days: None,
            },
        );
        assert_eq!(no_plu.unwrap_err().code(), "scale_plu_required");
    }

    #[test]
    fn the_plu_table_lists_prices_ttc_names_and_scale_settings() {
        let shop = shop();
        let tomato = weighed(&shop, "Tomates", 12, 12_000);
        set_item(
            &shop,
            &tomato,
            ScaleItemInput {
                hotkey: Some(3),
                tare_g: 120,
                shelf_life_days: Some(3),
            },
        )
        .unwrap();
        let table = shop.db.read(plu_table).unwrap();
        let keys: Vec<&str> = table.columns.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "scalePlu",
                "scaleHotkey",
                "scaleName",
                "scaleNameAr",
                "scaleWeighed",
                "scaleUnit",
                "scalePrice",
                "scaleTare",
                "scaleShelfLife",
                "scaleBarcode"
            ]
        );
        assert_eq!(table.rows.len(), 1);
        let row = &table.rows[0];
        assert_eq!(row[0], Cell::Int(12));
        assert_eq!(row[1], Cell::Int(3));
        assert_eq!(row[2], Cell::text("Tomates"));
        assert_eq!(row[4], Cell::Int(1));
        assert_eq!(row[6], Cell::Int(12_000));
        assert_eq!(row[9], Cell::text("2100012"));
    }

    #[test]
    fn the_barcode_start_comes_from_a_layout_that_holds_the_plu() {
        let shop = shop();
        shop.set(
            scale_codes::SETTING,
            json!([
                { "prefix": "23", "pluDigits": 4, "verifier": false, "value": "weight", "valueDigits": 6, "decimals": 3 },
                { "prefix": "24", "pluDigits": 5, "verifier": false, "value": "weight", "valueDigits": 5, "decimals": 3 }
            ]),
        );
        weighed(&shop, "Tomates", 12, 12_000);
        weighed(&shop, "Olives", 12_345, 90_000);
        let mut eggs = ProductInput::simple("Œufs", UNIT_PIECE_ID, &shop.tva9, Money(2_500));
        eggs.plu = Some(40);
        shop.product_with(eggs);
        let starts: Vec<Option<String>> = shop
            .db
            .read(list)
            .unwrap()
            .into_iter()
            .map(|i| i.barcode_prefix)
            .collect();
        assert_eq!(
            starts,
            [Some("230012".into()), None, Some("2412345".into())],
            "12 fits 4 digits; no count layout for the piece; 12345 needs the 5-digit layout"
        );
    }
}
