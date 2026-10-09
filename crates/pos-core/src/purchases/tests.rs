//! Purchasing (PUR-002…008, PRD-031, STK-016): orders, receipts, invoices, returns.

use super::invoices::{InvoiceInput, PaymentNow, record_invoice};
use super::lines::PurchaseLineInput;
use super::orders::{OrderInput, remaining, save_order, validate_order};
use super::receipts::{InvoiceNow, ReceiptInput, receive};
use super::returns::{SupplierReturnInput, SupplierReturnLine, return_to_supplier};
use super::{OrderStatus, PurchaseQuery, cost_history, list, order_status};
use crate::catalog::products::Product;
use crate::documents::{self, DocStatus, DocType, DocumentQuery};
use crate::error::AppError;
use crate::money::{Money, Qty};
use crate::payments::PaymentMethod;
use crate::tax::tva::Discount;
use crate::testkit::{Shop, shop};

fn water(shop: &Shop) -> Product {
    shop.product("Eau Ifri 1,5 L", Money(5_000), &shop.tva9)
}

fn line(product: &str, qty: i64, cost: i64) -> PurchaseLineInput {
    PurchaseLineInput {
        product_id: product.into(),
        qty: Qty(qty),
        unit_cost: Money(cost),
        pack_qty: None,
        tax_rate_id: None,
        discount: Discount::None,
        origin_line_id: None,
        lot_number: None,
        expiry_date: None,
        serials: Vec::new(),
        note: None,
    }
}

#[test]
fn an_order_is_drafted_edited_validated_and_received_in_two_parts() {
    let shop = shop();
    let w = water(&shop);
    let oil = shop.product("Huile Elio 5 L", Money(65_000), &shop.tva19);
    let s = shop.supplier("Grossiste Bab Ezzouar");
    let order = |lines: Vec<PurchaseLineInput>| OrderInput {
        supplier_id: s.id.clone(),
        lines,
        supplier_ref: None,
        note: None,
    };
    let ctx = shop.owner_till();
    let draft = shop
        .db
        .write({
            let o = order(vec![line(&w.id, 60_000, 3_000)]);
            move |tx| save_order(tx, &ctx, None, o)
        })
        .unwrap();
    assert_eq!(
        (draft.status, draft.number.clone()),
        (DocStatus::Draft, None)
    );
    let ctx = shop.owner_till();
    let id = draft.id.clone();
    let edited = shop
        .db
        .write({
            let o = order(vec![
                line(&w.id, 120_000, 3_000),
                line(&oil.id, 10_000, 52_000),
            ]);
            move |tx| save_order(tx, &ctx, Some(&id), o)
        })
        .unwrap();
    assert_eq!(
        (edited.id.clone(), edited.lines.len()),
        (draft.id.clone(), 2),
        "the draft keeps its id"
    );
    let ctx = shop.owner_till();
    let id = draft.id.clone();
    let sent = shop
        .db
        .write(move |tx| validate_order(tx, &ctx, &id))
        .unwrap();
    assert_eq!(sent.number.as_deref(), Some("BC2026-000001"));
    assert_eq!(
        shop.db
            .read({
                let id = draft.id.clone();
                move |c| order_status(c, &id)
            })
            .unwrap(),
        OrderStatus::Sent
    );
    let ctx = shop.owner_till();
    let id = draft.id.clone();
    let again = shop.db.write({
        let o = order(vec![line(&w.id, 1_000, 3_000)]);
        move |tx| save_order(tx, &ctx, Some(&id), o)
    });
    assert_eq!(again.unwrap_err().code(), "document_already_validated");
    let rest = shop
        .db
        .read({
            let id = draft.id.clone();
            move |c| remaining(c, &id)
        })
        .unwrap();
    let mut first = line(&w.id, 120_000, 3_000);
    first.origin_line_id = Some(rest[0].line.id.clone());
    let mut half = line(&oil.id, 5_000, 52_000);
    half.origin_line_id = Some(rest[1].line.id.clone());
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: Some(draft.id.clone()),
        location_id: None,
        lines: vec![first, half],
        supplier_ref: Some("BL 4471".into()),
        note: None,
        invoice: None,
    };
    let ctx = shop.owner_till();
    let done = shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
    assert_eq!(done.receipt.number.as_deref(), Some("RC2026-000001"));
    assert_eq!(done.receipt.parent_id.as_deref(), Some(draft.id.as_str()));
    assert_eq!(shop.count("SELECT SUM(qty) FROM stock_levels"), 125_000);
    assert_eq!(
        shop.db
            .read({
                let id = draft.id.clone();
                move |c| order_status(c, &id)
            })
            .unwrap(),
        OrderStatus::Partial
    );
    let rest = shop
        .db
        .read({
            let id = draft.id.clone();
            move |c| remaining(c, &id)
        })
        .unwrap();
    assert_eq!(
        rest.iter().map(|r| r.remaining.0).collect::<Vec<_>>(),
        vec![0, 5_000]
    );
    let q = PurchaseQuery {
        doc_type: Some(DocType::PurchaseOrder),
        ..PurchaseQuery::default()
    };
    let orders = shop.db.read(move |c| list(c, &q)).unwrap();
    assert_eq!(
        (orders[0].order_status, orders[0].supplier_name.as_deref()),
        (Some(OrderStatus::Partial), Some("Grossiste Bab Ezzouar"))
    );
}

