//! In-store weight and price barcodes (spec §8, SAL-014, HW-030): EAN-13 numbers of the GS1
//! restricted-circulation range (prefixes 20–29, and the UPC-style 02) printed by label scales
//! carry a PLU and a weight, a price or a count. The layouts are data: the regulatory pack lists
//! the conventions of the scales sold in Algeria (`scaleBarcodes`; GS1 Algeria publishes no
//! national layout), the shop's setting `scale.barcode_formats` replaces them. The EAN-13 check
//! digit is always verified; a layout's internal price/weight verifier digit is skipped (not
//! checked) and such layouts are never used to print.

use crate::catalog::codes::{ean13_check_digit, is_valid_ean13};
use crate::catalog::products::{self, Product};
use crate::company::{self, PriceMode};
use crate::error::AppError;
use crate::money::{Money, QTY_SCALE, Qty};
use crate::tax::tva::{self, Discount, LineInput};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::sync::OnceLock;

pub const SETTING: &str = "scale.barcode_formats";
pub const MAX_LAYOUTS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ScaleValue {
    Weight,
    Price,
    Count,
}

/// `PP` prefix, `C…` PLU, optional verifier `V`, `W…` value, EAN-13 check digit: 13 digits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleBarcodeLayout {
    pub prefix: String,
    pub plu_digits: u8,
    pub verifier: bool,
    pub value: ScaleValue,
    pub value_digits: u8,
    /// Implied decimals of the value: a weight in kg with 3 (grams), a price in DA with 0 or 2.
    pub decimals: u8,
}

impl ScaleBarcodeLayout {
    /// The field at fault, if any.
    pub fn problem(&self) -> Option<&'static str> {
        let p = self.prefix.as_bytes();
        let prefix_ok = p.len() == 2
            && p.iter().all(u8::is_ascii_digit)
            && (p.first() == Some(&b'2') || self.prefix == "02");
        if !prefix_ok {
            return Some("prefix");
        }
        if !(4..=6).contains(&self.plu_digits) {
            return Some("pluDigits");
        }
        if !(4..=6).contains(&self.value_digits)
            || 2 + self.plu_digits + u8::from(self.verifier) + self.value_digits != 12
        {
            return Some("valueDigits");
        }
        let most = match self.value {
            ScaleValue::Weight => 3,
            ScaleValue::Price => 2,
            ScaleValue::Count => 0,
        };
        if self.decimals > most {
            return Some("decimals");
        }
        None
    }

    /// Milli-units (weight, count) or centimes (price) per unit of the printed value.
    fn unit(&self) -> Option<i64> {
        match self.value {
            ScaleValue::Weight => 10_i64.checked_pow(u32::from(3_u8.checked_sub(self.decimals)?)),
            ScaleValue::Price => 10_i64.checked_pow(u32::from(2_u8.checked_sub(self.decimals)?)),
            ScaleValue::Count => (self.decimals == 0).then_some(QTY_SCALE),
        }
    }
}

pub fn validate_layouts(layouts: &[ScaleBarcodeLayout]) -> Result<(), AppError> {
    if layouts.len() > MAX_LAYOUTS {
        return Err(AppError::validation("layouts", "value_invalid"));
    }
    for (i, layout) in layouts.iter().enumerate() {
        if let Some(field) = layout.problem() {
            return Err(AppError::validation(
                &format!("layouts.{i}.{field}"),
                "value_invalid",
            ));
        }
        if layouts.iter().take(i).any(|l| l.prefix == layout.prefix) {
            return Err(AppError::validation(
                &format!("layouts.{i}.prefix"),
                "prefix_taken",
            ));
        }
    }
    Ok(())
}

