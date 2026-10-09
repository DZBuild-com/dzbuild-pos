//! Two PCs: the controlled-medicine register is the main PC's. A till's
//! sale of a controlled medicine stays the till's ticket (its series, its drawer) and the main PC
//! writes the register entry against it once the sale committed; the till reads the register
//! report from the main PC; offline, the till refuses such a sale before anything is written.
//! A sale whose voucher the main PC refuses waits on the till and holds back none of its later
//! entries (Task 31 review).

use super::registers;
use super::two_pcs::TwoPcs;
use crate::commands::{cash as cash_cmd, loyalty, packs, pharmacy, reports, sales};
use crate::state::AppState;
use pos_core::company::ShopActivity;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::packs::Module;
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::reports::{ReportKind, ReportRequest};
use pos_core::sales::extensions::{HostRegisters, pending_host_registers};
use pos_core::sales::{Cart, CartLine, SaleInput};

fn today(st: &AppState) -> String {
    pos_core::time::business_date(st.now())
        .format("%Y-%m-%d")
        .to_string()
}

/// A ticket of one piece of `product_id` paid `cash` centimes, with (or without) a prescription
/// for its line.
fn sale_of(product_id: &str, cash: i64, prescribed: bool) -> sales::SaleRequest {
    let cart = Cart::new(Lang::Fr, vec![CartLine::product(product_id, Qty(1_000))]);
    let mut request: sales::SaleRequest = serde_json::from_value(serde_json::json!({
        "sale": { "cart": cart, "payments": [{ "method": "cash", "amount": cash }] },
        "printReceipt": false,
    }))
    .unwrap();
    if prescribed {
        request.extras = serde_json::from_value(serde_json::json!({
            "prescriptions": [{ "line": 0, "prescriberName": "Dr Benali", "prescriberRef": null,
                "prescriptionNo": "0457", "prescriptionDate": "2026-09-27",
                "patientName": "M. Haddad", "patientRef": null }]
        }))
        .unwrap();
    }
    request
}

fn tickets(st: &AppState) -> i64 {
    st.db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket'",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap()
}

/// A pharmacy's main PC with a controlled medicine in stock, its till caught up, the owner signed
/// in at the till with its drawer open.
fn pharmacy_pcs() -> (TwoPcs, String) {
    let pcs = TwoPcs::new();
    packs::apply_template(&pcs.host.st, ShopActivity::Pharmacy, false).unwrap();
    let med = pcs.host.controlled_medicine("Tramadol 50 mg", 45_000);
    pcs.host.stock_in(&med, 10);
    pcs.wait_caught_up();
    let till = pcs.till();
    till.owner();
    cash_cmd::open_session(&till.st, Money(100_000)).unwrap();
    (pcs, med)
}

#[test]
fn a_tills_controlled_sale_is_registered_on_the_main_pc_and_read_from_there() {
    let (pcs, med) = pharmacy_pcs();
    let till = pcs.till();

    // still checked on the till: no prescription, no sale
    let refused = sales::create(&till.st, sale_of(&med, 50_000, false)).unwrap_err();
    assert_eq!(refused.code(), "register_entry_required");

    // the sale is the till's; its register entry is the main PC's, against the till's ticket
    let sold = sales::create(&till.st, sale_of(&med, 50_000, true)).unwrap();
    let number = sold.sale.document.number.clone().unwrap();
    let day = today(&till.st);
    let entries = pharmacy::register_entries(&pcs.host.st, day.clone(), day.clone()).unwrap();
    assert_eq!(
        entries.len(),
        1,
        "written on the main PC once the till's sale committed"
    );
    assert_eq!(entries[0].document_id, sold.sale.document.id);
    assert_eq!(entries[0].document_number.as_deref(), Some(number.as_str()));
    assert_eq!(entries[0].patient_name, "M. Haddad");
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket'"),
        1,
        "the main PC sold nothing itself: the ticket is the till's"
    );
    assert!(
        till.st
            .db
            .read(pos_core::sales::extensions::pending_host_registers)
            .unwrap()
            .is_empty(),
        "nothing left to send"
    );
    assert_eq!(registers::send_pending(&till.st), 0);

    // The till's register report is the main PC's register
    let view = reports::run(
        &till.st,
        ReportRequest::new(ReportKind::ControlledRegister, &day, &day),
    )
    .unwrap();
    assert_eq!(view.report.table.rows.len(), 1);
}

