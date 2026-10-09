//! Cash sessions, sales, held tickets and returns through the commands.

use super::auth::{self, OverrideInput};
use super::testshop::{OWNER_PIN, file_printer, shop, shop_in, wait_for_bytes};
use super::{cash as cash_cmd, returns as returns_cmd, sales as sales_cmd};
use pos_core::auth::Permission;
use pos_core::cash::CloseInput;
use pos_core::cash::closing::CountLine;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty, Rate};
use pos_core::payments::PaymentInput;
use pos_core::payments::PaymentMethod;
use pos_core::returns::{ReturnInput, ReturnLineInput, ReturnReason};
use pos_core::sales::held::CartEventInput;
use pos_core::sales::{Cart, CartLine, SaleInput};
use pos_core::tax::tva::Discount;

fn cart(product_id: &str, pieces: i64) -> Cart {
    Cart::new(
        Lang::Fr,
        vec![CartLine::product(product_id, Qty(pieces * 1_000))],
    )
}

fn approve(shop: &super::testshop::TestShop, permission: Permission) {
    auth::manager_override(
        &shop.st,
        &OverrideInput {
            approver_id: shop.owner.user_id.clone(),
            pin: OWNER_PIN.into(),
            permission,
        },
    )
    .unwrap();
}

#[test]
fn a_sale_needs_an_open_session() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    let request = sales_cmd::SaleRequest {
        sale: SaleInput {
            cart: cart(&milk.id, 1),
            payments: vec![PaymentInput::cash(Money(15_000))],
        },
        print_receipt: false,
        extras: Default::default(),
    };
    assert_eq!(
        sales_cmd::create(&shop.st, request).unwrap_err().code(),
        "cash_session_closed"
    );
    assert!(cash_cmd::current(&shop.st).unwrap().is_none());
}

#[test]
fn the_preview_and_the_sale_agree_on_duty_and_change() {
    let shop = shop();
    let tv = shop.product("Téléviseur 32\"", 100_000, 1_900, None);
    shop.cashier();
    cash_cmd::open_session(&shop.st, Money(500_000)).unwrap();
    let tenders = vec![PaymentInput::cash(Money(120_000))];
    let preview = sales_cmd::preview(
        &shop.st,
        sales_cmd::PreviewInput {
            cart: cart(&tv.id, 1),
            tenders: tenders.clone(),
            store_credit: None,
        },
    )
    .unwrap();
    // 1 000 DA in cash: 10 tranches of 100 DA → 10 DA of duty (spec §6)
    assert_eq!(preview.settlement.stamp_duty, Money(1_000));
    assert_eq!(preview.settlement.total_due, Money(101_000));
    assert_eq!(preview.settlement.change, Money(19_000));
    assert!(
        preview
            .cart
            .lines
            .iter()
            .all(|l| l.cost_price == Money::ZERO),
        "no cost at the till"
    );
    let done = sales_cmd::create(
        &shop.st,
        sales_cmd::SaleRequest {
            sale: SaleInput {
                cart: cart(&tv.id, 1),
                payments: tenders,
            },
            print_receipt: false,
            extras: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(
        done.sale.settlement.stamp_duty,
        preview.settlement.stamp_duty
    );
    assert_eq!(done.sale.settlement.total_due, preview.settlement.total_due);
    assert_eq!(done.sale.settlement.change, preview.settlement.change);
    assert!(
        done.sale
            .document
            .number
            .as_deref()
            .is_some_and(|n| n.starts_with("TK"))
    );
    assert!(
        done.print_queued,
        "a cash sale queues the drawer kick even without a receipt (dropped silently without a printer)"
    );
}

#[test]
fn a_discount_above_the_limit_needs_a_fresh_approval_each_sale() {
    let shop = shop();
    let oil = shop.product("Huile Elio 5 L", 65_000, 1_900, None);
    shop.cashier();
    cash_cmd::open_session(&shop.st, Money(0)).unwrap();
    let discounted = || {
        let mut c = cart(&oil.id, 1);
        c.lines[0].discount = Discount::Percent(Rate(500));
        c
    };
    let preview = |c| {
        sales_cmd::preview(
            &shop.st,
            sales_cmd::PreviewInput {
                cart: c,
                tenders: vec![],
                store_credit: None,
            },
        )
    };
    assert_eq!(
        preview(discounted()).unwrap_err().code(),
        "discount_not_allowed"
    );
    approve(&shop, Permission::SalesDiscount);
    let priced = preview(discounted()).unwrap();
    assert_eq!(priced.cart.totals.discount_total, Money(3_250));
    let sell = |c| {
        sales_cmd::create(
            &shop.st,
            sales_cmd::SaleRequest {
                sale: SaleInput {
                    cart: c,
                    payments: vec![PaymentInput::cash(Money(70_000))],
                },
                print_receipt: false,
                extras: Default::default(),
            },
        )
    };
    sell(discounted()).unwrap();
    assert_eq!(
        sell(discounted()).unwrap_err().code(),
        "discount_not_allowed",
        "the approval was spent"
    );
}

#[test]
fn a_sale_commits_even_when_no_printer_is_configured() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    cash_cmd::open_session(&shop.st, Money(0)).unwrap();
    let done = sales_cmd::create(
        &shop.st,
        sales_cmd::SaleRequest {
            sale: SaleInput {
                cart: cart(&milk.id, 1),
                payments: vec![PaymentInput::cash(Money(20_000))],
            },
            print_receipt: true,
            extras: Default::default(),
        },
    )
    .unwrap();
    assert!(done.print_queued);
    assert!(
        done.sale.document.number.is_some(),
        "numbered and committed before any printing"
    );
}

#[test]
fn the_receipt_prints_after_the_sale_with_the_drawer() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _) = shop_in(dir.path());
    let out = dir.path().join("receipt.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    cash_cmd::open_session(&shop.st, Money(0)).unwrap();
    sales_cmd::create(
        &shop.st,
        sales_cmd::SaleRequest {
            sale: SaleInput {
                cart: cart(&milk.id, 2),
                payments: vec![PaymentInput::cash(Money(50_000))],
            },
            print_receipt: true,
            extras: Default::default(),
        },
    )
    .unwrap();
    let bytes = wait_for_bytes(&out, 200);
    assert!(bytes.windows(9).any(|w| w == b"Lait Cand"));
    assert!(
        bytes.ends_with(&[0x1B, b'p', 0, 25, 250]),
        "cash sale: the drawer opens after the cut"
    );
}

