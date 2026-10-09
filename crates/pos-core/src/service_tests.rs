//! Deposits, the voucher router and service tickets (Task 9).

use crate::deposits::{self, DepositInput, OwnerType};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::SaleExtras;
use crate::service_tickets::{
    self as st, IntakeInput, PickupInput, ServiceItemInput, ServiceKind, ServiceStatus,
};
use crate::testkit::{Shop, shop};
use crate::vouchers::{self, VoucherTarget};

fn repair_shop() -> (Shop, String) {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Repairs, Module::Staff]))
        .unwrap();
    shop.open_till(200_000);
    let customer = shop.customer("Nassim Ould Ali").id;
    (shop, customer)
}

fn intake(shop: &Shop, customer: &str) -> st::ServiceTicket {
    let ctx = shop.till();
    let input = IntakeInput {
        kind: ServiceKind::Repair,
        customer_id: customer.into(),
        item: "Samsung A54".into(),
        brand: Some("Samsung".into()),
        model: Some("A54".into()),
        serial_no: Some("356938035643809".into()),
        accessories: Some("Chargeur, coque".into()),
        condition: Some("Écran fissuré".into()),
        fault: "Remplacement écran".into(),
        estimate: Some(Money(1_500_000)),
        promised_on: Some("2026-10-01".into()),
        technician_id: None,
        lang: Lang::Ar,
        items: vec![],
    };
    shop.db
        .write(move |tx| st::intake(tx, &ctx, input))
        .unwrap()
}

fn cash_deposit(
    shop: &Shop,
    owner: &str,
    amount: i64,
) -> Result<deposits::Deposit, crate::error::AppError> {
    let ctx = shop.till();
    let input = DepositInput {
        owner_type: OwnerType::ServiceTicket,
        owner_id: owner.into(),
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    };
    shop.db.write(move |tx| deposits::take(tx, &ctx, input))
}

#[test]
fn vouchers_route_by_their_reference() {
    assert_eq!(vouchers::classify(None), VoucherTarget::StoreCredit);
    assert_eq!(
        vouchers::classify(Some("BR2026-C1-000004")),
        VoucherTarget::StoreCredit
    );
    assert_eq!(
        vouchers::classify(Some("RP2026-000012")),
        VoucherTarget::Deposit {
            owner_type: OwnerType::ServiceTicket,
            number: "RP2026-000012".into()
        }
    );
    assert_eq!(
        vouchers::classify(Some(" cc2026-000003 ")),
        VoucherTarget::Deposit {
            owner_type: OwnerType::CustomerOrder,
            number: "CC2026-000003".into()
        }
    );
}

