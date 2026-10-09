//! The controlled-medicine register is the main PC's (`HostOnly`). A till's sale with a
//! prescription commits on the till without a register row and keeps its entries for the main
//! PC; the main PC numbers them against the till's ticket once the ticket reached it, and a second
//! sending writes nothing twice.

use super::extensions::{
    HostRegisters, needs_host_registers, pending_host_registers, record_host_registers,
};
use super::{Cart, CartLine, SaleExtras, SaleInput, create_sale_with};
use crate::audit::Actor;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::Module;
use crate::payments::PaymentInput;
use crate::pharmacy::PharmacyProductInput;
use crate::pharmacy::register::PrescriptionInput;
use crate::sync::testkit::{LanPair, lan_pair};
use crate::testkit::NOON;
use crate::till::TillContext;
use std::sync::Arc;

/// A pharmacy main PC with one controlled medicine in stock, and its till caught up.
fn pharmacy_pair() -> (LanPair, String) {
    let pair = lan_pair();
    let actor = Actor::system(None);
    pair.host
        .db
        .write(move |tx| crate::packs::set_modules(tx, &actor, &[Module::Pharmacy]))
        .unwrap();
    let med = pair
        .host
        .product("Tramadol 50 mg", Money(45_000), &pair.host.exempt);
    let (actor, id) = (pair.host.owner.actor(), med.id.clone());
    pair.host
        .db
        .write(move |tx| {
            crate::pharmacy::set(
                tx,
                &actor,
                &id,
                PharmacyProductInput {
                    dci: Some("Tramadol".into()),
                    controlled: true,
                    schedule: Some("Psychotrope".into()),
                    prescription_required: true,
                },
            )
            .map(drop)
        })
        .unwrap();
    pair.host.stock_in(&med.id, 10_000);
    pair.pull();
    (pair, med.id)
}

fn owner_on_till(pair: &LanPair) -> TillContext {
    TillContext::new(pair.till_owner.clone(), Arc::clone(&pair.host.pack), NOON)
}

fn prescribed() -> SaleExtras {
    SaleExtras {
        prescriptions: vec![PrescriptionInput {
            line: 0,
            prescriber_name: "Dr Benali".into(),
            prescriber_ref: None,
            prescription_no: Some("0457".into()),
            prescription_date: "2026-09-27".into(),
            patient_name: "M. Haddad".into(),
            patient_ref: None,
        }],
        ..SaleExtras::default()
    }
}

#[test]
fn a_tills_controlled_sale_keeps_its_entries_for_the_main_pc_which_numbers_them_once() {
    let (pair, med) = pharmacy_pair();
    let ctx = owner_on_till(&pair);
    let open = ctx.clone();
    pair.till
        .write(move |tx| crate::cash::open_session(tx, &open, Money(10_000)))
        .unwrap();
    assert!(
        pair.till
            .read({
                let id = med.clone();
                move |c| needs_host_registers(c, [id.as_str()])
            })
            .unwrap(),
        "a controlled medicine needs the main PC's register"
    );
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&med, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(50_000))],
    };
    let sale = pair
        .till
        .write(move |tx| create_sale_with(tx, &ctx, input, &prescribed()))
        .expect("the till's sale commits");
    assert_eq!(
        pair.count_till("SELECT COUNT(*) FROM controlled_register"),
        0,
        "the register is the main PC's"
    );
    let pending = pair.till.read(pending_host_registers).unwrap();
    assert_eq!(pending.len(), 1);
    let registers: HostRegisters = pending[0].clone();
    assert_eq!(registers.document_id, sale.document.id);
    assert_eq!(registers.controlled.len(), 1);
    assert_eq!(registers.controlled[0].patient_name, "M. Haddad");

    // the main PC writes nothing before the till's ticket reached it
    let host_ctx = pair.host.owner_till();
    let early = registers.clone();
    let refused = pair
        .host
        .db
        .write({
            let ctx = host_ctx.clone();
            move |tx| record_host_registers(tx, &ctx, &early)
        })
        .unwrap_err();
    assert_eq!(refused.code(), "document_not_found");

    pair.push();
    let kept = registers.clone();
    let written = pair
        .host
        .db
        .write({
            let ctx = host_ctx.clone();
            move |tx| record_host_registers(tx, &ctx, &kept)
        })
        .unwrap();
    assert_eq!(written.written(), 1);
    let again = pair
        .host
        .db
        .write(move |tx| record_host_registers(tx, &host_ctx, &registers))
        .unwrap();
    assert_eq!(again.written(), 0, "sent twice, written once");
    assert_eq!(
        pair.count_host(
            "SELECT COUNT(*) FROM controlled_register WHERE patient_name = 'M. Haddad'"
        ),
        1
    );
}

