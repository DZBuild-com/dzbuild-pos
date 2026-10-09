//! One shift through the commands, in the order the screens drive them: open the till, hold a
//! cart, scan and sell with the receipt and the drawer, reprint in Arabic, recall the held cart
//! and pay it by card + cash, one ticket for each other tender, a refund and an exchange
//! approved by a manager, the X, a close whose count is off, the Z, and every chain (audit
//! journal, tickets, return notes, Z closures) still verifies. The same shift runs on a single
//! PC, on a main PC serving a till, on a till that sees its main PC and on a till whose main PC
//! is down (M5).

use super::auth::{self, OverrideInput};
use super::testshop::{OWNER_PIN, TestShop, file_printer, shop_in, wait_for_bytes};
use super::{
    audit as audit_cmd, cash as cash_cmd, documents, products, returns as returns_cmd,
    sales as sales_cmd,
};
use pos_core::audit::AuditFilter;
use pos_core::auth::Permission;
use pos_core::cash::CloseInput;
use pos_core::cash::closing::CountLine;
use pos_core::catalog::products::Product;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::returns::{ReturnInput, ReturnLineInput, ReturnReason};
use pos_core::sales::{Cart, CartLine, SaleInput};
use std::path::Path;
use std::time::Duration;

/// `ESC p 0 25 250`: the drawer on pin 2.
const DRAWER_KICK: [u8; 5] = [0x1B, b'p', 0, 25, 250];

fn approve(shop: &TestShop, permission: Permission) {
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

fn sale(
    lines: Vec<CartLine>,
    payments: Vec<PaymentInput>,
    print_receipt: bool,
) -> sales_cmd::SaleRequest {
    sales_cmd::SaleRequest {
        sale: SaleInput {
            cart: Cart::new(Lang::Fr, lines),
            payments,
        },
        print_receipt,
        extras: Default::default(),
    }
}

/// One piece of `line_id` back from `original_id`, refunded in cash.
fn one_back(original_id: &str, line_id: &str, reason: ReturnReason) -> ReturnInput {
    ReturnInput {
        original_id: Some(original_id.into()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(line_id.into()),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: true,
            reason,
            note: None,
        }],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    }
}