#[test]
fn an_offline_till_refuses_a_controlled_sale_before_writing_anything() {
    let (pcs, med) = pharmacy_pcs();
    pcs.host_down();
    let till = pcs.till();
    let before = tickets(&till.st);
    let offline = sales::create(&till.st, sale_of(&med, 50_000, true)).unwrap_err();
    assert_eq!(offline.code(), "host_required");
    assert_eq!(tickets(&till.st), before, "no ticket, no number used");
    // Offline, the register cannot be read either
    let day = today(&till.st);
    let report = reports::run(
        &till.st,
        ReportRequest::new(ReportKind::ControlledRegister, &day, &day),
    )
    .unwrap_err();
    assert_eq!(report.code(), "host_required");
    // any other sale goes through offline
    sales::create(&till.st, sale_of(&pcs.milk.id, 20_000, false)).unwrap();
    assert_eq!(tickets(&till.st), before + 1);
}

/// `pieces` of `product_id` paid by `payments`.
fn paid(product_id: &str, pieces: i64, payments: Vec<PaymentInput>) -> sales::SaleRequest {
    sales::SaleRequest {
        sale: SaleInput {
            cart: Cart::new(
                Lang::Fr,
                vec![CartLine::product(product_id, Qty(pieces * 1_000))],
            ),
            payments,
        },
        print_receipt: false,
        extras: Default::default(),
    }
}

fn voucher(code: &str, amount: i64) -> PaymentInput {
    PaymentInput {
        method: PaymentMethod::Voucher,
        amount: Money(amount),
        reference: Some(code.to_owned()),
        cheque_bank: None,
        cheque_date: None,
    }
}

fn waiting(st: &AppState) -> Vec<HostRegisters> {
    st.db.read(pending_host_registers).unwrap()
}

/// Task 31 review. A: the till takes a 300 DA gift voucher and the call to the main PC fails. B:
/// the same voucher again on the till is refused there, A's spend counted. The main PC spends the
/// voucher itself meanwhile, so it refuses A's booking once it can. C: a controlled medicine;
/// its register entry reaches the main PC past A, which waits on the till marked with the
/// refusal.
#[test]
fn a_refused_voucher_never_holds_back_the_tills_later_register_entries() {
    let (pcs, med) = pharmacy_pcs();
    let host = &pcs.host;
    let mut modules = packs::state(&host.st).unwrap().modules;
    modules.push(Module::GiftVouchers);
    packs::set_modules(&host.st, modules).unwrap();
    cash_cmd::open_session(&host.st, Money(100_000)).unwrap();
    let code = loyalty::voucher_sell(
        &host.st,
        loyalty::voucher_sale(30_000, PaymentMethod::CardCib),
    )
    .unwrap()
    .voucher
    .code;
    pcs.wait_caught_up();
    let till = pcs.till();

    // A: the link drops between the till's checks and its call to the main PC
    pcs.host_down();
    *till.st.lan.test_link.lock().unwrap() = Some((true, None));
    let a = sales::create(
        &till.st,
        paid(&pcs.milk.id, 2, vec![voucher(&code, 30_000)]),
    )
    .expect("the till checked the voucher on its copy");
    let pending = waiting(&till.st);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].document_id, a.sale.document.id);
    assert_eq!(
        pending[0].refused, None,
        "a call that never reached the main PC is no refusal"
    );

    // B: the till's copy still shows the voucher whole; A's spend is counted all the same
    let before = tickets(&till.st);
    let b = sales::create(
        &till.st,
        paid(&pcs.milk.id, 1, vec![voucher(&code, 15_000)]),
    )
    .unwrap_err();
    assert_eq!(b.code(), "voucher_balance_insufficient");
    assert_eq!(tickets(&till.st), before, "B left no ticket");

    // the main PC spends the voucher itself, then comes back
    *till.st.lan.test_link.lock().unwrap() = None;
    sales::create(
        &host.st,
        paid(&pcs.milk.id, 2, vec![voucher(&code, 30_000)]),
    )
    .unwrap();
    pcs.host_up();

    // C: the main PC refuses A's voucher and keeps C's register entry all the same
    let c = sales::create(&till.st, sale_of(&med, 50_000, true)).unwrap();
    let day = today(&till.st);
    let entries = pharmacy::register_entries(&host.st, day.clone(), day).unwrap();
    assert_eq!(entries.len(), 1, "C's entry reached the main PC past A");
    assert_eq!(entries[0].document_id, c.sale.document.id);
    let pending = waiting(&till.st);
    assert_eq!(pending.len(), 1, "only A waits");
    assert_eq!(pending[0].document_id, a.sale.document.id);
    assert_eq!(
        pending[0].refused.as_deref(),
        Some("voucher_balance_insufficient"),
        "A is marked with the main PC's refusal"
    );
    assert_eq!(
        registers::send_pending(&till.st),
        0,
        "A is refused again, and stays"
    );
    assert_eq!(waiting(&till.st).len(), 1);
}

