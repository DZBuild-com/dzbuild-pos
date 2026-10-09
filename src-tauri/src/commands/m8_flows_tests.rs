//! Command flows of phones, services, instalments, rounds and orders (Task 20).

use super::testshop::{TestShop, shop_in, wait_for_text};
use super::{instalments, orders, services, wholesale};
use pos_core::company::ShopActivity;
use pos_core::deposits::{DepositInput, OwnerType};
use pos_core::instalments::{GuaranteeInput, GuaranteeKind, QuoteInput, SignInput};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::orders::{CustomerOrderInput, OrderDeposit, OrderKind};
use pos_core::packs::Module;
use pos_core::payments::PaymentMethod;
use pos_core::sales::{Cart, CartLine, SaleExtras};
use pos_core::service_tickets::{IntakeInput, ServiceKind, ServiceTicket};
use pos_core::wholesale::rounds::{RoundInput, StopReport, StopStatus};

/// The activity template of `activity` with its seeds, as the setup wizard applies it.
fn template(t: &TestShop, activity: ShopActivity) {
    let (actor, pack) = (t.owner.actor(), t.st.datapack.clone());
    t.st.db
        .write(move |tx| {
            pos_core::packs::apply_template(tx, &actor, &pack, activity, Lang::Fr, true)
        })
        .unwrap();
}

fn modules(t: &TestShop, modules: Vec<Module>) {
    let actor = t.owner.actor();
    t.st.db
        .write(move |tx| pos_core::packs::set_modules(tx, &actor, &modules))
        .unwrap();
}

/// Walid's phone with a broken screen, in French.
fn intake_sample(kind: ServiceKind, customer_id: &str) -> IntakeInput {
    IntakeInput {
        kind,
        customer_id: customer_id.into(),
        item: "Samsung A54".into(),
        brand: Some("Samsung".into()),
        model: Some("A54".into()),
        serial_no: Some("356938035643809".into()),
        accessories: Some("Chargeur".into()),
        condition: Some("Écran fissuré".into()),
        fault: "Remplacement écran".into(),
        estimate: Some(Money(1_500_000)),
        promised_on: None,
        technician_id: None,
        lang: Lang::Fr,
        items: Vec::new(),
    }
}

