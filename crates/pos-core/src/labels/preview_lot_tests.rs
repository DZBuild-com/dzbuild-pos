//! The designer's preview of a lot-tracked product carries a lot (décret 13-378), so the preview
//! can show where the lot and the expiry date go, and say when the label drops them.

use super::*;
use crate::catalog::UNIT_PIECE_ID;
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::catalog::products::{Product, ProductInput};
use crate::money::Money;
use crate::stock::lots::{self, LotInput};
use crate::testkit::{Shop, shop};

const DAY: &str = "2026-09-28";

fn yogurt(shop: &Shop, track_lots: bool) -> Product {
    let mut input = ProductInput::simple("Yaourt Soummam", UNIT_PIECE_ID, &shop.tva9, Money(4_000));
    input.name_ar = "ياغورت الصومام".into();
    let product = shop.product_with(input);
    if track_lots {
        let (id, actor) = (product.id.clone(), shop.owner.actor());
        let input = ProductExtrasInput {
            track_lots: true,
            ..ProductExtrasInput::default()
        };
        shop.db
            .write(move |tx| extras::set(tx, &actor, &id, input))
            .unwrap();
    }
    product
}

fn lot(shop: &Shop, product_id: &str, number: &str, expiry: Option<&str>, at: i64) {
    let (product_id, input) = (
        product_id.to_owned(),
        LotInput {
            lot_number: number.into(),
            expiry_date: expiry.map(Into::into),
        },
    );
    shop.db
        .write(move |tx| lots::get_or_create(tx, &product_id, &input, None, at))
        .unwrap();
}

/// The 40 × 25 price label, printing the lot and the expiry date too.
fn with_lot() -> LabelTemplate {
    let mut template = builtin_templates()[0].clone();
    template.id = "shop.lot_40x25".into();
    (template.fields.lot, template.fields.expiry) = (true, true);
    template
}

fn preview(shop: &Shop, template: &LabelTemplate, product_id: &str) -> LabelData {
    let (template, product_id) = (template.clone(), product_id.to_owned());
    shop.db
        .read(move |c| preview_label(c, &template, Some(&product_id), None, DAY))
        .unwrap()
}

#[test]
fn a_lot_tracked_product_is_previewed_with_its_newest_lot() {
    let shop = shop();
    let yogurt = yogurt(&shop, true);
    lot(&shop, &yogurt.id, "L0901", Some("2026-10-05"), 1_000);
    lot(&shop, &yogurt.id, "L0927", Some("2026-10-30"), 2_000);
    let label = preview(&shop, &with_lot(), &yogurt.id);
    assert_eq!(
        (label.lot_number.as_deref(), label.expiry.as_deref()),
        (Some("L0927"), Some("2026-10-30"))
    );
}

#[test]
fn a_lot_tracked_product_without_a_lot_yet_is_previewed_with_a_sample_lot() {
    let shop = shop();
    let yogurt = yogurt(&shop, true);
    let label = preview(&shop, &with_lot(), &yogurt.id);
    assert_eq!(
        (label.lot_number.as_deref(), label.expiry.as_deref()),
        (Some(SAMPLE_LOT_NUMBER), Some("2027-09-28")),
        "a year after the business day"
    );
}

#[test]
fn only_a_lot_tracked_product_and_a_template_printing_them_show_a_lot() {
    let shop = shop();
    let plain = yogurt(&shop, false);
    lot(&shop, &plain.id, "L0927", Some("2026-10-30"), 2_000);
    let label = preview(&shop, &with_lot(), &plain.id);
    assert_eq!((label.lot_number, label.expiry), (None, None));
    let tracked = yogurt(&shop, true);
    lot(&shop, &tracked.id, "L0927", Some("2026-10-30"), 2_000);
    let mut expiry_only = with_lot();
    expiry_only.fields.lot = false;
    let label = preview(&shop, &expiry_only, &tracked.id);
    assert_eq!(
        (label.lot_number.as_deref(), label.expiry.as_deref()),
        (None, Some("2026-10-30"))
    );
    let label = preview(&shop, &builtin_templates()[0], &tracked.id);
    assert_eq!((label.lot_number, label.expiry), (None, None));
}
