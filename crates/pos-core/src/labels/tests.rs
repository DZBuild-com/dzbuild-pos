//! Label contents (LBL-001…013, DOC-034, PRM-021) against a real test shop.

use super::*;
use crate::catalog::products::{Barcode, ProductInput};
use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID};
use crate::company::{PriceMode, TaxRegime};
use crate::money::{Money, Qty};
use crate::promotions::{self, Campaign, PromoKind, PromoScope, PromotionInput, TriggerMode};
use crate::testkit::{NOON, Shop, shop, shop_with};
use serde_json::json;

const DAY: &str = "2026-09-28";

fn milk(shop: &Shop) -> crate::catalog::products::Product {
    let mut input =
        ProductInput::simple("Lait Candia 1 L", UNIT_PIECE_ID, &shop.tva9, Money(15_000));
    input.name_ar = "حليب كانديا 1 لتر".into();
    input.barcodes = vec![Barcode {
        barcode: "6130123456782".into(),
        pack_qty: Qty::ONE,
        label: None,
    }];
    shop.product_with(input)
}

fn one(product_id: &str, copies: u16) -> LabelRequest {
    LabelRequest {
        source: LabelSource::Products {
            items: vec![LabelItem {
                product_id: product_id.into(),
                copies,
                lot_id: None,
            }],
        },
        template_id: None,
        lang: None,
    }
}

fn first(batch: &LabelBatch) -> &LabelData {
    &batch.groups[0].labels[0]
}

#[test]
fn built_in_templates_are_valid_bilingual_and_stored_as_the_default() {
    let shop = shop();
    let templates = shop.db.read(template::templates).unwrap();
    assert_eq!(templates, builtin_templates());
    let ids: Vec<&str> = templates.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "builtin.price_40x25",
            "builtin.product_50x30",
            "builtin.shelf_58x40",
            WEIGHED_TEMPLATE,
            "builtin.jewel"
        ]
    );
    assert!(
        templates
            .iter()
            .all(|t| t.problem().is_none() && t.lang == LabelLang::ArFr)
    );
    let mut wide = templates[0].clone();
    wide.columns = 3;
    wide.width_dmm = 400;
    assert_eq!(
        wide.problem(),
        Some("columnGapDmm"),
        "3 × 40 mm does not fit a 4-inch head"
    );
}

#[test]
fn a_label_carries_names_barcode_and_the_ttc_price() {
    let shop = shop();
    let milk = milk(&shop);
    let request = one(&milk.id, 3);
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(
        (batch.total, batch.groups.len(), batch.warnings.len()),
        (3, 1, 0)
    );
    let label = first(&batch);
    assert_eq!(label.name, "حليب كانديا 1 لتر");
    assert_eq!(label.name_second.as_deref(), Some("Lait Candia 1 L"));
    assert_eq!(label.price, Money(15_000));
    assert_eq!(
        label.barcode,
        Some(LabelBarcode {
            symbology: LabelSymbology::Ean13,
            data: "6130123456782".into()
        })
    );
    assert_eq!(batch.groups[0].template.id, "builtin.price_40x25");
}

#[test]
fn the_label_price_is_the_ttc_paid_with_the_old_price_during_a_promotion() {
    let shop = shop_with(TaxRegime::Reel, PriceMode::Ht);
    let oil = shop.product("Huile Elio 5 L", Money(50_000), &shop.tva19);
    let request = one(&oil.id, 1);
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(
        first(&batch).price,
        Money(59_500),
        "500 DA HT + 19 % = 595 DA TTC (LBL-005)"
    );
    assert_eq!(first(&batch).old_price, None);
    let owner = shop.owner.actor();
    let scope = PromoScope::Products {
        ids: vec![oil.id.clone()],
    };
    shop.db
        .write(move |tx| {
            promotions::create(
                tx,
                &owner,
                PromotionInput {
                    name_ar: String::new(),
                    name_fr: "Rentrée".into(),
                    name_en: String::new(),
                    kind: PromoKind::Percent,
                    campaign: Campaign::Promotion,
                    trigger_mode: TriggerMode::Auto,
                    scope,
                    value: 1_000,
                    buy_qty: None,
                    get_qty: None,
                    min_qty: Qty::ZERO,
                    priority: 0,
                    starts_on: Some("2026-09-01".into()),
                    ends_on: Some("2026-09-30".into()),
                    active: true,
                },
            )
        })
        .unwrap();
    let request = LabelRequest {
        template_id: Some("builtin.product_50x30".into()),
        ..one(&oil.id, 1)
    };
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(
        (first(&batch).price, first(&batch).old_price),
        (Money(53_550), None),
        "450 DA HT after 10 % + 19 %; this template does not show the old price"
    );
    let request = LabelRequest {
        template_id: Some("builtin.shelf_58x40".into()),
        ..one(&oil.id, 1)
    };
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(
        (first(&batch).price, first(&batch).old_price),
        (Money(53_550), Some(Money(59_500))),
        "the shelf label shows the price before the promotion (PRM-021)"
    );
    let request = one(&oil.id, 1);
    let after = shop
        .db
        .read(move |c| build(c, &request, "2026-10-01"))
        .unwrap();
    assert_eq!(first(&after).price, Money(59_500), "the promotion ended");
}