#[test]
fn a_repair_goes_from_intake_to_pickup_with_its_deposit_applied() {
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    assert_eq!(
        (ticket.number.as_str(), ticket.status),
        ("RP2026-000001", ServiceStatus::Received)
    );
    let deposit = cash_deposit(&shop, &ticket.id, 500_000).unwrap();
    assert_eq!(
        (deposit.receipt_number.as_deref(), deposit.stamp_duty),
        (Some("AC2026-000001"), Money(5_000))
    );
    assert_eq!(
        shop.count(
            "SELECT SUM(amount) FROM cash_movements WHERE kind = 'credit_payment' AND ref_type = 'deposit'"
        ),
        505_000
    );
    let screen = shop.product("Écran A54", Money(900_000), &shop.tva19).id;
    shop.stock_in(&screen, 2_000);
    // The phones template seeds SVC-REPAIR as an open-price service; a template resets the
    // modules, so they are set again.
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| {
            crate::packs::apply_template(
                tx,
                &actor,
                &pack,
                crate::company::ShopActivity::Phones,
                Lang::Fr,
                true,
            )
        })
        .unwrap();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Repairs, Module::Staff]))
        .unwrap();
    let labour: String = shop
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT id FROM products WHERE reference = 'SVC-REPAIR'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let ctx = shop.till();
    let id = ticket.id.clone();
    let items = vec![
        ServiceItemInput {
            product_id: screen.clone(),
            qty: Qty(1_000),
            unit_price: None,
        },
        ServiceItemInput {
            product_id: labour,
            qty: Qty(1_000),
            unit_price: Some(Money(300_000)),
        },
    ];
    shop.db
        .write(move |tx| st::set_items(tx, &ctx, &id, items))
        .unwrap();
    let ctx = shop.till();
    let id = ticket.id.clone();
    let early = shop.db.write(move |tx| {
        st::pickup(
            tx,
            &ctx,
            PickupInput {
                id,
                payments: vec![PaymentInput::cash(Money(800_000))],
                extras: SaleExtras::default(),
            },
        )
    });
    assert_eq!(early.unwrap_err().code(), "service_not_ready");
    for status in [ServiceStatus::InProgress, ServiceStatus::Ready] {
        let ctx = shop.till();
        let id = ticket.id.clone();
        shop.db
            .write(move |tx| st::set_status(tx, &ctx, &id, status, None))
            .unwrap();
    }
    let text = shop
        .db
        .read({
            let id = ticket.id.clone();
            move |c| st::ready_message(c, &id, Lang::Fr)
        })
        .unwrap();
    assert!(text.contains("RP2026-000001") && text.contains("Samsung A54"));
    let ctx = shop.till();
    let id = ticket.id.clone();
    let done = shop
        .db
        .write(move |tx| {
            st::pickup(
                tx,
                &ctx,
                PickupInput {
                    id,
                    payments: vec![PaymentInput::cash(Money(800_000))],
                    extras: SaleExtras::default(),
                },
            )
        })
        .unwrap();
    let doc = &done.sale.document;
    assert_eq!(doc.total_ttc, Money(1_200_000));
    let tenders: Vec<(PaymentMethod, Money, Option<&str>)> = doc
        .payments
        .iter()
        .map(|p| (p.method, p.amount, p.reference.as_deref()))
        .collect();
    assert!(tenders.contains(&(
        PaymentMethod::Voucher,
        Money(500_000),
        Some("RP2026-000001")
    )));
    assert_eq!(done.ticket.status, ServiceStatus::Delivered);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let after = shop
        .db
        .write(move |tx| st::set_items(tx, &ctx, &id, vec![]));
    assert_eq!(
        after.unwrap_err().code(),
        "service_status_invalid",
        "a delivered ticket is final"
    );
    let id = ticket.id.clone();
    assert_eq!(
        shop.db
            .read(move |c| deposits::held(c, OwnerType::ServiceTicket, &id))
            .unwrap(),
        Money::ZERO
    );
}

#[test]
fn a_deposit_is_applied_once_and_never_below_zero() {
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    cash_deposit(&shop, &ticket.id, 100_000).unwrap();
    let ctx = shop.till();
    let id = ticket.id.clone();
    let too_much = shop.db.write(move |tx| {
        deposits::refund(
            tx,
            &ctx,
            OwnerType::ServiceTicket,
            &id,
            Money(100_001),
            PaymentMethod::Cash,
            None,
        )
    });
    assert_eq!(too_much.unwrap_err().code(), "deposit_insufficient");
    let other = shop.product("Coque", Money(150_000), &shop.tva19).id;
    let ctx = shop.till();
    let mut cart = crate::sales::Cart::new(
        Lang::Fr,
        vec![crate::sales::CartLine::product(&other, Qty(1_000))],
    );
    cart.customer_id = Some(customer.clone());
    let spend = crate::sales::SaleInput {
        cart,
        payments: vec![PaymentInput {
            method: PaymentMethod::Voucher,
            amount: Money(150_000),
            reference: Some("RP2026-000001".into()),
            cheque_bank: None,
            cheque_date: None,
        }],
    };
    let over = shop
        .db
        .write(move |tx| crate::sales::create_sale(tx, &ctx, spend));
    assert_eq!(
        over.unwrap_err().code(),
        "deposit_insufficient",
        "a tender never takes more than is held"
    );
    let ctx = shop.till();
    let id = ticket.id.clone();
    let cancel = shop
        .db
        .write(move |tx| st::set_status(tx, &ctx, &id, ServiceStatus::Cancelled, None));
    assert_eq!(
        cancel.unwrap_err().code(),
        "deposit_held",
        "refund the deposit before closing the ticket"
    );
}

