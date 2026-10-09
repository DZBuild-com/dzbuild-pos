//! Task 24: the payment dialog shows the points the ticket earns an enrolled customer
//! (`sales_points_preview`, LOY-002), computed by the backend's rule, never by the webview.

use super::testshop::shop;
use super::{packs, sales};
use pos_core::auth::Permission;
use pos_core::money::Money;
use pos_core::packs::Module;
use pos_core::payments::{PaymentInput, PaymentMethod};

#[test]
fn the_cashier_sees_the_points_of_an_enrolled_customer() {
    let t = shop();
    let st = &t.st;
    packs::set_modules(st, vec![Module::Loyalty]).unwrap();
    t.open_till(50_000);
    let karim = t.b2b_customer("Karim Benali").id;
    let nadia = t.b2b_customer("Nadia Ouali").id;
    let ctx = st.till(Permission::SalesCreate).unwrap();
    let enrolled = karim.clone();
    st.db
        .write(move |tx| pos_core::loyalty::enrol(tx, &ctx, &enrolled, None))
        .unwrap();
    t.cashier();
    let voucher = PaymentInput {
        method: PaymentMethod::Voucher,
        amount: Money(300_000),
        reference: Some("BA2026-000001".into()),
        cheque_bank: None,
        cheque_date: None,
    };
    assert_eq!(
        sales::points_preview(st, karim.clone(), Money(2_345_600), vec![]).unwrap(),
        Some(234)
    );
    assert_eq!(
        sales::points_preview(st, karim, Money(2_345_600), vec![voucher]).unwrap(),
        Some(204)
    );
    assert_eq!(
        sales::points_preview(st, nadia, Money(2_345_600), vec![]).unwrap(),
        None
    );
}