#[test]
fn a_food_shop_label_without_arabic_is_flagged() {
    let shop = shop(); // activity "general" (superette): every category is guarded
    let milk = milk(&shop);
    let request = LabelRequest {
        lang: Some(LabelLang::Fr),
        ..one(&milk.id, 1)
    };
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(batch.warnings.len(), 1);
    assert_eq!(batch.warnings[0].code, LabelWarningCode::ArabicRequired);
    let refused = check_arabic(&batch, false).unwrap_err();
    assert_eq!(refused.code(), "label_arabic_required");
    assert_eq!(
        refused.payload().details,
        Some(json!({ "productIds": [milk.id] }))
    );
    check_arabic(&batch, true).unwrap();
    shop.set("shop.activity", json!("clothing"));
    let request = LabelRequest {
        lang: Some(LabelLang::Fr),
        ..one(&milk.id, 1)
    };
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert!(
        batch.warnings.is_empty(),
        "a clothing shop is not guarded unless it lists categories"
    );
    shop.set("labels.arabic_categories", json!([]));
    shop.set("shop.activity", json!("general"));
    let request = LabelRequest {
        lang: Some(LabelLang::Fr),
        ..one(&milk.id, 1)
    };
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert!(
        batch.warnings.is_empty(),
        "an explicit empty list turns the guard off"
    );
}

#[test]
fn a_butcher_shop_label_without_arabic_is_flagged() {
    let shop = shop();
    shop.set("shop.activity", json!("butcher"));
    let milk = milk(&shop);
    let request = LabelRequest {
        lang: Some(LabelLang::Fr),
        ..one(&milk.id, 1)
    };
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    let codes: Vec<LabelWarningCode> = batch.warnings.iter().map(|w| w.code).collect();
    assert_eq!(
        codes,
        vec![LabelWarningCode::ArabicRequired],
        "DOC-034: a butcher sells food, every category is guarded"
    );
    assert_eq!(
        check_arabic(&batch, false).unwrap_err().code(),
        "label_arabic_required"
    );
}

#[test]
fn every_food_or_regulated_goods_activity_guards_every_category() {
    use crate::company::ShopActivity;
    let guarded = [
        ShopActivity::General,
        ShopActivity::Butcher,
        ShopActivity::Bakery,
        ShopActivity::FoodService,
        ShopActivity::Parapharmacy,
        ShopActivity::Pharmacy,
        ShopActivity::Cosmetics,
        ShopActivity::Wholesale,
        ShopActivity::Kiosk,
        ShopActivity::Containers,
        ShopActivity::Importer,
        ShopActivity::VanSales,
    ];
    let shop = shop();
    for activity in ShopActivity::ALL {
        shop.set("shop.activity", json!(activity.as_str()));
        let guard = shop.db.read(ArabicGuard::load).unwrap();
        assert_eq!(
            guard.required(None),
            guarded.contains(&activity),
            "DOC-034 guard for {}",
            activity.as_str()
        );
    }
}