#[test]
fn packs_convert_to_units_and_each_receipt_moves_the_cmup() {
    let shop = shop();
    let w = water(&shop);
    let s = shop.supplier("Ifri");
    let mut cartons = line(&w.id, 5_000, 36_000);
    cartons.pack_qty = Some(Qty(12_000));
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: None,
        location_id: None,
        lines: vec![cartons],
        supplier_ref: None,
        note: None,
        invoice: None,
    };
    let ctx = shop.owner_till();
    let done = shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
    let l = &done.receipt.lines[0];
    // 5 cartons of 12 at 360 DA HT: 60 bottles at 30 DA; 1 800 DA HT + 9 % = 1 962 DA.
    assert_eq!(
        (l.qty, l.unit_price, l.total_ht, l.total_ttc),
        (Qty(60_000), Money(3_000), Money(180_000), Money(196_200))
    );
    assert_eq!(done.cost_changes[0].cost_after, Money(3_000));
    let history = shop.db.read(move |c| cost_history(c, &w.id, 10)).unwrap();
    assert_eq!(
        (history.len(), history[0].source_number.as_deref()),
        (1, Some("RC2026-000001"))
    );
}

#[test]
fn a_receipt_can_record_the_supplier_invoice_and_pay_it_from_the_till() {
    let shop = shop();
    let w = water(&shop);
    let s = shop.supplier("Ifri");
    shop.open_till(2_000_000);
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: None,
        location_id: None,
        lines: vec![line(&w.id, 60_000, 3_000)],
        supplier_ref: None,
        note: None,
        invoice: Some(InvoiceNow {
            supplier_number: "F-2026/0912".into(),
            payment: Some(PaymentNow {
                amount: Money(100_000),
                method: PaymentMethod::Cash,
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                from_till: true,
            }),
        }),
    };
    let ctx = shop.owner_till();
    let done = shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
    let invoice = done.invoice.unwrap();
    assert_eq!(
        (invoice.number.as_deref(), invoice.source_ref.as_deref()),
        (Some("FF2026-000001"), Some("F-2026/0912"))
    );
    assert_eq!(invoice.total_ttc, Money(196_200));
    let supplier = shop
        .db
        .read({
            let id = s.id.clone();
            move |c| crate::suppliers::get(c, &id)
        })
        .unwrap();
    assert_eq!(supplier.balance, Money(96_200));
    assert_eq!(
        shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'paid_out'"),
        -100_000
    );
}

