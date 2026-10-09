//! The diagnostic snapshot's integrity result covers every fiscal chain, not the audit journal
//! alone: each document series, the Z closures of every terminal and the versement receipts
//! `RV…` (the M4 merge concern; `fiscal::archive::chains`), under `chains` beside the journal.

use super::snapshot;
use crate::customers::credit::{CollectInput, collect, set_opening};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::{NOON, Shop, shop};
use crate::till::TillContext;
use serde_json::{Value, json};
use std::sync::Arc;

/// A ticket `TK2026-C1-000001` and a cash versement `RV2026-000001` from Karim.
fn a_day_of_sales() -> Shop {
    let shop = shop();
    let milk = shop.product("Lait", Money(12_000), &shop.tva9);
    shop.stock_in(&milk.id, 5_000);
    shop.open_till(0);
    let ctx = shop.till();
    let sale = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(12_000))],
    };
    shop.db
        .write(move |tx| create_sale(tx, &ctx, sale))
        .unwrap();
    let karim = shop.customer("Karim").id;
    let (owner, id) = (shop.owner_till(), karim.clone());
    shop.db
        .write(move |tx| set_opening(tx, &owner, &id, Money(1_000_000), None).map(drop))
        .unwrap();
    let ctx = TillContext::new(shop.cashier.clone(), Arc::clone(&shop.pack), NOON);
    let versement = CollectInput {
        customer_id: karim,
        amount: Money(100_000),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    };
    shop.db
        .write(move |tx| collect(tx, &ctx, versement).map(drop))
        .unwrap();
    shop
}

fn tickets(snapshot: &Value) -> Value {
    snapshot["chains"]["documents"]
        .as_array()
        .and_then(|chains| chains.iter().find(|c| c["docType"] == json!("ticket")))
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn the_snapshot_checks_every_document_series_the_closures_and_the_receipts() {
    let shop = a_day_of_sales();
    let intact = shop.db.read(snapshot).unwrap();
    let ticket = tickets(&intact);
    assert_eq!(ticket["rows"], json!(1), "{intact}");
    assert_eq!(ticket["firstBrokenSeq"], Value::Null, "{intact}");
    assert_eq!(
        intact["chains"]["receipts"],
        json!({ "rows": 1, "firstBrokenNumber": null })
    );
    assert_eq!(intact["chains"]["closures"][0]["terminal"], json!("C1"));
    assert_eq!(intact["chains"]["closures"][0]["firstBrokenZ"], Value::Null);
    assert_eq!(intact["journal"]["firstBrokenSeq"], Value::Null);
    assert!(!intact.to_string().contains("Karim"), "{intact}");

    shop.db
        .write(|tx| {
            Ok(tx.execute_batch(
                "DROP TRIGGER documents_final_once_validated;
                 UPDATE documents SET hash = prev_hash WHERE doc_type = 'ticket' AND chain_seq = 1;
                 DROP TRIGGER customer_ledger_no_update;
                 UPDATE customer_ledger SET amount = -90000 WHERE receipt_number = 'RV2026-000001';",
            )?)
        })
        .unwrap();
    let broken = shop.db.read(snapshot).unwrap();
    let ticket = tickets(&broken);
    assert_eq!(ticket["firstBrokenSeq"], json!(1), "{broken}");
    assert_eq!(ticket["firstBrokenNumber"], json!("TK2026-C1-000001"));
    assert_eq!(
        broken["chains"]["receipts"]["firstBrokenNumber"],
        json!("RV2026-000001")
    );
}