#[test]
fn the_main_pc_refuses_entries_that_name_no_line_of_the_sale() {
    let (pair, med) = pharmacy_pair();
    let ctx = owner_on_till(&pair);
    let open = ctx.clone();
    pair.till
        .write(move |tx| crate::cash::open_session(tx, &open, Money(10_000)))
        .unwrap();
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&med, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(50_000))],
    };
    pair.till
        .write(move |tx| create_sale_with(tx, &ctx, input, &prescribed()))
        .unwrap();
    pair.push();
    let mut forged = pair.till.read(pending_host_registers).unwrap().remove(0);
    forged.controlled[0].line_id = "00000000-0000-7000-8000-000000000bad".into();
    let host_ctx = pair.host.owner_till();
    let refused = pair
        .host
        .db
        .write(move |tx| record_host_registers(tx, &host_ctx, &forged))
        .unwrap_err();
    assert_eq!(refused.code(), "value_invalid");
    assert_eq!(
        pair.count_host("SELECT COUNT(*) FROM controlled_register"),
        0
    );
}

#[test]
fn a_sale_on_the_main_pc_still_writes_its_register_entry_in_its_transaction() {
    let (pair, med) = pharmacy_pair();
    let ctx = pair.host.owner_till();
    let open = ctx.clone();
    pair.host
        .db
        .write(move |tx| crate::cash::open_session(tx, &open, Money(10_000)))
        .unwrap();
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&med, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(50_000))],
    };
    pair.host
        .db
        .write(move |tx| create_sale_with(tx, &ctx, input, &prescribed()))
        .unwrap();
    assert_eq!(
        pair.count_host("SELECT COUNT(*) FROM controlled_register"),
        1
    );
    assert!(
        pair.host
            .db
            .read(pending_host_registers)
            .unwrap()
            .is_empty()
    );
}

/// A till reads the controlled register from the main PC (its entries, a host service);
/// the report it builds from them is the very report the main PC runs.
#[test]
fn the_register_report_built_from_the_main_pcs_entries_is_the_main_pcs_report() {
    let (pair, med) = pharmacy_pair();
    let ctx = pair.host.owner_till();
    let open = ctx.clone();
    pair.host
        .db
        .write(move |tx| crate::cash::open_session(tx, &open, Money(10_000)))
        .unwrap();
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&med, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(50_000))],
    };
    pair.host
        .db
        .write(move |tx| create_sale_with(tx, &ctx, input, &prescribed()))
        .unwrap();
    let request = crate::reports::ReportRequest::new(
        crate::reports::ReportKind::ControlledRegister,
        "2026-09-28",
        "2026-09-28",
    );
    let (run, pack) = (request.clone(), Arc::clone(&pair.host.pack));
    let on_host = pair
        .host
        .db
        .read(move |c| crate::reports::run_with(c, &run, &pack))
        .unwrap();
    let entries = pair
        .host
        .db
        .read(|c| crate::pharmacy::register::entries(c, "2026-09-28", "2026-09-28"))
        .unwrap();
    let built = crate::reports::controlled_register_from(&request, entries).unwrap();
    assert_eq!(built, on_host);
    assert_eq!(built.table.rows.len(), 1);
}

const IMEI_1: &str = "356938035643809";
const IMEI_2: &str = "356938035643817";