/// The setting's value: `null` (the pack's layouts) or a valid list. The error names the row and
/// field at fault (`layouts.{i}.{field}`), which `settings::set_value` passes on to the UI.
pub fn check_setting(value: &serde_json::Value) -> Result<(), AppError> {
    if value.is_null() {
        return Ok(());
    }
    let layouts: Vec<ScaleBarcodeLayout> = serde_json::from_value(value.clone())
        .map_err(|_| AppError::validation("value", "value_invalid"))?;
    validate_layouts(&layouts)
}

/// The setting's validator (`SettingDef.validate`).
pub fn valid_setting(value: &serde_json::Value) -> bool {
    check_setting(value).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleCode {
    pub prefix: String,
    pub plu: u32,
    pub value: ScaleValue,
    /// Weight or count in milli-units, price in centimes.
    pub raw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleScan {
    pub code: ScaleCode,
    /// The price printed on a price label.
    pub label_price: Option<Money>,
    /// What the till charges, TTC, for the computed quantity of a price label (set exactly when
    /// `label_price` is): the amount the UI shows beside the label's price, never a sum it makes.
    /// Weight and count labels are priced like any weighed line, by `sales_preview`.
    pub till: Option<Money>,
    /// What the till charges for the computed quantity minus the label's price (price labels:
    /// the quantity is the nearest gram, so a few centimes may differ; HW-030 "discrepancy").
    pub difference: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaleHit {
    pub product: Product,
    pub qty: Qty,
    pub scan: ScaleScan,
}

/// The bundled pack's layouts, parsed once.
pub fn pack_layouts() -> &'static [ScaleBarcodeLayout] {
    static DEFAULTS: OnceLock<Vec<ScaleBarcodeLayout>> = OnceLock::new();
    DEFAULTS.get_or_init(|| match crate::datapack::bundled() {
        Ok(pack) => pack.scale_barcodes.default_layouts,
        Err(error) => {
            tracing::error!(%error, "no scale barcode layouts: the data pack is invalid");
            Vec::new()
        }
    })
}

/// The shop's layouts, else the pack's.
pub fn layouts(conn: &Connection) -> Result<Vec<ScaleBarcodeLayout>, AppError> {
    let shop: Option<Vec<ScaleBarcodeLayout>> = crate::settings::get(conn, SETTING)?;
    Ok(shop.unwrap_or_else(|| pack_layouts().to_vec()))
}

pub fn parse(code: &str, layouts: &[ScaleBarcodeLayout]) -> Option<ScaleCode> {
    let code = code.trim();
    if !is_valid_ean13(code) {
        return None;
    }
    let layout = layouts
        .iter()
        .find(|l| l.problem().is_none() && code.starts_with(l.prefix.as_str()))?;
    let body = code.get(2..12)?;
    let plu_end = usize::from(layout.plu_digits);
    let value_start = plu_end + usize::from(layout.verifier);
    let plu: u32 = body.get(..plu_end)?.parse().ok()?;
    let value: i64 = body.get(value_start..)?.parse().ok()?;
    Some(ScaleCode {
        prefix: layout.prefix.clone(),
        plu,
        value: layout.value,
        raw: value.checked_mul(layout.unit()?)?,
    })
}

/// The EAN-13 a label printed by the POS carries (weighed labels, Task 3): `raw` rounded half up
/// to the layout's unit. `None` for a verifier layout, a PLU or a value that does not fit.
pub fn encode(layout: &ScaleBarcodeLayout, plu: u32, raw: i64) -> Option<String> {
    if layout.problem().is_some() || layout.verifier || raw <= 0 || plu == 0 {
        return None;
    }
    let unit = layout.unit()?;
    let value = raw.checked_add(unit / 2)? / unit;
    let (pw, vw) = (
        usize::from(layout.plu_digits),
        usize::from(layout.value_digits),
    );
    let plu_max = 10_u64.checked_pow(u32::from(layout.plu_digits))?;
    let value_max = 10_i64.checked_pow(u32::from(layout.value_digits))?;
    if u64::from(plu) >= plu_max || value == 0 || value >= value_max {
        return None;
    }
    let body = format!("{}{plu:0pw$}{value:0vw$}", layout.prefix);
    let check = ean13_check_digit(&body)?;
    Some(format!("{body}{check}"))
}

/// A scanned scale code: the product of the PLU and the quantity the label means. `Ok(None)`
/// when the code is no scale code; a scale code the shop cannot sell is refused with a code the
/// cashier can act on.
pub fn resolve(conn: &Connection, code: &str) -> Result<Option<ScaleHit>, AppError> {
    let Some(parsed) = parse(code, &layouts(conn)?) else {
        return Ok(None);
    };
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM products WHERE plu = ?1 AND archived_at IS NULL",
            [parsed.plu],
            |r| r.get(0),
        )
        .optional()?;
    let Some(id) = id else {
        return Err(AppError::rejected(
            json!({ "plu": parsed.plu }),
            "scale_plu_unknown",
        ));
    };
    let product = products::get(conn, &id)?;
    let by_weight = matches!(parsed.value, ScaleValue::Weight | ScaleValue::Price);
    if by_weight && (!product.is_weighed || product.unit_code != "kg") {
        return Err(AppError::rejected(
            json!({ "plu": parsed.plu, "productId": product.id }),
            "scale_product_not_weighed",
        ));
    }
    let invalid = || AppError::validation("code", "value_invalid");
    // A price label shows what the customer pays (TTC): the quantity is the label divided by the
    // TTC price per kg, which in an HT shop carries its TVA (the tax module rounds it).
    let (qty, label) = match parsed.value {
        ScaleValue::Weight | ScaleValue::Count => (Qty(parsed.raw), None),
        ScaleValue::Price => {
            if !product.sell_price.is_positive() {
                return Err(AppError::rejected(
                    json!({ "plu": parsed.plu }),
                    "scale_price_missing",
                ));
            }
            let mode = company::get(conn)?
                .ok_or_else(|| AppError::conflict("setup_required"))?
                .price_mode;
            let per_kg = charged(mode, &product, Qty::ONE)?;
            let qty = Money(parsed.raw)
                .mul_div(QTY_SCALE, per_kg.0)
                .ok_or_else(invalid)?;
            (Qty(qty.0), Some((Money(parsed.raw), mode)))
        }
    };
    if !qty.is_positive() {
        return Err(invalid());
    }
    let (label_price, till, difference) = match label {
        Some((label, mode)) => {
            let till = charged(mode, &product, qty)?;
            let difference = till.checked_sub(label).ok_or_else(invalid)?;
            (Some(label), Some(till), difference)
        }
        None => (None, None, Money::ZERO),
    };
    Ok(Some(ScaleHit {
        product,
        qty,
        scan: ScaleScan {
            code: parsed,
            label_price,
            till,
            difference,
        },
    }))
}

/// What the till charges, TTC, for `qty` of `product` at its catalogue price in the shop's price
/// mode: the tax module's line amount, so the label check rounds exactly as the sale does.
fn charged(mode: PriceMode, product: &Product, qty: Qty) -> Result<Money, AppError> {
    let line = LineInput {
        unit_price: product.sell_price,
        qty,
        discount: Discount::None,
        rate: product.tax_rate_bp,
    };
    Ok(tva::compute_line(mode, &line)?.total_ttc)
}

/// Layout prefixes that also start the shop's in-store EAN-13 codes: an unregistered in-store
/// code would read as a weight. The settings screen warns (M2 PRD-003 keeps `20` by default).
pub fn internal_prefix_overlaps(conn: &Connection) -> Result<Vec<String>, AppError> {
    let internal: String = crate::settings::get(conn, "products.internal_ean_prefix")?;
    Ok(layouts(conn)?
        .into_iter()
        .map(|l| l.prefix)
        .filter(|p| internal.starts_with(p.as_str()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::catalog::products::{Barcode, ProductInput};
    use crate::catalog::search::find_by_code;
    use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID};
    use crate::company::{PriceMode, TaxRegime};
    use crate::db::Db;
    use crate::testkit::{Shop, shop, shop_with};
    use proptest::prelude::*;
    use serde_json::json;

    fn ean(body: &str) -> String {
        format!("{body}{}", ean13_check_digit(body).unwrap())
    }

    fn weighed(shop: &Shop, name: &str, plu: u32, price: i64) -> Product {
        let mut input = ProductInput::simple(name, UNIT_KG_ID, &shop.tva9, Money(price));
        input.is_weighed = true;
        input.plu = Some(plu);
        shop.product_with(input)
    }

    fn layout(prefix: &str, value: ScaleValue, decimals: u8) -> ScaleBarcodeLayout {
        ScaleBarcodeLayout {
            prefix: prefix.into(),
            plu_digits: 5,
            verifier: false,
            value,
            value_digits: 5,
            decimals,
        }
    }

    #[test]
    fn parses_the_default_weight_price_and_verifier_layouts() {
        let layouts = pack_layouts();
        let weight = parse(&ean("211001201750"), layouts).unwrap();
        assert_eq!(
            (weight.plu, weight.value, weight.raw),
            (10012, ScaleValue::Weight, 1_750),
            "21 10012 01750: 1.750 kg"
        );
        let price = parse(&ean("220012300450"), layouts).unwrap();
        assert_eq!(
            (price.plu, price.value, price.raw),
            (123, ScaleValue::Price, 45_000),
            "22 00123 00450: 450 DA"
        );
        let verifier = parse(&ean("290828700274"), layouts).unwrap();
        assert_eq!(
            (verifier.plu, verifier.raw),
            (828, 274),
            "29 0828 [7] 00274: the verifier digit is skipped"
        );
        let count = layout("23", ScaleValue::Count, 0);
        let pieces = parse(&ean("230007700006"), &[count]).unwrap();
        assert_eq!(
            (pieces.plu, pieces.value, pieces.raw),
            (77, ScaleValue::Count, 6_000)
        );
    }

    #[test]
    fn refuses_a_wrong_check_digit_and_unknown_prefixes() {
        let layouts = pack_layouts();
        let good = ean("211001201750");
        let last = good.chars().last().and_then(|c| c.to_digit(10)).unwrap();
        let bad = format!("{}{}", &good[..12], (last + 1) % 10);
        assert_eq!(
            parse(&bad, layouts),
            None,
            "the EAN-13 check digit is verified"
        );
        assert_eq!(
            parse(&ean("613000000001"), layouts),
            None,
            "a GS1 Algeria product"
        );
        assert_eq!(
            parse(&ean("251001201750"), layouts),
            None,
            "25 has no layout"
        );
        assert_eq!(
            parse(&ean("200000000001"), layouts),
            None,
            "20 stays for in-store codes"
        );
        assert_eq!(parse("21100120175", layouts), None, "12 digits");
        assert_eq!(
            parse(&format!(" {good} "), layouts),
            parse(&good, layouts),
            "spaces around a scan are ignored"
        );
    }

    #[test]
    fn layouts_fill_twelve_digits_and_keep_their_prefixes_apart() {
        let ok = layout("21", ScaleValue::Weight, 3);
        assert_eq!(ok.problem(), None);
        for (bad, field) in [
            (
                ScaleBarcodeLayout {
                    prefix: "31".into(),
                    ..ok.clone()
                },
                "prefix",
            ),
            (
                ScaleBarcodeLayout {
                    prefix: "2".into(),
                    ..ok.clone()
                },
                "prefix",
            ),
            (
                ScaleBarcodeLayout {
                    plu_digits: 3,
                    ..ok.clone()
                },
                "pluDigits",
            ),
            (
                ScaleBarcodeLayout {
                    value_digits: 6,
                    ..ok.clone()
                },
                "valueDigits",
            ),
            (
                ScaleBarcodeLayout {
                    value: ScaleValue::Price,
                    decimals: 3,
                    ..ok.clone()
                },
                "decimals",
            ),
            (
                ScaleBarcodeLayout {
                    value: ScaleValue::Count,
                    decimals: 1,
                    ..ok.clone()
                },
                "decimals",
            ),
        ] {
            assert_eq!(bad.problem(), Some(field), "{bad:?}");
        }
        assert_eq!(
            layout("02", ScaleValue::Weight, 3).problem(),
            None,
            "UPC-style 02"
        );
        let twice = validate_layouts(&[ok.clone(), ok]).unwrap_err().payload();
        assert_eq!(
            (twice.code.as_str(), twice.field.as_deref()),
            ("prefix_taken", Some("layouts.1.prefix"))
        );
    }

    #[test]
    fn a_weight_label_becomes_the_line_quantity() {
        let shop = shop();
        let tomato = weighed(&shop, "Tomates", 10012, 12_000);
        let code = ean("211001201750");
        let hit = shop.db.read(move |c| resolve(c, &code)).unwrap().unwrap();
        assert_eq!((hit.product.id, hit.qty), (tomato.id, Qty(1_750)));
        assert_eq!(
            (hit.scan.label_price, hit.scan.difference),
            (None, Money::ZERO)
        );
    }

    #[test]
    fn a_price_label_gives_the_nearest_quantity_and_reports_the_difference() {
        let shop = shop();
        weighed(&shop, "Tomates", 123, 12_000);
        let exact = ean("220012300450");
        let hit = shop.db.read(move |c| resolve(c, &exact)).unwrap().unwrap();
        assert_eq!(
            (hit.qty, hit.scan.label_price, hit.scan.difference),
            (Qty(3_750), Some(Money(45_000)), Money::ZERO),
            "450 DA at 120 DA/kg is 3.750 kg exactly"
        );
        let odd = ean("220012300451");
        let hit = shop.db.read(move |c| resolve(c, &odd)).unwrap().unwrap();
        assert_eq!(
            (hit.qty, hit.scan.difference),
            (Qty(3_758), Money(-4)),
            "451 DA → 3 758 g → 450,96 DA at the till: 4 centimes less than the label"
        );
    }

    #[test]
    fn a_price_label_in_an_ht_shop_is_read_at_the_price_with_tva() {
        let shop = shop_with(TaxRegime::Reel, PriceMode::Ht);
        weighed(&shop, "Tomates", 123, 12_000);
        let code = ean("220012300450");
        let hit = shop.db.read(move |c| resolve(c, &code)).unwrap().unwrap();
        assert_eq!(
            (hit.qty, hit.scan.label_price, hit.scan.difference),
            (Qty(3_440), Some(Money(45_000)), Money(-5)),
            "120 DA HT/kg + 9 % = 130,80 DA/kg: 450 DA → 3 440 g → 412,80 HT + 37,15 TVA = 449,95 DA"
        );
    }

    #[test]
    fn a_scale_code_needs_a_known_plu_and_a_weighed_product() {
        let shop = shop();
        let unknown = ean("211001201750");
        let refused = shop.db.read(move |c| resolve(c, &unknown)).unwrap_err();
        assert_eq!(refused.code(), "scale_plu_unknown");
        assert_eq!(refused.payload().details, Some(json!({ "plu": 10012 })));
        let mut eggs = ProductInput::simple("Œufs x30", UNIT_PIECE_ID, &shop.tva9, Money(90_000));
        eggs.plu = Some(10012);
        shop.product_with(eggs);
        let code = ean("211001201750");
        let refused = shop.db.read(move |c| resolve(c, &code)).unwrap_err();
        assert_eq!(refused.code(), "scale_product_not_weighed");
        weighed(&shop, "Pois chiches", 555, 0);
        let free = ean("220055500450");
        let refused = shop.db.read(move |c| resolve(c, &free)).unwrap_err();
        assert_eq!(refused.code(), "scale_price_missing");
    }

    #[test]
    fn find_by_code_prefers_a_registered_barcode_then_reads_scale_codes() {
        let shop = shop();
        let tomato = weighed(&shop, "Tomates", 10012, 12_000);
        let code = ean("211001201750");
        let scanned = code.clone();
        let hit = shop
            .db
            .read(move |c| find_by_code(c, &scanned))
            .unwrap()
            .unwrap();
        assert_eq!(
            (hit.product.id.as_str(), hit.pack_qty),
            (tomato.id.as_str(), Qty(1_750))
        );
        assert_eq!(hit.scale.as_ref().map(|s| s.code.plu), Some(10012));
        let mut honey =
            ProductInput::simple("Miel 500 g", UNIT_PIECE_ID, &shop.tva9, Money(150_000));
        honey.barcodes = vec![Barcode {
            barcode: code.clone(),
            pack_qty: Qty::ONE,
            label: None,
        }];
        let honey = shop.product_with(honey);
        let hit = shop
            .db
            .read(move |c| find_by_code(c, &code))
            .unwrap()
            .unwrap();
        assert_eq!(
            (hit.product.id, hit.pack_qty, hit.scale),
            (honey.id, Qty::ONE, None)
        );
    }

    #[test]
    fn the_shop_layouts_replace_the_pack_defaults() {
        let shop = shop();
        weighed(&shop, "Tomates", 10012, 12_000);
        shop.set(
            SETTING,
            json!([{ "prefix": "24", "pluDigits": 5, "verifier": false, "value": "weight", "valueDigits": 5, "decimals": 3 }]),
        );
        let pack_code = ean("211001201750");
        assert!(
            shop.db
                .read(move |c| resolve(c, &pack_code))
                .unwrap()
                .is_none()
        );
        let shop_code = ean("241001201750");
        assert_eq!(
            shop.db
                .read(move |c| resolve(c, &shop_code))
                .unwrap()
                .map(|h| h.qty),
            Some(Qty(1_750))
        );
        let refused = shop.db.write(|tx| {
            crate::settings::set_value(
                tx,
                &Actor::system(None),
                SETTING,
                json!([{ "prefix": "31", "pluDigits": 5, "verifier": false, "value": "weight", "valueDigits": 5, "decimals": 3 }]),
            )
        });
        assert_eq!(refused.unwrap_err().code(), "value_invalid");
    }

    #[test]
    fn saving_the_layouts_names_the_row_and_field_at_fault() {
        let db = Db::open_memory().unwrap();
        let refused = |value: serde_json::Value| {
            let error = db
                .write(move |tx| {
                    crate::settings::set_value(tx, &Actor::system(None), SETTING, value)
                })
                .unwrap_err()
                .payload();
            (error.code, error.field)
        };
        let row = json!({ "prefix": "21", "pluDigits": 5, "verifier": false, "value": "weight", "valueDigits": 5, "decimals": 3 });
        assert_eq!(
            refused(json!([row.clone(), row.clone()])),
            ("prefix_taken".into(), Some("layouts.1.prefix".into()))
        );
        let mut short = row.clone();
        short["prefix"] = json!("27");
        short["pluDigits"] = json!(3);
        assert_eq!(
            refused(json!([row, short])),
            ("value_invalid".into(), Some("layouts.1.pluDigits".into()))
        );
        assert_eq!(
            refused(json!([{ "prefix": "21" }])),
            ("value_invalid".into(), Some("value".into())),
            "not a list of layouts"
        );
    }

    #[test]
    fn a_layout_starting_the_internal_prefix_is_reported() {
        let shop = shop();
        assert!(shop.db.read(internal_prefix_overlaps).unwrap().is_empty());
        shop.set("products.internal_ean_prefix", json!("210"));
        assert_eq!(shop.db.read(internal_prefix_overlaps).unwrap(), ["21"]);
    }

    proptest! {
        #[test]
        fn weight_codes_round_trip(plu in 1_u32..=99_999, grams in 1_i64..=99_999) {
            let layout = layout("21", ScaleValue::Weight, 3);
            let code = encode(&layout, plu, grams).unwrap();
            let parsed = parse(&code, std::slice::from_ref(&layout)).unwrap();
            prop_assert_eq!((parsed.plu, parsed.raw), (plu, grams));
        }

        #[test]
        fn price_labels_never_miss_by_more_than_half_a_gram(price in 1_000_i64..=500_000, label in 100_i64..=9_999_900) {
            let unit = Money(price);
            let qty = Qty(Money(label).mul_div(QTY_SCALE, unit.0).unwrap().0);
            let charged = unit.mul_qty(qty).unwrap();
            prop_assert!((charged.0 - label).abs() * 2 * QTY_SCALE <= price + 2 * QTY_SCALE);
        }
    }

    #[test]
    fn verifier_layouts_are_never_encoded() {
        let with_verifier = pack_layouts().iter().find(|l| l.verifier).unwrap();
        assert_eq!(encode(with_verifier, 828, 274), None);
        assert_eq!(
            encode(&layout("21", ScaleValue::Weight, 3), 100_000, 10),
            None,
            "PLU too long"
        );
        assert_eq!(
            encode(&layout("21", ScaleValue::Weight, 3), 10, 100_000),
            None,
            "100 kg does not fit"
        );
    }

    /// The UI never computes money: a price label carries what the till charges, so the settings'
    /// test box shows it as received instead of adding the difference to the label's price.
    mod till {
        use super::*;

        #[test]
        fn a_price_label_carries_what_the_till_charges() {
            let shop = shop();
            weighed(&shop, "Tomates", 123, 12_000);
            let odd = ean("220012300451");
            let hit = shop.db.read(move |c| resolve(c, &odd)).unwrap().unwrap();
            assert_eq!(
                (hit.scan.label_price, hit.scan.till, hit.scan.difference),
                (Some(Money(45_100)), Some(Money(45_096)), Money(-4)),
                "451 DA → 3 758 g → 450,96 DA at the till"
            );
        }

        #[test]
        fn the_till_amount_of_an_ht_shop_includes_the_tva() {
            let shop = shop_with(TaxRegime::Reel, PriceMode::Ht);
            weighed(&shop, "Tomates", 123, 12_000);
            let code = ean("220012300450");
            let hit = shop.db.read(move |c| resolve(c, &code)).unwrap().unwrap();
            assert_eq!(
                (hit.scan.till, hit.scan.difference),
                (Some(Money(44_995)), Money(-5)),
                "3 440 g at 120 DA HT/kg: 412,80 HT + 37,15 TVA = 449,95 DA"
            );
        }

        #[test]
        fn weight_and_count_labels_carry_no_till_amount() {
            let shop = shop();
            weighed(&shop, "Tomates", 10012, 12_000);
            let code = ean("211001201750");
            let hit = shop.db.read(move |c| resolve(c, &code)).unwrap().unwrap();
            assert_eq!(
                (hit.scan.label_price, hit.scan.till, hit.scan.difference),
                (None, None, Money::ZERO),
                "the till prices a weight label as any weighed line (sales_preview)"
            );
        }

        #[test]
        fn the_difference_is_the_till_amount_minus_the_label_price() {
            let shop = shop();
            weighed(&shop, "Tomates", 123, 12_345);
            for body in [
                "220012300450",
                "220012300451",
                "220012300999",
                "220012301234",
            ] {
                let code = ean(body);
                let hit = shop.db.read(move |c| resolve(c, &code)).unwrap().unwrap();
                let (Some(label), Some(till)) = (hit.scan.label_price, hit.scan.till) else {
                    panic!("{body}: a price label has both amounts");
                };
                assert_eq!(till.checked_sub(label), Some(hit.scan.difference), "{body}");
            }
        }
    }
}