#[test]
fn an_anonymous_sale_never_spends_a_customers_deposit() {
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    cash_deposit(&shop, &ticket.id, 100_000).unwrap();
    let other = shop.product("Coque", Money(50_000), &shop.tva19).id;
    shop.stock_in(&other, 1_000);
    let ctx = shop.till();
    // No customer on the ticket: the RP number printed on the slip is not enough.
    let cart = crate::sales::Cart::new(
        Lang::Fr,
        vec![crate::sales::CartLine::product(&other, Qty(1_000))],
    );
    let spend = crate::sales::SaleInput {
        cart,
        payments: vec![PaymentInput {
            method: PaymentMethod::Voucher,
            amount: Money(50_000),
            reference: Some("RP2026-000001".into()),
            cheque_bank: None,
            cheque_date: None,
        }],
    };
    let refused = shop
        .db
        .write(move |tx| crate::sales::create_sale(tx, &ctx, spend));
    assert_eq!(refused.unwrap_err().code(), "deposit_owner_invalid");
    let id = ticket.id.clone();
    assert_eq!(
        shop.db
            .read(move |c| deposits::held(c, OwnerType::ServiceTicket, &id))
            .unwrap(),
        Money(100_000),
        "the deposit stays held"
    );
}

#[test]
fn a_pickup_never_closes_a_ticket_on_a_deposit_above_its_price() {
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    cash_deposit(&shop, &ticket.id, 1_000_000).unwrap();
    let screen = shop.product("Écran A54", Money(600_000), &shop.tva19).id;
    shop.stock_in(&screen, 1_000);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let items = vec![ServiceItemInput {
        product_id: screen,
        qty: Qty(1_000),
        unit_price: None,
    }];
    shop.db
        .write(move |tx| st::set_items(tx, &ctx, &id, items))
        .unwrap();
    let ctx = shop.till();
    let id = ticket.id.clone();
    shop.db
        .write(move |tx| st::set_status(tx, &ctx, &id, ServiceStatus::Ready, None))
        .unwrap();
    let pick = |shop: &Shop| {
        let ctx = shop.till();
        let id = ticket.id.clone();
        shop.db.write(move |tx| {
            st::pickup(
                tx,
                &ctx,
                PickupInput {
                    id,
                    payments: vec![],
                    extras: SaleExtras::default(),
                },
            )
        })
    };
    let err = pick(&shop).unwrap_err();
    assert_eq!(err.code(), "deposit_held");
    assert_eq!(
        serde_json::to_value(&err).unwrap()["details"],
        serde_json::json!({ "held": 1_000_000, "total": 600_000 }),
        "the cashier sees what to refund first"
    );
    let id = ticket.id.clone();
    let still = shop.db.read(move |c| st::get(c, &id)).unwrap();
    assert_eq!(
        (still.status, still.deposits),
        (ServiceStatus::Ready, Money(1_000_000))
    );
    // The excess is refunded on the open ticket, then the pickup spends exactly the rest.
    let ctx = shop.till();
    let id = ticket.id.clone();
    shop.db
        .write(move |tx| {
            deposits::refund(
                tx,
                &ctx,
                OwnerType::ServiceTicket,
                &id,
                Money(400_000),
                PaymentMethod::Cash,
                None,
            )
        })
        .unwrap();
    let done = pick(&shop).unwrap();
    assert_eq!(
        (done.ticket.status, done.sale.document.total_ttc),
        (ServiceStatus::Delivered, Money(600_000))
    );
    let id = ticket.id.clone();
    assert_eq!(
        shop.db
            .read(move |c| deposits::held(c, OwnerType::ServiceTicket, &id))
            .unwrap(),
        Money::ZERO
    );
}

