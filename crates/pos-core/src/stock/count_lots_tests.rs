//! STK-004 (migration 0009): a lot-tracked product counted lot by lot. The
//! product's line holds the sum of its lot lines; the close posts each lot's difference in that
//! lot, takes the lots nobody found to zero, and the stock the books held without a lot.

use super::counts::{self, CountInput, CountLineView, CountMode, CountScope, Uncounted};
use super::lots::LotInput;
use super::{MovementReason, NewMovement, ReceiveLine, SHOP_LOCATION_ID, level, lots, receive};
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::table::Cell;
use crate::testkit::{NOON, Shop, shop};

fn lotted(shop: &Shop, name: &str) -> String {
    let id = shop.product(name, Money(25_000), &shop.tva9).id;
    let (actor, p) = (shop.owner.actor(), id.clone());
    shop.db
        .write(move |tx| {
            extras::set(
                tx,
                &actor,
                &p,
                ProductExtrasInput {
                    track_lots: true,
                    ..ProductExtrasInput::default()
                },
            )
        })
        .unwrap();
    id
}

fn receive_lot(shop: &Shop, product: &str, lot: &str, expiry: &str, qty: i64) {
    let line = ReceiveLine {
        product_id: product.into(),
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
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| receive(tx, &actor, &line, 1))
        .unwrap();
}

