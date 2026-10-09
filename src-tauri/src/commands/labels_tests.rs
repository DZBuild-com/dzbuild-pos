//! Label commands (Review Focus 3): who may print, the DOC-034 guard and its journal, the label
//! printer's bytes, scale items, collector files.

use super::labels::{self, LabelPrintInput};
use super::testshop::{TestShop, shop, shop_in};
use pos_core::auth::STOREKEEPER_ROLE_ID;
use pos_core::catalog::products::{Barcode, Product, ProductInput};
use pos_core::catalog::scale_items::ScaleItemInput;
use pos_core::labels::{LabelItem, LabelLang, LabelRequest, LabelSource, LabelWarningCode};
use pos_core::money::{Money, Qty};
use pos_hw::config::LabelPrinterConfig;
use pos_hw::driver::Transport;
use pos_hw::labels::{LabelPreset, LabelProfile};
use std::collections::BTreeMap;

fn one(product_id: &str, copies: u16, lang: Option<LabelLang>) -> LabelRequest {
    LabelRequest {
        source: LabelSource::Products {
            items: vec![LabelItem {
                product_id: product_id.into(),
                copies,
                lot_id: None,
            }],
        },
        template_id: None,
        lang,
    }
}

fn with_label_printer(shop: &TestShop, dir: &std::path::Path) -> std::path::PathBuf {
    let out = dir.join("labels.prn");
    let mut config = shop.st.hw.config().unwrap();
    config.label_printers.push(LabelPrinterConfig {
        id: "l1".into(),
        name: "Étiquettes".into(),
        transport: Transport::File {
            path: out.to_string_lossy().into_owned(),
        },
        profile: LabelProfile::preset(LabelPreset::Xprinter),
    });
    config.label_printer_id = Some("l1".into());
    super::hardware::save(&shop.st, config).unwrap();
    out
}

/// A piece product with its Arabic name (a guarded label needs it, DOC-034) and `barcode`.
fn food(
    shop: &TestShop,
    name_fr: &str,
    name_ar: &str,
    price: i64,
    barcode: Option<&str>,
) -> Product {
    let mut input = ProductInput::simple(
        name_fr,
        pos_core::catalog::UNIT_PIECE_ID,
        &shop.rate_id(900),
        Money(price),
    );
    input.name_ar = name_ar.into();
    input.barcodes = barcode
        .map(|code| Barcode {
            barcode: code.into(),
            pack_qty: Qty(1_000),
            label: None,
        })
        .into_iter()
        .collect();
    shop.product_with(input)
}

fn journal(shop: &TestShop, action: &'static str) -> Vec<serde_json::Value> {
    shop.st
        .db
        .read(move |c| {
            let mut stmt =
                c.prepare("SELECT payload FROM audit_log WHERE action = ?1 ORDER BY seq")?;
            let rows = stmt
                .query_map([action], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows
                .into_iter()
                .map(|p| serde_json::from_str(&p).unwrap())
                .collect())
        })
        .unwrap()
}

#[test]
fn french_only_food_labels_need_the_override_and_it_is_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = shop_in(dir.path());
    shop.owner();
    let out = with_label_printer(&shop, dir.path());
    let milk = food(
        &shop,
        "Lait Candia 1 L",
        "حليب كانديا 1 ل",
        15_000,
        Some("6130123456782"),
    );
    let print = |arabic_override| LabelPrintInput {
        request: one(&milk.id, 2, Some(LabelLang::Fr)),
        printer_id: None,
        assign_missing_barcodes: false,
        arabic_override,
    };
    let refused = labels::print(&shop.st, print(false)).unwrap_err();
    assert_eq!(
        refused.code(),
        "label_arabic_required",
        "the first-run shop is a superette"
    );
    assert!(!out.exists(), "nothing printed before the decision");
    let done = labels::print(&shop.st, print(true)).unwrap();
    assert_eq!(
        (done.labels, done.outcome.printer.as_str()),
        (2, "Étiquettes")
    );
    assert!(done.warnings.is_empty(), "{:?}", done.warnings);
    assert!(
        std::fs::read(&out)
            .unwrap()
            .starts_with(b"SIZE 40 mm,25 mm\r\n")
    );
    let printed = journal(&shop, "labels.printed");
    assert_eq!(printed.len(), 1);
    assert_eq!(printed[0]["arabicWarningOverridden"], true);
    let arabic = LabelPrintInput {
        request: one(&milk.id, 1, None),
        printer_id: None,
        assign_missing_barcodes: false,
        arabic_override: false,
    };
    labels::print(&shop.st, arabic).unwrap();
    assert_eq!(
        journal(&shop, "labels.printed")[1]["arabicWarningOverridden"],
        false
    );
}

