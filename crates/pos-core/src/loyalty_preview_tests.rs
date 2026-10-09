//! The points a ticket earns its customer, shown at the payment before it is validated (Task 24,
//! LOY-002): the same rule as the earn row written by the sale.

use crate::lang::Lang;
use crate::loyalty;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, create_sale_with};
use crate::testkit::{Shop, shop};

fn with_loyalty(on: bool) -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    let modules = if on { vec![Module::Loyalty] } else { vec![] };
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &modules))
        .unwrap();
    shop.open_till(100_000);
    shop
}

fn enrol(shop: &Shop, name: &str) -> String {
    let id = shop.customer(name).id;
    let (ctx, c) = (shop.owner_till(), id.clone());
    shop.db
        .write(move |tx| loyalty::enrol(tx, &ctx, &c, None))
        .unwrap();
    id
}

fn voucher(amount: i64) -> PaymentInput {
    PaymentInput {
        method: PaymentMethod::Voucher,
        amount: Money(amount),
        reference: Some("BA2026-000001".into()),
        cheque_bank: None,
        cheque_date: None,
    }
}

fn preview(shop: &Shop, customer: &str, total: i64, payments: Vec<PaymentInput>) -> Option<i64> {
    let c = customer.to_owned();
    shop.db
        .read(move |conn| loyalty::preview_points(conn, &c, Money(total), &payments))
        .unwrap()
}

#[test]
fn an_enrolled_customer_sees_the_points_of_the_money_not_paid_by_vouchers() {
    let shop = with_loyalty(true);
    let karim = enrol(&shop, "Karim Benali");
    let nadia = shop.customer("Nadia Ouali").id;
    // 23 456 DA: 234 whole hundreds of dinars, one point each (the default rule)
    assert_eq!(preview(&shop, &karim, 2_345_600, vec![]), Some(234));
    // 3 000 DA of it paid by a voucher earns nothing again
    assert_eq!(
        preview(
            &shop,
            &karim,
            2_345_600,
            vec![voucher(300_000), PaymentInput::cash(Money(2_045_600))]
        ),
        Some(204)
    );
    // a voucher above the total earns nothing, and never takes points away
    assert_eq!(
        preview(&shop, &karim, 50_000, vec![voucher(80_000)]),
        Some(0)
    );
    // no live card: no points line
    assert_eq!(preview(&shop, &nadia, 2_345_600, vec![]), None);
}

#[test]
fn the_preview_is_what_the_sale_earns() {
    let shop = with_loyalty(true);
    let karim = enrol(&shop, "Karim Benali");
    let dates = shop.product("Dattes Deglet Nour 1 kg", Money(235_000), &shop.tva9);
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&dates.id, Qty(10_000))]);
    cart.customer_id = Some(karim.clone());
    let done = shop
        .db
        .write(move |tx| {
            create_sale_with(
                tx,
                &ctx,
                SaleInput {
                    cart,
                    payments: vec![PaymentInput::cash(Money(2_500_000))],
                },
                &SaleExtras::default(),
            )
        })
        .unwrap();
    let total = done.document.total_ttc.0;
    let c = karim.clone();
    let balance = shop
        .db
        .read(move |conn| loyalty::account(conn, &c, "2026-09-28"))
        .unwrap()
        .balance;
    assert_eq!(preview(&shop, &karim, total, vec![]), Some(balance));
}

#[test]
fn no_points_line_while_the_loyalty_module_is_off() {
    let shop = with_loyalty(false);
    let karim = shop.customer("Karim Benali").id;
    assert_eq!(preview(&shop, &karim, 2_345_600, vec![]), None);
}