#[test]
fn held_carts_are_recalled_repriced_and_discarded() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    let held = sales_cmd::hold(&shop.st, "Table 3".into(), cart(&milk.id, 3)).unwrap();
    assert_eq!(sales_cmd::held(&shop.st).unwrap().len(), 1);
    let recalled = sales_cmd::recall(&shop.st, held.id).unwrap();
    assert_eq!(recalled.priced.totals.total_ttc, Money(45_000));
    assert!(sales_cmd::held(&shop.st).unwrap().is_empty());
    let again = sales_cmd::hold(&shop.st, "Ali".into(), cart(&milk.id, 1)).unwrap();
    // Throwing a held ticket away cancels it: a cashier without sales.void_line needs a manager.
    assert_eq!(
        sales_cmd::discard(&shop.st, again.id.clone())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    assert_eq!(sales_cmd::held(&shop.st).unwrap().len(), 1, "still held");
    approve(&shop, Permission::SalesVoidLine);
    sales_cmd::discard(&shop.st, again.id).unwrap();
    assert!(sales_cmd::held(&shop.st).unwrap().is_empty());
    assert!(
        shop.st.till(Permission::SalesVoidLine).is_err(),
        "the approval is spent"
    );
    let payload: String = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'sale.held_discarded'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["total"], serde_json::json!(15_000));
    assert_eq!(
        payload["approvals"],
        serde_json::json!([["sales.void_line", shop.owner.user_id]])
    );
}

#[test]
fn a_lowered_quantity_needs_void_line_and_is_journaled() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    let event = || CartEventInput::QtyReduced {
        cart: cart(&milk.id, 10),
        index: 0,
        qty: Qty(1_000),
    };
    assert_eq!(
        sales_cmd::record_event(&shop.st, event())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    approve(&shop, Permission::SalesVoidLine);
    sales_cmd::record_event(&shop.st, event()).unwrap();
    let payload: String = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'sale.qty_reduced'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        (
            payload["event"]["removedQty"].clone(),
            payload["event"]["amount"].clone()
        ),
        (serde_json::json!(9_000), serde_json::json!(135_000))
    );
}