#[test]
fn a_guarded_product_without_an_arabic_name_is_flagged_on_an_arabic_template() {
    let shop = shop(); // activity "general": every category is guarded
    let cheese = shop.product("Fromage La Vache qui rit", Money(32_000), &shop.tva9);
    let request = one(&cheese.id, 1);
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(batch.groups[0].template.lang, LabelLang::ArFr);
    assert_eq!(
        (
            first(&batch).name.as_str(),
            first(&batch).name_second.as_deref()
        ),
        ("Fromage La Vache qui rit", None),
        "the Arabic slot falls back to the French name: the label is French only"
    );
    let codes: Vec<LabelWarningCode> = batch.warnings.iter().map(|w| w.code).collect();
    assert!(
        codes.contains(&LabelWarningCode::ArabicNameMissing),
        "DOC-034: {codes:?}"
    );
    assert!(!codes.contains(&LabelWarningCode::ArabicRequired));
    let refused = check_arabic(&batch, false).unwrap_err();
    assert_eq!(
        refused.code(),
        "label_arabic_name_missing",
        "switching templates does not help: the product needs its Arabic name"
    );
    assert_eq!(
        refused.payload().details,
        Some(json!({ "productIds": [cheese.id] }))
    );
    check_arabic(&batch, true).unwrap();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| record_printed(tx, &actor, &batch, true, NOON))
        .unwrap();
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'labels.printed'
               AND json_extract(payload, '$.arabicWarningOverridden') = 1"
        ),
        1,
        "the override is journaled"
    );
    shop.set("shop.activity", json!("clothing"));
    let request = one(&cheese.id, 1);
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert!(
        batch
            .warnings
            .iter()
            .all(|w| w.code != LabelWarningCode::ArabicNameMissing),
        "an unguarded shop is not flagged"
    );
}

#[test]
fn the_template_refusal_comes_before_the_arabic_name_refusal() {
    let warning = |id: &str, code| LabelWarning {
        product_id: id.into(),
        name: id.into(),
        code,
    };
    let batch = LabelBatch {
        groups: Vec::new(),
        warnings: vec![
            warning("p-cheese", LabelWarningCode::ArabicNameMissing),
            warning("p-milk", LabelWarningCode::ArabicRequired),
            warning("p-oil", LabelWarningCode::NoBarcode),
        ],
        total: 2,
    };
    let refused = check_arabic(&batch, false).unwrap_err();
    assert_eq!(
        (refused.code().as_str(), refused.payload().details),
        (
            "label_arabic_required",
            Some(json!({ "productIds": ["p-milk"] }))
        )
    );
    check_arabic(&batch, true).unwrap();
}

#[test]
fn copies_and_batches_are_bounded() {
    let shop = shop();
    let milk = milk(&shop);
    let zero = one(&milk.id, 0);
    let refused = shop
        .db
        .read(move |c| build(c, &zero, DAY))
        .unwrap_err()
        .payload();
    assert_eq!(
        (refused.code.as_str(), refused.field.as_deref()),
        ("value_invalid", Some("items.0.copies"))
    );
    let empty = LabelRequest {
        source: LabelSource::Products { items: vec![] },
        template_id: None,
        lang: None,
    };
    assert_eq!(
        shop.db
            .read(move |c| build(c, &empty, DAY))
            .unwrap_err()
            .code(),
        "labels_empty"
    );
    let items = (0..6)
        .map(|_| LabelItem {
            product_id: milk.id.clone(),
            copies: 999,
            lot_id: None,
        })
        .collect();
    let many = LabelRequest {
        source: LabelSource::Products { items },
        template_id: None,
        lang: None,
    };
    let refused = shop.db.read(move |c| build(c, &many, DAY)).unwrap_err();
    assert_eq!(refused.code(), "labels_too_many");
    let ghost = LabelRequest {
        template_id: Some("nope".into()),
        ..one(&milk.id, 1)
    };
    assert_eq!(
        shop.db
            .read(move |c| build(c, &ghost, DAY))
            .unwrap_err()
            .code(),
        "label_template_not_found"
    );
}

