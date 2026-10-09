//! Task 23: the payment dialog of a table ticket reads the bill's figures from the backend
//! (`dining_ticket_preview`) and they are the ones the payment takes.

use super::testshop::shop;
use super::{packs, restaurant};
use pos_core::company::ShopActivity;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::packs::Module;
use pos_core::payments::PaymentInput;
use pos_core::restaurant::bill::PayInput;
use pos_core::restaurant::tickets::{AddLineInput, DiningMode, OpenInput};
use pos_core::sales::CartLine;

#[test]
fn the_table_payment_shows_what_the_payment_takes() {
    let t = shop();
    let st = &t.st;
    packs::apply_template(st, ShopActivity::FoodService, true).unwrap();
    t.open_till(50_000);
    let plat = t.product_in_category(
        "Chorba",
        35_000,
        &restaurant::stations(st).unwrap()[0].category_ids[0],
    );
    let table = restaurant::tables(st, None).unwrap().remove(0);
    let ticket = restaurant::ticket_open(
        st,
        OpenInput {
            mode: DiningMode::DineIn,
            table_id: Some(table.id),
            label: None,
            covers: 2,
            customer_id: None,
            delivery_address: None,
            lang: Lang::Fr,
            price_level_id: None,
        },
    )
    .unwrap();
    restaurant::ticket_add_line(
        st,
        ticket.id.clone(),
        AddLineInput {
            cart_line: CartLine::product(&plat, Qty(2_000)),
            seat: None,
            course: None,
            modifiers: Vec::new(),
        },
    )
    .unwrap();
    let tenders = vec![PaymentInput::cash(Money(100_000))];
    let preview = restaurant::ticket_preview(st, ticket.id.clone(), tenders.clone()).unwrap();
    assert_eq!(preview.cart.totals.total_ttc, Money(70_000));
    let paid = restaurant::ticket_pay(
        st,
        PayInput {
            ticket_id: ticket.id.clone(),
            payments: tenders,
        },
        false,
    )
    .unwrap();
    assert_eq!(paid.paid.sale.settlement, preview.settlement);
    assert_eq!(
        restaurant::ticket_preview(st, ticket.id, Vec::new())
            .unwrap_err()
            .code(),
        "ticket_not_open"
    );
}

#[test]
fn no_table_payment_preview_while_the_restaurant_is_off() {
    let t = shop();
    let st = &t.st;
    packs::set_modules(st, vec![Module::Pharmacy]).unwrap();
    assert_eq!(
        restaurant::ticket_preview(st, "missing".into(), Vec::new())
            .unwrap_err()
            .code(),
        "module_disabled"
    );
}
