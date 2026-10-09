//! The points line of a receipt (LOY-002, Task 19): what a ticket earned and the customer's
//! balance, read back from the earn row of that ticket.

use crate::lang::Lang;
use crate::loyalty::{self, DocumentPoints};
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::{Shop, shop};

fn sell(shop: &Shop, product: &str, customer: Option<&str>) -> String {
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(1_000))]);
    cart.customer_id = customer.map(str::to_owned);
    let payments = vec![PaymentInput::cash(Money(100_000))];
    shop.db
        .write(move |tx| create_sale(tx, &ctx, SaleInput { cart, payments }))
        .unwrap()
        .document
        .id
}

fn points(shop: &Shop, document: &str) -> Option<DocumentPoints> {
    let id = document.to_owned();
    shop.db
        .read(move |c| loyalty::points_of_document(c, &id))
        .unwrap()
}

#[test]
fn a_ticket_names_the_points_it_earned_and_the_balance_after_it() {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Loyalty]))
        .unwrap();
    shop.open_till(100_000);
    let customer = shop.customer("Nadia").id;
    let (ctx, c) = (shop.owner_till(), customer.clone());
    let card = shop
        .db
        .write(move |tx| loyalty::enrol(tx, &ctx, &c, None))
        .unwrap();
    let product = shop.product("Parfum", Money(45_000), &shop.tva19).id;
    let first = sell(&shop, &product, Some(&customer));
    let second = sell(&shop, &product, Some(&customer));
    let earned = points(&shop, &first).expect("the first ticket earned points");
    assert!(earned.earned > 0);
    assert_eq!(earned.customer_id, customer);
    assert_eq!(earned.card.as_deref(), Some(card.code.as_str()));
    let after = points(&shop, &second).unwrap();
    assert_eq!(
        after.balance,
        earned.earned + after.earned,
        "the balance is the customer's points now"
    );
    assert_eq!(
        points(&shop, &sell(&shop, &product, None)),
        None,
        "no customer, no points"
    );
}