/// Task 31: a main PC with the serials, flexy, gift-voucher and repairs modules on, a phone
/// tracked by IMEI with two pieces in stock (`IMEI_1`, `IMEI_2`), a Mobilis float of 10 000 DA
/// and a 5 000 DA gift voucher; its till caught up, with the owner's drawer open there. The
/// phone and the voucher's code.
fn ledgers_pair() -> (LanPair, String, String) {
    use crate::flexy::FloatPurchase;
    let pair = lan_pair();
    let (actor, pack) = (pair.host.owner.actor(), Arc::clone(&pair.host.pack));
    pair.host
        .db
        .write(move |tx| {
            crate::packs::switch_modules(
                tx,
                &actor,
                &pack,
                &[
                    Module::Serials,
                    Module::Flexy,
                    Module::GiftVouchers,
                    Module::Repairs,
                ],
            )
        })
        .unwrap();
    let phone = pair
        .host
        .product("Redmi Note 14", Money(3_200_000), &pair.host.tva19)
        .id;
    let id = phone.clone();
    pair.host
        .db
        .write(move |tx| {
            crate::catalog::extras::set_quiet(
                tx,
                &id,
                crate::catalog::extras::ProductExtrasInput {
                    track_serials: true,
                    ..Default::default()
                },
            )?;
            for imei in [IMEI_1, IMEI_2] {
                crate::stock::serials::register(
                    tx,
                    &id,
                    imei,
                    Some(crate::stock::SHOP_LOCATION_ID),
                    None,
                    NOON,
                )?;
            }
            Ok(())
        })
        .unwrap();
    pair.host.stock_in(&phone, 2_000);
    pair.host.open_till(10_000);
    let (ctx, pack) = (pair.host.owner_till(), Arc::clone(&pair.host.pack));
    pair.host
        .db
        .write(move |tx| {
            crate::flexy::purchase(
                tx,
                &ctx,
                &pack,
                FloatPurchase {
                    operator: "mobilis".into(),
                    amount: Money(1_000_000),
                    cost: Money(960_000),
                    from_till: false,
                    note: None,
                },
            )
            .map(drop)
        })
        .unwrap();
    let ctx = pair.host.owner_till();
    let voucher = pair
        .host
        .db
        .write(move |tx| {
            crate::gift_vouchers::sell(
                tx,
                &ctx,
                crate::gift_vouchers::SellInput {
                    amount: Money(500_000),
                    method: crate::payments::PaymentMethod::CardCib,
                    reference: Some("A1".into()),
                    cheque_bank: None,
                    cheque_date: None,
                    customer_id: None,
                    buyer_name: None,
                },
            )
        })
        .unwrap();
    pair.pull();
    let open = owner_on_till(&pair);
    pair.till
        .write(move |tx| crate::cash::open_session(tx, &open, Money(10_000)))
        .unwrap();
    (pair, phone, voucher.code)
}

/// A `voucher` tender naming `reference` (a gift voucher, a deposit's `RP…`).
fn tender(reference: &str, amount: i64) -> PaymentInput {
    PaymentInput {
        method: crate::payments::PaymentMethod::Voucher,
        amount: Money(amount),
        reference: Some(reference.to_owned()),
        cheque_bank: None,
        cheque_date: None,
    }
}

fn card(amount: i64) -> PaymentInput {
    PaymentInput::electronic(
        crate::payments::PaymentMethod::CardCib,
        Money(amount),
        Some("A2"),
    )
}

/// A Mobilis top-up line of `amount` centimes, as the till's flexy dialog makes it.
fn top_up(pair: &LanPair, amount: i64) -> CartLine {
    let pack = Arc::clone(&pair.host.pack);
    pair.till
        .read(move |c| crate::flexy::line(c, &pack, "mobilis", "0661234567", Money(amount)))
        .unwrap()
}

/// The owner's sale on the till: `lines` for `customer`, paid by `payments`, cart line 0 sold by
/// its IMEI when one is given.
fn till_sale(
    pair: &LanPair,
    lines: Vec<CartLine>,
    customer: Option<&str>,
    payments: Vec<PaymentInput>,
    imei: Option<&str>,
) -> Result<super::ValidatedSale, crate::error::AppError> {
    let mut cart = Cart::new(Lang::Fr, lines);
    cart.customer_id = customer.map(str::to_owned);
    let extras = SaleExtras {
        serials: imei
            .map(|no| super::extensions::SaleSerial {
                line: 0,
                serial_no: no.to_owned(),
            })
            .into_iter()
            .collect(),
        ..SaleExtras::default()
    };
    let (ctx, input) = (owner_on_till(pair), SaleInput { cart, payments });
    pair.till
        .write(move |tx| create_sale_with(tx, &ctx, input, &extras))
}

