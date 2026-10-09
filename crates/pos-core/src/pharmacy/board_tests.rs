//! The expiry board's cost column (Task 25): every lot carries its value at the product's CMUP,
//! and each table's total is the sum of its lots.

use super::expiry_board;
use super::tests::{medicine, pharmacy};
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::money::{Money, Qty};
use crate::stock::lots::LotInput;
use crate::stock::{ReceiveLine, SHOP_LOCATION_ID, receive};
use crate::testkit::NOON;

#[test]
fn every_lot_of_the_expiry_board_carries_its_value_at_cost() {
    let shop = pharmacy();
    let amox = medicine(&shop, "Amoxicilline 500 mg", 45_000, "Amoxicilline", false);
    let (actor, pid) = (shop.owner.actor(), amox.clone());
    shop.db
        .write(move |tx| {
            extras::set_quiet(
                tx,
                &pid,
                ProductExtrasInput {
                    track_lots: true,
                    ..Default::default()
                },
            )?;
            for (lot, expiry, qty) in [
                ("A1", "2026-08-31", 2_000),
                ("A2", "2026-09-10", 1_000),
                ("A3", "2026-10-15", 3_000),
                ("A4", "2027-03-01", 5_000),
            ] {
                receive(
                    tx,
                    &actor,
                    &ReceiveLine {
                        product_id: pid.clone(),
                        location_id: SHOP_LOCATION_ID.into(),
                        qty: Qty(qty),
                        unit_cost: Money(25_000),
                        lot: Some(LotInput {
                            lot_number: lot.into(),
                            expiry_date: Some(expiry.into()),
                        }),
                        serials: Vec::new(),
                        supplier_id: None,
                        ref_type: "test".into(),
                        ref_id: "t".into(),
                        note: None,
                    },
                    NOON,
                )?;
            }
            Ok(())
        })
        .unwrap();
    let board = shop.db.read(|c| expiry_board(c, "2026-09-28", 30)).unwrap();
    let lots = |rows: &[super::BoardLot]| -> Vec<(String, Money)> {
        rows.iter()
            .map(|r| (r.lot.lot_number.clone(), r.cost))
            .collect()
    };
    assert_eq!(
        lots(&board.expired),
        vec![("A1".into(), Money(50_000)), ("A2".into(), Money(25_000))],
        "2 and 1 boxes at 250 DA"
    );
    assert_eq!(lots(&board.soon), vec![("A3".into(), Money(75_000))]);
    assert_eq!(
        (board.expired_cost, board.soon_cost),
        (Money(75_000), Money(75_000)),
        "each total is the sum of its lots"
    );
}
