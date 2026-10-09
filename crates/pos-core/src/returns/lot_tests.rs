//! Customer returns of lot-tracked goods (STK-004, PRD-053): the goods go back to the lots the
//! ticket took them from, so FEFO and the expiry reports stay right.

use super::*;
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::payments::PaymentInput;
use crate::sales::cart::{Cart, CartLine};
use crate::sales::create_sale;
use crate::stock::lots::{LotInput, expiry_list};
use crate::stock::{DAMAGED_LOCATION_ID, ReceiveLine, SHOP_LOCATION_ID, receive};
use crate::testkit::{NOON, Shop, shop};

/// A lot-tracked medicine at 250 DA with L-A (2 boxes, 2026-11-30) and L-B (5 boxes,
/// 2027-06-30) in the shop, and one box without a lot.
fn lotted(shop: &Shop) -> String {
    let id = shop.product("Doliprane 500", Money(25_000), &shop.tva9).id;
    let (actor, pid) = (shop.owner.actor(), id.clone());
    shop.db
        .write(move |tx| {
            extras::set(
                tx,
                &actor,
                &pid,
                ProductExtrasInput {
                    track_lots: true,
                    ..ProductExtrasInput::default()
                },
            )?;
            for (lot, expiry, qty) in [("L-A", "2026-11-30", 2_000), ("L-B", "2027-06-30", 5_000)] {
                let line = ReceiveLine {
                    product_id: pid.clone(),
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
                receive(tx, &actor, &line, NOON)?;
            }
            Ok(())
        })
        .unwrap();
    shop.stock_in(&id, 1_000);
    id
}

fn sell(shop: &Shop, product: &str, qty: i64) -> Document {
    let ctx = shop.till();
    let total = Money(25 * qty);
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(qty))]),
        payments: vec![PaymentInput::electronic(
            PaymentMethod::CardCib,
            total,
            Some("A1"),
        )],
    };
    shop.db
        .write(move |tx| create_sale(tx, &ctx, input))
        .unwrap()
        .document
}

fn give_back(shop: &Shop, ticket: &Document, qty: i64, restock: bool) {
    let input = ReturnInput {
        original_id: Some(ticket.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(ticket.lines[0].id.clone()),
            product_id: None,
            qty: Qty(qty),
            unit_price: None,
            restock,
            reason: ReturnReason::ChangedMind,
            note: None,
        }],
        refund_method: PaymentMethod::CardCib,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    let ctx = shop.till();
    shop.db
        .write(move |tx| create_return(tx, &ctx, input))
        .unwrap();
}

/// (lot, location, qty) of every lot level that holds stock.
fn lot_levels(shop: &Shop) -> Vec<(String, String, i64)> {
    shop.db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT l.lot_number, ll.location_id, ll.qty FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
                 WHERE ll.qty <> 0 ORDER BY l.lot_number, ll.location_id",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

fn level(lot: &str, location: &str, qty: i64) -> (String, String, i64) {
    (lot.to_owned(), location.to_owned(), qty)
}

#[test]
fn a_returned_box_goes_back_to_the_lot_the_ticket_took_it_from() {
    let shop = shop();
    let product = lotted(&shop);
    shop.open_till(0);
    // FEFO: the 3 boxes sold are the 2 of L-A and 1 of L-B.
    let ticket = sell(&shop, &product, 3_000);
    assert_eq!(lot_levels(&shop), [level("L-B", SHOP_LOCATION_ID, 4_000)]);
    give_back(&shop, &ticket, 2_000, true);
    assert_eq!(
        lot_levels(&shop),
        [
            level("L-A", SHOP_LOCATION_ID, 2_000),
            level("L-B", SHOP_LOCATION_ID, 4_000)
        ],
        "back to the lot the ticket took, the soonest expiry first"
    );
    let soon = shop.db.read(|c| expiry_list(c, "2026-09-28", 90)).unwrap();
    assert_eq!(
        soon.iter()
            .map(|r| (r.lot_number.as_str(), r.qty))
            .collect::<Vec<_>>(),
        [("L-A", Qty(2_000))],
        "the expiry report sees the boxes returned"
    );
    // L-A got back all the ticket took from it: the last box returned is the L-B one, and it
    // keeps its lot in the damaged-goods location too.
    give_back(&shop, &ticket, 1_000, false);
    assert_eq!(
        lot_levels(&shop),
        [
            level("L-A", SHOP_LOCATION_ID, 2_000),
            level("L-B", SHOP_LOCATION_ID, 4_000),
            level("L-B", DAMAGED_LOCATION_ID, 1_000)
        ]
    );
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM stock_movements WHERE reason = 'return' AND lot_id IS NULL"
        ),
        0
    );
    assert_eq!(
        shop.count("SELECT SUM(qty) FROM stock_levels")
            - shop.count("SELECT SUM(qty) FROM lot_levels"),
        1_000,
        "the box without a lot is still the only stock without a lot"
    );
}

#[test]
fn what_the_ticket_took_without_a_lot_goes_back_without_a_lot() {
    let shop = shop();
    let product = lotted(&shop);
    shop.open_till(0);
    // The 7 lotted boxes, then the one without a lot.
    let ticket = sell(&shop, &product, 8_000);
    assert!(lot_levels(&shop).is_empty());
    give_back(&shop, &ticket, 8_000, true);
    assert_eq!(
        lot_levels(&shop),
        [
            level("L-A", SHOP_LOCATION_ID, 2_000),
            level("L-B", SHOP_LOCATION_ID, 5_000)
        ]
    );
    assert_eq!(
        shop.count(
            "SELECT COALESCE(SUM(qty_delta), 0) FROM stock_movements WHERE reason = 'return' AND lot_id IS NULL"
        ),
        1_000
    );
    assert_eq!(shop.count("SELECT SUM(qty) FROM stock_levels"), 8_000);
}