/// Task 31: the serials, the flexy float and the gift vouchers are the main
/// PC's ledgers. A till's sale of a phone by its IMEI with a top-up, paid with a gift voucher and
/// a card, commits on the till without touching them and keeps what they need for the main PC,
/// which links the serial, draws the float and spends the voucher against the till's ticket once
/// the ticket reached it, once.
#[test]
fn a_tills_serial_top_up_and_voucher_are_booked_by_the_main_pc_once() {
    let (pair, phone, voucher) = ledgers_pair();
    let ctx = owner_on_till(&pair);
    let top_up = top_up(&pair, 100_000);
    let flexy_product = top_up.product_id.clone().unwrap();
    assert!(
        pair.till
            .read({
                let ids = [phone.clone(), flexy_product];
                move |c| needs_host_registers(c, ids.iter().map(String::as_str))
            })
            .unwrap(),
        "a serial and a top-up need the main PC's ledgers"
    );
    let input = SaleInput {
        cart: Cart::new(
            Lang::Fr,
            vec![CartLine::product(&phone, Qty(1_000)), top_up],
        ),
        payments: vec![
            PaymentInput {
                method: crate::payments::PaymentMethod::Voucher,
                amount: Money(500_000),
                reference: Some(voucher.clone()),
                cheque_bank: None,
                cheque_date: None,
            },
            PaymentInput::electronic(
                crate::payments::PaymentMethod::CardCib,
                Money(2_800_000),
                Some("A2"),
            ),
        ],
    };
    let extras = SaleExtras {
        serials: vec![super::extensions::SaleSerial {
            line: 0,
            serial_no: "356938035643809".into(),
        }],
        ..SaleExtras::default()
    };
    let sale = pair
        .till
        .write(move |tx| create_sale_with(tx, &ctx, input, &extras))
        .expect("the till's sale commits");
    assert_eq!(
        (
            pair.count_till("SELECT COUNT(*) FROM serials WHERE status = 'sold'"),
            pair.count_till("SELECT COUNT(*) FROM flexy_float WHERE kind = 'sale'"),
            pair.count_till("SELECT COUNT(*) FROM gift_voucher_ledger WHERE kind = 'redeem'"),
        ),
        (0, 0, 0),
        "the main PC's ledgers are left to the main PC"
    );
    let pending = pair.till.read(pending_host_registers).unwrap();
    assert_eq!(pending.len(), 1);
    let json = serde_json::to_value(&pending[0]).unwrap();
    assert_eq!(json["documentId"], sale.document.id.as_str());
    assert_eq!(json["serials"][0]["serialNo"], "356938035643809");
    assert_eq!(
        json["serials"][0]["lineId"],
        sale.document.lines[0].id.as_str()
    );
    assert_eq!(json["ledgers"], true);

    let host_ctx = pair.host.owner_till();
    let early = pending[0].clone();
    let refused = pair
        .host
        .db
        .write({
            let ctx = host_ctx.clone();
            move |tx| record_host_registers(tx, &ctx, &early)
        })
        .unwrap_err();
    assert_eq!(refused.code(), "document_not_found");
    pair.push();
    let kept = pending[0].clone();
    let written = pair
        .host
        .db
        .write({
            let ctx = host_ctx.clone();
            move |tx| record_host_registers(tx, &ctx, &kept)
        })
        .unwrap();
    assert!(written.written() > 0);
    assert_eq!(written.ledgers_refused, None);
    let again = pending[0].clone();
    assert_eq!(
        pair.host
            .db
            .write(move |tx| record_host_registers(tx, &host_ctx, &again))
            .unwrap()
            .written(),
        0,
        "sent twice, booked once"
    );
    assert_eq!(
        (
            pair.count_host(
                "SELECT COUNT(*) FROM serials WHERE status = 'sold' AND sale_line_id IS NOT NULL"
            ),
            pair.count_host("SELECT COALESCE(SUM(amount), 0) FROM flexy_float WHERE kind = 'sale'"),
            pair.count_host(
                "SELECT COALESCE(SUM(amount), 0) FROM gift_voucher_ledger WHERE kind = 'redeem'"
            ),
        ),
        (1, -100_000, -500_000),
        "the main PC linked the IMEI, drew the float and spent the voucher"
    );
}