#[test]
fn the_duty_of_a_cash_deposit_is_declared_and_its_absorbed_part_is_a_cost() {
    use crate::reports::{ReportKind, ReportRequest, run};
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    // 5 000 DA in cash, duty charged to the customer: 50 DA.
    let charged = cash_deposit(&shop, &ticket.id, 500_000).unwrap();
    assert_eq!(charged.stamp_charged, Money(5_000));
    let statement = |shop: &Shop| {
        let req = ReportRequest::new(ReportKind::StampDutyStatement, "2026-09-01", "2026-09-30");
        shop.db.read(move |c| run(c, &req)).unwrap().table
    };
    let t = statement(&shop);
    assert_eq!(
        (
            t.money_total("cashBase"),
            t.money_total("versementsDuty"),
            t.money_total("dutyTotal")
        ),
        (Money(505_000), Money(5_000), Money(5_000))
    );
    // 10 000 DA in cash, duty absorbed by the shop: still due, and a cost in the P&L.
    shop.set("stamp_duty.mode", serde_json::json!("absorb"));
    let absorbed = cash_deposit(&shop, &ticket.id, 1_000_000).unwrap();
    assert_eq!(
        (absorbed.stamp_duty, absorbed.stamp_charged),
        (Money(10_000), Money::ZERO)
    );
    let t = statement(&shop);
    assert_eq!(
        (t.money_total("cashBase"), t.money_total("dutyTotal")),
        (Money(1_505_000), Money(15_000))
    );
    let pack = shop.pack.clone();
    let g50 = shop
        .db
        .read(move |c| crate::fiscal::g50::g50(c, &pack, "2026-09-01", "2026-09-30"))
        .unwrap();
    assert_eq!(
        g50.lines
            .iter()
            .find(|l| l.key == "stampDuty")
            .map(|l| l.amount),
        Some(Money(15_000))
    );
    let req = ReportRequest::new(ReportKind::ProfitAndLoss, "2026-09-01", "2026-09-30");
    let pnl = shop.db.read(move |c| run(c, &req)).unwrap().table;
    let stamp_absorbed = pnl
        .rows
        .iter()
        .find(|r| r[0].as_str() == Some("stamp_absorbed"))
        .and_then(|r| r[2].as_i64());
    assert_eq!(stamp_absorbed, Some(-10_000));
}

#[test]
fn uncollected_tickets_are_listed_after_the_set_days() {
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    let ctx = shop.till();
    let id = ticket.id.clone();
    shop.db
        .write(move |tx| st::set_status(tx, &ctx, &id, ServiceStatus::Ready, None))
        .unwrap();
    shop.set("repairs.uncollected_days", serde_json::json!(10));
    assert!(
        shop.db
            .read(|c| st::uncollected(c, "2026-10-05"))
            .unwrap()
            .is_empty()
    );
    let late = shop.db.read(|c| st::uncollected(c, "2026-10-09")).unwrap();
    assert_eq!((late.len(), late[0].number.as_str()), (1, "RP2026-000001"));
}

#[test]
fn the_service_reports_list_the_tickets_and_what_waits_too_long() {
    use crate::reports::{ReportKind, ReportRequest, run};
    use crate::table::Cell;
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    cash_deposit(&shop, &ticket.id, 200_000).unwrap();
    let ctx = shop.till();
    let id = ticket.id.clone();
    shop.db
        .write(move |tx| st::set_status(tx, &ctx, &id, ServiceStatus::Ready, None))
        .unwrap();
    let req = ReportRequest::new(ReportKind::ServiceTickets, "2026-09-01", "2026-10-08");
    let report = shop.db.read(move |c| run(c, &req)).unwrap();
    let row = &report.table.rows[0];
    assert_eq!(
        (&row[0], &row[1], &row[2], &row[5], &row[6], &row[7]),
        (
            &Cell::text("RP2026-000001"),
            &Cell::text("repair"),
            &Cell::text("ready"),
            &Cell::int(10),
            &Cell::int(1_500_000),
            &Cell::NULL
        )
    );
    let req = ReportRequest::new(ReportKind::UncollectedTickets, "2026-10-09", "2026-10-29");
    let report = shop.db.read(move |c| run(c, &req)).unwrap();
    assert_eq!(
        report.table.rows,
        vec![vec![
            Cell::text("RP2026-000001"),
            Cell::text("Nassim Ould Ali"),
            Cell::text("Samsung A54"),
            Cell::text("2026-09-28"),
            Cell::int(31),
            Cell::int(200_000),
        ]]
    );
    let req = ReportRequest::new(ReportKind::UncollectedTickets, "2026-10-28", "2026-10-28");
    assert!(
        shop.db
            .read(move |c| run(c, &req))
            .unwrap()
            .table
            .rows
            .is_empty(),
        "30 days by default: waiting 30 days is not yet too long"
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[]))
        .unwrap();
    let req = ReportRequest::new(ReportKind::ServiceTickets, "2026-09-28", "2026-09-28");
    assert_eq!(
        shop.db.read(move |c| run(c, &req)).unwrap_err().code(),
        "module_disabled"
    );
}

