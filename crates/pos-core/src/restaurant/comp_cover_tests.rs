//! Task 5 review: an offered line keeps the table workable for everyone after the manager's
//! approval, and a split by seat moves a cover instead of creating one.

use super::bill::{self, PayInput, SplitBy};
use super::tests::{add, dine_in, restaurant};
use super::tickets::{self, CompReason, Ticket, TicketStatus, VoidReason};
use crate::auth::Permission;
use crate::money::{Money, Qty, Rate};
use crate::payments::PaymentInput;
use crate::reports::{ReportKind, ReportRequest, run};
use crate::table::Cell;
use crate::testkit::Shop;

fn comp(shop: &Shop, line: &str, reason: CompReason) -> Ticket {
    let ctx = shop.approved_till(Permission::SalesDiscount);
    let line = line.to_owned();
    shop.db
        .write(move |tx| bill::comp_line(tx, &ctx, &line, reason))
        .unwrap()
}

/// The addition then the payment, both at the cashier's till with no approval.
fn bill_and_pay(shop: &Shop, ticket_id: &str) -> bill::PaidTicket {
    let ctx = shop.till();
    let id = ticket_id.to_owned();
    let addition = shop
        .db
        .write(move |tx| bill::prebill(tx, &ctx, &id, None))
        .unwrap();
    let ctx = shop.till();
    let id = ticket_id.to_owned();
    shop.db
        .write(move |tx| {
            bill::pay(
                tx,
                &ctx,
                PayInput {
                    ticket_id: id,
                    payments: vec![PaymentInput::cash(addition.cash_due)],
                },
            )
        })
        .unwrap()
}

/// RST-008: a manager offers the dessert; the cashier (no discount right, no approval) still
/// adds a coffee, changes a quantity, has a line voided with a void approval only, prints the
/// addition and cashes the table. The ticket keeps the free line, 100 % off, with its reason.
#[test]
fn an_offered_line_is_approved_once_and_the_table_goes_on() {
    let (shop, t1, _) = restaurant();
    let dessert = shop.product("Tamina", Money(20_000), &shop.tva9).id;
    let coffee = shop.product("Café", Money(5_000), &shop.tva9).id;
    let water = shop.product("Eau", Money(4_000), &shop.tva9).id;
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 2);
    let ticket = add(&shop, &ticket.id, &dessert, 1_000, None, vec![]);
    let comped = comp(&shop, &ticket.lines[0].id, CompReason::Offered);
    assert_eq!(comped.total_ttc, Money::ZERO);

    let ticket = add(&shop, &ticket.id, &coffee, 1_000, None, vec![]);
    assert_eq!(ticket.total_ttc, Money(5_000), "the waiter adds a coffee");
    let ticket = add(&shop, &ticket.id, &water, 1_000, None, vec![]);
    let ctx = shop.till();
    let coffee_line = ticket.lines[1].id.clone();
    let ticket = shop
        .db
        .write(move |tx| tickets::set_qty(tx, &ctx, &coffee_line, Qty(2_000)))
        .unwrap();
    assert_eq!(ticket.total_ttc, Money(14_000));
    let ctx = shop.approved_till(Permission::SalesVoidLine);
    let water_line = ticket.lines[2].id.clone();
    let voided = shop
        .db
        .write(move |tx| bill::void_line(tx, &ctx, &water_line, None, VoidReason::Mistake))
        .unwrap();
    assert_eq!(
        voided.ticket.total_ttc,
        Money(10_000),
        "a void approval alone reprices a ticket with an offered line"
    );

    let paid = bill_and_pay(&shop, &ticket.id);
    assert_eq!(paid.ticket.status, TicketStatus::Paid);
    let doc = &paid.sale.document;
    assert_eq!(doc.total_ttc, Money(10_000));
    let free = doc
        .lines
        .iter()
        .find(|l| l.product_id.as_deref() == Some(dessert.as_str()))
        .unwrap();
    assert_eq!(
        (
            free.discount_bp,
            free.discount_amount,
            free.total_ttc,
            free.reason_code.as_deref()
        ),
        (Rate::FULL, Money(20_000), Money::ZERO, Some("comp_offered"))
    );
    let coffee_sold = doc
        .lines
        .iter()
        .find(|l| l.product_id.as_deref() == Some(coffee.as_str()))
        .unwrap();
    assert_eq!(
        (coffee_sold.discount_bp, coffee_sold.reason_code.as_deref()),
        (Rate::ZERO, None),
        "only the offered line carries the comp"
    );
}

/// The M1a hook: a granted line skips the discount right only; its other rights and every other
/// line of the cart are checked as at the till.
#[test]
fn a_granted_discount_skips_only_the_discount_right_of_its_line() {
    use crate::lang::Lang;
    use crate::sales::cart::{Cart, CartLine, price_cart_granted};
    use crate::tax::tva::Discount;
    let (shop, _, _) = restaurant();
    let dessert = shop.product("Tamina", Money(20_000), &shop.tva9).id;
    let free = |unit_price: Option<Money>| {
        let mut line = CartLine::product(&dessert, Qty(1_000));
        line.discount = Discount::Percent(Rate::FULL);
        line.unit_price = unit_price;
        line
    };
    let price = |lines: Vec<CartLine>, granted: Vec<bool>| {
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, lines);
        shop.db
            .read(move |c| price_cart_granted(c, &ctx, &cart, &granted))
    };
    let priced = price(vec![free(None)], vec![true]).unwrap();
    assert_eq!(priced.totals.total_ttc, Money::ZERO);
    let err = price(vec![free(None)], vec![false]).unwrap_err();
    assert_eq!(err.code(), "discount_not_allowed");
    let err = price(vec![free(None), free(None)], vec![true]).unwrap_err();
    assert_eq!(
        err,
        crate::error::AppError::validation("lines.1.discount", "discount_not_allowed"),
        "only the granted line"
    );
    let err = price(vec![free(Some(Money(10_000)))], vec![true]).unwrap_err();
    assert_eq!(err.code(), "price_override_not_allowed");
}