#[test]
fn a_food_product_without_its_arabic_name_needs_the_override_too() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = shop_in(dir.path());
    shop.owner();
    with_label_printer(&shop, dir.path());
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, Some("6130123456782"));
    let print = |arabic_override| LabelPrintInput {
        request: one(&milk.id, 1, None),
        printer_id: None,
        assign_missing_barcodes: false,
        arabic_override,
    };
    let refused = labels::print(&shop.st, print(false)).unwrap_err();
    assert_eq!(refused.code(), "label_arabic_name_missing");
    assert_eq!(
        refused.payload().details,
        Some(serde_json::json!({ "productIds": [milk.id] }))
    );
    labels::print(&shop.st, print(true)).unwrap();
    assert_eq!(
        journal(&shop, "labels.printed")[0]["arabicWarningOverridden"],
        true
    );
}

#[test]
fn a_cashier_prints_no_shelf_labels_but_can_print_a_weighed_label() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = shop_in(dir.path());
    shop.owner();
    with_label_printer(&shop, dir.path());
    let milk = food(
        &shop,
        "Lait Candia 1 L",
        "حليب كانديا 1 ل",
        15_000,
        Some("6130123456782"),
    );
    shop.cashier();
    let input = LabelPrintInput {
        request: one(&milk.id, 1, None),
        printer_id: None,
        assign_missing_barcodes: false,
        arabic_override: false,
    };
    assert_eq!(
        labels::print(&shop.st, input).unwrap_err().code(),
        "insufficient_permission"
    );
    let mut tomato = ProductInput::simple(
        "Tomates",
        pos_core::catalog::UNIT_KG_ID,
        &shop.rate_id(900),
        Money(12_000),
    );
    tomato.name_ar = "طماطم".into();
    tomato.is_weighed = true;
    tomato.plu = Some(10012);
    let tomato = shop.product_with(tomato);
    let weighed = labels::WeighedLabelInput {
        request: pos_core::labels::WeighedLabelRequest {
            product_id: tomato.id,
            net: Qty(1_250),
            copies: 1,
            template_id: None,
            lang: None,
        },
        printer_id: None,
        arabic_override: false,
    };
    assert_eq!(
        labels::print_weighed(&shop.st, weighed)
            .unwrap()
            .outcome
            .printer,
        "Étiquettes"
    );
}

#[test]
fn missing_barcodes_are_assigned_only_with_products_edit() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = shop_in(dir.path());
    shop.owner();
    with_label_printer(&shop, dir.path());
    let olives = food(&shop, "Olives vrac", "زيتون سائب", 80_000, None);
    let clerk = shop.user("Rachid", STOREKEEPER_ROLE_ID, "1357", 0);
    shop.sign_in(&clerk, "1357");
    let input = |assign| LabelPrintInput {
        request: one(&olives.id, 1, None),
        printer_id: None,
        assign_missing_barcodes: assign,
        arabic_override: false,
    };
    let storekeeper_can_label = shop
        .st
        .require(pos_core::auth::Permission::ProductsLabels)
        .is_ok();
    if storekeeper_can_label
        && shop
            .st
            .require(pos_core::auth::Permission::ProductsEdit)
            .is_err()
    {
        assert_eq!(
            labels::print(&shop.st, input(true)).unwrap_err().code(),
            "insufficient_permission"
        );
    }
    shop.owner();
    let done = labels::print(&shop.st, input(true)).unwrap();
    assert_eq!(done.assigned.len(), 1);
    assert!(done.assigned[0].barcode.len() == 13);
    assert_eq!(journal(&shop, "labels.barcodes_assigned").len(), 1);
}

#[test]
fn the_preview_is_a_png_data_url_and_the_queue_follows_prices() {
    let shop = shop();
    shop.owner();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, Some("6130123456782"));
    let template = labels::templates(&shop.st).unwrap().templates[0].clone();
    let preview = labels::preview(
        &shop.st,
        labels::LabelPreviewInput {
            template,
            product_id: Some(milk.id.clone()),
            lang: None,
        },
    )
    .unwrap();
    assert!(preview.png.starts_with("data:image/png;base64,"));
    assert_eq!(preview.label.product_id.as_deref(), Some(milk.id.as_str()));
    assert!(labels::stale(&shop.st, None).unwrap().is_empty());
}