/// The slip of a deposit names what it is held for and whose it is (Task 20).
#[test]
fn a_deposit_slip_reads_its_owner_number_and_customer() {
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    let (owner, by) = (ticket.id.clone(), customer.clone());
    let (number, name, nobody) = shop
        .db
        .read(move |c| {
            Ok((
                deposits::owner_number(c, OwnerType::ServiceTicket, &owner)?,
                deposits::customer_name(c, Some(&by))?,
                deposits::customer_name(c, None)?,
            ))
        })
        .unwrap();
    assert_eq!(number, ticket.number);
    assert_eq!((name.as_deref(), nobody), (Some("Nassim Ould Ali"), None));
    let missing = shop
        .db
        .read(|c| deposits::owner_number(c, OwnerType::CustomerOrder, "nope"))
        .unwrap_err();
    assert_eq!(missing.code(), "order_not_found");
}

/// Final review: a pickup paid by a custom payment type. The backend inserts the deposit tender
/// first, so the payment type the till names for its first tender follows it: the use is recorded
/// on the "Prélèvement CCP" transfer, never on the deposit.
#[test]
fn a_pickup_paid_by_a_payment_type_records_it_on_its_own_tender() {
    const CCP_DEBIT: &str = "00000000-0000-7000-8000-000000000601";
    let (shop, customer) = repair_shop();
    let ticket = intake(&shop, &customer);
    cash_deposit(&shop, &ticket.id, 500_000).unwrap();
    let screen = shop.product("Écran A54", Money(900_000), &shop.tva19).id;
    shop.stock_in(&screen, 2_000);
    let (ctx, id) = (shop.till(), ticket.id.clone());
    let items = vec![ServiceItemInput {
        product_id: screen,
        qty: Qty(1_000),
        unit_price: None,
    }];
    shop.db
        .write(move |tx| st::set_items(tx, &ctx, &id, items))
        .unwrap();
    for status in [ServiceStatus::InProgress, ServiceStatus::Ready] {
        let (ctx, id) = (shop.till(), ticket.id.clone());
        shop.db
            .write(move |tx| st::set_status(tx, &ctx, &id, status, None))
            .unwrap();
    }
    let (ctx, id) = (shop.till(), ticket.id.clone());
    let done = shop
        .db
        .write(move |tx| {
            st::pickup(
                tx,
                &ctx,
                PickupInput {
                    id,
                    payments: vec![PaymentInput::electronic(
                        PaymentMethod::Transfer,
                        Money(400_000),
                        Some("PRLV 2026-10 n° 0042"),
                    )],
                    extras: SaleExtras {
                        payment_types: vec![crate::payment_types::TenderType {
                            payment: 0,
                            payment_type_id: CCP_DEBIT.into(),
                        }],
                        ..SaleExtras::default()
                    },
                },
            )
        })
        .expect("the type follows its tender past the deposit");
    let doc = done.sale.document.id.clone();
    let typed: (String, Option<String>) = shop
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT p.method, p.reference FROM payment_type_uses u JOIN payments p ON p.id = u.payment_id
                 WHERE p.document_id = ?1",
                [&doc],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(
        typed,
        (
            "transfer".to_owned(),
            Some("PRLV 2026-10 n° 0042".to_owned())
        )
    );
}
