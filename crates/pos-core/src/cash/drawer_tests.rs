//! The drawer of a host service (Task 20 review, finding 1): the money a main PC takes or gives
//! for a till is in that till's drawer, so the write books it on the till's open session (as the
//! main PC's copy has it) and moves no drawer on the main PC; the till records the movement in
//! its own copy from the answer ([`DrawerShare`], [`record_share`]). On a single PC every write
//! records exactly the share it reports.

use super::{DrawerShare, MovementKind, record_share};
use crate::customers::credit::{self, CollectInput};
use crate::deposits::{self, DepositInput, OwnerType};
use crate::flexy::{self, FloatPurchase};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleInput};
use crate::service_tickets::{self as st, IntakeInput, ServiceKind};
use crate::testkit::{NOON, Shop, shop};
use crate::till::TillContext;
use std::sync::Arc;

const C2: &str = "00000000-0000-7000-8000-0000000000c2";

/// A shop with the repairs and flexy modules on, whose own drawer stays closed (a back-office
/// main PC), and the till C2 as its copy knows it.
fn main_pc() -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(tx, &actor, &[Module::Repairs, Module::Flexy])?;
            tx.execute(
                "INSERT INTO terminals (id, code, name, role, created_at) VALUES (?1, 'C2', 'Caisse 2', 'client', ?2)",
                rusqlite::params![C2, NOON],
            )?;
            Ok(())
        })
        .unwrap();
    shop
}

/// The cashier signed in at C2.
fn at_c2(shop: &Shop) -> TillContext {
    let mut session = shop.cashier.clone();
    session.terminal_id = C2.into();
    session.terminal_code = "C2".into();
    TillContext::new(session, Arc::clone(&shop.pack), NOON)
}

/// The till C2 opens its drawer with `float` (a replayed open session, on the main PC's copy).
fn open_c2(shop: &Shop, float: i64) -> String {
    let ctx = at_c2(shop);
    shop.db
        .write(move |tx| super::open_session(tx, &ctx, Money(float)))
        .unwrap()
        .id
}

/// The cashier's write run on the main PC for C2 (an M5 host service).
fn for_c2(shop: &Shop) -> TillContext {
    shop.till().for_till(C2)
}

fn ticket(shop: &Shop) -> st::ServiceTicket {
    let customer = shop.customer("Nassim").id;
    let ctx = shop.till();
    let input = IntakeInput {
        kind: ServiceKind::Repair,
        customer_id: customer,
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
    };
    shop.db
        .write(move |tx| st::intake(tx, &ctx, input))
        .unwrap()
}

