//! Task 12 review: an order delivered as an invoice spends its deposit once — never through a
//! voucher tender (M3's invoice validation books no voucher), and a layaway paid in full by its
//! deposits is invoiced with a credit part the deposits settle.

use crate::documents::DocType;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::orders::{
    self, CustomerOrderInput, CustomerOrderStatus, DeliverInput, OrderDeposit, OrderKind,
};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine};
use crate::testkit::{Shop, shop};
use serde_json::json;

/// A shop selling a 60 000 DA sofa, ready to invoice, and a consumer customer.
fn invoicing_shop() -> (Shop, String, String) {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Orders]))
        .unwrap();
    shop.set("stock.negative_policy", json!("block"));
    shop.complete_company();
    shop.open_till(100_000);
    let sofa = shop
        .product("Canapé 3 places", Money(6_000_000), &shop.tva19)
        .id;
    shop.stock_in(&sofa, 5_000);
    let customer = shop.consumer("Karim Bensalah").id;
    (shop, sofa, customer)
}

fn tender(method: PaymentMethod, amount: i64, reference: Option<&str>) -> PaymentInput {
    PaymentInput {
        method,
        amount: Money(amount),
        reference: reference.map(Into::into),
        cheque_bank: None,
        cheque_date: None,
    }
}

fn cash_deposit(amount: i64) -> OrderDeposit {
    OrderDeposit {
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }
}

fn place(shop: &Shop, sofa: &str, customer: &str, kind: OrderKind) -> orders::Order {
    let ctx = shop.owner_till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(sofa, Qty(1_000))]);
    cart.customer_id = Some(customer.into());
    let input = CustomerOrderInput {
        draft_id: None,
        cart,
        kind,
        due_on: Some("2026-10-15".into()),
        reserve: true,
        deposit: Some(cash_deposit(1_500_000)),
        note: None,
    };
    shop.db
        .write(move |tx| orders::create(tx, &ctx, input))
        .unwrap()
}

fn deliver(
    shop: &Shop,
    id: &str,
    payments: Vec<PaymentInput>,
) -> Result<crate::billing::validate::ValidatedDocument, crate::error::AppError> {
    let ctx = shop.owner_till();
    let input = DeliverInput {
        id: id.into(),
        target: DocType::Invoice,
        payments,
    };
    shop.db.write(move |tx| orders::deliver(tx, &ctx, input))
}

fn balance(shop: &Shop, customer: &str) -> Money {
    let c = customer.to_owned();
    shop.db
        .read(move |conn| crate::customers::get(conn, &c))
        .unwrap()
        .balance
}

/// A layaway its deposits paid in full is invoiced with a credit part equal to them: the deposits
/// settle exactly that debt, nothing is left owed nor held, and no advance appears.
#[test]
fn a_layaway_paid_by_its_deposits_is_invoiced_on_credit_the_deposits_settle() {
    let (shop, sofa, customer) = invoicing_shop();
    let o = place(&shop, &sofa, &customer, OrderKind::Layaway);
    let (ctx, id) = (shop.owner_till(), o.id.clone());
    shop.db
        .write(move |tx| orders::pay_in(tx, &ctx, &id, cash_deposit(4_500_000)))
        .unwrap();
    assert_eq!(
        deliver(
            &shop,
            &o.id,
            vec![tender(PaymentMethod::Credit, 6_000_001, None)]
        )
        .unwrap_err()
        .code(),
        "layaway_not_paid",
        "the credit part never exceeds the deposits held"
    );
    let invoice = deliver(
        &shop,
        &o.id,
        vec![tender(PaymentMethod::Credit, 6_000_000, None)],
    )
    .unwrap();
    assert_eq!(
        (invoice.document.doc_type, invoice.document.total_due),
        (DocType::Invoice, Money(6_000_000))
    );
    assert_eq!(balance(&shop, &customer), Money::ZERO);
    let c = customer.clone();
    let open = shop
        .db
        .read(move |conn| crate::ledger::open_debits(conn, crate::ledger::Side::Customer, &c))
        .unwrap();
    assert!(open.is_empty(), "the deposits settled the invoice's debt");
    let id = o.id.clone();
    let after = shop.db.read(move |c| orders::get(c, &id)).unwrap();
    assert_eq!(
        (after.status, after.held),
        (CustomerOrderStatus::Fulfilled, Money::ZERO)
    );
}

/// A voucher naming a deposit (`CC…`, `RP…`) is never a tender of a delivered invoice: M3's
/// validation would accept it without spending anything, and the deposit would then be applied a
/// second time on the account. Nothing is written.
#[test]
fn an_invoice_delivery_refuses_a_voucher_naming_a_deposit() {
    let (shop, sofa, customer) = invoicing_shop();
    let o = place(&shop, &sofa, &customer, OrderKind::Order);
    for reference in [o.number.as_str(), "RP2026-000001"] {
        let err = deliver(
            &shop,
            &o.id,
            vec![
                tender(PaymentMethod::Voucher, 1_500_000, Some(reference)),
                tender(PaymentMethod::Cash, 5_000_000, None),
            ],
        )
        .unwrap_err();
        assert_eq!(
            (err.code(), err.field()),
            ("value_invalid".to_owned(), Some("payments.0.reference")),
            "{reference}"
        );
    }
    let id = o.id.clone();
    let after = shop.db.read(move |c| orders::get(c, &id)).unwrap();
    assert_eq!(
        (after.status, after.held),
        (CustomerOrderStatus::Open, Money(1_500_000))
    );
    assert_eq!(balance(&shop, &customer), Money::ZERO);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM documents WHERE doc_type = 'invoice'"),
        0
    );
}
