//! Décret 13-378: a lot or an expiry date the layout had to leave out is reported, never printed
//! away silently (`pos_core::labels::LabelWarningCode::LotOrExpiryDropped`).

use super::dropped;
use super::model::LabelGroup;
use super::profile::LabelDpi;
use crate::raster::Typesetter;
use crate::registry::Registry;
use crate::testdata::{milk_label, oil_shelf_label, tomato_weighed_label};
use pos_core::labels::{LabelLang, LabelWarningCode};

const LONG_LOT: &str = "L2609-ALGER-BAB-EZZOUAR-000123-A";

#[test]
fn a_lot_the_layout_left_out_is_a_warning() {
    let (mut tpl, mut label) = tomato_weighed_label();
    tpl.fields.lot = true;
    label.lot_number = Some(LONG_LOT.into());
    let mut ts = Typesetter::new();
    assert_eq!(
        dropped(&tpl, &label, 8, &mut ts).unwrap(),
        None,
        "the 58 mm label holds the lot on its two lines"
    );
    tpl.width_dmm = 400;
    let warning = dropped(&tpl, &label, 8, &mut ts).unwrap().unwrap();
    assert_eq!(
        (warning.product_id.as_str(), warning.code),
        ("p-1", LabelWarningCode::LotOrExpiryDropped),
        "at 40 mm the lot cannot be printed whole"
    );
}

#[test]
fn a_label_without_a_lot_or_whose_template_hides_it_is_not_checked() {
    let (tpl, label) = milk_label(LabelLang::Fr);
    let mut ts = Typesetter::new();
    assert_eq!(dropped(&tpl, &label, 8, &mut ts).unwrap(), None);
    let (mut tpl, mut label) = oil_shelf_label();
    label.lot_number = Some("L2609".into());
    label.expiry = Some("2027-03-31".into());
    (tpl.fields.lot, tpl.fields.expiry) = (false, false);
    assert_eq!(
        dropped(&tpl, &label, 8, &mut ts).unwrap(),
        None,
        "the template does not print them"
    );
}

#[test]
fn the_registry_checks_every_label_of_a_batch_at_the_printer_resolution() {
    let (mut tpl, mut tomato) = tomato_weighed_label();
    (tpl.width_dmm, tpl.fields.lot) = (400, true);
    tomato.lot_number = Some(LONG_LOT.into());
    let (_, mut other) = tomato_weighed_label();
    other.product_id = Some("p-2".into());
    let group = LabelGroup {
        template: tpl,
        labels: vec![tomato, other],
    };
    let registry = Registry::default();
    let warnings = registry
        .labels_dropped(std::slice::from_ref(&group), LabelDpi::D203)
        .unwrap();
    let ids: Vec<&str> = warnings.iter().map(|w| w.product_id.as_str()).collect();
    assert_eq!(ids, ["p-1"]);
    assert!(
        registry
            .labels_dropped(&[], LabelDpi::D300)
            .unwrap()
            .is_empty()
    );
}