#[test]
fn a_voided_line_needs_void_line_and_is_journaled() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    // The till sends the cart and the line; the name and the amount are the backend's.
    let event = || CartEventInput::LineVoided {
        cart: cart(&milk.id, 1),
        index: 0,
    };
    assert_eq!(
        sales_cmd::record_event(&shop.st, event())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    approve(&shop, Permission::SalesVoidLine);
    sales_cmd::record_event(&shop.st, event()).unwrap();
    assert!(sales_cmd::record_event(&shop.st, event()).is_err(), "spent");
    let payload: String = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'sale.line_voided'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        (
            payload["event"]["description"].as_str(),
            payload["event"]["amount"].as_i64()
        ),
        (Some("Lait Candia 1 L"), Some(15_000))
    );
}

#[test]
fn a_return_is_quoted_without_keeping_anything_then_refunded() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    cash_cmd::open_session(&shop.st, Money(100_000)).unwrap();
    let sale = sales_cmd::create(
        &shop.st,
        sales_cmd::SaleRequest {
            sale: SaleInput {
                cart: cart(&milk.id, 2),
                payments: vec![PaymentInput::cash(Money(30_000))],
            },
            print_receipt: false,
            extras: Default::default(),
        },
    )
    .unwrap()
    .sale;
    let lines = returns_cmd::returnable(&shop.st, sale.document.id.clone()).unwrap();
    assert_eq!(lines[0].remaining, Qty(2_000));
    let input = || ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(lines[0].line.id.clone()),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: true,
            reason: ReturnReason::ChangedMind,
            note: None,
        }],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    assert_eq!(
        returns_cmd::quote(&shop.st, input()).unwrap_err().code(),
        "insufficient_permission",
        "a cashier needs a manager"
    );
    approve(&shop, Permission::SalesReturn);
    let quote = returns_cmd::quote(&shop.st, input()).unwrap();
    assert_eq!(quote.value, Money(15_000));
    assert_eq!(
        returns_cmd::returnable(&shop.st, sale.document.id.clone()).unwrap()[0].remaining,
        Qty(2_000),
        "nothing kept"
    );
    let done = returns_cmd::create(
        &shop.st,
        returns_cmd::ReturnRequest {
            input: input(),
            print_receipt: false,
            extras: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(done.returned.refund, Money(15_000));
    assert!(
        done.returned
            .document
            .number
            .as_deref()
            .is_some_and(|n| n.starts_with("BR"))
    );
    assert_eq!(
        returns_cmd::returnable(&shop.st, sale.document.id).unwrap()[0].remaining,
        Qty(1_000)
    );
}

#[test]
fn a_large_variance_needs_a_manager_then_the_z_prints() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _) = shop_in(dir.path());
    let out = dir.path().join("z.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.cashier();
    let session = cash_cmd::open_session(&shop.st, Money(1_000_000)).unwrap();
    shop.sell(&milk.id, 1, 15_000);
    let x = cash_cmd::x_report(&shop.st).unwrap();
    assert_eq!(
        x.expected_cash, None,
        "a cashier without cash.view_expected sees no expected cash"
    );
    let short = CloseInput {
        counts: vec![CountLine {
            denomination: Money(200_000),
            count: 4,
        }],
        notes: None,
    };
    assert_eq!(
        cash_cmd::close_session(&shop.st, short.clone())
            .unwrap_err()
            .code(),
        "variance_needs_approval"
    );
    approve(&shop, Permission::CashViewExpected);
    let closed = cash_cmd::close_session(&shop.st, short).unwrap();
    assert!(closed.z.z_number.starts_with("ZC1-"));
    assert!(cash_cmd::current(&shop.st).unwrap().is_none());
    let printed = cash_cmd::print_z(&shop.st, session.id, Lang::Fr).unwrap();
    assert!(!printed.raster);
    let bytes = wait_for_bytes(&out, 50);
    assert!(bytes.windows(9).any(|w| w == b"Rapport Z"));
    assert!(
        bytes.windows(12).any(|w| w == b"Amine Benali"),
        "the approving manager by name, never the id"
    );
}
