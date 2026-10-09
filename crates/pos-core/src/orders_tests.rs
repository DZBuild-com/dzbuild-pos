//! Customer orders, layaway and appointments (Task 12).

use crate::appointments::{self as ap, AppointmentInput, AppointmentStatus};
use crate::billing::convert::{ConvertInput, convert};
use crate::billing::drafts::{DraftInput, save_draft};
use crate::billing::validate::{ValidateInput, validate};
use crate::deposits::{self, OwnerType};
use crate::documents::DocType;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::orders::{
    self, CustomerOrderInput, CustomerOrderStatus, DeliverInput, FulfilInput, OrderDeposit,
    OrderKind,
};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, create_sale_with};
use crate::testkit::{Shop, shop};
use serde_json::json;

fn furniture() -> (Shop, String, String) {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Orders, Module::Appointments]))
        .unwrap();
    shop.set("stock.negative_policy", json!("block"));
    shop.open_till(100_000);
    let sofa = shop
        .product("Canapé 3 places", Money(6_000_000), &shop.tva19)
        .id;
    shop.stock_in(&sofa, 5_000);
    let customer = shop.customer("Nadia Hamdi").id;
    (shop, sofa, customer)
}

fn cash(amount: i64) -> OrderDeposit {
    OrderDeposit {
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }
}

fn order(
    shop: &Shop,
    sofa: &str,
    customer: &str,
    kind: OrderKind,
    qty: i64,
    deposit: Option<OrderDeposit>,
) -> Result<orders::Order, crate::error::AppError> {
    let ctx = shop.owner_till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(sofa, Qty(qty))]);
    cart.customer_id = Some(customer.into());
    let input = CustomerOrderInput {
        draft_id: None,
        cart,
        kind,
        due_on: Some("2026-10-15".into()),
        reserve: true,
        deposit,
        note: None,
    };
    shop.db.write(move |tx| orders::create(tx, &ctx, input))
}

fn voucher(number: &str, amount: i64) -> PaymentInput {
    PaymentInput {
        method: PaymentMethod::Voucher,
        amount: Money(amount),
        reference: Some(number.into()),
        cheque_bank: None,
        cheque_date: None,
    }
}

#[test]
fn reserved_stock_is_not_available_to_other_sales() {
    let (shop, sofa, customer) = furniture();
    let o = order(
        &shop,
        &sofa,
        &customer,
        OrderKind::Layaway,
        4_000,
        Some(cash(5_000_000)),
    )
    .unwrap();
    assert_eq!(o.reserved[0].qty, Qty(4_000));
    let sell = |qty: i64| {
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&sofa, Qty(qty))]),
            payments: vec![PaymentInput::electronic(
                PaymentMethod::CardCib,
                Money(6_000_000 * qty / 1_000),
                Some("A1"),
            )],
        };
        shop.db
            .write(move |tx| create_sale_with(tx, &ctx, input, &SaleExtras::default()))
    };
    assert_eq!(sell(2_000).unwrap_err().code(), "stock_insufficient");
    sell(1_000).unwrap();
    let (ctx, id) = (shop.owner_till(), o.id.clone());
    let err = shop
        .db
        .write(move |tx| orders::close(tx, &ctx, &id, Some("Client injoignable".into())))
        .unwrap_err();
    assert_eq!(
        err.code(),
        "deposit_held",
        "the deposit is refunded or kept as a sale first"
    );
}