fn cash_deposit(owner_id: &str, amount: i64) -> DepositInput {
    DepositInput {
        owner_type: OwnerType::ServiceTicket,
        owner_id: owner_id.into(),
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

fn movements_of(shop: &Shop, ref_type: &'static str) -> Vec<(String, MovementKind, Money)> {
    shop.db
        .read(move |c| {
            let mut stmt = c.prepare(
                "SELECT session_id, kind, amount FROM cash_movements WHERE ref_type = ?1 ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([ref_type], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

fn session_of(shop: &Shop, sql: &'static str, id: String) -> Option<String> {
    shop.db
        .read(move |c| Ok(c.query_row(sql, [&id], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn a_deposit_taken_for_a_till_books_on_its_session_and_moves_no_drawer_here() {
    let shop = main_pc();
    let ticket = ticket(&shop);
    let ctx = for_c2(&shop);
    let input = cash_deposit(&ticket.id, 200_000);
    let refused = shop
        .db
        .write(move |tx| deposits::take(tx, &ctx, input))
        .unwrap_err();
    assert_eq!(
        refused.code(),
        "cash_session_closed",
        "the till's drawer must be open, never the main PC's"
    );
    let c2_session = open_c2(&shop, 10_000);
    let ctx = for_c2(&shop);
    let input = cash_deposit(&ticket.id, 200_000);
    let deposit = shop
        .db
        .write(move |tx| deposits::take(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        session_of(
            &shop,
            "SELECT cash_session_id FROM deposits WHERE id = ?1",
            deposit.id.clone()
        ),
        Some(c2_session.clone())
    );
    assert!(
        movements_of(&shop, "deposit").is_empty(),
        "no drawer moves on the main PC"
    );
    assert_eq!(
        deposits::drawer_share(&deposit),
        Some(DrawerShare {
            kind: MovementKind::CreditPayment,
            amount: Money(200_000 + deposit.stamp_charged.0),
            ref_type: "deposit".into(),
            ref_id: deposit.id.clone(),
            note: None,
            category: None,
        })
    );
    let ctx = for_c2(&shop);
    let id = ticket.id.clone();
    let refund = shop
        .db
        .write(move |tx| {
            deposits::refund(
                tx,
                &ctx,
                OwnerType::ServiceTicket,
                &id,
                Money(50_000),
                PaymentMethod::Cash,
                None,
            )
        })
        .unwrap();
    assert!(movements_of(&shop, "deposit").is_empty());
    assert_eq!(
        deposits::drawer_share(&refund).map(|s| (s.kind, s.amount)),
        Some((MovementKind::Refund, Money(-50_000)))
    );
}

#[test]
fn a_single_pc_records_exactly_the_share_its_write_reports() {
    let shop = main_pc();
    let own = shop.open_till(0).id;
    let ticket = ticket(&shop);
    let ctx = shop.till();
    let input = cash_deposit(&ticket.id, 200_000);
    let deposit = shop
        .db
        .write(move |tx| deposits::take(tx, &ctx, input))
        .unwrap();
    let share = deposits::drawer_share(&deposit).unwrap();
    assert_eq!(
        movements_of(&shop, "deposit"),
        vec![(own.clone(), share.kind, share.amount)]
    );
    let ctx = shop.till();
    let purchase = FloatPurchase {
        operator: "mobilis".into(),
        amount: Money(1_000_000),
        cost: Money(960_000),
        from_till: true,
        note: Some("Distributeur".into()),
    };
    let share = flexy::purchase_share(&purchase).unwrap().unwrap();
    let pack = Arc::clone(&shop.pack);
    shop.db
        .write(move |tx| flexy::purchase(tx, &ctx, &pack, purchase))
        .unwrap();
    assert_eq!(
        movements_of(&shop, "flexy_float"),
        vec![(own, share.kind, share.amount)]
    );
    assert_eq!(
        (share.kind, share.amount, share.note.as_deref()),
        (MovementKind::PaidOut, Money(-960_000), Some("Distributeur"))
    );
}

#[test]
fn a_ticket_sold_for_a_till_is_in_its_z_once_the_till_records_the_cash() {
    let shop = main_pc();
    let milk = shop.product("Lait", Money(45_000), &shop.tva19);
    shop.stock_in(&milk.id, 10_000);
    let c2_session = open_c2(&shop, 10_000);
    let ctx = for_c2(&shop);
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(100_000))],
    };
    let sale = shop
        .db
        .write(move |tx| crate::sales::create_sale(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        sale.document.cash_session_id.as_deref(),
        Some(c2_session.as_str())
    );
    assert!(movements_of(&shop, "document").is_empty());
    let share = sale.drawer_share().unwrap().unwrap();
    assert_eq!(
        (share.kind, share.amount, share.ref_id.as_str()),
        (
            MovementKind::Sale,
            sale.document.total_due,
            sale.document.id.as_str()
        ),
        "the cash kept, net of the change"
    );
    // the till's half, in its own drawer
    let till = at_c2(&shop);
    let kept = share.clone();
    let movement = shop
        .db
        .write(move |tx| record_share(tx, &till, &kept))
        .unwrap();
    assert_eq!(
        (movement.session_id.as_str(), movement.amount),
        (c2_session.as_str(), share.amount)
    );
    let id = c2_session.clone();
    let x = shop
        .db
        .read(move |c| super::x_report(c, &id, true))
        .unwrap();
    assert_eq!(
        (x.tickets, x.sales_ttc, x.cash_sales),
        (1, sale.document.total_ttc, share.amount)
    );
    assert_eq!(x.expected_cash, Some(Money(10_000 + share.amount.0)));
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'cash.host_share'"),
        1
    );
}

#[test]
fn the_till_records_a_share_only_in_its_own_open_drawer_and_in_the_right_direction() {
    let shop = main_pc();
    let share = DrawerShare {
        kind: MovementKind::CreditPayment,
        amount: Money(20_000),
        ref_type: "deposit".into(),
        ref_id: "00000000-0000-7000-8000-00000000d001".into(),
        note: None,
        category: None,
    };
    let (till, kept) = (at_c2(&shop), share.clone());
    let closed = shop
        .db
        .write(move |tx| record_share(tx, &till, &kept))
        .unwrap_err();
    assert_eq!(closed.code(), "cash_session_closed");
    open_c2(&shop, 0);
    for wrong in [
        DrawerShare {
            amount: Money(-20_000),
            ..share.clone()
        },
        DrawerShare {
            kind: MovementKind::Refund,
            ..share.clone()
        },
        DrawerShare {
            kind: MovementKind::Float,
            ..share.clone()
        },
    ] {
        let till = at_c2(&shop);
        let refused = shop
            .db
            .write(move |tx| record_share(tx, &till, &wrong))
            .unwrap_err();
        assert_eq!(refused.code(), "amount_invalid");
    }
}

#[test]
fn a_versement_and_a_float_paid_for_a_till_book_on_its_session_only() {
    let shop = main_pc();
    let c2_session = open_c2(&shop, 0);
    let customer = shop.customer("Épicerie Amine").id;
    let ctx = for_c2(&shop);
    let input = CollectInput {
        customer_id: customer,
        amount: Money(500_000),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: vec![],
        note: None,
    };
    let collected = shop
        .db
        .write(move |tx| credit::collect(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        collected.entry.cash_session_id.as_deref(),
        Some(c2_session.as_str())
    );
    assert!(movements_of(&shop, "customer_ledger").is_empty());
    assert_eq!(
        collected.drawer_share(),
        Some(DrawerShare {
            kind: MovementKind::CreditPayment,
            amount: collected.quote.to_receive,
            ref_type: "customer_ledger".into(),
            ref_id: collected.entry.id.clone(),
            note: None,
            category: None,
        })
    );
    let ctx = for_c2(&shop);
    let pack = Arc::clone(&shop.pack);
    let entry = shop
        .db
        .write(move |tx| {
            flexy::purchase(
                tx,
                &ctx,
                &pack,
                FloatPurchase {
                    operator: "mobilis".into(),
                    amount: Money(1_000_000),
                    cost: Money(960_000),
                    from_till: true,
                    note: None,
                },
            )
        })
        .unwrap();
    assert!(movements_of(&shop, "flexy_float").is_empty());
    assert_eq!(
        session_of(
            &shop,
            "SELECT cash_movement_id FROM flexy_float WHERE id = ?1",
            entry.id
        ),
        None
    );
}

/// The loyalty and staff packs: the gift vouchers and staff modules on.
fn with_vouchers_and_staff(shop: &Shop) {
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(
                tx,
                &actor,
                &[
                    Module::Repairs,
                    Module::Flexy,
                    Module::GiftVouchers,
                    Module::Staff,
                ],
            )
        })
        .unwrap();
}

fn voucher_sale(amount: i64) -> crate::gift_vouchers::SellInput {
    crate::gift_vouchers::SellInput {
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        customer_id: None,
        buyer_name: Some("Karim".into()),
    }
}

fn till_advance(user_id: &str, amount: i64) -> crate::staff::AdvanceInput {
    crate::staff::AdvanceInput {
        user_id: user_id.into(),
        amount: Money(amount),
        method: PaymentMethod::Cash,
        from_till: true,
        note: None,
    }
}

/// A gift voucher sold for cash, a salary advance paid
/// from the drawer and a repayment in cash, written on the main PC for the till C2, are booked
/// on C2's open session and move no drawer here; what each answer says to record is exactly the
/// movement C2 then records (`record_share`).
#[test]
fn a_voucher_an_advance_and_a_repayment_for_a_till_book_on_its_session_and_move_no_drawer_here() {
    let shop = main_pc();
    with_vouchers_and_staff(&shop);
    let employee = shop.cashier.user_id.clone();
    let ctx = for_c2(&shop);
    let refused = shop
        .db
        .write(move |tx| crate::gift_vouchers::sell(tx, &ctx, voucher_sale(500_000)))
        .unwrap_err();
    assert_eq!(refused.code(), "cash_session_closed", "the till's drawer");
    let (ctx, input) = (for_c2(&shop), till_advance(&employee, 200_000));
    let refused = shop
        .db
        .write(move |tx| crate::staff::advance(tx, &ctx, input))
        .unwrap_err();
    assert_eq!(refused.code(), "cash_session_closed", "the till's drawer");

    let c2_session = open_c2(&shop, 10_000);
    let ctx = for_c2(&shop);
    let voucher = shop
        .db
        .write(move |tx| crate::gift_vouchers::sell(tx, &ctx, voucher_sale(500_000)))
        .unwrap();
    assert_eq!(
        session_of(
            &shop,
            "SELECT cash_session_id FROM gift_voucher_ledger WHERE voucher_id = ?1",
            voucher.id.clone()
        ),
        Some(c2_session.clone())
    );
    let (ctx, input) = (for_c2(&shop), till_advance(&employee, 200_000));
    let advance = shop
        .db
        .write(move |tx| crate::staff::advance(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        advance.expense_id, None,
        "the till's drawer movement is the expense"
    );
    let ctx = for_c2(&shop);
    let repay_to = employee.clone();
    let repaid = shop
        .db
        .write(move |tx| {
            crate::staff::repay(
                tx,
                &ctx,
                &repay_to,
                Money(50_000),
                PaymentMethod::Cash,
                None,
            )
        })
        .unwrap();
    for id in [advance.id.clone(), repaid.id.clone()] {
        assert_eq!(
            session_of(
                &shop,
                "SELECT cash_session_id FROM staff_ledger WHERE id = ?1",
                id
            ),
            Some(c2_session.clone())
        );
    }
    assert!(
        movements_of(&shop, "gift_voucher").is_empty(),
        "no drawer moves here"
    );
    assert!(movements_of(&shop, "staff_ledger").is_empty());
    let expenses: i64 = shop
        .db
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM expenses", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(
        expenses, 0,
        "no expense row of the main PC's for the till's drawer"
    );

    let received = Money(500_000 + voucher.stamp_charged.0);
    let voucher_share = crate::gift_vouchers::drawer_share(&voucher, PaymentMethod::Cash).unwrap();
    assert_eq!(
        (voucher_share.kind, voucher_share.amount),
        (MovementKind::PaidIn, received)
    );
    let (entry, again) = (advance.clone(), repaid.clone());
    let (advance_share, repay_share) = shop
        .db
        .read(move |c| {
            Ok((
                crate::staff::drawer_share(c, &entry, true)?,
                crate::staff::drawer_share(c, &again, true)?,
            ))
        })
        .unwrap();
    let advance_share = advance_share.unwrap();
    assert_eq!(
        (
            advance_share.kind,
            advance_share.amount,
            advance_share.category
        ),
        (
            MovementKind::Expense,
            Money(-200_000),
            Some(super::ExpenseCategory::Wages)
        )
    );
    let repay_share = repay_share.unwrap();
    assert_eq!(
        (repay_share.kind, repay_share.amount),
        (MovementKind::PaidIn, Money(50_000))
    );
    let till = at_c2(&shop);
    let shares = vec![voucher_share, advance_share, repay_share];
    shop.db
        .write(move |tx| {
            for share in &shares {
                record_share(tx, &till, share)?;
            }
            Ok(())
        })
        .unwrap();
    let in_c2 = |ref_type| {
        movements_of(&shop, ref_type)
            .into_iter()
            .map(|(session, kind, amount)| {
                assert_eq!(session, c2_session);
                (kind, amount)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(in_c2("gift_voucher"), [(MovementKind::PaidIn, received)]);
    assert_eq!(
        in_c2("staff_ledger"),
        [
            (MovementKind::Expense, Money(-200_000)),
            (MovementKind::PaidIn, Money(50_000))
        ]
    );
}

/// On a single PC a cash voucher, an advance from the drawer and a cash repayment move this
/// PC's drawer exactly as their shares say (the advance through its wages expense, CSH-002).
#[test]
fn on_a_single_pc_a_voucher_an_advance_and_a_repayment_record_exactly_their_share() {
    let shop = shop();
    with_vouchers_and_staff(&shop);
    let ctx = shop.till();
    let session = shop
        .db
        .write(move |tx| super::open_session(tx, &ctx, Money(10_000)))
        .unwrap()
        .id;
    let employee = shop.cashier.user_id.clone();
    let ctx = shop.till();
    let voucher = shop
        .db
        .write(move |tx| crate::gift_vouchers::sell(tx, &ctx, voucher_sale(300_000)))
        .unwrap();
    let share = crate::gift_vouchers::drawer_share(&voucher, PaymentMethod::Cash).unwrap();
    assert_eq!(
        movements_of(&shop, "gift_voucher"),
        [(session.clone(), share.kind, share.amount)]
    );
    let (ctx, input) = (shop.till(), till_advance(&employee, 100_000));
    let advance = shop
        .db
        .write(move |tx| crate::staff::advance(tx, &ctx, input))
        .unwrap();
    assert!(advance.expense_id.is_some(), "the wages expense of this PC");
    let ctx = shop.till();
    let repay_to = employee.clone();
    let repaid = shop
        .db
        .write(move |tx| {
            crate::staff::repay(
                tx,
                &ctx,
                &repay_to,
                Money(40_000),
                PaymentMethod::Cash,
                None,
            )
        })
        .unwrap();
    let (entry, again) = (advance.clone(), repaid.clone());
    let (advance_share, repay_share, expense_moves) = shop
        .db
        .read(move |c| {
            let moved = c
                .prepare(
                    "SELECT kind, amount, category FROM cash_movements WHERE kind = 'expense'",
                )?
                .query_map([], |r| {
                    Ok((
                        r.get::<_, MovementKind>(0)?,
                        r.get::<_, Money>(1)?,
                        r.get::<_, Option<super::ExpenseCategory>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((
                crate::staff::drawer_share(c, &entry, true)?,
                crate::staff::drawer_share(c, &again, true)?,
                moved,
            ))
        })
        .unwrap();
    let advance_share = advance_share.unwrap();
    assert_eq!(
        expense_moves,
        [(
            advance_share.kind,
            advance_share.amount,
            advance_share.category
        )]
    );
    let repay_share = repay_share.unwrap();
    assert_eq!(
        movements_of(&shop, "staff_ledger"),
        [(session, repay_share.kind, repay_share.amount)]
    );
}
