//! Two PCs: a till's return of an IMEI phone, of a ticket that earned loyalty points
//! or refunded as an exchange voucher writes the main PC's ledgers (serials, points, vouchers),
//! so the till makes it on the main PC as a host service: the return note is the main PC's, its
//! cash refund lands in the till's drawer, its receipt prints on the till. Offline the till says
//! `host_required`; a ticket whose own entries still wait for the main PC is not returned yet.

use super::registers;
use super::two_pcs::TwoPcs;
use crate::commands::testshop::TestShop;
use crate::commands::{cash as cash_cmd, loyalty, packs, product_extras, returns, sales, stock_m2};
use crate::state::AppState;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::packs::Module;
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::returns::{ReturnInput, ReturnLineInput, ReturnReason};
use pos_core::sales::extensions::{ReturnExtras, ReturnSerial, SaleSerial};
use pos_core::sales::{Cart, CartLine, SaleExtras, SaleInput};

const IMEI: &str = "356938035643809";

fn switch_on(host: &TestShop, wanted: &[Module]) {
    let mut modules = packs::state(&host.st).unwrap().modules;
    modules.extend_from_slice(wanted);
    packs::set_modules(&host.st, modules).unwrap();
}

/// The owner signed in at the till, its drawer open with 1 000 DA.
fn open_till(pcs: &TwoPcs) -> &TestShop {
    let till = pcs.till();
    till.owner();
    cash_cmd::open_session(&till.st, Money(100_000)).unwrap();
    till
}

fn sale(
    product_id: &str,
    customer: Option<&str>,
    cash: i64,
    extras: SaleExtras,
) -> sales::SaleRequest {
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product_id, Qty(1_000))]);
    cart.customer_id = customer.map(str::to_owned);
    sales::SaleRequest {
        sale: SaleInput {
            cart,
            // paid with enough, the change given back
            payments: vec![PaymentInput::cash(Money(cash.max(10_000_000)))],
        },
        print_receipt: false,
        extras,
    }
}

/// The whole first line of `ticket` back, refunded by `method`.
fn whole_line(ticket: &pos_core::documents::Document, method: PaymentMethod) -> ReturnInput {
    ReturnInput {
        original_id: Some(ticket.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(ticket.lines[0].id.clone()),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: true,
            reason: ReturnReason::ChangedMind,
            note: None,
        }],
        refund_method: method,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    }
}

fn request(input: ReturnInput, extras: ReturnExtras) -> returns::ReturnRequest {
    returns::ReturnRequest {
        input,
        print_receipt: false,
        extras,
    }
}

