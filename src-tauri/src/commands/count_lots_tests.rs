//! STK-004 at the commands: a lot-tracked product (a pharmacy's medicines) is
//! counted lot by lot on the count sheet: the lots in stock are listed to look for, a lot found
//! on the shelf is added by its number, and once a lot is counted the product takes no lump sum.

use super::stock_m2;
use super::testshop::shop;
use pos_core::catalog::extras::{self, ProductExtrasInput};
use pos_core::money::{Money, Qty};
use pos_core::stock::counts::{CountInput, CountMode, CountScope};
use pos_core::stock::lots::LotInput;
use pos_core::stock::{ReceiveLine, SHOP_LOCATION_ID, receive};

#[test]
fn a_lot_tracked_product_is_counted_lot_by_lot() {
    let t = shop();
    let med = t.product("Doliprane 500", 25_000, 900, None).id;
    let (actor, id) = (t.owner.actor(), med.clone());
    t.st.db
        .write(move |tx| {
            extras::set(
                tx,
                &actor,
                &id,
                ProductExtrasInput {
                    track_lots: true,
                    ..ProductExtrasInput::default()
                },
            )
            .map(drop)
        })
        .unwrap();
    for (lot, expiry, qty) in [("L-A", "2027-01-31", 10_000), ("L-B", "2027-06-30", 5_000)] {
        let line = ReceiveLine {
            product_id: med.clone(),
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(qty),
            unit_cost: Money(18_000),
            lot: Some(LotInput {
                lot_number: lot.into(),
                expiry_date: Some(expiry.into()),
            }),
            serials: Vec::new(),
            supplier_id: None,
            ref_type: "document".into(),
            ref_id: "rc".into(),
            note: None,
        };
        let actor = t.owner.actor();
        t.st.db
            .write(move |tx| receive(tx, &actor, &line, 1).map(drop))
            .unwrap();
    }
    let count = stock_m2::start_count(
        &t.st,
        CountInput {
            location_id: None,
            scope: CountScope::Products,
            category_id: None,
            product_ids: vec![med.clone()],
            blind: false,
            note: None,
        },
    )
    .unwrap();

    // the lots in stock, first expiry first, are what the counter looks for
    let lots = stock_m2::count_lot_lines(&t.st, count.id.clone()).unwrap();
    let numbers: Vec<&str> = lots.iter().map(|l| l.lot_number.as_str()).collect();
    assert_eq!(numbers, ["L-A", "L-B"]);
    assert_eq!(lots[0].expected_qty, Some(Qty(10_000)));

    // a lot counted by its number; a lot found on the shelf is added by its number
    let line = stock_m2::set_counted_lot(
        &t.st,
        count.id.clone(),
        med.clone(),
        "L-A".into(),
        Qty(9_000),
        CountMode::Set,
    )
    .unwrap();
    assert_eq!(line.counted_qty, Some(Qty(9_000)));
    stock_m2::set_counted_lot(
        &t.st,
        count.id.clone(),
        med.clone(),
        "L-C".into(),
        Qty(1_000),
        CountMode::Set,
    )
    .unwrap();
    let lots = stock_m2::count_lot_lines(&t.st, count.id.clone()).unwrap();
    let counted: Vec<(&str, Option<Qty>)> = lots
        .iter()
        .map(|l| (l.lot_number.as_str(), l.counted_qty))
        .collect();
    assert!(counted.contains(&("L-A", Some(Qty(9_000)))), "{counted:?}");
    assert!(counted.contains(&("L-C", Some(Qty(1_000)))), "{counted:?}");
    assert!(counted.contains(&("L-B", None)), "{counted:?}");

    // counted by lot: no lump sum any more
    let refused =
        stock_m2::set_counted(&t.st, count.id, med, Qty(15_000), CountMode::Set).unwrap_err();
    assert_eq!(refused.code(), "count_lot_required");
}