/// The drawer kicks printed to `path`, once `n` of them have landed (≤ 10 s): the print queue
/// works on its own thread.
fn drawer_kicks(path: &Path, n: usize) -> usize {
    let count = || {
        std::fs::read(path).map_or(0, |bytes| {
            bytes.windows(5).filter(|w| *w == DRAWER_KICK).count()
        })
    };
    for _ in 0..200 {
        if count() >= n {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    count()
}

/// Where the shift runs: on a single PC, on a main PC serving a till, on a till that sees its
/// main PC, and on a till whose main PC is down all shift long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Alone,
    MainPc,
    TillOnline,
    TillOffline,
}

/// M1b's whole shift, unchanged in what it sells, prints and totals, on `shop` whose `milk`
/// (24 pieces) and `oil` (10 pieces) are in stock and whose receipt printer writes to `out`.
/// Returns the Z.
pub(crate) fn run_shift(
    shop: &TestShop,
    out: &Path,
    milk: &Product,
    oil: &Product,
    place: Place,
) -> pos_core::cash::closing::Closed {
    shop.cashier();
    let session = cash_cmd::open_session(&shop.st, Money(1_000_000)).unwrap();

    // a customer goes back to his car for his card: his oil waits on hold (F6)
    let held = sales_cmd::hold(
        &shop.st,
        "Karim".into(),
        Cart::new(Lang::Fr, vec![CartLine::product(&oil.id, Qty(1_000))]),
    )
    .unwrap();

    // scan, 2 milks, 500 DA in cash: 300 DA is within the duty exemption (≤ 300 DA), so no
    // duty and 200 DA back, the receipt then the drawer
    let hit = products::find_by_code(&shop.st, "6130000000017".into())
        .unwrap()
        .unwrap();
    assert_eq!(hit.product.id, milk.id, "the scan finds the milk");
    let first = sales_cmd::create(
        &shop.st,
        sale(
            vec![CartLine::product(&hit.product.id, Qty(2_000))],
            vec![PaymentInput::cash(Money(50_000))],
            true,
        ),
    )
    .unwrap();
    let ticket = first.sale.document.id.clone();
    assert_eq!(
        (
            first.sale.settlement.stamp_duty,
            first.sale.settlement.change
        ),
        (Money::ZERO, Money(20_000))
    );
    let receipt = wait_for_bytes(out, 200);
    assert!(receipt.windows(9).any(|w| w == b"Lait Cand"));
    assert!(
        receipt.ends_with(&DRAWER_KICK),
        "the drawer opens after the cut"
    );

    // the customer asks for it in Arabic: a raster DUPLICATA
    let reprint = documents::reprint(&shop.st, ticket.clone(), Some(Lang::Ar)).unwrap();
    assert!(reprint.raster);
    assert!(
        std::fs::read(out)
            .unwrap()
            .windows(3)
            .any(|w| w == [0x1D, b'v', b'0']),
        "GS v 0 bands"
    );

    // the held cart comes back (F7), repriced: 500 DA by CIB with its authorisation number,
    // the rest in cash; the cash part (150 DA) is exempt and the change comes from the cash
    let recalled = sales_cmd::recall(&shop.st, held.id).unwrap();
    assert_eq!(recalled.priced.totals.total_ttc, Money(65_000));
    assert!(sales_cmd::held(&shop.st).unwrap().is_empty());
    let card = PaymentInput {
        method: PaymentMethod::CardCib,
        amount: Money(50_000),
        reference: Some("A1B2C3".into()),
        cheque_bank: None,
        cheque_date: None,
    };
    let split = sales_cmd::create(
        &shop.st,
        sales_cmd::SaleRequest {
            sale: SaleInput {
                cart: recalled.cart,
                payments: vec![card, PaymentInput::cash(Money(20_000))],
            },
            print_receipt: false,
            extras: Default::default(),
        },
    )
    .unwrap();
    let paid = &split.sale.settlement;
    assert_eq!(paid.paid_total.0 - paid.change.0, paid.total_due.0);
    assert_eq!(
        (paid.stamp_duty, paid.total_due, paid.change),
        (Money::ZERO, Money(65_000), Money(5_000))
    );
    assert!(split.print_queued, "cash moved: the drawer opens");

    // every other tender, one bottle of oil each: exempt from the duty, and no drawer
    let tenders = [
        PaymentInput::electronic(PaymentMethod::Edahabia, Money(65_000), Some("ED7781")),
        PaymentInput::electronic(PaymentMethod::Baridimob, Money(65_000), Some("QR5521")),
        PaymentInput {
            method: PaymentMethod::Cheque,
            amount: Money(65_000),
            reference: Some("0012345".into()),
            cheque_bank: Some("BNA".into()),
            cheque_date: Some("2026-09-29".into()),
        },
        PaymentInput::electronic(PaymentMethod::Transfer, Money(65_000), Some("VIR0042")),
    ];
    for tender in tenders {
        let method = tender.method;
        let done = sales_cmd::create(
            &shop.st,
            sale(
                vec![CartLine::product(&oil.id, Qty(1_000))],
                vec![tender],
                false,
            ),
        )
        .unwrap();
        let settled = &done.sale.settlement;
        assert_eq!(
            (settled.stamp_duty, settled.total_due, settled.change),
            (Money::ZERO, Money(65_000), Money::ZERO),
            "{method:?}"
        );
        assert!(!done.print_queued, "{method:?}: no cash, no drawer");
    }

    // one milk back, in cash, after a manager's approval
    let line_id = returns_cmd::returnable(&shop.st, ticket.clone()).unwrap()[0]
        .line
        .id
        .clone();
    approve(shop, Permission::SalesReturn);
    let refund = returns_cmd::create(
        &shop.st,
        returns_cmd::ReturnRequest {
            input: one_back(&ticket, &line_id, ReturnReason::Defective),
            print_receipt: false,
            extras: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(refund.returned.refund, Money(15_000));
    assert!(refund.print_queued, "cash paid back: the drawer opens");

    // the other milk exchanged for a bottle of oil: its value pays part of the new ticket, and
    // the 500 DA left to pay in cash carry 5 DA of duty
    approve(shop, Permission::SalesReturn);
    let exchange = returns_cmd::exchange(
        &shop.st,
        returns_cmd::ExchangeRequest {
            ret: one_back(&ticket, &line_id, ReturnReason::ChangedMind),
            sale: SaleInput {
                cart: Cart::new(Lang::Fr, vec![CartLine::product(&oil.id, Qty(1_000))]),
                payments: vec![PaymentInput::cash(Money(60_000))],
            },
            print_receipt: false,
            return_extras: Default::default(),
            sale_extras: Default::default(),
        },
    )
    .unwrap()
    .exchange;
    assert_eq!(
        exchange.net_to_pay.0,
        exchange.sale.settlement.total_due.0 - 15_000
    );
    assert_eq!(
        (
            exchange.returned.store_credit,
            exchange.returned.refund,
            exchange.sale.settlement.stamp_duty,
            exchange.sale.settlement.change,
            exchange.net_to_pay
        ),
        (
            Money(15_000),
            Money::ZERO,
            Money(500),
            Money(9_500),
            Money(50_500)
        )
    );
    assert_eq!(
        returns_cmd::returnable(&shop.st, ticket.clone()).unwrap()[0].remaining,
        Qty(0)
    );

    // the X in the middle of the shift is blind for the cashier (CSH-004): the ticket counts
    // and the non-cash tenders, nothing that adds up to the expected cash
    let x = cash_cmd::x_report(&shop.st).unwrap();
    assert!(x.blind);
    assert_eq!(
        (x.tickets, x.returns, x.sales_ttc, x.returns_ttc, x.net_ttc),
        (7, 2, Money::ZERO, Money::ZERO, Money::ZERO)
    );
    assert_eq!((x.expected_cash, x.opening_float), (None, Money::ZERO));
    assert!(
        x.by_method
            .iter()
            .all(|m| !matches!(m.method, PaymentMethod::Cash | PaymentMethod::Change))
    );
    assert_eq!(
        cash_cmd::movements(&shop.st, x.session.id.clone())
            .unwrap_err()
            .code(),
        "insufficient_permission",
        "every drawer movement would add up to the expected cash"
    );
    cash_cmd::print_x(&shop.st, Lang::Fr).unwrap();
    assert!(
        std::fs::read(out)
            .unwrap()
            .windows(9)
            .any(|w| w == b"Rapport X")
    );

    // expected: 10 000 DA of float + 300 + 150 + 505 DA of cash sales − 150 DA refunded
    // = 10 805 DA; counted 5 × 2 000 DA: 805 DA short, over 500 DA, so a manager accepts it
    let count = CloseInput {
        counts: vec![CountLine {
            denomination: Money(200_000),
            count: 5,
        }],
        notes: None,
    };
    let closed = if place == Place::TillOffline {
        // the main PC is down: the Z waits for the outbox, or is closed offline with a manager
        assert_eq!(
            cash_cmd::close_session(&shop.st, count.clone())
                .unwrap_err()
                .code(),
            "outbox_pending_at_close"
        );
        // CSH-005 first, as online: the variance is refused (the count journaled) before any
        // approval, so the manager approves the offline Z with the variance in front of them
        assert_eq!(
            cash_cmd::close_offline(&shop.st, count.clone())
                .unwrap_err()
                .code(),
            "variance_needs_approval"
        );
        approve(shop, Permission::CashViewExpected);
        let closed = cash_cmd::close_offline(&shop.st, count).unwrap();
        assert!(closed.z.offline_pending.is_some_and(|n| n > 0));
        closed
    } else {
        assert_eq!(
            cash_cmd::close_session(&shop.st, count.clone())
                .unwrap_err()
                .code(),
            "variance_needs_approval"
        );
        approve(shop, Permission::CashViewExpected);
        cash_cmd::close_session(&shop.st, count).unwrap()
    };
    let z = &closed.z;
    assert_eq!(
        (z.counted_cash, z.summary.expected_cash, z.variance),
        (Money(1_000_000), Some(Money(1_080_500)), Money(-80_500))
    );
    assert_eq!(z.approved_by.as_deref(), Some(shop.owner.user_id.as_str()));
    assert_eq!((z.summary.tickets, z.summary.returns), (7, 2));
    assert_eq!(
        (
            z.summary.sales_ttc,
            z.summary.returns_ttc,
            z.summary.net_ttc,
            z.grand_total
        ),
        (
            Money(420_000),
            Money(-30_000),
            Money(390_000),
            Money(390_000)
        )
    );
    assert_eq!(
        (
            z.summary.stamp_charged,
            z.summary.cash_sales,
            z.summary.cash_refunds
        ),
        (Money(500), Money(95_500), Money(-15_000))
    );
    let by_method: Vec<_> = z
        .summary
        .by_method
        .iter()
        .map(|m| (m.method, m.received.0, m.paid_back.0, m.count))
        .collect();
    assert_eq!(
        by_method,
        [
            (PaymentMethod::Baridimob, 65_000, 0, 1),
            (PaymentMethod::CardCib, 50_000, 0, 1),
            (PaymentMethod::Cash, 130_000, -15_000, 4),
            (PaymentMethod::Change, 0, -34_500, 3),
            (PaymentMethod::Cheque, 65_000, 0, 1),
            (PaymentMethod::Edahabia, 65_000, 0, 1),
            (PaymentMethod::Transfer, 65_000, 0, 1),
            (PaymentMethod::Voucher, 15_000, -15_000, 2),
        ]
    );
    assert!(z.audit_anchor.intact && z.document_anchors.iter().all(|a| a.intact));
    let printed = cash_cmd::print_z(&shop.st, session.id, Lang::Fr).unwrap();
    assert_eq!(printed.printer, "Caisse 1");
    assert!(
        std::fs::read(out)
            .unwrap()
            .windows(9)
            .any(|w| w == b"Rapport Z")
    );
    assert_eq!(
        drawer_kicks(out, 4),
        4,
        "the drawer opened for the two cash sales, the refund and the exchange, and only then"
    );

    // the owner checks that nothing was altered and finds the reprint in the journal
    shop.owner();
    assert!(
        audit_cmd::verify(&shop.st)
            .unwrap()
            .first_broken_seq
            .is_none()
    );
    assert!(
        documents::verify_chain(&shop.st)
            .unwrap()
            .iter()
            .all(|c| c.first_broken_seq.is_none())
    );
    let closures = cash_cmd::verify_closures(&shop.st).unwrap();
    assert_eq!((closures.rows, closures.first_broken_z), (1, None));
    let reprints = audit_cmd::history(
        &shop.st,
        AuditFilter {
            action_prefix: Some("document.".into()),
            ..AuditFilter::default()
        },
    )
    .unwrap();
    assert!(
        reprints
            .rows
            .iter()
            .any(|r| r.action == "document.reprinted")
    );
    // the refused first count is on record next to the accepted one (CSH-005); offline, the
    // refusal was `outbox_pending_at_close`, which is not a count
    if place != Place::TillOffline {
        let counts = audit_cmd::history(
            &shop.st,
            AuditFilter {
                action_prefix: Some("cash.close".into()),
                ..AuditFilter::default()
            },
        )
        .unwrap();
        let refused: Vec<_> = counts
            .rows
            .iter()
            .filter(|r| r.action == "cash.close_refused")
            .collect();
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].payload["counted"], serde_json::json!(1_000_000));
    }
    closed
}

#[test]
fn a_whole_shift_prints_counts_closes_and_stays_verifiable() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _) = shop_in(dir.path());
    let out = dir.path().join("till.bin");
    shop.st.hw.save(file_printer(&out)).unwrap();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, Some("6130000000017"));
    let oil = shop.product("Huile Elio 5 L", 65_000, 1_900, None);
    shop.stock_in(&milk.id, 24);
    shop.stock_in(&oil.id, 10);
    run_shift(&shop, &out, &milk, &oil, Place::Alone);
}