/// `(kind, amount)` of the drawer movements naming a document on `st`.
fn refunds(st: &AppState) -> Vec<i64> {
    st.db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT amount FROM cash_movements WHERE kind = 'refund' ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

#[test]
fn a_tills_return_of_an_imei_phone_is_made_on_the_main_pc() {
    let pcs = TwoPcs::new();
    switch_on(&pcs.host, &[Module::Serials]);
    let phone = pcs.host.product("Redmi Note 14", 3_200_000, 1_900, None);
    product_extras::set_extras(
        &pcs.host.st,
        phone.id.clone(),
        pos_core::catalog::extras::ProductExtrasInput {
            track_serials: true,
            ..Default::default()
        },
    )
    .unwrap();
    pcs.host.stock_in(&phone.id, 1);
    stock_m2::register_serials(&pcs.host.st, phone.id.clone(), vec![IMEI.into()], None).unwrap();
    pcs.wait_caught_up();
    let till = open_till(&pcs);

    // sold while the link is down: its IMEI waits on the till for the main PC
    pcs.host_down();
    *till.st.lan.test_link.lock().unwrap() = Some((true, None));
    let sold = sales::create(
        &till.st,
        sale(
            &phone.id,
            None,
            3_200_000,
            SaleExtras {
                serials: vec![SaleSerial {
                    line: 0,
                    serial_no: IMEI.into(),
                }],
                ..SaleExtras::default()
            },
        ),
    )
    .unwrap()
    .sale
    .document;
    *till.st.lan.test_link.lock().unwrap() = None;
    pcs.host_up();
    pcs.wait_sent();
    let back = || {
        request(
            whole_line(&sold, PaymentMethod::Cash),
            ReturnExtras {
                serials: vec![ReturnSerial {
                    line: 0,
                    serial_no: IMEI.into(),
                }],
                ..ReturnExtras::default()
            },
        )
    };
    let early = returns::create(&till.st, back()).unwrap_err();
    assert_eq!(
        early.code(),
        "return_host_pending",
        "the main PC does not hold the sale's IMEI yet"
    );
    registers::send_pending(&till.st);
    pcs.wait_caught_up();

    let done = returns::create(&till.st, back()).expect("made on the main PC");
    assert_eq!(done.returned.refund, sold.total_ttc);
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM documents WHERE doc_type = 'return_note'"),
        1,
        "the return note is the main PC's"
    );
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM serials WHERE status = 'sold'"),
        0,
        "the IMEI is back"
    );
    assert_eq!(
        refunds(&till.st),
        [-sold.total_ttc.0],
        "the refund left the till's drawer"
    );
    assert!(
        refunds(&pcs.host.st).is_empty(),
        "the main PC's drawer is untouched"
    );

    pcs.host_down();
    let offline = returns::create(&till.st, back()).unwrap_err();
    assert_eq!(offline.code(), "host_required");
}

#[test]
fn a_tills_return_of_a_ticket_that_earned_points_is_made_on_the_main_pc() {
    let pcs = TwoPcs::new();
    switch_on(&pcs.host, &[Module::Loyalty]);
    let nadia = pcs.host.customer("Nadia");
    loyalty::enrol(&pcs.host.st, nadia.clone(), None).unwrap();
    pcs.wait_caught_up();
    let till = open_till(&pcs);
    let sold = sales::create(
        &till.st,
        sale(&pcs.oil.id, Some(&nadia), 65_000, SaleExtras::default()),
    )
    .unwrap()
    .sale
    .document;
    pcs.wait_sent();
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM loyalty_ledger WHERE kind = 'earn'"),
        1,
        "the till earned the points"
    );
    returns::create(
        &till.st,
        request(
            whole_line(&sold, PaymentMethod::Cash),
            ReturnExtras::default(),
        ),
    )
    .expect("made on the main PC");
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM loyalty_ledger WHERE kind = 'reversal'"),
        1,
        "the points are taken back on the main PC"
    );
    assert_eq!(refunds(&till.st), [-sold.total_ttc.0]);
}

#[test]
fn a_tills_refund_as_an_exchange_voucher_is_made_on_the_main_pc() {
    let pcs = TwoPcs::new();
    switch_on(&pcs.host, &[Module::GiftVouchers]);
    pcs.wait_caught_up();
    let till = open_till(&pcs);
    let sold = sales::create(
        &till.st,
        sale(&pcs.oil.id, None, 65_000, SaleExtras::default()),
    )
    .unwrap()
    .sale
    .document;
    pcs.wait_sent();
    let done = returns::create(
        &till.st,
        request(
            whole_line(&sold, PaymentMethod::Voucher),
            ReturnExtras {
                exchange_voucher: true,
                ..ReturnExtras::default()
            },
        ),
    )
    .expect("made on the main PC");
    let voucher = done
        .returned
        .exchange_voucher
        .expect("the exchange voucher");
    assert!(voucher.code.starts_with("BA"), "numbered by the main PC");
    assert_eq!(voucher.initial_amount, sold.total_ttc);
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM gift_vouchers WHERE kind = 'exchange'"),
        1
    );
    assert!(refunds(&till.st).is_empty(), "no cash left the drawer");
}