#[test]
fn a_goods_receipt_prints_one_label_per_piece_received_with_its_lot() {
    let shop = shop();
    let milk = milk(&shop);
    let ctx = shop.till();
    let pack = shop.pack.clone();
    let owner = shop.owner.actor();
    let milk_id = milk.id.clone();
    let receipt = shop
        .db
        .write(move |tx| {
            let supplier = crate::suppliers::create(
                tx,
                &owner,
                &pack,
                crate::suppliers::SupplierInput::named("Cevital"),
            )?;
            crate::purchases::receipts::receive(
                tx,
                &ctx,
                crate::purchases::receipts::ReceiptInput {
                    supplier_id: supplier.id,
                    order_id: None,
                    location_id: None,
                    lines: vec![crate::purchases::PurchaseLineInput {
                        product_id: milk_id,
                        qty: Qty(12_000),
                        unit_cost: Money(11_000),
                        pack_qty: None,
                        tax_rate_id: None,
                        discount: crate::tax::tva::Discount::None,
                        origin_line_id: None,
                        lot_number: Some("L2609".into()),
                        expiry_date: Some("2026-12-31".into()),
                        serials: Vec::new(),
                        note: None,
                    }],
                    supplier_ref: None,
                    note: None,
                    invoice: None,
                },
            )
        })
        .unwrap()
        .receipt;
    let request = LabelRequest {
        source: LabelSource::GoodsReceipt {
            document_id: receipt.id.clone(),
        },
        template_id: Some("builtin.shelf_58x40".into()),
        lang: None,
    };
    let mut shelf = builtin_templates()[2].clone();
    shelf.fields.lot = true;
    shelf.fields.expiry = true;
    shop.set(
        "labels.templates",
        serde_json::to_value(vec![shelf]).unwrap(),
    );
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    let label = first(&batch);
    assert_eq!((batch.total, label.copies), (12, 12));
    assert_eq!(
        (label.lot_number.as_deref(), label.expiry.as_deref()),
        (Some("L2609"), Some("2026-12-31"))
    );
    let not_a_receipt = LabelRequest {
        source: LabelSource::GoodsReceipt {
            document_id: milk.id.clone(),
        },
        template_id: None,
        lang: None,
    };
    assert!(
        shop.db
            .read(move |c| build(c, &not_a_receipt, DAY))
            .is_err()
    );
}

#[test]
fn a_count_prints_the_counted_pieces() {
    let shop = shop();
    let milk = milk(&shop);
    let id = milk.id.clone();
    shop.db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO stock_counts (id, location_id, scope, status, started_at, user_id)
                 VALUES ('00000000-0000-7000-8000-0000000c0001',
                         (SELECT id FROM stock_locations ORDER BY rowid LIMIT 1), 'products', 'open', 0, 'u1')",
                [],
            )?;
            tx.execute(
                "INSERT INTO stock_count_lines (count_id, product_id, expected_qty, counted_qty)
                 VALUES ('00000000-0000-7000-8000-0000000c0001', ?1, 0, 7000)",
                [id],
            )?;
            Ok(())
        })
        .unwrap();
    let request = LabelRequest {
        source: LabelSource::Count {
            count_id: "00000000-0000-7000-8000-0000000c0001".into(),
        },
        template_id: None,
        lang: None,
    };
    assert_eq!(
        shop.db
            .read(move |c| build(c, &request, DAY))
            .unwrap()
            .total,
        7
    );
}

#[test]
fn a_weighed_label_prints_weight_amount_and_a_weight_barcode() {
    let shop = shop();
    let mut input = ProductInput::simple("Tomates", UNIT_KG_ID, &shop.tva9, Money(12_000));
    input.name_ar = "طماطم".into();
    input.is_weighed = true;
    input.plu = Some(10012);
    let tomato = shop.product_with(input);
    let id = tomato.id.clone();
    shop.db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO scale_items (product_id, shelf_life_days, updated_at) VALUES (?1, 3, 0)",
                [id],
            )?;
            Ok(())
        })
        .unwrap();
    let request = WeighedLabelRequest {
        product_id: tomato.id.clone(),
        net: Qty(1_250),
        copies: 1,
        template_id: None,
        lang: None,
    };
    let batch = shop.db.read(move |c| weighed(c, &request, DAY)).unwrap();
    assert_eq!(batch.groups[0].template.id, WEIGHED_TEMPLATE);
    let label = first(&batch);
    assert_eq!(
        (label.weight, label.unit_price, label.price),
        (Some(Qty(1_250)), Some(Money(12_000)), Money(15_000))
    );
    assert_eq!(
        (label.packed_on.as_deref(), label.expiry.as_deref()),
        (Some(DAY), Some("2026-10-01"))
    );
    let barcode = label.barcode.clone().unwrap();
    assert_eq!(barcode.symbology, LabelSymbology::Ean13);
    let parsed = crate::catalog::scale_codes::parse(
        &barcode.data,
        crate::catalog::scale_codes::pack_layouts(),
    )
    .unwrap();
    assert_eq!(
        (parsed.plu, parsed.raw),
        (10012, 1_250),
        "the till reads the label back"
    );
    let milk = milk(&shop);
    let pieces = WeighedLabelRequest {
        product_id: milk.id,
        net: Qty(1_000),
        copies: 1,
        template_id: None,
        lang: None,
    };
    assert_eq!(
        shop.db
            .read(move |c| weighed(c, &pieces, DAY))
            .unwrap_err()
            .code(),
        "scale_product_not_weighed"
    );
}