#[test]
fn the_same_shift_on_a_main_pc_serving_a_till() {
    let pcs = crate::lan::two_pcs::TwoPcs::new();
    let out = pcs.printer(&pcs.host);
    run_shift(&pcs.host, &out, &pcs.milk, &pcs.oil, Place::MainPc);
    pcs.wait_caught_up();
    let host_id = pcs.host.st.terminal.id.clone();
    assert_eq!(
        crate::lan::two_pcs::tickets_of(&pcs.till().st, &host_id),
        crate::lan::two_pcs::tickets_of(&pcs.host.st, &host_id),
        "the till mirrors the main PC's documents"
    );
}

#[test]
fn the_same_shift_on_a_till_reaches_the_main_pc() {
    let pcs = crate::lan::two_pcs::TwoPcs::new();
    let out = pcs.printer(pcs.till());
    let closed = run_shift(pcs.till(), &out, &pcs.milk, &pcs.oil, Place::TillOnline);
    pcs.wait_sent();
    let till_id = pcs.till().st.terminal.id.clone();
    assert_eq!(
        crate::lan::two_pcs::tickets_of(&pcs.host.st, &till_id),
        crate::lan::two_pcs::tickets_of(&pcs.till().st, &till_id)
    );
    let zs = pcs
        .host
        .st
        .db
        .read(move |c| pos_core::sync::tills::closures(c, &till_id, 10))
        .unwrap();
    assert_eq!(zs[0].z_number, closed.z.z_number);
}

