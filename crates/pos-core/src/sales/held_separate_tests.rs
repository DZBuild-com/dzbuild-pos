//! A weighed package (a scale label, F11) is a cart line of its own that later scans never merge
//! into: holding and recalling the ticket keeps it so (VRT-010, SAL-003).

use super::cart::{Cart, CartLine};
use super::held::{hold, list, recall};
use crate::catalog::UNIT_KG_ID;
use crate::catalog::products::ProductInput;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::testkit::shop;

#[test]
fn a_held_weighed_package_comes_back_as_a_line_of_its_own() {
    let shop = shop();
    let tomato = shop.product_with(ProductInput::simple(
        "Tomates",
        UNIT_KG_ID,
        &shop.tva9,
        Money(12_000),
    ));
    let package = CartLine {
        separate: true,
        ..CartLine::product(&tomato.id, Qty(1_750))
    };
    let loose = CartLine::product(&tomato.id, Qty(500));
    let cart = Cart::new(Lang::Fr, vec![package, loose]);
    let ctx = shop.till();
    let held = shop
        .db
        .write(move |tx| hold(tx, &ctx, "Table 2", cart))
        .unwrap();
    let separate = |lines: &[CartLine]| lines.iter().map(|l| l.separate).collect::<Vec<_>>();
    assert_eq!(separate(&held.cart.lines), [true, false]);
    let terminal = shop.terminal.id.clone();
    let listed = shop.db.read(move |c| list(c, &terminal)).unwrap();
    assert_eq!(separate(&listed[0].cart.lines), [true, false]);
    let (ctx, id) = (shop.till(), held.id.clone());
    let recalled = shop.db.write(move |tx| recall(tx, &ctx, &id)).unwrap();
    assert_eq!(separate(&recalled.cart.lines), [true, false]);
}

#[test]
fn a_cart_held_before_the_flag_existed_reads_as_merged_lines() {
    let line = CartLine::product("p-milk", Qty(1_000));
    let mut json = serde_json::to_value(&line).unwrap();
    assert_eq!(json["separate"], false);
    json.as_object_mut().unwrap().remove("separate");
    let older: CartLine = serde_json::from_value(json).unwrap();
    assert_eq!(older, line);
}
