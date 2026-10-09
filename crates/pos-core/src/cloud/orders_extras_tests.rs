//! An online order sold at the till carries the till's pack extras (Task 19): a
//! controlled medicine ordered online writes its register entry when it is sold, and is refused
//! without one.

use super::orders::{OrderState, RemoteOrder, claim, get, sell, sell_with, upsert_remote};
use super::testkit::linked_shop;
use crate::money::Money;
use crate::packs::{self, Module};
use crate::payments::PaymentInput;
use crate::pharmacy::tests::{medicine, prescription};
use crate::sales::SaleExtras;
use crate::testkit::NOON;
use serde_json::json;

fn order_of(external_id: &str, price: i64) -> RemoteOrder {
    let value = json!({
        "id": 9001, "order_number": 1042, "status": "confirmed", "payment_method": "cod", "delivery_type": "pickup",
        "customer_name": "Samira B.", "customer_phone": "0550 12 34 56", "wilaya_id": 16, "commune": "Bab Ezzouar",
        "address": "Cité 5 juillet", "customer_notes": null,
        "subtotal": price, "shipping_cost": 0, "discount": 0, "total": price,
        "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
        "items": [
            { "product_id": 88121, "external_id": external_id, "product_name": "Tramadol", "price": price, "quantity": 1, "total": price }
        ]
    });
    // Amounts are read from the raw JSON text (ApiMoney).
    serde_json::from_str(&value.to_string()).unwrap()
}

#[test]
fn an_online_order_sold_at_the_till_carries_the_pack_extras_of_its_sale() {
    let (shop, link) = linked_shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Pharmacy]))
        .unwrap();
    let med = medicine(&shop, "Tramadol 50 mg", 45_000, "tramadol", true);
    shop.stock_in(&med, 5_000);
    shop.open_till(0);
    let (l, order) = (link.clone(), order_of(&med, 450));
    let id: String = shop
        .db
        .write(move |tx| {
            upsert_remote(tx, &l, &order, NOON)?;
            Ok(tx.query_row(
                "SELECT id FROM online_orders WHERE remote_id = '9001'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let (till, l, i) = (shop.till(), link.clone(), id.clone());
    shop.db
        .write(move |tx| claim(tx, &till, &l, &i, false).map(drop))
        .unwrap();
    let cash = || vec![PaymentInput::cash(Money(50_000))];
    let (till, l, i) = (shop.till(), link.clone(), id.clone());
    let refused = shop
        .db
        .write(move |tx| sell(tx, &till, &l, &i, "Retrait".into(), cash()))
        .unwrap_err();
    assert_eq!(refused.code(), "register_entry_required");
    let extras = SaleExtras {
        prescriptions: vec![prescription(0)],
        ..SaleExtras::default()
    };
    let (till, l, i) = (shop.owner_till(), link.clone(), id.clone());
    let sale = shop
        .db
        .write(move |tx| sell_with(tx, &till, &l, &i, "Retrait".into(), cash(), &extras))
        .unwrap();
    let document = sale.document.id.clone();
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
    let terminal = shop.terminal.id.clone();
    let order = shop.db.read(move |c| get(c, &id, &terminal)).unwrap();
    assert_eq!(order.local_state, OrderState::Fulfilled);
}