#[test]
fn a_layaway_is_sold_only_when_fully_paid() {
    let (shop, sofa, customer) = furniture();
    let low = order(
        &shop,
        &sofa,
        &customer,
        OrderKind::Layaway,
        1_000,
        Some(cash(500_000)),
    )
    .unwrap_err();
    assert_eq!(
        low.code(),
        "layaway_deposit_too_low",
        "20 % of 60 000 DA is 12 000 DA"
    );
    let o = order(
        &shop,
        &sofa,
        &customer,
        OrderKind::Layaway,
        1_000,
        Some(cash(1_500_000)),
    )
    .unwrap();
    assert_eq!(
        (o.number.as_str(), o.held, o.status),
        ("CC2026-000001", Money(1_500_000), CustomerOrderStatus::Open)
    );
    let fulfil = |payments: Vec<PaymentInput>| {
        let ctx = shop.owner_till();
        let input = FulfilInput {
            id: o.id.clone(),
            payments,
            extras: SaleExtras::default(),
        };
        shop.db.write(move |tx| orders::fulfil(tx, &ctx, input))
    };
    let credit = PaymentInput {
        method: PaymentMethod::Credit,
        amount: Money(4_500_000),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    };
    assert_eq!(
        fulfil(vec![voucher("CC2026-000001", 1_500_000), credit])
            .unwrap_err()
            .code(),
        "layaway_not_paid"
    );
    let (ctx, id) = (shop.owner_till(), o.id.clone());
    shop.db
        .write(move |tx| orders::pay_in(tx, &ctx, &id, cash(4_500_000)))
        .unwrap();
    assert_eq!(
        fulfil(vec![voucher("CC2026-000001", 6_000_001)])
            .unwrap_err()
            .code(),
        "deposit_insufficient"
    );
    let done = fulfil(vec![voucher("CC2026-000001", 6_000_000)]).unwrap();
    assert_eq!(
        (done.order.status, done.order.held),
        (CustomerOrderStatus::Fulfilled, Money::ZERO)
    );
    assert_eq!(
        done.sale.document.stamp_duty,
        Money::ZERO,
        "the duty was paid on the deposits' cash"
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM stock_reservations WHERE released_at IS NULL"),
        0
    );
    let s = sofa.clone();
    assert_eq!(
        shop.db.read(move |c| crate::stock::on_hand(c, &s)).unwrap(),
        Qty(4_000)
    );
    let (ctx, id) = (shop.owner_till(), o.id.clone());
    assert_eq!(
        shop.db
            .write(move |tx| orders::close(tx, &ctx, &id, None))
            .unwrap_err()
            .code(),
        "order_status_invalid"
    );
}

#[test]
fn an_order_keeps_its_price_and_a_changed_price_needs_the_override() {
    let (shop, sofa, customer) = furniture();
    let o = order(
        &shop,
        &sofa,
        &customer,
        OrderKind::Order,
        1_000,
        Some(cash(1_000_000)),
    )
    .unwrap();
    let s = sofa.clone();
    shop.db
        .write(move |tx| {
            Ok(tx.execute(
                "UPDATE products SET sell_price = 6500000 WHERE id = ?1",
                [&s],
            )?)
        })
        .unwrap();
    let ctx = shop.till();
    let input = FulfilInput {
        id: o.id.clone(),
        payments: vec![
            voucher("CC2026-000001", 1_000_000),
            PaymentInput::electronic(PaymentMethod::CardCib, Money(5_000_000), Some("A2")),
        ],
        extras: SaleExtras::default(),
    };
    let err = shop
        .db
        .write(move |tx| orders::fulfil(tx, &ctx, input))
        .unwrap_err();
    assert_eq!(err.code(), "price_override_not_allowed");
    let ctx = shop.owner_till();
    let input = FulfilInput {
        id: o.id.clone(),
        payments: vec![
            voucher("CC2026-000001", 1_000_000),
            PaymentInput::electronic(PaymentMethod::CardCib, Money(5_000_000), Some("A2")),
        ],
        extras: SaleExtras::default(),
    };
    let done = shop
        .db
        .write(move |tx| orders::fulfil(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        done.sale.document.total_ttc,
        Money(6_000_000),
        "the ordered price holds"
    );
}

/// A quote converted into an order (M3's editor), then delivered on a BL. A delivery note posts
/// no debt in M3 (its invoice does), so the BL takes no tender and the deposit goes on the
/// customer's account as an advance the BL's invoice takes later.
#[test]
fn a_quote_becomes_an_order_then_a_delivery_note_with_its_deposit_on_the_account() {
    let (shop, sofa, customer) = furniture();
    let ctx = shop.owner_till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&sofa, Qty(2_000))]);
    cart.customer_id = Some(customer.clone());
    let quote = shop
        .db
        .write(move |tx| {
            let draft = save_draft(
                tx,
                &ctx,
                None,
                DraftInput {
                    doc_type: DocType::Quote,
                    cart,
                    valid_until: None,
                    due_date: None,
                    second_lang: None,
                    draft_id: None,
                },
            )?;
            Ok(validate(
                tx,
                &ctx,
                ValidateInput {
                    id: draft.id,
                    payments: Vec::new(),
                },
            )?
            .document)
        })
        .unwrap();
    let ctx = shop.owner_till();
    let q = quote.id.clone();
    let draft = shop
        .db
        .write(move |tx| {
            convert(
                tx,
                &ctx,
                ConvertInput {
                    source_id: q,
                    target: DocType::CustomerOrder,
                    reprice: false,
                },
            )
        })
        .unwrap();
    assert_eq!(draft.doc_type, DocType::CustomerOrder);
    let ctx = shop.owner_till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&sofa, Qty(2_000))]);
    cart.customer_id = Some(customer.clone());
    let deposit = OrderDeposit {
        amount: Money(2_000_000),
        method: PaymentMethod::Transfer,
        reference: Some("VIR 88213".into()),
        cheque_bank: None,
        cheque_date: None,
    };
    let input = CustomerOrderInput {
        draft_id: Some(draft.id.clone()),
        cart,
        kind: OrderKind::Order,
        due_on: None,
        reserve: true,
        deposit: Some(deposit),
        note: None,
    };
    let o = shop
        .db
        .write(move |tx| orders::create(tx, &ctx, input))
        .unwrap();
    let ctx = shop.owner_till();
    let credit = PaymentInput {
        method: PaymentMethod::Credit,
        amount: Money(12_000_000),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    };
    let input = DeliverInput {
        id: o.id.clone(),
        target: DocType::DeliveryNote,
        payments: vec![credit],
    };
    let err = shop
        .db
        .write(move |tx| orders::deliver(tx, &ctx, input))
        .unwrap_err();
    assert_eq!(err.code(), "value_invalid", "a BL takes no tender in M3");
    let ctx = shop.owner_till();
    let input = DeliverInput {
        id: o.id.clone(),
        target: DocType::DeliveryNote,
        payments: Vec::new(),
    };
    let bl = shop
        .db
        .write(move |tx| orders::deliver(tx, &ctx, input))
        .unwrap();
    assert_eq!(bl.document.doc_type, DocType::DeliveryNote);
    let c = customer.clone();
    assert_eq!(
        shop.db
            .read(move |conn| crate::customers::get(conn, &c))
            .unwrap()
            .balance,
        Money(-2_000_000),
        "the deposit waits on the account for the BL's invoice"
    );
    let id = o.id.clone();
    let after = shop.db.read(move |c| orders::get(c, &id)).unwrap();
    assert_eq!(
        (after.status, after.held, after.fulfilled_number),
        (
            CustomerOrderStatus::Fulfilled,
            Money::ZERO,
            bl.document.number.clone()
        )
    );
    let id = o.id.clone();
    assert_eq!(
        shop.db
            .read(move |c| deposits::held(c, OwnerType::CustomerOrder, &id))
            .unwrap(),
        Money::ZERO
    );
    let s = sofa.clone();
    assert_eq!(
        shop.db.read(move |c| crate::stock::on_hand(c, &s)).unwrap(),
        Qty(3_000),
        "the BL took the stock the order had reserved"
    );
}