#[test]
fn a_price_change_and_a_promotion_put_the_product_in_the_queue() {
    let shop = shop();
    let milk = milk(&shop);
    let request = one(&milk.id, 2);
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| record_printed(tx, &actor, &batch, false, NOON))
        .unwrap();
    assert!(shop.db.read(|c| stale(c, DAY, 100)).unwrap().is_empty());
    let id = milk.id.clone();
    let owner = shop.owner.actor();
    shop.db
        .write(move |tx| {
            crate::catalog::products::set_price(
                tx,
                &owner,
                &id,
                crate::catalog::RETAIL_LEVEL_ID,
                Money(16_000),
            )
        })
        .unwrap();
    let queue = shop.db.read(|c| stale(c, DAY, 100)).unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(
        (
            queue[0].printed_price,
            queue[0].price,
            queue[0].reasons.clone()
        ),
        (Money(15_000), Money(16_000), vec![StaleReason::Price])
    );
    let actor = shop.owner.actor();
    let ids = vec![milk.id.clone()];
    assert_eq!(
        shop.db
            .write(move |tx| dismiss(tx, &actor, &ids, DAY, NOON))
            .unwrap(),
        1
    );
    assert!(shop.db.read(|c| stale(c, DAY, 100)).unwrap().is_empty());
    let owner = shop.owner.actor();
    let scope = PromoScope::Products {
        ids: vec![milk.id.clone()],
    };
    shop.db
        .write(move |tx| {
            promotions::create(
                tx,
                &owner,
                PromotionInput {
                    name_ar: String::new(),
                    name_fr: "Lait -10 %".into(),
                    name_en: String::new(),
                    kind: PromoKind::Percent,
                    campaign: Campaign::Promotion,
                    trigger_mode: TriggerMode::Auto,
                    scope,
                    value: 1_000,
                    buy_qty: None,
                    get_qty: None,
                    min_qty: Qty::ZERO,
                    priority: 0,
                    starts_on: None,
                    ends_on: None,
                    active: true,
                },
            )
        })
        .unwrap();
    let queue = shop.db.read(|c| stale(c, DAY, 100)).unwrap();
    assert_eq!(
        queue[0].reasons,
        vec![StaleReason::Price, StaleReason::Promotion]
    );
    let journal = shop.count(
        "SELECT COUNT(*) FROM audit_log WHERE action IN ('labels.printed', 'labels.dismissed')",
    );
    assert_eq!(journal, 2);
}

#[test]
fn unbarcoded_goods_get_an_internal_ean13() {
    let shop = shop();
    let mut input = ProductInput::simple(
        "Olives vertes vrac",
        UNIT_PIECE_ID,
        &shop.tva9,
        Money(80_000),
    );
    input.name_ar = "زيتون أخضر سائب".into();
    let loose = shop.product_with(input);
    let request = one(&loose.id, 1);
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(batch.warnings[0].code, LabelWarningCode::NoBarcode);
    let actor = shop.owner.actor();
    let ids = vec![loose.id.clone()];
    let assigned = shop
        .db
        .write(move |tx| assign_missing_barcodes(tx, &actor, &ids))
        .unwrap();
    assert_eq!(assigned.len(), 1);
    assert!(assigned[0].barcode.starts_with("200") && assigned[0].barcode.len() == 13);
    let request = one(&loose.id, 1);
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(
        first(&batch).barcode.as_ref().map(|b| b.symbology),
        Some(LabelSymbology::Ean13)
    );
    assert!(batch.warnings.is_empty());
}

