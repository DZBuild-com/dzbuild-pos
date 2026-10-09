//! The pack host services between a till and the main PC (Task 20 review): the money a till's
//! cashier takes or gives through the main PC lands in the till's own drawer and Z, never in the
//! main PC's (finding 1); a manager's approval given at the till travels with the call and the
//! main PC confirms it (finding 2); a connected till books delivery rounds through the main PC
//! (finding 3).

use super::rpc::AppRpc;
use super::two_pcs::{TwoPcs, close_approved};
use crate::commands::testshop::{CASHIER_PIN, MANAGER_PIN, TestShop, shop};
use crate::commands::{auth, cash as cash_cmd, services, wholesale};
use pos_core::auth::{CASHIER_ROLE_ID, MANAGER_ROLE_ID, Permission};
use pos_core::cash::{self, MovementKind};
use pos_core::deposits::{Deposit, DepositInput, OwnerType};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::packs::Module;
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::service_tickets::{
    IntakeInput, PickupInput, ServiceItemInput, ServiceKind, ServiceStatus, ServiceTicket,
};
use pos_core::till::TillContext;
use pos_core::wholesale::rounds::{
    ClosedRound, Round, RoundFilter, RoundInput, StopReport, StopStatus,
};
use pos_server::host::{RpcCaller, RpcHandler};
use pos_server::wire::RpcGrant;
use serde_json::{Value, json};

const C2: &str = "00000000-0000-7000-8000-0000000000c2";

fn modules(shop: &TestShop, modules: &[Module]) {
    let (actor, modules) = (shop.owner.actor(), modules.to_vec());
    shop.st
        .db
        .write(move |tx| pos_core::packs::set_modules(tx, &actor, &modules))
        .unwrap();
}

/// The till C2 as the main PC's copy has it once the till's outbox is in: its terminal and its
/// drawer opened with `float` by the owner. The open session's id.
fn till_c2(shop: &TestShop, float: i64) -> String {
    let mut session = shop.owner.clone();
    session.terminal_id = C2.into();
    session.terminal_code = "C2".into();
    let ctx = TillContext::new(session, shop.st.datapack.clone(), shop.st.now());
    shop.st
        .db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO terminals (id, code, name, role, created_at) VALUES (?1, 'C2', 'Caisse 2', 'client', 0)",
                [C2],
            )?;
            cash::open_session(tx, &ctx, Money(float))
        })
        .unwrap()
        .id
}

fn caller(user_id: &str, approvals: Vec<RpcGrant>) -> RpcCaller {
    RpcCaller {
        terminal_id: C2.into(),
        terminal_code: "C2".into(),
        user_id: user_id.into(),
        approvals,
        idempotency_key: None,
    }
}

fn grant(approver_id: &str, permission: Permission) -> RpcGrant {
    RpcGrant {
        approver_id: approver_id.into(),
        permission,
        granted_at: pos_core::time::now_ms(),
    }
}

fn intake(customer_id: &str) -> IntakeInput {
    IntakeInput {
        kind: ServiceKind::Repair,
        customer_id: customer_id.into(),
        item: "Samsung A54".into(),
        brand: None,
        model: None,
        serial_no: None,
        accessories: None,
        condition: None,
        fault: "Remplacement écran".into(),
        estimate: Some(Money(1_200_000)),
        promised_on: None,
        technician_id: None,
        lang: Lang::Fr,
        items: Vec::new(),
    }
}