fn lot_qty(shop: &Shop, product: &str, number: &str) -> i64 {
    let (p, n) = (product.to_owned(), number.to_owned());
    shop.db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COALESCE((SELECT ll.qty FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
                                  WHERE l.product_id = ?1 AND l.lot_number = ?2 AND ll.location_id = ?3), 0)",
                [p, n, SHOP_LOCATION_ID.to_owned()],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

fn shelf(shop: &Shop, product: &str) -> i64 {
    let p = product.to_owned();
    shop.db
        .read(move |c| level(c, &p, SHOP_LOCATION_ID))
        .unwrap()
        .0
}

fn start(shop: &Shop, products: &[&str], blind: bool) -> String {
    let ctx = shop.owner_till();
    let input = CountInput {
        location_id: None,
        scope: CountScope::Products,
        category_id: None,
        product_ids: products.iter().map(|p| (*p).to_owned()).collect(),
        blind,
        note: None,
    };
    shop.db
        .write(move |tx| counts::start(tx, &ctx, input))
        .unwrap()
        .id
}

fn count_lot(
    shop: &Shop,
    id: &str,
    product: &str,
    lot: &'static str,
    qty: i64,
    mode: CountMode,
) -> Result<CountLineView, AppError> {
    let (ctx, id, p) = (shop.owner_till(), id.to_owned(), product.to_owned());
    shop.db
        .write(move |tx| counts::set_counted_lot(tx, &ctx, &id, &p, lot, Qty(qty), mode))
}

fn close(shop: &Shop, id: &str) -> counts::CountResult {
    let (ctx, id) = (shop.owner_till(), id.to_owned());
    shop.db
        .write(move |tx| counts::close(tx, &ctx, &id, Uncounted::Ignore))
        .unwrap()
}

/// (lot number, expected, counted, variance) of the count's lot lines.
type LotRow = (String, Option<i64>, Option<i64>, Option<i64>);

fn lot_rows(shop: &Shop, id: &str, reveal: bool) -> Vec<LotRow> {
    let id = id.to_owned();
    shop.db
        .read(move |c| counts::lot_lines(c, &id, reveal))
        .unwrap()
        .into_iter()
        .map(|l| {
            (
                l.lot_number,
                l.expected_qty.map(|q| q.0),
                l.counted_qty.map(|q| q.0),
                l.variance.map(|q| q.0),
            )
        })
        .collect()
}

fn row(n: &str, expected: Option<i64>, counted: Option<i64>, variance: Option<i64>) -> LotRow {
    (n.to_owned(), expected, counted, variance)
}

#[test]
fn each_lot_is_counted_and_the_product_line_holds_their_sum() {
    let shop = shop();
    let doliprane = lotted(&shop, "Doliprane 500");
    receive_lot(&shop, &doliprane, "L-A", "2027-01-31", 10_000);
    receive_lot(&shop, &doliprane, "L-B", "2027-06-30", 5_000);
    shop.stock_in(&doliprane, 2_000);
    let id = start(&shop, &[&doliprane], false);
    let line = count_lot(&shop, &id, &doliprane, "L-A", 8_000, CountMode::Set).unwrap();
    assert_eq!(line.counted_qty, Some(Qty(8_000)));
    count_lot(&shop, &id, &doliprane, "L-B", 4_000, CountMode::Set).unwrap();
    let line = count_lot(&shop, &id, &doliprane, "L-B", 2_000, CountMode::Add).unwrap();
    assert_eq!(
        line.counted_qty,
        Some(Qty(14_000)),
        "the product line is the sum of its lots"
    );
    assert_eq!(
        line.variance,
        Some(Qty(-3_000)),
        "17 in the books, 14 on the shelf"
    );
    assert_eq!(
        lot_rows(&shop, &id, true),
        [
            row("L-A", Some(10_000), Some(8_000), Some(-2_000)),
            row("L-B", Some(5_000), Some(6_000), Some(1_000)),
        ]
    );
    let (ctx, i, p) = (shop.owner_till(), id.clone(), doliprane.clone());
    let err = shop
        .db
        .write(move |tx| counts::set_counted(tx, &ctx, &i, &p, Qty(3_000), CountMode::Set))
        .unwrap_err();
    assert_eq!(
        err,
        AppError::validation("lotNumber", "count_lot_required"),
        "a product counted by lot is not counted as a whole on top"
    );
    let tea = shop.product("Thé vert", Money(50_000), &shop.tva19).id;
    assert_eq!(
        count_lot(&shop, &id, &tea, "X1", 1_000, CountMode::Set).unwrap_err(),
        AppError::validation("lotNumber", "product_not_lot_tracked")
    );
    assert_eq!(
        count_lot(&shop, &id, &doliprane, "L-A", -1, CountMode::Set).unwrap_err(),
        AppError::validation("qty", "qty_invalid")
    );
    let result = close(&shop, &id);
    assert_eq!(result.adjusted, 1);
    assert_eq!(result.variance_value, Money(-54_000), "3 boxes at 180 DA");
    assert_eq!(
        (
            lot_qty(&shop, &doliprane, "L-A"),
            lot_qty(&shop, &doliprane, "L-B")
        ),
        (8_000, 6_000)
    );
    assert_eq!(
        shelf(&shop, &doliprane),
        14_000,
        "the 2 pieces without a lot were not on the shelf"
    );
    assert_eq!(
        lot_rows(&shop, &id, true),
        [
            row("L-A", Some(10_000), Some(8_000), Some(-2_000)),
            row("L-B", Some(5_000), Some(6_000), Some(1_000)),
        ]
    );
    let i = id.clone();
    let lines = shop
        .db
        .read(move |c| counts::lines(c, &i, true, false))
        .unwrap();
    assert_eq!(
        (lines[0].counted_qty, lines[0].variance),
        (Some(Qty(14_000)), Some(Qty(-3_000)))
    );
    let i = id.clone();
    let sheet = shop
        .db
        .read(move |c| counts::sheet(c, &i, Lang::Fr, true))
        .unwrap();
    let lot = sheet
        .columns
        .iter()
        .position(|c| c.key == "lot")
        .expect("a lot column");
    let cells: Vec<&Cell> = sheet.rows.iter().map(|r| &r[lot]).collect();
    assert_eq!(
        cells,
        [&Cell::NULL, &Cell::text("L-A"), &Cell::text("L-B")],
        "the product, then one row per lot"
    );
}

#[test]
fn a_lot_found_on_the_shelf_comes_in_and_a_lot_nobody_found_goes_to_zero() {
    let shop = shop();
    let doliprane = lotted(&shop, "Doliprane 500");
    receive_lot(&shop, &doliprane, "L-A", "2027-01-31", 10_000);
    receive_lot(&shop, &doliprane, "L-B", "2027-06-30", 5_000);
    let id = start(&shop, &[&doliprane], true);
    assert_eq!(
        lot_rows(&shop, &id, true),
        [
            row("L-A", Some(10_000), None, None),
            row("L-B", Some(5_000), None, None),
        ],
        "the lots in stock are listed for the counter"
    );
    count_lot(&shop, &id, &doliprane, "L-A", 10_000, CountMode::Set).unwrap();
    count_lot(&shop, &id, &doliprane, "L-NEW", 3_000, CountMode::Set).unwrap();
    assert_eq!(
        lot_rows(&shop, &id, false),
        [
            row("L-A", None, Some(10_000), None),
            row("L-B", None, None, None),
            row("L-NEW", None, Some(3_000), None),
        ],
        "a blind count hides the books"
    );
    close(&shop, &id);
    assert_eq!(
        (
            lot_qty(&shop, &doliprane, "L-A"),
            lot_qty(&shop, &doliprane, "L-B"),
            lot_qty(&shop, &doliprane, "L-NEW")
        ),
        (10_000, 0, 3_000)
    );
    assert_eq!(shelf(&shop, &doliprane), 13_000);
    assert_eq!(
        lot_rows(&shop, &id, true),
        [
            row("L-A", Some(10_000), Some(10_000), Some(0)),
            row("L-B", Some(5_000), Some(0), Some(-5_000)),
            row("L-NEW", Some(0), Some(3_000), Some(3_000)),
        ],
        "the closed count keeps what each lot posted"
    );
}

/// A sale made after the shelf was counted is not undone by the close: each lot posts its
/// difference with its level when counted; a lot the sale emptied passes the rest of its
/// difference to the next lots, first-expired first.
#[test]
fn the_sales_made_after_the_count_survive_the_close() {
    let shop = shop();
    let doliprane = lotted(&shop, "Doliprane 500");
    receive_lot(&shop, &doliprane, "L-A", "2027-01-31", 10_000);
    receive_lot(&shop, &doliprane, "L-B", "2027-06-30", 5_000);
    let id = start(&shop, &[&doliprane], false);
    count_lot(&shop, &id, &doliprane, "L-A", 2_000, CountMode::Set).unwrap();
    count_lot(&shop, &id, &doliprane, "L-B", 5_000, CountMode::Set).unwrap();
    // 4 boxes sold after the count: the books take them from L-A (first expiry), the shelf
    // gave L-A's last 2 and 2 of L-B.
    let (actor, p) = (shop.owner.actor(), doliprane.clone());
    shop.db
        .write(move |tx| {
            let sale = NewMovement {
                product_id: p,
                location_id: SHOP_LOCATION_ID.into(),
                qty_delta: Qty(-4_000),
                reason: MovementReason::Sale,
                reason_code: None,
                ref_type: Some("document".into()),
                ref_id: Some("tk".into()),
                unit_cost: None,
                note: None,
            };
            lots::take(tx, &actor, &sale, "2026-09-28", NOON)
        })
        .unwrap();
    assert_eq!(lot_qty(&shop, &doliprane, "L-A"), 6_000);
    close(&shop, &id);
    assert_eq!(
        (
            lot_qty(&shop, &doliprane, "L-A"),
            lot_qty(&shop, &doliprane, "L-B")
        ),
        (0, 3_000),
        "what the shelf holds now"
    );
    assert_eq!(shelf(&shop, &doliprane), 3_000);
}