#[test]
fn an_invoice_takes_each_receipt_once_and_each_supplier_number_once() {
    let shop = shop();
    let w = water(&shop);
    let s = shop.supplier("Ifri");
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: None,
        location_id: None,
        lines: vec![line(&w.id, 12_000, 3_000)],
        supplier_ref: None,
        note: None,
        invoice: None,
    };
    let ctx = shop.owner_till();
    let rc = shop
        .db
        .write(move |tx| receive(tx, &ctx, receipt))
        .unwrap()
        .receipt;
    let invoice =
        |number: &str, receipts: Vec<String>, lines: Vec<PurchaseLineInput>| InvoiceInput {
            supplier_id: s.id.clone(),
            supplier_number: number.into(),
            receipt_ids: receipts,
            lines,
            note: None,
            payment: None,
        };
    let ctx = shop.owner_till();
    let first = shop
        .db
        .write({
            let i = invoice("F-1", vec![rc.id.clone()], vec![]);
            move |tx| record_invoice(tx, &ctx, i)
        })
        .unwrap();
    assert_eq!(
        first.invoice.lines[0].origin_line_id.as_deref(),
        Some(rc.lines[0].id.as_str())
    );
    assert_eq!(first.entry.amount, first.invoice.total_ttc);
    let ctx = shop.owner_till();
    let twice = shop.db.write({
        let i = invoice("F-2", vec![rc.id.clone()], vec![]);
        move |tx| record_invoice(tx, &ctx, i)
    });
    assert_eq!(
        twice.unwrap_err(),
        AppError::validation("receiptIds.0", "receipt_already_invoiced")
    );
    let transport = shop.product_with({
        let mut i = crate::catalog::products::ProductInput::simple(
            "Transport",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(0),
        );
        i.kind = crate::catalog::products::ProductKind::Service;
        i.track_stock = false;
        i
    });
    let ctx = shop.owner_till();
    let same_number = shop.db.write({
        let i = invoice("F-1", vec![], vec![line(&transport.id, 1_000, 150_000)]);
        move |tx| record_invoice(tx, &ctx, i)
    });
    assert_eq!(
        same_number.unwrap_err(),
        AppError::validation("supplierNumber", "supplier_invoice_duplicate")
    );
}

#[test]
fn a_supplier_return_takes_stock_out_credits_the_supplier_and_stays_out_of_the_tickets() {
    let shop = shop();
    let w = water(&shop);
    let s = shop.supplier("Ifri");
    let ctx = shop.owner_till();
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: None,
        location_id: None,
        lines: vec![line(&w.id, 24_000, 3_000)],
        supplier_ref: None,
        note: None,
        invoice: Some(InvoiceNow {
            supplier_number: "F-9".into(),
            payment: None,
        }),
    };
    shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
    let input = SupplierReturnInput {
        supplier_id: s.id.clone(),
        location_id: None,
        lines: vec![SupplierReturnLine {
            product_id: w.id.clone(),
            qty: Qty(6_000),
            unit_cost: None,
            lot_number: None,
            serials: Vec::new(),
            note: Some("bouteilles fuyantes".into()),
        }],
        credit_supplier: true,
        note: None,
    };
    let ctx = shop.owner_till();
    let done = shop
        .db
        .write(move |tx| return_to_supplier(tx, &ctx, input))
        .unwrap();
    assert_eq!(done.document.number.as_deref(), Some("BR2026-F-000001"));
    assert_eq!(shop.count("SELECT SUM(qty) FROM stock_levels"), 18_000);
    // 24 bottles invoiced 784.80 DA TTC; 6 returned at the 30 DA CMUP + 9 % = 196.20 DA: 588.60 DA left.
    let supplier = shop
        .db
        .read({
            let id = s.id.clone();
            move |c| crate::suppliers::get(c, &id)
        })
        .unwrap();
    assert_eq!(supplier.balance, Money(58_860));
    // Every filter empty (the M1 history's "all types"); `limit` is the one field M1a requires.
    let all: DocumentQuery = serde_json::from_value(serde_json::json!({ "limit": 50 })).unwrap();
    let tickets = shop.db.read(move |c| documents::list(c, &all)).unwrap();
    assert!(
        tickets.is_empty(),
        "purchase documents never show in the tickets history"
    );
}