/// The approval covered the quantity offered: offering more is a new discount, asked again.
#[test]
fn offering_more_of_a_line_needs_the_discount_right_again() {
    let (shop, t1, _) = restaurant();
    let dessert = shop.product("Tamina", Money(20_000), &shop.tva9).id;
    let ticket = dine_in(&shop, &t1, 1);
    let ticket = add(&shop, &ticket.id, &dessert, 1_000, None, vec![]);
    let line = ticket.lines[0].id.clone();
    comp(&shop, &line, CompReason::Staff);
    let ctx = shop.till();
    let more = line.clone();
    let err = shop
        .db
        .write(move |tx| tickets::set_qty(tx, &ctx, &more, Qty(3_000)))
        .unwrap_err();
    assert_eq!(err.code(), "discount_not_allowed");
    let ctx = shop.approved_till(Permission::SalesDiscount);
    let more = line.clone();
    let raised = shop
        .db
        .write(move |tx| tickets::set_qty(tx, &ctx, &more, Qty(2_000)))
        .unwrap();
    assert_eq!(
        (raised.lines[0].qty, raised.total_ttc),
        (Qty(2_000), Money::ZERO)
    );
    let ctx = shop.till();
    let less = line.clone();
    let lowered = shop
        .db
        .write(move |tx| tickets::set_qty(tx, &ctx, &less, Qty(1_000)))
        .unwrap();
    assert_eq!(
        (lowered.lines[0].qty, lowered.total_ttc),
        (Qty(1_000), Money::ZERO),
        "offering less needs no right"
    );
}

/// RST-005: a split by seat takes one of the table's covers with it; the covers of both
/// tickets add up to the table's, so the cover charge and the report count each guest once.
#[test]
fn a_split_by_seat_moves_a_cover_and_never_creates_one() {
    let (shop, t1, _) = restaurant();
    let cover = shop.product("Couvert", Money(5_000), &shop.tva9).id;
    let (a, b) = (
        shop.product("Plat A", Money(50_000), &shop.tva9).id,
        shop.product("Plat B", Money(40_000), &shop.tva9).id,
    );
    shop.set("restaurant.cover_product_id", serde_json::json!(cover));
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &a, 1_000, Some(1), vec![]);
    add(&shop, &ticket.id, &b, 1_000, Some(2), vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let done = shop
        .db
        .write(move |tx| bill::split(tx, &ctx, &id, SplitBy::Seat { seat: 2 }))
        .unwrap();
    assert_eq!(
        (done.from.covers, done.to.covers),
        (1, 1),
        "the cover moved"
    );
    let mut cover_qty = Qty::ZERO;
    let mut cover_ttc = Money::ZERO;
    for id in [done.from.id.clone(), done.to.id.clone()] {
        let paid = bill_and_pay(&shop, &id);
        for l in paid
            .sale
            .document
            .lines
            .iter()
            .filter(|l| l.product_id.as_deref() == Some(cover.as_str()))
        {
            cover_qty = cover_qty.checked_add(l.qty).unwrap();
            cover_ttc = cover_ttc.checked_add(l.total_ttc).unwrap();
        }
    }
    assert_eq!(
        (cover_qty, cover_ttc),
        (Qty(2_000), Money(10_000)),
        "2 covers × 50 DA, not 3"
    );
    let req = ReportRequest::new(ReportKind::SalesByTable, "2026-09-28", "2026-09-28");
    let report = shop.db.read(move |c| run(c, &req)).unwrap();
    let t1_row = report
        .table
        .rows
        .iter()
        .find(|r| r[1] == Cell::text("T1"))
        .unwrap();
    assert_eq!(
        &t1_row[3..5],
        &[Cell::int(2), Cell::int(2)],
        "two tickets, the table's two covers"
    );
}

/// A table of one (or with no cover counted) keeps its cover: the new ticket gets none.
#[test]
fn a_split_by_seat_of_one_cover_or_none_keeps_the_count() {
    for covers in [0_u8, 1] {
        let (shop, t1, _) = restaurant();
        let (a, b) = (
            shop.product("Plat A", Money(50_000), &shop.tva9).id,
            shop.product("Plat B", Money(40_000), &shop.tva9).id,
        );
        let ticket = dine_in(&shop, &t1, covers);
        add(&shop, &ticket.id, &a, 1_000, Some(1), vec![]);
        add(&shop, &ticket.id, &b, 1_000, Some(2), vec![]);
        let ctx = shop.till();
        let id = ticket.id.clone();
        let done = shop
            .db
            .write(move |tx| bill::split(tx, &ctx, &id, SplitBy::Seat { seat: 2 }))
            .unwrap();
        assert_eq!((done.from.covers, done.to.covers), (covers, 0));
    }
}