#[test]
fn the_designer_previews_an_unsaved_template_on_the_first_barcoded_product() {
    let shop = shop();
    shop.product("Sucre 1 kg", Money(9_500), &shop.tva9);
    let milk = milk(&shop);
    let mut draft = builtin_templates()[0].clone();
    draft.id = "draft".into();
    draft.lang = LabelLang::En;
    let preview = {
        let draft = draft.clone();
        shop.db
            .read(move |c| preview_label(c, &draft, None, None, DAY))
            .unwrap()
    };
    assert_eq!(
        (preview.product_id, preview.lang),
        (Some(milk.id), LabelLang::En)
    );
    draft.width_dmm = 50;
    let refused = shop
        .db
        .read(move |c| preview_label(c, &draft, None, None, DAY))
        .unwrap_err()
        .payload();
    assert_eq!(refused.field.as_deref(), Some("template.widthDmm"));
}

#[test]
fn check_digits_and_symbologies_follow_the_data() {
    assert_eq!(gtin_check_digit("613012345678"), Some(2));
    assert_eq!(gtin_check_digit("9638507"), Some(4), "EAN-8 96385074");
    assert_eq!(
        gtin_check_digit("03600029145"),
        Some(2),
        "UPC-A 036000291452"
    );
    let kind = |d: &str| auto_barcode(d).map(|b| b.symbology);
    assert_eq!(kind("6130123456782"), Some(LabelSymbology::Ean13));
    assert_eq!(kind("96385074"), Some(LabelSymbology::Ean8));
    assert_eq!(kind("036000291452"), Some(LabelSymbology::UpcA));
    assert_eq!(
        kind("6130123456780"),
        Some(LabelSymbology::Code128),
        "a wrong check digit is only text"
    );
    assert_eq!(
        kind("REF-12\"A"),
        None,
        "a quote would break the printer command"
    );
    assert_eq!(kind("P000123"), Some(LabelSymbology::Code128));
    assert_eq!(
        kind("REF\\12"),
        None,
        "pos-hw refuses a backslash inside a quoted field"
    );
}

// Beyond the brief: the weighed amount is the sale line's, the weight barcode carries the
// printed weight, the queue compares what the label shows, and a layout that lost a lot or an
// expiry date warns.

#[test]
fn a_weighed_label_shows_what_the_till_charges_in_an_ht_shop() {
    let shop = shop_with(TaxRegime::Reel, PriceMode::Ht);
    let mut input = ProductInput::simple("Viande hachée", UNIT_KG_ID, &shop.tva9, Money(45_050));
    input.is_weighed = true;
    input.plu = Some(20001);
    let meat = shop.product_with(input);
    let request = WeighedLabelRequest {
        product_id: meat.id,
        net: Qty(100),
        copies: 1,
        template_id: None,
        lang: None,
    };
    let batch = shop.db.read(move |c| weighed(c, &request, DAY)).unwrap();
    let label = first(&batch);
    assert_eq!(
        label.unit_price,
        Some(Money(49_105)),
        "450,50 DA HT + 9 % per kg"
    );
    assert_eq!(
        label.price,
        Money(4_910),
        "the till's line (45,05 HT + 4,05 TVA), not 491,05 × 0,1 kg = 49,11"
    );
}

#[test]
fn a_weight_barcode_carries_the_printed_weight_exactly() {
    let shop = shop();
    shop.set(
        "scale.barcode_formats",
        json!([{
            "prefix": "21", "pluDigits": 5, "verifier": false,
            "value": "weight", "valueDigits": 5, "decimals": 2
        }]),
    );
    let mut input = ProductInput::simple("Tomates", UNIT_KG_ID, &shop.tva9, Money(12_000));
    input.name_ar = "طماطم".into();
    input.is_weighed = true;
    input.plu = Some(10012);
    let tomato = shop.product_with(input);
    let request = |net: i64| WeighedLabelRequest {
        product_id: tomato.id.clone(),
        net: Qty(net),
        copies: 1,
        template_id: None,
        lang: None,
    };
    let exact = request(1_250);
    let batch = shop.db.read(move |c| weighed(c, &exact, DAY)).unwrap();
    assert!(first(&batch).barcode.is_some() && batch.warnings.is_empty());
    let grams = request(1_253);
    let batch = shop.db.read(move |c| weighed(c, &grams, DAY)).unwrap();
    assert_eq!(
        first(&batch).barcode,
        None,
        "a 10 g barcode would charge 1,25 kg for 1,253 kg"
    );
    assert_eq!(batch.warnings[0].code, LabelWarningCode::NoBarcode);
}