#[test]
fn an_invoice_refuses_the_same_receipt_listed_twice() {
    let shop = shop();
    let w = water(&shop);
    let s = shop.supplier("Ifri");
    let ctx = shop.owner_till();
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: None,
        location_id: None,
        lines: vec![line(&w.id, 12_000, 3_000)],
        supplier_ref: None,
        note: None,
        invoice: None,
    };
    let rc = shop
        .db
        .write(move |tx| receive(tx, &ctx, receipt))
        .unwrap()
        .receipt;
    let input = InvoiceInput {
        supplier_id: s.id.clone(),
        supplier_number: "F-1".into(),
        receipt_ids: vec![rc.id.clone(), rc.id.clone()],
        lines: Vec::new(),
        note: None,
        payment: None,
    };
    let ctx = shop.owner_till();
    let twice = shop.db.write(move |tx| record_invoice(tx, &ctx, input));
    assert_eq!(
        twice.unwrap_err(),
        AppError::validation("receiptIds.1", "receipt_already_invoiced")
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM documents WHERE doc_type = 'supplier_invoice'"),
        0
    );
}

#[test]
fn a_pack_line_is_priced_on_its_packs_so_it_matches_the_supplier_document() {
    let shop = shop();
    let w = water(&shop);
    let s = shop.supplier("Ifri");
    // 5 cartons of 12 at 1 000 DA HT: 5 000 DA HT, whatever 1 000 / 12 rounds to.
    let mut cartons = line(&w.id, 5_000, 100_000);
    cartons.pack_qty = Some(Qty(12_000));
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: None,
        location_id: None,
        lines: vec![cartons],
        supplier_ref: None,
        note: None,
        invoice: Some(InvoiceNow {
            supplier_number: "F-12".into(),
            payment: None,
        }),
    };
    let ctx = shop.owner_till();
    let done = shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
    let l = &done.receipt.lines[0];
    // 60 bottles shown at 83.33 DA each; 5 000 DA HT + 9 % = 5 450 DA.
    assert_eq!(
        (l.qty, l.unit_price, l.total_ht, l.tax_amount, l.total_ttc),
        (
            Qty(60_000),
            Money(8_333),
            Money(500_000),
            Money(45_000),
            Money(545_000)
        )
    );
    assert_eq!(done.cost_changes[0].cost_after, Money(8_333));
    let invoice = done.invoice.unwrap();
    assert_eq!(
        (invoice.subtotal_ht, invoice.tax_total, invoice.total_ttc),
        (Money(500_000), Money(45_000), Money(545_000))
    );
    let supplier = shop
        .db
        .read({
            let id = s.id.clone();
            move |c| crate::suppliers::get(c, &id)
        })
        .unwrap();
    assert_eq!(supplier.balance, Money(545_000));
}