fn deposit_for(ticket: &ServiceTicket, amount: Money, method: PaymentMethod) -> DepositInput {
    DepositInput {
        owner_type: OwnerType::ServiceTicket,
        owner_id: ticket.id.clone(),
        amount,
        method,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

fn round_input(driver: &str) -> RoundInput {
    RoundInput {
        driver_name: driver.into(),
        vehicle_plate: Some("01234-116-16".into()),
        ..RoundInput::default()
    }
}

fn delivered(seq: u32, collected: Money, method: PaymentMethod) -> StopReport {
    StopReport {
        seq,
        status: StopStatus::Delivered,
        collected,
        method: Some(method),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

fn order_input(
    customer_id: &str,
    product_id: &str,
    pieces: i64,
    kind: OrderKind,
    deposit: Option<Money>,
) -> CustomerOrderInput {
    let mut cart = Cart::new(
        Lang::Fr,
        vec![CartLine::product(product_id, Qty(pieces * 1_000))],
    );
    cart.customer_id = Some(customer_id.into());
    CustomerOrderInput {
        draft_id: None,
        cart,
        kind,
        due_on: None,
        reserve: false,
        deposit: deposit.map(|amount| OrderDeposit {
            amount,
            method: PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        }),
        note: None,
    }
}

#[test]
fn a_repair_is_taken_in_with_a_deposit_and_both_slips_print() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    template(&t, ShopActivity::Phones);
    super::testshop::use_file_printers(st, dir.path(), &["caisse.bin"]);
    t.open_till(50_000);
    let customer = t.customer("Walid");
    let ticket = services::intake(st, intake_sample(ServiceKind::Repair, &customer)).unwrap();
    assert!(ticket.number.starts_with("RP2026-"), "{}", ticket.number);
    let deposit = services::deposit_take(
        st,
        deposit_for(&ticket, Money(200_000), PaymentMethod::Cash),
    )
    .unwrap();
    assert_eq!(deposit.receipt_number.as_deref(), Some("AC2026-000001"));
    let caisse = dir.path().join("caisse.bin");
    wait_for_text(&caisse, &ticket.number);
    let printed = wait_for_text(&caisse, "AC2026-000001");
    assert!(
        printed.contains("Samsung A54"),
        "the intake slip names the item"
    );
}

#[test]
fn an_instalment_contract_opens_as_a_print_host_page() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    template(&t, ShopActivity::Instalments);
    t.open_till(50_000);
    let fridge = t.product("Réfrigérateur", 12_000_000, 1_900, None);
    t.stock_in(&fridge.id, 2);
    let customer = t.customer("Farid");
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&fridge.id, Qty(1_000))]);
    cart.customer_id = Some(customer);
    let quote = QuoteInput {
        cart,
        down: Money(3_000_000),
        down_method: PaymentMethod::Cash,
        count: 6,
        first_due_on: "2026-10-31".into(),
    };
    let signed = instalments::sign(
        st,
        SignInput {
            quote,
            down_reference: None,
            cheque_bank: None,
            cheque_date: None,
            guarantees: vec![GuaranteeInput {
                kind: GuaranteeKind::PaySlip,
                holder_name: None,
                reference: None,
                phone: None,
                note: None,
            }],
            extras: SaleExtras::default(),
            note: None,
        },
        false,
    )
    .unwrap();
    assert!(
        signed.print_queued,
        "the down payment in cash opens the drawer"
    );
    let page = instalments::contract(st, signed.signed.plan.id.clone(), Lang::Ar).unwrap();
    match page {
        crate::print_host::HostPage::Contract(c) => {
            assert_eq!((c.dues.len(), c.rtl), (6, true));
            assert_eq!(c.buyer.name, "Farid");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_round_closes_with_its_receipts_and_an_order_is_fulfilled_at_the_till() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    template(&t, ShopActivity::Wholesale);
    modules(
        &t,
        vec![Module::Wholesale, Module::DeliveryRounds, Module::Orders],
    );
    super::testshop::use_file_printers(st, dir.path(), &["caisse.bin"]);
    t.open_till(50_000);
    let flour = t.product("Farine 25 kg", 250_000, 900, None);
    t.stock_in(&flour.id, 50);
    let amine = t.customer("Épicerie Amine");
    let bl = t.delivery_note_on_account(&amine, &flour.id, 2);
    let round = wholesale::round_create(st, round_input("Rachid")).unwrap();
    wholesale::round_set_stops(st, round.id.clone(), vec![bl.id.clone()]).unwrap();
    wholesale::round_start(st, round.id.clone()).unwrap();
    wholesale::round_report_stop(
        st,
        round.id.clone(),
        delivered(1, Money(500_000), PaymentMethod::Cash),
    )
    .unwrap();
    let closed = wholesale::round_close(st, round.id.clone()).unwrap();
    assert_eq!(closed.round.versements.len(), 1);
    assert!(closed.print_queued);
    wait_for_text(&dir.path().join("caisse.bin"), "RV2026-");
    let order = orders::create(
        st,
        order_input(&amine, &flour.id, 3, OrderKind::Order, Some(Money(100_000))),
    )
    .unwrap();
    assert_eq!(order.held, Money(100_000));
    wait_for_text(&dir.path().join("caisse.bin"), "AC2026-000001");
    let cart = orders::fulfil_cart(st, order.id.clone()).unwrap();
    assert_eq!(cart.lines[0].unit_price, Some(Money(250_000)));
}

#[test]
fn the_route_sheet_opens_as_a_print_host_page() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    template(&t, ShopActivity::Wholesale);
    t.open_till(50_000);
    let flour = t.product("Farine 25 kg", 250_000, 900, None);
    t.stock_in(&flour.id, 50);
    let amine = t.customer("Épicerie Amine");
    let bl = t.delivery_note_on_account(&amine, &flour.id, 2);
    let round = wholesale::round_create(st, round_input("Rachid")).unwrap();
    wholesale::round_set_stops(st, round.id.clone(), vec![bl.id.clone()]).unwrap();
    match wholesale::route_sheet(st, round.id.clone(), Lang::Fr).unwrap() {
        crate::print_host::HostPage::RouteSheet(r) => {
            assert_eq!((r.stops.len(), r.driver.as_str()), (1, "Rachid"));
            assert_eq!(r.total_due, Money(500_000));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_delivery_note_names_the_bl_as_invoice_decision_once_it_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    let st = &t.st;
    template(&t, ShopActivity::Wholesale);
    t.open_till(50_000);
    let flour = t.product("Farine 25 kg", 250_000, 900, None);
    t.stock_in(&flour.id, 50);
    let amine = t.customer("Épicerie Amine");
    let bl = t.delivery_note_on_account(&amine, &flour.id, 2);
    let legal = |st: &crate::state::AppState| {
        let input = super::print::PdfDocumentInput {
            id: bl.id.clone(),
            options: Default::default(),
        };
        match super::print::print_model(st, input).unwrap() {
            crate::print_host::HostPage::Document(m) => m.legal,
            other => panic!("{other:?}"),
        }
    };
    let before = legal(st);
    assert!(
        before.iter().any(|l| l.contains("pas une facture")),
        "{before:?}"
    );
    let actor = t.owner.actor();
    st.db
        .write(move |tx| {
            pos_core::settings::set_value(
                tx,
                &actor,
                "documents.bl_as_invoice",
                serde_json::json!({ "number": "117/2026", "date": "2026-01-15" }),
            )
            .map(drop)
        })
        .unwrap();
    let after = legal(st);
    assert!(
        after
            .iter()
            .any(|l| l == "BL valant facture — autorisation n° 117/2026 du 2026-01-15"),
        "{after:?}"
    );
    assert!(
        !after.iter().any(|l| l.contains("pas une facture")),
        "a delivery note standing as an invoice never says it is not one: {after:?}"
    );
}

/// A till's repair intake and its deposit are written on the main PC as the till's
/// user (the main PC's terminal, the shop-wide `RP…` and `AC…` series); the answer carries what
/// the till prints. The deposit is booked on the till's own drawer (Task 20 review).
#[test]
fn a_tills_repair_and_deposit_are_host_services_run_as_its_user() {
    use pos_server::host::{RpcCaller, RpcHandler};
    let dir = tempfile::tempdir().unwrap();
    let (t, _failures) = shop_in(dir.path());
    template(&t, ShopActivity::Phones);
    t.open_till(50_000);
    let customer = t.customer("Walid");
    // the till C2 as this copy has it once its outbox is in: its terminal and its open drawer
    let mut at_c2 = t.owner.clone();
    at_c2.terminal_id = "00000000-0000-7000-8000-0000000000c2".into();
    at_c2.terminal_code = "C2".into();
    let ctx = pos_core::till::TillContext::new(at_c2, t.st.datapack.clone(), t.st.now());
    let c2_session = t
        .st
        .db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO terminals (id, code, name, role, created_at) VALUES (?1, 'C2', 'Caisse 2', 'client', 0)",
                ["00000000-0000-7000-8000-0000000000c2"],
            )?;
            pos_core::cash::open_session(tx, &ctx, Money(0))
        })
        .unwrap()
        .id;
    let rpc = crate::lan::rpc::AppRpc::new(std::sync::Arc::downgrade(&t.st));
    let caller = || RpcCaller {
        terminal_id: "00000000-0000-7000-8000-0000000000c2".into(),
        terminal_code: "C2".into(),
        user_id: t.owner.user_id.clone(),
        approvals: Vec::new(),
        idempotency_key: None,
    };
    let input = intake_sample(ServiceKind::Repair, &customer);
    let ticket: ServiceTicket = serde_json::from_value(
        rpc.call(
            caller(),
            "service.intake",
            serde_json::json!({ "input": input }),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(ticket.number.starts_with("RP2026-"));
    let deposit = deposit_for(&ticket, Money(200_000), PaymentMethod::Cash);
    let taken: services::TakenDeposit = serde_json::from_value(
        rpc.call(
            caller(),
            "deposits.take",
            serde_json::json!({ "input": deposit }),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (
            taken.deposit.receipt_number.as_deref(),
            taken.owner_number.as_str(),
            taken.customer.as_deref(),
            taken.held
        ),
        (
            Some("AC2026-000001"),
            ticket.number.as_str(),
            Some("Walid"),
            Money(200_000)
        )
    );
    let id = taken.deposit.id.clone();
    let (terminal, user, session): (String, String, Option<String>) =
        t.st.db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT terminal_id, user_id, cash_session_id FROM deposits WHERE id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?)
            })
            .unwrap();
    assert_eq!(
        (terminal, user, session),
        (
            t.st.terminal.id.clone(),
            t.owner.user_id.clone(),
            Some(c2_session)
        )
    );
    assert_eq!(
        t.count("SELECT COUNT(*) FROM cash_movements WHERE ref_type = 'deposit'"),
        0,
        "the cash went into the till's drawer, never this PC's"
    );
}