/// Task 31 review: a till checks a sale against its copy of the main PC's ledgers, and the copy
/// shows nothing of what this till's earlier sales claimed while the main PC has not booked them
/// (the call failed, or the main PC refused it). Those claims are counted: the same IMEI, the
/// float a top-up drew, a gift voucher or a deposit already spent are not sold or spent again on
/// this till.
#[test]
fn a_tills_next_sale_counts_what_its_sales_still_waiting_for_the_main_pc_claim() {
    use crate::deposits::{DepositInput, OwnerType};
    use crate::service_tickets::{IntakeInput, ServiceKind};
    let (pair, phone, voucher) = ledgers_pair();
    // Nassim's repair holds a 2 000 DA deposit, taken on the main PC
    let nassim = pair.host.customer("Nassim").id;
    let (ctx, customer_id) = (pair.host.owner_till(), nassim.clone());
    let repair = pair
        .host
        .db
        .write(move |tx| {
            let ticket = crate::service_tickets::intake(
                tx,
                &ctx,
                IntakeInput {
                    kind: ServiceKind::Repair,
                    customer_id,
                    item: "Samsung A54".into(),
                    brand: None,
                    model: None,
                    serial_no: None,
                    accessories: None,
                    condition: None,
                    fault: "Écran".into(),
                    estimate: Some(Money(1_500_000)),
                    promised_on: None,
                    technician_id: None,
                    lang: Lang::Fr,
                    items: vec![],
                },
            )?;
            crate::deposits::take(
                tx,
                &ctx,
                DepositInput {
                    owner_type: OwnerType::ServiceTicket,
                    owner_id: ticket.id.clone(),
                    amount: Money(200_000),
                    method: crate::payments::PaymentMethod::Cash,
                    reference: None,
                    cheque_bank: None,
                    cheque_date: None,
                    note: None,
                },
            )?;
            Ok(ticket.number)
        })
        .unwrap();
    pair.pull();

    // A: the first IMEI, 6 000 DA of the 10 000 DA float, the whole voucher and the whole deposit;
    // the main PC has not booked it (nothing sent here)
    let a = till_sale(
        &pair,
        vec![
            CartLine::product(&phone, Qty(1_000)),
            top_up(&pair, 600_000),
        ],
        Some(&nassim),
        vec![
            tender(&voucher, 500_000),
            tender(&repair, 200_000),
            card(3_100_000),
        ],
        Some(IMEI_1),
    )
    .expect("A commits on the till");
    assert_eq!(pair.till.read(pending_host_registers).unwrap().len(), 1);

    let same_imei = till_sale(
        &pair,
        vec![CartLine::product(&phone, Qty(1_000))],
        None,
        vec![card(3_200_000)],
        Some(IMEI_1),
    )
    .expect_err("A's IMEI is not sold twice");
    assert_eq!(same_imei.code(), "serial_status_invalid");
    let float = till_sale(
        &pair,
        vec![top_up(&pair, 500_000)],
        None,
        vec![card(500_000)],
        None,
    )
    .expect_err("A drew 6 000 DA of the float: 5 000 DA more is short");
    assert_eq!(float.code(), "flexy_float_insufficient");
    let spent_voucher = till_sale(
        &pair,
        vec![CartLine::product(&phone, Qty(1_000))],
        None,
        vec![tender(&voucher, 10_000), card(3_190_000)],
        Some(IMEI_2),
    )
    .expect_err("A spent the whole voucher");
    assert_eq!(spent_voucher.code(), "voucher_balance_insufficient");
    let spent_deposit = till_sale(
        &pair,
        vec![CartLine::product(&phone, Qty(1_000))],
        Some(&nassim),
        vec![tender(&repair, 10_000), card(3_190_000)],
        Some(IMEI_2),
    )
    .expect_err("A spent the whole deposit");
    assert_eq!(spent_deposit.code(), "deposit_insufficient");

    // what A did not claim still sells
    till_sale(
        &pair,
        vec![
            CartLine::product(&phone, Qty(1_000)),
            top_up(&pair, 400_000),
        ],
        None,
        vec![card(3_600_000)],
        Some(IMEI_2),
    )
    .expect("the other IMEI and the 4 000 DA left of the float");
    assert_eq!(
        pair.count_till("SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket'"),
        2,
        "A and the last sale; every refused sale left no ticket"
    );
    assert!(a.document.number.is_some());
}

/// Task 31 review: once the main PC's booking of a waiting sale reached the till's copy, the
/// copy's balance holds it; the claim still waiting here (its answer was lost) is not counted a
/// second time.
#[test]
fn a_claim_the_main_pc_booked_is_counted_once() {
    let (pair, phone, voucher) = ledgers_pair();
    till_sale(
        &pair,
        vec![CartLine::product(&phone, Qty(1_000))],
        None,
        vec![tender(&voucher, 200_000), card(3_000_000)],
        Some(IMEI_1),
    )
    .expect("A spends 2 000 DA of the voucher");
    let pending = pair.till.read(pending_host_registers).unwrap();
    pair.push();
    let (ctx, sent) = (pair.host.owner_till(), pending[0].clone());
    pair.host
        .db
        .write(move |tx| record_host_registers(tx, &ctx, &sent))
        .unwrap();
    pair.pull();
    assert_eq!(
        pair.till.read(pending_host_registers).unwrap(),
        pending,
        "the answer was lost: A still waits here"
    );
    till_sale(
        &pair,
        vec![CartLine::product(&phone, Qty(1_000))],
        None,
        vec![tender(&voucher, 300_000), card(2_900_000)],
        Some(IMEI_2),
    )
    .expect("the 3 000 DA the copy shows left are spent once");
}