#[test]
fn the_order_status_follows_each_order_line_not_the_total_quantity() {
    let shop = shop();
    let w = water(&shop);
    let oil = shop.product("Huile Elio 5 L", Money(65_000), &shop.tva19);
    let s = shop.supplier("Grossiste Bab Ezzouar");
    let order = OrderInput {
        supplier_id: s.id.clone(),
        lines: vec![line(&w.id, 120_000, 3_000), line(&oil.id, 10_000, 52_000)],
        supplier_ref: None,
        note: None,
    };
    let ctx = shop.owner_till();
    let draft = shop
        .db
        .write(move |tx| save_order(tx, &ctx, None, order))
        .unwrap();
    let ctx = shop.owner_till();
    let id = draft.id.clone();
    shop.db
        .write(move |tx| validate_order(tx, &ctx, &id))
        .unwrap();
    let status = || {
        shop.db
            .read({
                let id = draft.id.clone();
                move |c| order_status(c, &id)
            })
            .unwrap()
    };
    let rest = || {
        shop.db
            .read({
                let id = draft.id.clone();
                move |c| remaining(c, &id)
            })
            .unwrap()
    };
    let lines = rest();
    // 130 bottles against the 120 ordered, and 10 oils the receipt does not tie to their line.
    let mut over = line(&w.id, 130_000, 3_000);
    over.origin_line_id = Some(lines[0].line.id.clone());
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: Some(draft.id.clone()),
        location_id: None,
        lines: vec![over, line(&oil.id, 10_000, 52_000)],
        supplier_ref: None,
        note: None,
        invoice: None,
    };
    let ctx = shop.owner_till();
    shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
    assert_eq!(
        rest().iter().map(|r| r.remaining.0).collect::<Vec<_>>(),
        vec![0, 10_000]
    );
    assert_eq!(status(), OrderStatus::Partial, "the oil line is still due");
    let q = PurchaseQuery {
        doc_type: Some(DocType::PurchaseOrder),
        ..PurchaseQuery::default()
    };
    let listed = shop.db.read(move |c| list(c, &q)).unwrap();
    assert_eq!(listed[0].order_status, Some(OrderStatus::Partial));
    let mut oils = line(&oil.id, 10_000, 52_000);
    oils.origin_line_id = Some(lines[1].line.id.clone());
    let receipt = ReceiptInput {
        supplier_id: s.id.clone(),
        order_id: Some(draft.id.clone()),
        location_id: None,
        lines: vec![oils],
        supplier_ref: None,
        note: None,
        invoice: None,
    };
    let ctx = shop.owner_till();
    shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
    assert_eq!(status(), OrderStatus::Received);
}

fn track(shop: &Shop, product_id: &str, lots: bool, serials: bool) {
    let actor = shop.owner.actor();
    let id = product_id.to_owned();
    shop.db
        .write(move |tx| {
            crate::catalog::extras::set(
                tx,
                &actor,
                &id,
                crate::catalog::extras::ProductExtrasInput {
                    track_lots: lots,
                    track_serials: serials,
                    ..crate::catalog::extras::ProductExtrasInput::default()
                },
            )
        })
        .unwrap();
}

fn depot(shop: &Shop) -> String {
    let actor = shop.owner.actor();
    let input = crate::stock::LocationInput {
        code: "DEPOT".into(),
        name_fr: "Dépôt".into(),
        ..crate::stock::LocationInput::default()
    };
    shop.db
        .write(move |tx| crate::stock::create_location(tx, &actor, input))
        .unwrap()
        .id
}

fn bring(shop: &Shop, supplier: &str, location: Option<&str>, lines: Vec<PurchaseLineInput>) {
    let receipt = ReceiptInput {
        supplier_id: supplier.into(),
        order_id: None,
        location_id: location.map(str::to_owned),
        lines,
        supplier_ref: None,
        note: None,
        invoice: None,
    };
    let ctx = shop.owner_till();
    shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
}

fn give_back(
    shop: &Shop,
    supplier: &str,
    product: &str,
    qty: i64,
    lot: Option<&str>,
    serials: &[&str],
) -> Result<super::returns::SupplierReturnDone, AppError> {
    let input = SupplierReturnInput {
        supplier_id: supplier.into(),
        location_id: None,
        lines: vec![SupplierReturnLine {
            product_id: product.into(),
            qty: Qty(qty),
            unit_cost: None,
            lot_number: lot.map(str::to_owned),
            serials: serials.iter().map(|s| (*s).to_owned()).collect(),
            note: None,
        }],
        credit_supplier: false,
        note: None,
    };
    let ctx = shop.owner_till();
    shop.db.write(move |tx| return_to_supplier(tx, &ctx, input))
}