/// A size within the template bounds can still be too small for what the label carries: the
/// designer shows `pos-hw`'s refusal on the template's own field.
#[test]
fn a_label_too_small_for_its_barcode_is_refused_on_the_template_field() {
    let shop = shop();
    shop.owner();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, Some("6130123456782"));
    let mut template = labels::templates(&shop.st).unwrap().templates[0].clone();
    template.height_dmm = 100;
    let refused = labels::preview(
        &shop.st,
        labels::LabelPreviewInput {
            template,
            product_id: Some(milk.id),
            lang: None,
        },
    )
    .unwrap_err()
    .payload();
    assert_eq!(
        (refused.code.as_str(), refused.field.as_deref()),
        ("printer_setting_invalid", Some("template.heightDmm"))
    );
}

/// Décret 13-378: a lot too long for a small label is left out by the layout; the print result
/// says so.
#[test]
fn a_lot_the_label_cannot_hold_is_reported_with_the_printed_labels() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = shop_in(dir.path());
    shop.owner();
    with_label_printer(&shop, dir.path());
    let milk = food(
        &shop,
        "Lait Candia 1 L",
        "حليب كانديا 1 ل",
        15_000,
        Some("6130123456782"),
    );
    let (product_id, now) = (milk.id.clone(), shop.st.now());
    let lot = shop
        .st
        .db
        .write(move |tx| {
            pos_core::stock::lots::get_or_create(
                tx,
                &product_id,
                &pos_core::stock::lots::LotInput {
                    lot_number: "L2609-ALGER-BAB-EZZOUAR-000123-A".into(),
                    expiry_date: Some("2027-03-31".into()),
                },
                None,
                now,
            )
        })
        .unwrap();
    let mut templates = labels::templates(&shop.st).unwrap().templates;
    let mut with_lot = templates[0].clone();
    with_lot.id = "shop.lot_40x25".into();
    (with_lot.fields.lot, with_lot.fields.expiry) = (true, true);
    templates.push(with_lot);
    labels::save_templates(&shop.st, templates, BTreeMap::new()).unwrap();
    let done = labels::print(
        &shop.st,
        LabelPrintInput {
            request: LabelRequest {
                source: LabelSource::Products {
                    items: vec![LabelItem {
                        product_id: milk.id.clone(),
                        copies: 1,
                        lot_id: Some(lot.id),
                    }],
                },
                template_id: Some("shop.lot_40x25".into()),
                lang: None,
            },
            printer_id: None,
            assign_missing_barcodes: false,
            arabic_override: false,
        },
    )
    .unwrap();
    assert_eq!(done.labels, 1, "printed all the same");
    let dropped: Vec<(&str, LabelWarningCode)> = done
        .warnings
        .iter()
        .map(|w| (w.product_id.as_str(), w.code))
        .collect();
    assert_eq!(
        dropped,
        [(milk.id.as_str(), LabelWarningCode::LotOrExpiryDropped)]
    );
}

#[test]
fn scale_items_and_the_plu_file() {
    let shop = shop();
    shop.owner();
    let mut tomato = ProductInput::simple(
        "Tomates",
        pos_core::catalog::UNIT_KG_ID,
        &shop.rate_id(900),
        Money(12_000),
    );
    tomato.is_weighed = true;
    tomato.plu = Some(12);
    let tomato = shop.product_with(tomato);
    let item = labels::set_scale_item(
        &shop.st,
        tomato.id.clone(),
        ScaleItemInput {
            hotkey: Some(3),
            tare_g: 120,
            shelf_life_days: Some(3),
        },
    )
    .unwrap();
    assert_eq!(item.hotkey, Some(3));
    assert_eq!(labels::scale_items(&shop.st).unwrap().len(), 1);
    let csv = labels::plu_bytes(
        &shop.st,
        pos_docs::sheet::SheetFormat::Csv,
        pos_core::lang::Lang::Fr,
    )
    .unwrap();
    let text = String::from_utf8(csv).unwrap();
    assert!(
        text.contains("PLU") && text.contains("Tomates") && text.contains("120,00"),
        "{text}"
    );
    shop.cashier();
    assert_eq!(
        labels::set_scale_item(&shop.st, tomato.id, ScaleItemInput::default())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn a_collector_file_fills_the_open_count_and_reports_bad_lines() {
    let shop = shop();
    shop.owner();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, Some("6130123456782"));
    shop.stock_in(&milk.id, 10);
    let count = super::stock_m2::start_count_for_tests(&shop.st);
    let imported = labels::import_collector_bytes(
        &shop.st,
        count.clone(),
        b"6130123456782;4\r\n999;1\r\n61 30;2\r\n",
    )
    .unwrap();
    assert_eq!(
        (imported.matched, imported.unknown.clone()),
        (1, vec!["999".to_owned()])
    );
    assert_eq!(imported.problems.len(), 1);
    assert_eq!(
        labels::import_collector_bytes(&shop.st, count, b"")
            .unwrap_err()
            .code(),
        "collector_file_invalid"
    );
}