/// The brief's figures on an invoice: 120 000 DA on credit, the 20 000 DA deposit settles part of
/// that invoice's debt.
#[test]
fn an_order_delivered_as_an_invoice_on_credit_takes_its_deposit_off_the_debt() {
    let (shop, sofa, _) = furniture();
    shop.complete_company();
    let customer = shop.consumer("Karim Bensalah").id;
    let o = order(
        &shop,
        &sofa,
        &customer,
        OrderKind::Order,
        2_000,
        Some(cash(2_000_000)),
    )
    .unwrap();
    let ctx = shop.owner_till();
    let credit = PaymentInput {
        method: PaymentMethod::Credit,
        amount: Money(12_000_000),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    };
    let input = DeliverInput {
        id: o.id.clone(),
        target: DocType::Invoice,
        payments: vec![credit],
    };
    let invoice = shop
        .db
        .write(move |tx| orders::deliver(tx, &ctx, input))
        .unwrap();
    assert_eq!(invoice.document.doc_type, DocType::Invoice);
    let c = customer.clone();
    assert_eq!(
        shop.db
            .read(move |conn| crate::customers::get(conn, &c))
            .unwrap()
            .balance,
        Money(10_000_000)
    );
    let c = customer.clone();
    let open = shop
        .db
        .read(move |conn| crate::ledger::open_debits(conn, crate::ledger::Side::Customer, &c))
        .unwrap();
    assert_eq!(
        open.iter().map(|e| e.open).collect::<Vec<_>>(),
        [Money(10_000_000)],
        "the deposit settled the invoice's own debt"
    );
    let id = o.id.clone();
    let after = shop.db.read(move |c| orders::get(c, &id)).unwrap();
    assert_eq!(
        (after.status, after.held, after.fulfilled_number),
        (
            CustomerOrderStatus::Fulfilled,
            Money::ZERO,
            invoice.document.number.clone()
        )
    );
}