#[test]
fn a_return_from_a_named_lot_takes_only_what_that_lot_holds_at_that_location() {
    let shop = shop();
    let syrup = shop.product("Sirop Toplexil", Money(40_000), &shop.tva9);
    track(&shop, &syrup.id, true, false);
    let s = shop.supplier("Biopharm");
    let depot = depot(&shop);
    let lotted = |lot: &str, qty: i64| {
        let mut l = line(&syrup.id, qty, 20_000);
        l.lot_number = Some(lot.into());
        l.expiry_date = Some("2026-01-31".into());
        l
    };
    bring(
        &shop,
        &s.id,
        None,
        vec![lotted("L1", 10_000), lotted("L3", 10_000)],
    );
    bring(&shop, &s.id, Some(&depot), vec![lotted("L2", 5_000)]);
    // The shop holds 20 of the product, so only the lot can refuse.
    shop.set("stock.negative_policy", serde_json::json!("block"));
    let more = give_back(&shop, &s.id, &syrup.id, 12_000, Some("L1"), &[]).unwrap_err();
    let payload = more.payload();
    assert_eq!(payload.code, "stock_insufficient");
    let details = payload.details.unwrap();
    let shortage = &details["shortages"][0];
    assert_eq!(
        (shortage["available"].as_i64(), shortage["needed"].as_i64()),
        (Some(10_000), Some(12_000))
    );
    let elsewhere = give_back(&shop, &s.id, &syrup.id, 1_000, Some("L2"), &[]);
    assert_eq!(
        elsewhere.unwrap_err(),
        AppError::validation("lines.0.lotNumber", "lot_not_found"),
        "L2 was never in the shop"
    );
    let unknown = give_back(&shop, &s.id, &syrup.id, 1_000, Some("L9"), &[]);
    assert_eq!(
        unknown.unwrap_err(),
        AppError::validation("lines.0.lotNumber", "lot_not_found")
    );
    give_back(&shop, &s.id, &syrup.id, 4_000, Some("L1"), &[]).unwrap();
    assert_eq!(
        shop.count(
            "SELECT ll.qty FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
             WHERE l.lot_number = 'L1' AND ll.location_id = '00000000-0000-7000-8000-000000000301'"
        ),
        6_000
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM lot_levels WHERE qty < 0"),
        0
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM lot_levels WHERE location_id = '00000000-0000-7000-8000-000000000301'"),
        2,
        "no L2 row appeared in the shop"
    );
}

#[test]
fn a_return_sends_back_one_serial_per_piece_from_that_location() {
    let shop = shop();
    let phone = shop.product("Redmi Note 13", Money(4_500_000), &shop.tva19);
    track(&shop, &phone.id, false, true);
    let s = shop.supplier("Condor");
    let depot = depot(&shop);
    let phones = |qty: i64, serials: &[&str]| {
        let mut l = line(&phone.id, qty, 3_800_000);
        l.serials = serials.iter().map(|s| (*s).to_owned()).collect();
        l
    };
    bring(
        &shop,
        &s.id,
        None,
        vec![phones(3_000, &["IMEI-A1", "IMEI-A2", "IMEI-A3"])],
    );
    bring(
        &shop,
        &s.id,
        Some(&depot),
        vec![phones(1_000, &["IMEI-D1"])],
    );
    let too_many = give_back(
        &shop,
        &s.id,
        &phone.id,
        1_000,
        None,
        &["IMEI-A1", "IMEI-A2"],
    );
    assert_eq!(
        too_many.unwrap_err(),
        AppError::validation("lines.0.serials", "serials_count_mismatch")
    );
    let none = give_back(&shop, &s.id, &phone.id, 1_000, None, &[]);
    assert_eq!(
        none.unwrap_err(),
        AppError::validation("lines.0.serials", "serials_count_mismatch")
    );
    let other_place = give_back(&shop, &s.id, &phone.id, 1_000, None, &["IMEI-D1"]);
    assert_eq!(
        other_place.unwrap_err(),
        AppError::validation("lines.0.serials", "serial_not_at_location")
    );
    give_back(
        &shop,
        &s.id,
        &phone.id,
        2_000,
        None,
        &["IMEI-A1", "IMEI-A2"],
    )
    .unwrap();
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM serials WHERE status = 'returned_to_supplier'"),
        2
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM serials WHERE status = 'in_stock'"),
        2,
        "IMEI-A3 in the shop and IMEI-D1 in the depot"
    );
    assert_eq!(
        shop.count("SELECT qty FROM stock_levels WHERE location_id = '00000000-0000-7000-8000-000000000301'"),
        1_000
    );
}