fn cash_deposit(ticket: &ServiceTicket, amount: i64) -> DepositInput {
    DepositInput {
        owner_type: OwnerType::ServiceTicket,
        owner_id: ticket.id.clone(),
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

fn refund(ticket: &ServiceTicket, amount: i64) -> Value {
    json!({
        "ownerType": "service_ticket",
        "ownerId": ticket.id,
        "amount": amount,
        "method": "cash",
        "reference": null,
    })
}

/// `(kind, amount)` of the drawer movements naming `ref_type` on `shop`.
fn moves(shop: &TestShop, ref_type: &'static str) -> Vec<(MovementKind, Money)> {
    shop.st
        .db
        .read(move |c| {
            let mut stmt = c.prepare(
                "SELECT kind, amount FROM cash_movements WHERE ref_type = ?1 ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([ref_type], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

fn user_named(shop: &TestShop, name: &str) -> String {
    shop.st
        .db
        .read(pos_core::auth::users::list_tiles)
        .unwrap()
        .into_iter()
        .find(|t| t.display_name == name)
        .map(|t| t.id)
        .unwrap()
}

fn open_session_of(shop: &TestShop) -> String {
    let terminal = shop.st.terminal.id.clone();
    shop.st
        .db
        .read(move |c| cash::require_open(c, &terminal))
        .unwrap()
        .id
}

/// Finding 1, on the main PC: a deposit a till takes is booked on the till's open drawer, never
/// on the main PC's (which a back-office main PC never opens), and moves no drawer here.
#[test]
fn a_deposit_a_till_takes_is_booked_on_its_drawer_not_the_main_pcs() {
    let shop = shop();
    modules(&shop, &[Module::Repairs]);
    let customer = shop.customer("Walid");
    let rpc = AppRpc::new(std::sync::Arc::downgrade(&shop.st));
    let owner = shop.owner.user_id.clone();
    let ticket: ServiceTicket = serde_json::from_value(
        rpc.call(
            caller(&owner, vec![]),
            "service.intake",
            json!({ "input": intake(&customer) }),
        )
        .unwrap(),
    )
    .unwrap();
    let input = json!({ "input": cash_deposit(&ticket, 200_000) });
    let closed = rpc
        .call(caller(&owner, vec![]), "deposits.take", input.clone())
        .unwrap_err();
    assert_eq!(closed.code(), "cash_session_closed", "the till's drawer");
    let c2_session = till_c2(&shop, 0);
    let taken: services::TakenDeposit = serde_json::from_value(
        rpc.call(caller(&owner, vec![]), "deposits.take", input)
            .unwrap(),
    )
    .unwrap();
    let id = taken.deposit.id.clone();
    let booked: Option<String> = shop
        .st
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT cash_session_id FROM deposits WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(booked, Some(c2_session));
    assert!(moves(&shop, "deposit").is_empty(), "no drawer moves here");
    let terminal = shop.st.terminal.id.clone();
    assert_eq!(
        shop.st
            .db
            .read(move |c| cash::current(c, &terminal))
            .unwrap(),
        None,
        "the main PC's drawer was never needed"
    );
}

/// Finding 2: a manager's approval given at a till counts on the main PC once the main PC has
/// confirmed the approver against its own users and roles; a cashier's own or another
/// cashier's "approval" counts for nothing.
#[test]
fn a_managers_approval_given_at_a_till_travels_with_its_host_service() {
    let shop = shop();
    modules(&shop, &[Module::Repairs]);
    till_c2(&shop, 0);
    let customer = shop.customer("Walid");
    let cashier = shop.user("Yacine", CASHIER_ROLE_ID, CASHIER_PIN, 1_000);
    let other = shop.user("Sofiane", CASHIER_ROLE_ID, "6142", 0);
    let manager = shop.user("Samira", MANAGER_ROLE_ID, MANAGER_PIN, 2_000);
    let rpc = AppRpc::new(std::sync::Arc::downgrade(&shop.st));
    let ticket: ServiceTicket = serde_json::from_value(
        rpc.call(
            caller(&cashier, vec![]),
            "service.intake",
            json!({ "input": intake(&customer) }),
        )
        .unwrap(),
    )
    .unwrap();
    rpc.call(
        caller(&cashier, vec![]),
        "deposits.take",
        json!({ "input": cash_deposit(&ticket, 200_000) }),
    )
    .unwrap();
    for approvals in [
        vec![],
        vec![grant(&cashier, Permission::SalesReturn)],
        vec![grant(&other, Permission::SalesReturn)],
        vec![grant(&manager, Permission::SalesDiscount)],
    ] {
        let refused = rpc
            .call(
                caller(&cashier, approvals),
                "deposits.refund",
                refund(&ticket, 50_000),
            )
            .unwrap_err();
        assert_eq!(refused.code(), "insufficient_permission");
    }
    let refunded: Deposit = serde_json::from_value(
        rpc.call(
            caller(&cashier, vec![grant(&manager, Permission::SalesReturn)]),
            "deposits.refund",
            refund(&ticket, 50_000),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(refunded.amount, Money(-50_000));
    let id = refunded.id.clone();
    let user: String = shop
        .st
        .db
        .read(move |c| {
            Ok(
                c.query_row("SELECT user_id FROM deposits WHERE id = ?1", [&id], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(
        user, cashier,
        "made by the till's cashier, approved by Samira"
    );
}

/// Finding 3, on the main PC: every write and read of a round is a host service a till calls as
/// its user; the round's collections are booked on the till's drawer.
#[test]
fn a_round_is_booked_through_host_services_and_its_cash_goes_to_the_tills_drawer() {
    let shop = shop();
    modules(
        &shop,
        &[Module::Wholesale, Module::DeliveryRounds, Module::Orders],
    );
    let flour = shop.product("Farine 25 kg", 250_000, 900, None);
    shop.stock_in(&flour.id, 50);
    let amine = shop.customer("Épicerie Amine");
    let bl = shop.delivery_note_on_account(&amine, &flour.id, 2);
    let c2_session = till_c2(&shop, 0);
    let rpc = AppRpc::new(std::sync::Arc::downgrade(&shop.st));
    let owner = shop.owner.user_id.clone();
    let call = |service: &str, input: Value| rpc.call(caller(&owner, vec![]), service, input);
    let round: Round = serde_json::from_value(
        call(
            "rounds.create",
            json!({ "input": RoundInput { driver_name: "Rachid".into(), ..RoundInput::default() } }),
        )
        .unwrap(),
    )
    .unwrap();
    call(
        "rounds.set_stops",
        json!({ "id": round.id, "documentIds": [bl.id] }),
    )
    .unwrap();
    call("rounds.start", json!({ "id": round.id })).unwrap();
    let report = StopReport {
        seq: 1,
        status: StopStatus::Delivered,
        collected: Money(500_000),
        method: Some(PaymentMethod::Cash),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    };
    call(
        "rounds.report_stop",
        json!({ "id": round.id, "report": report }),
    )
    .unwrap();
    let day = services::business_day(&shop.st);
    let filter = RoundFilter {
        status: None,
        from: day.clone(),
        to: day,
    };
    let listed: Vec<Round> =
        serde_json::from_value(call("rounds.list", json!({ "filter": filter })).unwrap()).unwrap();
    assert_eq!(listed.len(), 1);
    let got: Round =
        serde_json::from_value(call("rounds.get", json!({ "id": round.id })).unwrap()).unwrap();
    assert_eq!(got.stops.len(), 1);
    let closed: ClosedRound =
        serde_json::from_value(call("rounds.close", json!({ "id": round.id })).unwrap()).unwrap();
    assert_eq!(closed.versements.len(), 1);
    assert_eq!(
        closed.versements[0].entry.cash_session_id.as_deref(),
        Some(c2_session.as_str())
    );
    assert!(
        moves(&shop, "customer_ledger").is_empty(),
        "the driver's cash goes into the till's drawer, recorded by the till"
    );
}

/// Findings 1 and 2 end to end on two PCs: the till's cashier takes a deposit, refunds part of
/// it with her manager's approval given at the till, and hands the repaired phone back; the main
/// PC never opens a drawer, and the till's Z holds every dinar of its drawer.
#[test]
fn a_tills_deposit_refund_and_pickup_land_in_its_own_drawer_and_z() {
    let pcs = TwoPcs::new();
    modules(&pcs.host, &[Module::Repairs]);
    let walid = pcs.host.customer("Walid");
    let screen = pcs.host.product("Écran A54", 1_200_000, 1_900, None);
    pcs.host.stock_in(&screen.id, 3);
    pcs.wait_caught_up();
    let till = pcs.till();
    let manager = user_named(till, "Samira");
    let cashier = till.cashier();
    cash_cmd::open_session(&till.st, Money(1_000_000)).unwrap();
    let ticket = services::intake(&till.st, intake(&walid)).unwrap();
    let deposit = services::deposit_take(&till.st, cash_deposit(&ticket, 200_000)).unwrap();
    let received = Money(200_000 + deposit.stamp_charged.0);
    assert_eq!(
        moves(till, "deposit"),
        vec![(MovementKind::CreditPayment, received)],
        "the deposit and its duty are in the till's drawer"
    );

    // a refund needs sales.return: the manager approves it at the till
    let refused = services::deposit_refund(
        &till.st,
        OwnerType::ServiceTicket,
        ticket.id.clone(),
        Money(50_000),
        PaymentMethod::Cash,
        None,
    )
    .unwrap_err();
    assert_eq!(refused.code(), "insufficient_permission");
    auth::manager_override(
        &till.st,
        &auth::OverrideInput {
            approver_id: manager,
            pin: MANAGER_PIN.into(),
            permission: Permission::SalesReturn,
        },
    )
    .unwrap();
    services::deposit_refund(
        &till.st,
        OwnerType::ServiceTicket,
        ticket.id.clone(),
        Money(50_000),
        PaymentMethod::Cash,
        None,
    )
    .unwrap();
    assert!(
        till.st
            .approvals
            .pending(&cashier, till.st.now())
            .unwrap()
            .is_empty(),
        "the approval is spent once the main PC used it"
    );
    assert_eq!(
        moves(till, "deposit"),
        vec![
            (MovementKind::CreditPayment, received),
            (MovementKind::Refund, Money(-50_000))
        ]
    );

    // the phone is repaired and handed back: the rest is paid in cash at the till
    let items = vec![ServiceItemInput {
        product_id: screen.id.clone(),
        qty: Qty(1_000),
        unit_price: None,
    }];
    services::set_items(&till.st, ticket.id.clone(), items).unwrap();
    services::set_status(&till.st, ticket.id.clone(), ServiceStatus::Ready, None).unwrap();
    let picked = services::pickup(
        &till.st,
        PickupInput {
            id: ticket.id.clone(),
            payments: vec![PaymentInput::cash(Money(1_100_000))],
            extras: Default::default(),
        },
        false,
    )
    .unwrap();
    let sold = picked.picked.sale.drawer_share().unwrap().unwrap().amount;
    assert_eq!(
        moves(till, "document"),
        vec![(MovementKind::Sale, sold)],
        "the cash of the pickup ticket is in the till's drawer"
    );

    // the main PC never opened a drawer: everything was booked on the till's
    let host_terminal = pcs.host.st.terminal.id.clone();
    assert_eq!(
        pcs.host
            .st
            .db
            .read(move |c| cash::current(c, &host_terminal))
            .unwrap(),
        None
    );
    let till_session = open_session_of(till);
    let ticket_doc = picked.picked.sale.document.id.clone();
    let on_host: Option<String> = pcs
        .host
        .st
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT cash_session_id FROM documents WHERE id = ?1",
                [&ticket_doc],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(on_host.as_deref(), Some(till_session.as_str()));

    // the till's Z: the pickup ticket and every dinar of its drawer
    pcs.wait_sent();
    pcs.wait_caught_up();
    let z = close_approved(till).z.summary;
    assert_eq!(z.tickets, 1, "the pickup ticket is in the till's Z");
    assert_eq!(
        (z.credit_payments, z.cash_refunds, z.cash_sales),
        (received, Money(-50_000), sold)
    );
    assert_eq!(
        z.expected_cash,
        Some(Money(1_000_000 + received.0 - 50_000 + sold.0))
    );
}

/// Finding 3 end to end: a connected till plans, runs and closes a delivery round through the
/// main PC (its rows never reach a till), prints the route sheet from it, and takes the driver's
/// cash into its own drawer; offline it is told the main PC is needed.
#[test]
fn a_connected_till_books_a_delivery_round_through_the_main_pc() {
    let pcs = TwoPcs::new();
    modules(
        &pcs.host,
        &[Module::Wholesale, Module::DeliveryRounds, Module::Orders],
    );
    let flour = pcs.host.product("Farine 25 kg", 250_000, 900, None);
    pcs.host.stock_in(&flour.id, 50);
    let amine = pcs.host.customer("Épicerie Amine");
    let bl = pcs.host.delivery_note_on_account(&amine, &flour.id, 2);
    pcs.wait_caught_up();
    let till = pcs.till();
    till.owner();
    cash_cmd::open_session(&till.st, Money(0)).unwrap();
    let round = wholesale::round_create(
        &till.st,
        RoundInput {
            driver_name: "Rachid".into(),
            ..RoundInput::default()
        },
    )
    .unwrap();
    assert!(round.number.starts_with("TR2026-"), "{}", round.number);
    wholesale::round_set_stops(&till.st, round.id.clone(), vec![bl.id.clone()]).unwrap();
    wholesale::round_start(&till.st, round.id.clone()).unwrap();
    wholesale::round_report_stop(
        &till.st,
        round.id.clone(),
        StopReport {
            seq: 1,
            status: StopStatus::Delivered,
            collected: Money(500_000),
            method: Some(PaymentMethod::Cash),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            note: None,
        },
    )
    .unwrap();
    let day = services::business_day(&till.st);
    let filter = RoundFilter {
        status: None,
        from: day.clone(),
        to: day,
    };
    assert_eq!(
        wholesale::rounds(&till.st, filter.clone()).unwrap().len(),
        1
    );
    match wholesale::route_sheet(&till.st, round.id.clone(), Lang::Fr).unwrap() {
        crate::print_host::HostPage::RouteSheet(sheet) => {
            assert_eq!((sheet.stops.len(), sheet.driver.as_str()), (1, "Rachid"))
        }
        other => panic!("{other:?}"),
    }
    let closed = wholesale::round_close(&till.st, round.id.clone()).unwrap();
    assert_eq!(closed.round.versements.len(), 1);
    let received = closed.round.versements[0].quote.to_receive;
    assert_eq!(
        moves(till, "customer_ledger"),
        vec![(MovementKind::CreditPayment, received)],
        "the driver's cash is in the till's drawer"
    );
    let session = open_session_of(till);
    let x = till
        .st
        .db
        .read(move |c| cash::x_report(c, &session, true))
        .unwrap();
    assert_eq!(x.credit_payments, received);

    pcs.host_down();
    assert_eq!(
        wholesale::rounds(&till.st, filter).unwrap_err().code(),
        "host_required"
    );
}

/// `instalments.sign` input: one fridge on 6 dues for `customer`, 30 000 DA down in cash.
fn fridge_on_instalments(product_id: &str, customer: &str) -> pos_core::instalments::SignInput {
    use pos_core::instalments::{GuaranteeInput, GuaranteeKind, QuoteInput, SignInput};
    let mut cart = pos_core::sales::Cart::new(
        Lang::Fr,
        vec![pos_core::sales::CartLine::product(product_id, Qty(1_000))],
    );
    cart.customer_id = Some(customer.to_owned());
    SignInput {
        quote: QuoteInput {
            cart,
            down: Money(3_000_000),
            down_method: PaymentMethod::Cash,
            count: 6,
            first_due_on: "2026-10-31".into(),
        },
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
        extras: Default::default(),
        note: None,
    }
}

/// Final review: the main PC committed a till's deposit and a till's instalment contract, and
/// the till lost both answers. Each call waits on the till with its key and goes again (the
/// maintenance loop, or before the next money call): the main PC answers from what it answered,
/// so no second deposit and no second contract, and the cash lands in the till's drawer once.
#[test]
fn a_money_call_whose_answer_was_lost_goes_again_and_is_never_written_twice() {
    use std::sync::atomic::Ordering;
    let pcs = TwoPcs::new();
    modules(&pcs.host, &[Module::Repairs, Module::Instalments]);
    let walid = pcs.host.customer("Walid");
    let fridge = pcs.host.product("Réfrigérateur", 12_000_000, 1_900, None);
    pcs.host.stock_in(&fridge.id, 2);
    pcs.wait_caught_up();
    let till = pcs.till();
    till.owner();
    cash_cmd::open_session(&till.st, Money(1_000_000)).unwrap();
    let ticket = services::intake(&till.st, intake(&walid)).unwrap();

    // deposits.take: committed on the main PC, its answer lost
    till.st.lan.test_lose_answer.store(true, Ordering::SeqCst);
    let lost = services::deposit_take(&till.st, cash_deposit(&ticket, 200_000)).unwrap_err();
    assert_eq!(lost.code(), "lan_answer_pending");
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM deposits WHERE kind = 'deposit'"),
        1,
        "the main PC took it"
    );
    assert!(moves(till, "deposit").is_empty(), "not in the drawer yet");
    let listed = super::registers::pending_entries(&till.st).unwrap();
    assert_eq!(listed.calls.len(), 1);
    assert_eq!(listed.calls[0].service, "deposits.take");
    assert_eq!(
        super::route::replay_calls(&till.st),
        1,
        "sent again, settled"
    );
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM deposits WHERE kind = 'deposit'"),
        1,
        "answered from what it answered: no second deposit"
    );
    let taken = moves(till, "deposit");
    assert_eq!(taken.len(), 1, "the deposit's cash is in the drawer once");
    assert_eq!(taken[0].0, MovementKind::CreditPayment);
    assert!(
        super::registers::pending_entries(&till.st)
            .unwrap()
            .calls
            .is_empty()
    );

    // instalments.sign: lost too; the next money call sends it first
    till.st.lan.test_lose_answer.store(true, Ordering::SeqCst);
    let lost = crate::commands::instalments::sign(
        &till.st,
        fridge_on_instalments(&fridge.id, &walid),
        false,
    )
    .unwrap_err();
    assert_eq!(lost.code(), "lan_answer_pending");
    assert_eq!(pcs.host.count("SELECT COUNT(*) FROM instalment_plans"), 1);
    let down_payments = || {
        moves(till, "document")
            .into_iter()
            .filter(|(kind, _)| *kind == MovementKind::Sale)
            .count()
    };
    assert_eq!(down_payments(), 0);
    services::deposit_take(&till.st, cash_deposit(&ticket, 10_000)).unwrap();
    assert_eq!(
        pcs.host.count("SELECT COUNT(*) FROM instalment_plans"),
        1,
        "the contract was signed once"
    );
    assert_eq!(down_payments(), 1, "its down payment is in the drawer once");
    assert_eq!(
        pcs.host
            .count("SELECT COUNT(*) FROM deposits WHERE kind = 'deposit'"),
        2,
        "the next deposit is a new one"
    );
    assert!(
        super::registers::pending_entries(&till.st)
            .unwrap()
            .calls
            .is_empty()
    );
}

/// Final review: the delivery rounds (like the legal registers) are kept on the main PC only. A
/// till's delivery-rounds report is the main PC's, run there as the till's user; offline the till
/// says `host_required` instead of showing an empty report.
#[test]
fn a_tills_delivery_rounds_report_is_the_main_pcs() {
    use pos_core::reports::{ReportKind, ReportRequest};
    let pcs = TwoPcs::new();
    modules(
        &pcs.host,
        &[Module::Wholesale, Module::DeliveryRounds, Module::Orders],
    );
    wholesale::round_create(
        &pcs.host.st,
        RoundInput {
            driver_name: "Rachid".into(),
            ..RoundInput::default()
        },
    )
    .unwrap();
    pcs.wait_caught_up();
    let till = pcs.till();
    till.owner();
    let day = services::business_day(&till.st);
    let request = || ReportRequest::new(ReportKind::DeliveryRounds, &day, &day);
    let view = crate::commands::reports::run(&till.st, request()).unwrap();
    assert_eq!(view.report.table.rows.len(), 1, "the main PC's round");
    assert_eq!(
        view.report.table.rows[0][2],
        pos_core::table::Cell::text("Rachid")
    );
    pcs.host_down();
    let offline = crate::commands::reports::run(&till.st, request()).unwrap_err();
    assert_eq!(offline.code(), "host_required");
}

/// Final review (PLT-024): a till compares its regulatory data pack with its main PC's and says
/// when they differ, so a till never charges an old stamp duty or ceiling unseen.
#[test]
fn a_till_says_when_its_data_pack_differs_from_the_main_pcs() {
    let pcs = TwoPcs::new();
    let till = pcs.till();
    let gap = |st: &crate::state::AppState| {
        st.lan
            .status(st)
            .unwrap()
            .client
            .and_then(|c| c.datapack_gap)
    };
    assert_eq!(
        super::datapack::follow_host(&till.st).unwrap(),
        None,
        "the same bundled pack: nothing to install"
    );
    assert_eq!(gap(&till.st), None);
    let version = pcs.host.st.datapack.version;
    // the till ran an older pack (a finance law installed on the main PC only)
    till.st
        .db
        .write(move |tx| pos_core::datapack_update::note_active(tx, version - 1))
        .unwrap();
    super::datapack::follow_host(&till.st).unwrap();
    assert_eq!(gap(&till.st).as_deref(), Some("behind"));
    // or a newer one than the main PC's
    till.st
        .db
        .write(move |tx| pos_core::datapack_update::note_active(tx, version + 1))
        .unwrap();
    assert_eq!(gap(&till.st).as_deref(), Some("ahead"));
    // the main PC answers whoever asks, signed in or not (the maintenance loop)
    crate::commands::auth::logout(&till.st).unwrap();
    assert!(super::datapack::follow_host(&till.st).is_ok());
}
