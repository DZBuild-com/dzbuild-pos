//! A table payment carries the till's pack extras (Task 19): a controlled medicine
//! paid at a table writes its register entry like a till sale, and is refused without one.

use super::bill::{self, PayInput};
use super::tests::{add, dine_in, restaurant};
use crate::money::Money;
use crate::packs::{self, Module};
use crate::payments::PaymentInput;
use crate::pharmacy::tests::{medicine, prescription};
use crate::sales::SaleExtras;

#[test]
fn a_table_payment_carries_the_pack_extras_of_its_sale() {
    let (shop, t1, _) = restaurant();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(
                tx,
                &actor,
                &[Module::Restaurant, Module::Pharmacy, Module::Staff],
            )
        })
        .unwrap();
    let med = medicine(&shop, "Tramadol 50 mg", 45_000, "tramadol", true);
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 1);
    add(&shop, &ticket.id, &med, 1_000, None, vec![]);
    let pay = |extras: SaleExtras| {
        let (ctx, id) = (shop.owner_till(), ticket.id.clone());
        shop.db.write(move |tx| {
            bill::pay_with(
                tx,
                &ctx,
                PayInput {
                    ticket_id: id,
                    payments: vec![PaymentInput::cash(Money(50_000))],
                },
                &extras,
            )
        })
    };
    assert_eq!(
        pay(SaleExtras::default()).unwrap_err().code(),
        "register_entry_required"
    );
    let paid = pay(SaleExtras {
        prescriptions: vec![prescription(0)],
        ..SaleExtras::default()
    })
    .unwrap();
    let document = paid.sale.document.id.clone();
    let entries: i64 = shop
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM controlled_register WHERE document_id = ?1",
                [&document],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(entries, 1);
}