/// Final review: a till's ticket of a controlled medicine paid with a gift voucher another PC
/// spent meanwhile. The main PC writes the register entry all the same (it depends on no balance)
/// and refuses the voucher alone, which rolls back alone; the till's pending entry keeps only the
/// refused ledger part. Sent again, the entry is written once and the refusal stands.
#[test]
fn a_refused_voucher_never_keeps_a_controlled_medicine_out_of_the_register() {
    let (pair, med) = pharmacy_pair();
    let (actor, pack) = (pair.host.owner.actor(), Arc::clone(&pair.host.pack));
    pair.host
        .db
        .write(move |tx| {
            crate::packs::switch_modules(
                tx,
                &actor,
                &pack,
                &[Module::Pharmacy, Module::GiftVouchers],
            )
        })
        .unwrap();
    pair.host.open_till(10_000);
    let code = sell_voucher(&pair, pair.host.owner_till(), 30_000);
    pair.pull();
    let open = owner_on_till(&pair);
    pair.till
        .write(move |tx| crate::cash::open_session(tx, &open, Money(10_000)))
        .unwrap();
    let (ctx, voucher_code) = (owner_on_till(&pair), code.clone());
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&med, Qty(1_000))]),
        payments: vec![
            tender(&voucher_code, 30_000),
            PaymentInput::cash(Money(15_000)),
        ],
    };
    let sale = pair
        .till
        .write(move |tx| create_sale_with(tx, &ctx, input, &prescribed()))
        .expect("the till's copy shows the voucher whole");
    // the main PC spends the whole voucher on its own ticket first
    let other = pair
        .host
        .product("Doliprane 1 g", Money(30_000), &pair.host.exempt);
    pair.host.stock_in(&other.id, 1_000);
    let (host_ctx, spent) = (pair.host.owner_till(), code.clone());
    pair.host
        .db
        .write(move |tx| {
            create_sale_with(
                tx,
                &host_ctx,
                SaleInput {
                    cart: Cart::new(Lang::Fr, vec![CartLine::product(&other.id, Qty(1_000))]),
                    payments: vec![tender(&spent, 30_000)],
                },
                &SaleExtras::default(),
            )
        })
        .unwrap();
    pair.push();

    let pending = pair.till.read(pending_host_registers).unwrap();
    assert_eq!(pending.len(), 1);
    let (ctx, sent) = (pair.host.owner_till(), pending[0].clone());
    let booking = pair
        .host
        .db
        .write(move |tx| record_host_registers(tx, &ctx, &sent))
        .expect("the registers part is kept");
    assert_eq!(booking.registers_written, 1);
    assert!(!booking.ledgers_booked);
    assert_eq!(
        booking.ledgers_refused.as_deref(),
        Some("voucher_balance_insufficient")
    );
    assert_eq!(
        pair.count_host(
            "SELECT COUNT(*) FROM controlled_register WHERE patient_name = 'M. Haddad'"
        ),
        1,
        "the register entry is on the main PC"
    );
    let till_doc = sale.document.id.clone();
    assert_eq!(
        pair.host
            .db
            .read(move |c| Ok(c.query_row(
                "SELECT COUNT(*) FROM gift_voucher_ledger WHERE document_id = ?1",
                [&till_doc],
                |r| r.get::<_, i64>(0),
            )?))
            .unwrap(),
        0,
        "the refused spend rolled back alone"
    );
    assert_eq!(
        pair.count_host("SELECT COUNT(*) FROM audit_log WHERE action = 'registers.host_booked'"),
        1
    );
    assert_eq!(
        pair.count_host(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'pharmacy.register_recorded'"
        ),
        0,
        "journaled once, by the booking's own row"
    );

    // the till keeps only the refused ledger part, its health data gone
    let (id, answered) = (sale.document.id.clone(), booking.clone());
    pair.till
        .write(move |tx| super::extensions::host_registers_answered(tx, &id, &answered))
        .unwrap();
    let left = pair.till.read(pending_host_registers).unwrap();
    assert_eq!(left.len(), 1);
    assert!(
        left[0].controlled.is_empty(),
        "no patient data waits on the till"
    );
    assert!(left[0].ledgers);
    assert_eq!(
        left[0].refused.as_deref(),
        Some("voucher_balance_insufficient")
    );
    let entries = pair
        .till
        .read(super::extensions::pending_host_entries)
        .unwrap();
    assert_eq!(
        entries[0].kinds,
        vec![super::extensions::PendingKind::Ledgers]
    );
    assert!(!entries[0].holds_registers());

    // sent again (the full entry, as after a lost answer): nothing is written twice
    let (ctx, again) = (pair.host.owner_till(), pending[0].clone());
    let second = pair
        .host
        .db
        .write(move |tx| record_host_registers(tx, &ctx, &again))
        .unwrap();
    assert_eq!(second.registers_written, 0);
    assert_eq!(
        second.ledgers_refused.as_deref(),
        Some("voucher_balance_insufficient")
    );
    assert_eq!(
        pair.count_host("SELECT COUNT(*) FROM controlled_register"),
        1
    );
    assert_eq!(
        pair.count_host("SELECT COUNT(*) FROM audit_log WHERE action = 'registers.host_booked'"),
        1,
        "nothing written, nothing journaled"
    );
}

