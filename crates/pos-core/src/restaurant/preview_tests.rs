//! Task 23: the payment dialog of a table ticket shows the backend's figures for the bill the
//! payment will take (the cover charge and the offered lines included), without writing anything.

use super::bill::{self, PayInput};
use super::tests::{add, dine_in, restaurant};
use super::tickets::{self, CompReason, TicketStatus};
use crate::auth::Permission;
use crate::money::Money;
use crate::payments::PaymentInput;

#[test]
fn the_payment_dialog_sees_the_bill_the_payment_takes() {
    let (shop, t1, _) = restaurant();
    let cover = shop.product("Couvert", Money(5_000), &shop.tva9).id;
    let plat = shop.product("Plat", Money(50_000), &shop.tva9).id;
    let dessert = shop.product("Tamina", Money(20_000), &shop.tva9).id;
    shop.set("restaurant.cover_product_id", serde_json::json!(cover));
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &plat, 1_000, None, vec![]);
    let ticket = add(&shop, &ticket.id, &dessert, 1_000, None, vec![]);
    let ctx = shop.approved_till(Permission::SalesDiscount);
    let line = ticket.lines[1].id.clone();
    shop.db
        .write(move |tx| bill::comp_line(tx, &ctx, &line, CompReason::Offered))
        .unwrap();

    // the cashier holds no discount right: the offered dessert is not asked again
    let tenders = vec![PaymentInput::cash(Money(80_000))];
    let ctx = shop.till();
    let (id, typed) = (ticket.id.clone(), tenders.clone());
    let preview = shop
        .db
        .read(move |c| bill::preview(c, &ctx, &id, &typed))
        .unwrap();
    assert_eq!(
        preview.cart.totals.total_ttc,
        Money(60_000),
        "the dish and 2 covers × 50 DA; the dessert is offered"
    );
    assert_eq!(
        preview.cart.lines.len(),
        3,
        "the dish, the dessert, then the covers"
    );
    assert_eq!(preview.settlement.total_ttc, Money(60_000));
    assert!(preview.settlement.change.is_positive());

    let id = ticket.id.clone();
    let still = shop.db.read(move |c| tickets::get(c, &id)).unwrap();
    assert_eq!(
        (still.status, still.billed_at),
        (TicketStatus::Open, None),
        "a preview writes nothing"
    );

    let ctx = shop.till();
    let id = ticket.id.clone();
    let paid = shop
        .db
        .write(move |tx| {
            bill::pay(
                tx,
                &ctx,
                PayInput {
                    ticket_id: id,
                    payments: tenders,
                },
            )
        })
        .unwrap();
    assert_eq!(paid.sale.document.total_ttc, preview.cart.totals.total_ttc);
    assert_eq!(
        paid.sale.settlement, preview.settlement,
        "duty, amount due and change as the dialog showed them"
    );
}

#[test]
fn an_empty_or_paid_ticket_has_no_payment_preview() {
    let (shop, t1, _) = restaurant();
    let plat = shop.product("Plat", Money(50_000), &shop.tva9).id;
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 1);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let empty = shop
        .db
        .read(move |c| bill::preview(c, &ctx, &id, &[]))
        .unwrap_err();
    assert_eq!(empty.code(), "cart_empty");

    add(&shop, &ticket.id, &plat, 1_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    shop.db
        .write(move |tx| {
            bill::pay(
                tx,
                &ctx,
                PayInput {
                    ticket_id: id,
                    payments: vec![PaymentInput::cash(Money(60_000))],
                },
            )
        })
        .unwrap();
    let ctx = shop.till();
    let id = ticket.id.clone();
    let paid = shop
        .db
        .read(move |c| bill::preview(c, &ctx, &id, &[]))
        .unwrap_err();
    assert_eq!(paid.code(), "ticket_not_open");
}