#[test]
fn the_same_shift_on_a_till_whose_main_pc_is_down_reaches_it_later() {
    let pcs = crate::lan::two_pcs::TwoPcs::new();
    let out = pcs.printer(pcs.till());
    pcs.host_down();
    let closed = run_shift(pcs.till(), &out, &pcs.milk, &pcs.oil, Place::TillOffline);
    pcs.host_up();
    pcs.wait_sent();
    let till_id = pcs.till().st.terminal.id.clone();
    assert_eq!(
        crate::lan::two_pcs::tickets_of(&pcs.host.st, &till_id),
        crate::lan::two_pcs::tickets_of(&pcs.till().st, &till_id)
    );
    let zs = pcs
        .host
        .st
        .db
        .read({
            let till_id = till_id.clone();
            move |c| pos_core::sync::tills::closures(c, &till_id, 10)
        })
        .unwrap();
    assert_eq!(
        (zs[0].z_number.clone(), zs[0].offline_pending),
        (closed.z.z_number.clone(), closed.z.offline_pending)
    );
    let open = pcs
        .host
        .st
        .db
        .read(|c| pos_core::sync::notices::list(c, false, 100))
        .unwrap();
    assert!(
        open.iter()
            .any(|n| n.kind == pos_core::sync::notices::NoticeKind::OfflineZ)
    );
}