/// A gift voucher of `amount` centimes sold on the main PC by card with `ctx`; its code.
fn sell_voucher(pair: &LanPair, ctx: TillContext, amount: i64) -> String {
    pair.host
        .db
        .write(move |tx| {
            crate::gift_vouchers::sell(
                tx,
                &ctx,
                crate::gift_vouchers::SellInput {
                    amount: Money(amount),
                    method: crate::payments::PaymentMethod::CardCib,
                    reference: Some("A9".into()),
                    cheque_bank: None,
                    cheque_date: None,
                    customer_id: None,
                    buyer_name: None,
                },
            )
        })
        .unwrap()
        .code
}

/// Final review: a till's ticket of day D booked by the main PC on D+1 is booked as of D, under
/// the till's terminal and the ticket's time: its float, voucher and deposit rows, and a voucher
/// that expires on D is still accepted. The audit row of the booking is the main PC's.
#[test]
fn a_tills_ticket_booked_the_next_day_is_booked_on_its_own_day_and_terminal() {
    use crate::deposits::{DepositInput, OwnerType};
    use crate::service_tickets::{IntakeInput, ServiceKind};
    const DAY: i64 = 86_400_000;
    let (pair, _phone, _voucher) = ledgers_pair();
    // a voucher sold a month before D, valid one month: it expires on D
    pair.host
        .db
        .write(|tx| {
            crate::settings::set_quiet(tx, "vouchers.validity_months", serde_json::json!(1))
                .map(drop)
        })
        .unwrap();
    let mut a_month_before = pair.host.owner_till();
    a_month_before.now = NOON - 31 * DAY;
    let code = sell_voucher(&pair, a_month_before, 100_000);
    let nassim = pair.host.customer("Nassim").id;
    let (ctx, customer_id) = (pair.host.owner_till(), nassim.clone());
    let repair = pair
        .host
        .db
        .write(move |tx| {
            let ticket = crate::service_tickets::intake(
                tx,
                &ctx,
                IntakeInput {
                    kind: ServiceKind::Repair,
                    customer_id,
                    item: "Samsung A54".into(),
                    brand: None,
                    model: None,
                    serial_no: None,
                    accessories: None,
                    condition: None,
                    fault: "Écran".into(),
                    estimate: None,
                    promised_on: None,
                    technician_id: None,
                    lang: Lang::Fr,
                    items: vec![],
                },
            )?;
            crate::deposits::take(
                tx,
                &ctx,
                DepositInput {
                    owner_type: OwnerType::ServiceTicket,
                    owner_id: ticket.id.clone(),
                    amount: Money(50_000),
                    method: crate::payments::PaymentMethod::Cash,
                    reference: None,
                    cheque_bank: None,
                    cheque_date: None,
                    note: None,
                },
            )?;
            Ok(ticket.number)
        })
        .unwrap();
    pair.pull();
    let sale = till_sale(
        &pair,
        vec![top_up(&pair, 200_000)],
        Some(&nassim),
        vec![
            tender(&code, 100_000),
            tender(&repair, 50_000),
            card(50_000),
        ],
        None,
    )
    .expect("D's ticket on the till");
    pair.push();
    let pending = pair.till.read(pending_host_registers).unwrap();
    let mut next_day = pair.host.owner_till();
    next_day.now = NOON + DAY;
    let sent = pending[0].clone();
    let booking = pair
        .host
        .db
        .write(move |tx| record_host_registers(tx, &next_day, &sent))
        .expect("booked on D+1");
    assert_eq!(booking.ledgers_refused, None, "the voucher was valid on D");
    assert!(booking.ledgers_booked);
    let validated = sale.document.validated_at.unwrap();
    let till_terminal = pair.till_terminal.id.clone();
    for table in [
        "flexy_float WHERE kind = 'sale'",
        "gift_voucher_ledger WHERE kind = 'redeem'",
        "deposits WHERE kind = 'applied'",
    ] {
        let sql = format!(
            "SELECT business_date, terminal_id, created_at FROM {table} AND document_id = ?1"
        );
        let id = sale.document.id.clone();
        let row: (String, String, i64) = pair
            .host
            .db
            .read(move |c| Ok(c.query_row(&sql, [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?))
            .unwrap();
        assert_eq!(
            row,
            ("2026-09-28".to_owned(), till_terminal.clone(), validated),
            "{table}"
        );
    }
    let host_terminal = pair.host.terminal.id.clone();
    let audit: String = pair
        .host
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT terminal_id FROM audit_log WHERE action = 'registers.host_booked'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        audit, host_terminal,
        "the booking is journaled as the main PC's"
    );
}

/// Final review: a jewels-only booking writes its entry and exactly one audit row, without the
/// buyer's identity; sent again, nothing more.
#[test]
fn a_jewels_only_booking_adds_exactly_one_audit_row() {
    let pair = lan_pair();
    let actor = Actor::system(None);
    pair.host
        .db
        .write(move |tx| crate::packs::set_modules(tx, &actor, &[Module::Jewellery]))
        .unwrap();
    let ring = pair
        .host
        .product("Bague or 18 carats", Money(9_000_000), &pair.host.exempt);
    pair.host.stock_in(&ring.id, 1_000);
    pair.pull();
    let open = owner_on_till(&pair);
    pair.till
        .write(move |tx| crate::cash::open_session(tx, &open, Money(10_000)))
        .unwrap();
    let (ctx, id) = (owner_on_till(&pair), ring.id.clone());
    let sale = pair
        .till
        .write(move |tx| {
            create_sale_with(
                tx,
                &ctx,
                SaleInput {
                    cart: Cart::new(Lang::Fr, vec![CartLine::product(&id, Qty(1_000))]),
                    payments: vec![card(9_000_000)],
                },
                &SaleExtras::default(),
            )
        })
        .unwrap();
    pair.push();
    let registers = HostRegisters {
        document_id: sale.document.id.clone(),
        jewels: vec![crate::jewellery::JewelRow {
            business_date: sale.document.business_date.clone(),
            document_id: sale.document.id.clone(),
            line_id: sale.document.lines[0].id.clone(),
            product_id: ring.id.clone(),
            nature: "Bague".into(),
            pieces: 1,
            weight_mg: 5_200,
            titre_permille: 750,
            price: Money(9_000_000),
            buyer_name: "Mme Amrani".into(),
            buyer_id_kind: crate::jewellery::BuyerIdKind::Cni,
            buyer_id_number: "109876543".into(),
            buyer_address: "Tizi Ouzou".into(),
            created_at: NOON,
            user_id: pair.till_owner.user_id.clone(),
            terminal_id: pair.till_terminal.id.clone(),
        }],
        ..HostRegisters::default()
    };
    let before = pair.count_host("SELECT COUNT(*) FROM audit_log");
    let (ctx, sent) = (pair.host.owner_till(), registers.clone());
    let booking = pair
        .host
        .db
        .write(move |tx| record_host_registers(tx, &ctx, &sent))
        .unwrap();
    assert_eq!(booking.registers_written, 1);
    assert_eq!(
        pair.count_host("SELECT COUNT(*) FROM audit_log"),
        before + 1
    );
    let payload: String = pair
        .host
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'registers.host_booked'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(payload.contains("\"jewels\":1"), "{payload}");
    assert!(
        !payload.contains("Amrani") && !payload.contains("109876543"),
        "no buyer data in the journal"
    );
    let (ctx, again) = (pair.host.owner_till(), registers);
    pair.host
        .db
        .write(move |tx| record_host_registers(tx, &ctx, &again))
        .unwrap();
    assert_eq!(
        pair.count_host("SELECT COUNT(*) FROM audit_log"),
        before + 1,
        "sent twice, journaled once"
    );
}