/// Final review (critical): one ticket of a controlled medicine paid with a gift voucher the main
/// PC spent meanwhile. The main PC writes the register entry and refuses the voucher alone; the
/// till keeps only the refused ledger part (no patient data), lists it in Settings › Réseau
/// and « Réessayer » sends it again, refused again, the entry never written twice.
#[test]
fn a_controlled_medicine_paid_with_a_spent_voucher_still_reaches_the_register() {
    let (pcs, med) = pharmacy_pcs();
    let host = &pcs.host;
    let mut modules = packs::state(&host.st).unwrap().modules;
    modules.push(Module::GiftVouchers);
    packs::set_modules(&host.st, modules).unwrap();
    cash_cmd::open_session(&host.st, Money(100_000)).unwrap();
    let code = loyalty::voucher_sell(
        &host.st,
        loyalty::voucher_sale(30_000, PaymentMethod::CardCib),
    )
    .unwrap()
    .voucher
    .code;
    pcs.wait_caught_up();
    let till = pcs.till();

    // the till sells while its link is down; the main PC spends the voucher meanwhile
    pcs.host_down();
    *till.st.lan.test_link.lock().unwrap() = Some((true, None));
    let mut request = sale_of(&med, 15_000, true);
    request.sale.payments.insert(0, voucher(&code, 30_000));
    let sold = sales::create(&till.st, request).expect("the till checked the voucher on its copy");
    *till.st.lan.test_link.lock().unwrap() = None;
    sales::create(
        &host.st,
        paid(&pcs.milk.id, 2, vec![voucher(&code, 30_000)]),
    )
    .unwrap();
    pcs.host_up();

    assert_eq!(
        registers::send_pending(&till.st),
        0,
        "the voucher is refused"
    );
    let day = today(&till.st);
    let entries = pharmacy::register_entries(&host.st, day.clone(), day).unwrap();
    assert_eq!(entries.len(), 1, "the register entry is on the main PC");
    assert_eq!(entries[0].document_id, sold.sale.document.id);
    let pending = waiting(&till.st);
    assert_eq!(pending.len(), 1);
    assert!(pending[0].controlled.is_empty(), "no patient data waits");
    assert_eq!(
        pending[0].refused.as_deref(),
        Some("voucher_balance_insufficient")
    );

    // The owner sees it and tries again
    let listed = registers::pending_entries(&till.st).unwrap();
    assert_eq!(listed.refused, 1);
    assert_eq!(listed.refused_registers, 0, "the register part never waits");
    assert_eq!(
        listed.entries[0].number, sold.sale.document.number,
        "the ticket's number"
    );
    let json = serde_json::to_string(&listed).unwrap();
    assert!(!json.contains("Haddad") && !json.contains("Benali"));
    let again = registers::retry_pending(&till.st).unwrap();
    assert_eq!(again.refused, 1, "refused again");
    let day = today(&till.st);
    assert_eq!(
        pharmacy::register_entries(&host.st, day.clone(), day)
            .unwrap()
            .len(),
        1,
        "written once"
    );
}