#[test]
fn a_kept_deposit_is_sold_as_a_ticket_before_the_order_closes() {
    let (shop, sofa, customer) = furniture();
    let o = order(
        &shop,
        &sofa,
        &customer,
        OrderKind::Order,
        1_000,
        Some(cash(1_000_000)),
    )
    .unwrap();
    let (ctx, id) = (shop.owner_till(), o.id.clone());
    assert_eq!(
        shop.db
            .write(move |tx| orders::close(tx, &ctx, &id, None))
            .unwrap_err()
            .code(),
        "deposit_held"
    );
    let (ctx, id) = (shop.owner_till(), o.id.clone());
    let kept = shop
        .db
        .write(move |tx| orders::keep_deposit(tx, &ctx, &id, Some("Le client a renoncé".into())))
        .unwrap();
    assert_eq!(
        (kept.order.status, kept.order.held),
        (CustomerOrderStatus::Closed, Money::ZERO)
    );
    assert_eq!(
        (kept.sale.document.total_ttc, kept.sale.document.lines.len()),
        (Money(1_000_000), 1)
    );
    assert_eq!(kept.sale.document.lines[0].description, "Arrhes conservées");
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM stock_reservations WHERE released_at IS NULL"),
        0
    );
    let s = sofa.clone();
    assert_eq!(
        shop.db.read(move |c| crate::stock::on_hand(c, &s)).unwrap(),
        Qty(5_000),
        "no sofa left the shop"
    );
}

/// CRD-008 holds on a delivery too: no credit on a layaway's invoice, deposits covering its BL.
#[test]
fn a_layaway_is_delivered_only_when_fully_paid() {
    let (shop, sofa, customer) = furniture();
    let o = order(
        &shop,
        &sofa,
        &customer,
        OrderKind::Layaway,
        1_000,
        Some(cash(1_500_000)),
    )
    .unwrap();
    let deliver = |target: DocType, payments: Vec<PaymentInput>| {
        let ctx = shop.owner_till();
        let input = DeliverInput {
            id: o.id.clone(),
            target,
            payments,
        };
        shop.db.write(move |tx| orders::deliver(tx, &ctx, input))
    };
    let credit = PaymentInput {
        method: PaymentMethod::Credit,
        amount: Money(4_500_000),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    };
    assert_eq!(
        deliver(DocType::Invoice, vec![credit]).unwrap_err().code(),
        "layaway_not_paid"
    );
    assert_eq!(
        deliver(DocType::DeliveryNote, Vec::new())
            .unwrap_err()
            .code(),
        "layaway_not_paid"
    );
    let (ctx, id) = (shop.owner_till(), o.id.clone());
    shop.db
        .write(move |tx| orders::pay_in(tx, &ctx, &id, cash(4_500_000)))
        .unwrap();
    let bl = deliver(DocType::DeliveryNote, Vec::new()).unwrap();
    assert_eq!(bl.document.total_due, Money(6_000_000));
    let id = o.id.clone();
    let after = shop.db.read(move |c| orders::get(c, &id)).unwrap();
    assert_eq!(
        (after.status, after.held),
        (CustomerOrderStatus::Fulfilled, Money::ZERO)
    );
}

#[test]
fn a_staff_member_is_never_booked_twice_at_the_same_time() {
    let (shop, _, customer) = furniture();
    let staff = shop.owner.user_id.clone();
    let at = |h: i64, m: i64| crate::testkit::NOON - 12 * 3_600_000 + (h * 60 + m) * 60_000;
    let book = |start: i64, minutes: u16| {
        let ctx = shop.owner_till();
        let input = AppointmentInput {
            customer_id: Some(customer.clone()),
            customer_name: "Nadia Hamdi".into(),
            phone: Some("0555 12 34 56".into()),
            service_product_id: None,
            staff_user_id: Some(staff.clone()),
            starts_at: start,
            duration_min: minutes,
            note: None,
        };
        shop.db.write(move |tx| ap::book(tx, &ctx, input))
    };
    let first = book(at(10, 0), 45).unwrap();
    assert_eq!(
        book(at(10, 30), 30).unwrap_err().code(),
        "appointment_overlap"
    );
    book(at(10, 45), 30).unwrap();
    let (ctx, id) = (shop.owner_till(), first.id.clone());
    shop.db
        .write(move |tx| ap::set_status(tx, &ctx, &id, AppointmentStatus::Cancelled))
        .unwrap();
    book(at(10, 0), 30).unwrap();
    assert_eq!(
        book(at(10, 0), 3).unwrap_err().code(),
        "appointment_time_invalid"
    );
    let day = shop.db.read(|c| ap::day(c, "2026-09-28")).unwrap();
    assert_eq!(day.len(), 3);
}
