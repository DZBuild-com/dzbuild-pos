//! Serials coming back on a return (Task 8 review): the serials module only asks for the numbers
//! when it is on, a plain M2 return of a line whose serials were linked after the sale still
//! works, and a return that brings back every serial still sold on its line takes them all
//! without asking for names.

use crate::catalog::extras::{self, ProductExtrasInput};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::returns::{
    ReturnInput, ReturnLineInput, ReturnReason, create_return, create_return_with,
};
use crate::sales::extensions::{ReturnExtras, ReturnSerial, SaleSerial};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, ValidatedSale, create_sale_with};
use crate::stock::serials::{self, SerialLink, SerialStatus};
use crate::testkit::{NOON, Shop, shop};

const IMEI_A: &str = "356938035643809";
const IMEI_B: &str = "490154203237518";

/// A serial-tracked phone with its serials registered and in stock.
fn phone(shop: &Shop, numbers: &[&'static str]) -> String {
    let id = shop.product("Galaxy A16", Money(2_900_000), &shop.tva19).id;
    let (pid, numbers_owned): (String, Vec<String>) = (
        id.clone(),
        numbers.iter().map(|s| (*s).to_owned()).collect(),
    );
    shop.db
        .write(move |tx| {
            extras::set_quiet(
                tx,
                &pid,
                ProductExtrasInput {
                    track_serials: true,
                    ..Default::default()
                },
            )?;
            for no in &numbers_owned {
                serials::register(
                    tx,
                    &pid,
                    no,
                    Some(crate::stock::SHOP_LOCATION_ID),
                    None,
                    NOON,
                )?;
            }
            Ok(())
        })
        .unwrap();
    shop.stock_in(&id, 1_000 * i64::try_from(numbers.len()).unwrap());
    id
}

fn sell(shop: &Shop, product: &str, pieces: i64, extras: SaleExtras) -> ValidatedSale {
    let ctx = shop.till();
    let input = SaleInput {
        cart: Cart::new(
            Lang::Fr,
            vec![CartLine::product(product, Qty(pieces * 1_000))],
        ),
        payments: vec![PaymentInput::cash(Money(100_000_000))],
    };
    shop.db
        .write(move |tx| create_sale_with(tx, &ctx, input, &extras))
        .unwrap()
}

/// The M2 flow: the serials are linked to the ticket after the sale (shell `serials_link_sale`).
fn link_after_sale(shop: &Shop, sale: &ValidatedSale, numbers: &[&'static str]) {
    let (actor, doc) = (shop.owner.actor(), sale.document.id.clone());
    let links: Vec<SerialLink> = numbers
        .iter()
        .map(|no| SerialLink {
            line_id: sale.document.lines[0].id.clone(),
            serial_no: (*no).to_owned(),
        })
        .collect();
    shop.db
        .write(move |tx| serials::link_sale(tx, &actor, &doc, links))
        .unwrap();
}

fn return_of(sale: &ValidatedSale, pieces: i64) -> ReturnInput {
    ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(sale.document.lines[0].id.clone()),
            product_id: None,
            qty: Qty(pieces * 1_000),
            unit_price: None,
            restock: true,
            reason: ReturnReason::Other,
            note: None,
        }],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    }
}

fn status(shop: &Shop, no: &'static str) -> SerialStatus {
    shop.db.read(move |c| serials::find(c, no)).unwrap()[0].status
}

#[test]
fn a_plain_return_brings_back_the_serial_linked_after_the_sale_with_the_module_off() {
    let shop = shop();
    shop.open_till(500_000);
    assert!(
        !shop
            .db
            .read(|c| packs::enabled(c, Module::Serials))
            .unwrap()
    );
    let galaxy = phone(&shop, &[IMEI_A]);
    let sale = sell(&shop, &galaxy, 1, SaleExtras::default());
    link_after_sale(&shop, &sale, &[IMEI_A]);
    assert_eq!(status(&shop, IMEI_A), SerialStatus::Sold);
    let (ctx, input) = (
        shop.approved_till(crate::auth::Permission::SalesReturn),
        return_of(&sale, 1),
    );
    shop.db
        .write(move |tx| create_return(tx, &ctx, input))
        .unwrap();
    assert_eq!(status(&shop, IMEI_A), SerialStatus::InStock);
}

#[test]
fn a_partial_return_with_the_module_off_takes_back_only_the_named_serials() {
    let shop = shop();
    shop.open_till(500_000);
    let galaxy = phone(&shop, &[IMEI_A, IMEI_B]);
    let sale = sell(&shop, &galaxy, 2, SaleExtras::default());
    link_after_sale(&shop, &sale, &[IMEI_A, IMEI_B]);
    let ctx = shop.approved_till(crate::auth::Permission::SalesReturn);
    // One piece of two, nothing named: the choice is the shop's, the return goes through.
    let (c1, i1) = (ctx.clone(), return_of(&sale, 1));
    shop.db.write(move |tx| create_return(tx, &c1, i1)).unwrap();
    assert_eq!(status(&shop, IMEI_A), SerialStatus::Sold);
    assert_eq!(status(&shop, IMEI_B), SerialStatus::Sold);
    // A named serial is still checked against its line.
    let (c2, i2) = (ctx.clone(), return_of(&sale, 1));
    let stranger = ReturnExtras {
        serials: vec![ReturnSerial {
            line: 0,
            serial_no: "861234567890125".into(),
        }],
        ..ReturnExtras::default()
    };
    let err = shop
        .db
        .write(move |tx| create_return_with(tx, &c2, i2, None, &stranger))
        .unwrap_err();
    assert_eq!(err.code(), "serial_not_on_line");
}

#[test]
fn a_full_return_with_the_module_on_brings_every_serial_back_without_names() {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Serials]))
        .unwrap();
    shop.open_till(500_000);
    let galaxy = phone(&shop, &[IMEI_A, IMEI_B]);
    let both = SaleExtras {
        serials: vec![
            SaleSerial {
                line: 0,
                serial_no: IMEI_A.into(),
            },
            SaleSerial {
                line: 0,
                serial_no: IMEI_B.into(),
            },
        ],
        ..SaleExtras::default()
    };
    let sale = sell(&shop, &galaxy, 2, both);
    let ctx = shop.approved_till(crate::auth::Permission::SalesReturn);
    // One of two pieces without a name is ambiguous: the module asks which one.
    let (c1, i1) = (ctx.clone(), return_of(&sale, 1));
    let err = shop
        .db
        .write(move |tx| create_return(tx, &c1, i1))
        .unwrap_err();
    assert_eq!(err.code(), "serials_required");
    // Both pieces: every serial still sold on the line comes back.
    let input = return_of(&sale, 2);
    shop.db
        .write(move |tx| create_return(tx, &ctx, input))
        .unwrap();
    assert_eq!(status(&shop, IMEI_A), SerialStatus::InStock);
    assert_eq!(status(&shop, IMEI_B), SerialStatus::InStock);
}