#[test]
fn the_queue_compares_only_what_the_label_shows() {
    let shop = shop();
    let milk = milk(&shop);
    let mut input = ProductInput::simple(
        "Miel de jujubier",
        UNIT_PIECE_ID,
        &shop.tva9,
        Money(250_000),
    );
    input.reference = Some("MIEL-01".into());
    let honey = shop.product_with(input);
    let owner = shop.owner.actor();
    let scope = PromoScope::Products {
        ids: vec![milk.id.clone()],
    };
    shop.db
        .write(move |tx| {
            promotions::create(
                tx,
                &owner,
                PromotionInput {
                    name_ar: String::new(),
                    name_fr: "Lait -10 %".into(),
                    name_en: String::new(),
                    kind: PromoKind::Percent,
                    campaign: Campaign::Promotion,
                    trigger_mode: TriggerMode::Auto,
                    scope,
                    value: 1_000,
                    buy_qty: None,
                    get_qty: None,
                    min_qty: Qty::ZERO,
                    priority: 0,
                    starts_on: None,
                    ends_on: None,
                    active: true,
                },
            )
        })
        .unwrap();
    let request = LabelRequest {
        source: LabelSource::Products {
            items: vec![
                LabelItem {
                    product_id: milk.id.clone(),
                    copies: 1,
                    lot_id: None,
                },
                LabelItem {
                    product_id: honey.id.clone(),
                    copies: 1,
                    lot_id: None,
                },
            ],
        },
        template_id: Some("builtin.product_50x30".into()),
        lang: None,
    };
    let batch = shop.db.read(move |c| build(c, &request, DAY)).unwrap();
    assert_eq!(
        batch.groups[0].labels[1].barcode,
        Some(LabelBarcode {
            symbology: LabelSymbology::Code128,
            data: "MIEL-01".into()
        })
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| record_printed(tx, &actor, &batch, false, NOON))
        .unwrap();
    assert!(
        shop.db.read(|c| stale(c, DAY, 100)).unwrap().is_empty(),
        "a 50x30 label shows no old price; a reference printed as Code 128 is still the product's"
    );
}

#[test]
fn a_label_whose_layout_lost_its_lot_or_expiry_date_warns() {
    let label = LabelData {
        product_id: Some("p-milk".into()),
        copies: 1,
        lang: LabelLang::ArFr,
        name: "حليب".into(),
        name_second: None,
        reference: None,
        price: Money(15_000),
        old_price: None,
        unit_code: "pc".into(),
        unit_price: None,
        weight: None,
        lot_number: Some("L2609".into()),
        expiry: Some("2026-12-31".into()),
        packed_on: None,
        shop_name: None,
        barcode: None,
        demo: false,
        jewel: None,
    };
    assert_eq!(dropped_details(&label, true, true), None);
    let warning = dropped_details(&label, true, false).unwrap();
    assert_eq!(
        (warning.product_id.as_str(), warning.code),
        ("p-milk", LabelWarningCode::LotOrExpiryDropped)
    );
    let bare = LabelData {
        lot_number: None,
        expiry: None,
        ..label
    };
    assert_eq!(
        dropped_details(&bare, false, false),
        None,
        "nothing to lose"
    );
}

#[test]
fn ttc_is_what_the_till_charges_for_one_unit() {
    use crate::money::Rate;
    assert_eq!(
        ttc(PriceMode::Ht, Money(50_000), Rate(1_900)),
        Some(Money(59_500))
    );
    assert_eq!(
        ttc(PriceMode::Ttc, Money(59_500), Rate(1_900)),
        Some(Money(59_500))
    );
    assert_eq!(
        ttc(PriceMode::Ht, Money(-1), Rate(1_900)),
        None,
        "the tax module refuses it"
    );
}
