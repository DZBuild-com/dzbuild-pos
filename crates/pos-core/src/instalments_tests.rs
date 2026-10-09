//! Instalment sales (Task 10).

use crate::instalments::{
    self as ins, DueStatus, GuaranteeInput, GuaranteeKind, PlanPayment, QuoteInput, SignInput,
};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::PaymentMethod;
use crate::sales::{Cart, CartLine, SaleExtras};
use crate::testkit::{Shop, shop};
use chrono::NaiveDate;
use proptest::prelude::*;

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn store() -> (Shop, String, String) {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Instalments, Module::Staff]))
        .unwrap();
    shop.open_till(100_000);
    let fridge = shop
        .product("Réfrigérateur Condor 400 L", Money(12_000_000), &shop.tva19)
        .id;
    shop.stock_in(&fridge, 2_000);
    let customer = shop.customer("Farid Bouzid").id;
    (shop, fridge, customer)
}

fn quote_input(fridge: &str, customer: &str) -> QuoteInput {
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(fridge, Qty(1_000))]);
    cart.customer_id = Some(customer.into());
    QuoteInput {
        cart,
        down: Money(3_000_000),
        down_method: PaymentMethod::Cash,
        count: 6,
        first_due_on: "2026-10-31".into(),
    }
}

fn signed(shop: &Shop, fridge: &str, customer: &str) -> ins::Signed {
    let ctx = shop.owner_till();
    let input = SignInput {
        quote: quote_input(fridge, customer),
        down_reference: None,
        cheque_bank: None,
        cheque_date: None,
        guarantees: vec![GuaranteeInput {
            kind: GuaranteeKind::PaySlip,
            holder_name: Some("Farid Bouzid".into()),
            reference: Some("Fiche de paie août 2026".into()),
            phone: None,
            note: None,
        }],
        extras: SaleExtras::default(),
        note: None,
    };
    shop.db.write(move |tx| ins::sign(tx, &ctx, input)).unwrap()
}

#[test]
fn no_guarantee_kind_is_a_cheque() {
    assert!(
        ins::GuaranteeKind::ALL
            .iter()
            .all(|k| !k.as_str().contains("cheque") && !k.as_str().contains("check"))
    );
    assert_eq!(ins::GuaranteeKind::parse("cheque"), None);
    assert_eq!(
        serde_json::from_str::<GuaranteeKind>("\"cheque\"").ok(),
        None
    );
}

#[test]
fn dates_follow_the_calendar_and_clamp_to_the_month_end() {
    let dues = ins::schedule(Money(1_000_000), 3, day("2027-01-31")).unwrap();
    let dates: Vec<&str> = dues.iter().map(|d| d.due_on.as_str()).collect();
    assert_eq!(dates, ["2027-01-31", "2027-02-28", "2027-03-31"]);
    assert_eq!(
        ins::schedule(Money(100), 61, day("2027-01-01"))
            .unwrap_err()
            .code(),
        "instalment_count_invalid"
    );
}

#[test]
fn signing_sells_on_credit_with_the_down_payment_duty() {
    let (shop, fridge, customer) = store();
    let ctx = shop.owner_till();
    let q = quote_input(&fridge, &customer);
    let quote = shop.db.read(move |c| ins::quote(c, &ctx, &q)).unwrap();
    assert_eq!(
        (quote.total, quote.financed),
        (Money(12_000_000), Money(9_000_000))
    );
    assert_eq!(
        quote.stamp_duty,
        Money(30_000),
        "300 tranches of 100 DA at 1 DA on the 30 000 DA paid in cash"
    );
    assert_eq!(
        quote.down_to_receive,
        Money(3_030_000),
        "the duty is charged to the customer by default"
    );
    assert_eq!(
        quote.dues.iter().map(|d| d.amount.0).collect::<Vec<_>>(),
        [1_500_000; 6]
    );
    let s = signed(&shop, &fridge, &customer);
    assert_eq!(s.plan.number, "CT2026-000001");
    assert_eq!(
        (s.sale.document.credit_amount, s.sale.document.stamp_duty),
        (Money(9_000_000), Money(30_000))
    );
    assert_eq!(shop.count("SELECT balance FROM customers"), 9_000_000);
    assert_eq!(s.plan.guarantees.len(), 1);
    assert!(
        shop.db
            .write(|tx| Ok(tx.execute_batch("UPDATE instalment_dues SET amount = 1")?))
            .is_err(),
        "a signed schedule is immutable"
    );
}

#[test]
fn a_payment_settles_the_contract_sale_first_and_marks_the_oldest_dues() {
    let (shop, fridge, customer) = store();
    let other = shop.product("Ventilateur", Money(500_000), &shop.tva19).id;
    let ctx = shop.owner_till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&other, Qty(1_000))]);
    cart.customer_id = Some(customer.clone());
    let old_debt = crate::sales::SaleInput {
        cart,
        payments: vec![crate::payments::PaymentInput {
            method: PaymentMethod::Credit,
            amount: Money(500_000),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        }],
    };
    shop.db
        .write(move |tx| crate::sales::create_sale(tx, &ctx, old_debt))
        .unwrap();
    let s = signed(&shop, &fridge, &customer);
    let ctx = shop.owner_till();
    let id = s.plan.id.clone();
    let pay = PlanPayment {
        amount: Money(2_000_000),
        method: PaymentMethod::Transfer,
        reference: Some("CCP 00799999 12".into()),
        cheque_bank: None,
        cheque_date: None,
        note: None,
    };
    let collected = shop
        .db
        .write(move |tx| ins::collect(tx, &ctx, &id, pay))
        .unwrap();
    assert_eq!(
        collected.quote.stamp_duty,
        Money::ZERO,
        "a detailed CCP transfer pays no duty"
    );
    let id = s.plan.id.clone();
    let plan = shop
        .db
        .read(move |c| ins::get(c, &id, "2026-11-15"))
        .unwrap();
    assert_eq!(
        plan.paid,
        Money(2_000_000),
        "the older ventilator debt was not touched"
    );
    let states: Vec<(DueStatus, i64)> = plan
        .dues
        .iter()
        .take(3)
        .map(|d| (d.status, d.paid.0))
        .collect();
    assert_eq!(
        states,
        [
            (DueStatus::Paid, 1_500_000),
            (DueStatus::Partial, 500_000),
            (DueStatus::Upcoming, 0)
        ]
    );
    let id = s.plan.id.clone();
    let later = shop
        .db
        .read(move |c| ins::get(c, &id, "2027-01-05"))
        .unwrap();
    assert_eq!(
        (later.dues[1].status, later.dues[1].days_late),
        (DueStatus::Overdue, 36)
    );
}

#[test]
fn reminders_list_due_and_late_instalments_with_a_text_in_the_customers_language() {
    let (shop, fridge, customer) = store();
    let s = signed(&shop, &fridge, &customer);
    let soon = shop
        .db
        .read(|c| ins::reminders(c, "2026-10-29", 3))
        .unwrap();
    assert_eq!((soon.len(), soon[0].due.seq), (1, 1));
    let id = s.plan.id.clone();
    let text = shop
        .db
        .read(move |c| ins::reminder_text(c, &id, 1, Lang::Ar, "2026-10-29"))
        .unwrap();
    assert!(text.contains("CT2026-000001") && text.contains("15000.00"));
    let ctx = shop.owner_till();
    let id = s.plan.id.clone();
    shop.db
        .write(move |tx| ins::note_reminder(tx, &ctx, &id, 1, Some("Appel téléphonique".into())))
        .unwrap();
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM instalment_events WHERE kind = 'reminder_given'"),
        1
    );
}

#[test]
fn the_overdue_report_lists_every_late_due_as_of_its_day() {
    use crate::reports::{ReportKind, ReportRequest, run};
    use crate::table::Cell;
    let (shop, fridge, customer) = store();
    let s = signed(&shop, &fridge, &customer);
    let ctx = shop.owner_till();
    let id = s.plan.id.clone();
    let pay = PlanPayment {
        amount: Money(2_000_000),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    };
    shop.db
        .write(move |tx| ins::collect(tx, &ctx, &id, pay))
        .unwrap();
    let mut req = ReportRequest::new(ReportKind::InstalmentsOverdue, "2027-01-05", "2027-01-05");
    req.customer_id = Some(customer.clone());
    let report = shop.db.read(move |c| run(c, &req)).unwrap();
    let row = |due: &str, amount: i64, paid: i64, late: i64| {
        vec![
            Cell::text("CT2026-000001"),
            Cell::text("Farid Bouzid"),
            Cell::text(due),
            Cell::int(amount),
            Cell::int(paid),
            Cell::int(amount - paid),
            Cell::int(late),
        ]
    };
    assert_eq!(
        report.table.rows,
        [
            row("2026-11-30", 1_500_000, 500_000, 36),
            row("2026-12-31", 1_500_000, 0, 5)
        ]
    );
    assert_eq!(
        report.table.totals,
        Some(vec![
            Cell::NULL,
            Cell::NULL,
            Cell::NULL,
            Cell::int(3_000_000),
            Cell::int(500_000),
            Cell::int(2_500_000),
            Cell::NULL
        ])
    );
    let req = ReportRequest::new(ReportKind::InstalmentsOverdue, "2026-11-30", "2026-11-30");
    assert!(
        shop.db
            .read(move |c| run(c, &req))
            .unwrap()
            .table
            .rows
            .is_empty(),
        "a due is late the day after its date"
    );
    let plans = shop
        .db
        .read(|c| {
            ins::list(
                c,
                &ins::PlanFilter {
                    overdue_only: true,
                    ..Default::default()
                },
                "2027-01-05",
            )
        })
        .unwrap();
    assert_eq!(plans.len(), 1);
    let none = shop
        .db
        .read(|c| {
            ins::list(
                c,
                &ins::PlanFilter {
                    overdue_only: true,
                    ..Default::default()
                },
                "2026-11-15",
            )
        })
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn the_overdue_report_of_a_past_day_ignores_the_payments_made_after_it() {
    use crate::reports::{ReportKind, ReportRequest, run};
    use crate::table::Cell;
    use crate::testkit::NOON;
    use crate::till::TillContext;
    const DAY_MS: i64 = 86_400_000;
    let (shop, fridge, customer) = store();
    let s = signed(&shop, &fridge, &customer);
    // 20 000 DA paid on 2026-11-07, after the first due (2026-10-31) was late.
    let later = TillContext::new(
        shop.owner.clone(),
        std::sync::Arc::clone(&shop.pack),
        NOON + 40 * DAY_MS,
    );
    assert_eq!(later.business_day(), "2026-11-07");
    let id = s.plan.id.clone();
    let pay = PlanPayment {
        amount: Money(2_000_000),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        note: None,
    };
    shop.db
        .write(move |tx| ins::collect(tx, &later, &id, pay))
        .unwrap();
    let mut req = ReportRequest::new(ReportKind::InstalmentsOverdue, "2026-11-05", "2026-11-05");
    req.customer_id = Some(customer.clone());
    let report = shop.db.read(move |c| run(c, &req)).unwrap();
    assert_eq!(
        report.table.rows,
        [vec![
            Cell::text("CT2026-000001"),
            Cell::text("Farid Bouzid"),
            Cell::text("2026-10-31"),
            Cell::int(1_500_000),
            Cell::int(0),
            Cell::int(1_500_000),
            Cell::int(5),
        ]],
        "on 2026-11-05 the first due was late and nothing had been paid on it yet"
    );
    let req = ReportRequest::new(ReportKind::InstalmentsOverdue, "2026-11-10", "2026-11-10");
    assert!(
        shop.db
            .read(move |c| run(c, &req))
            .unwrap()
            .table
            .rows
            .is_empty(),
        "once the payment is recorded the first due is paid and the second not yet due"
    );
}

#[test]
fn guarantees_are_added_later_and_a_module_off_takes_no_new_contract() {
    let (shop, fridge, customer) = store();
    let s = signed(&shop, &fridge, &customer);
    let ctx = shop.owner_till();
    let id = s.plan.id.clone();
    let guarantor = GuaranteeInput {
        kind: GuaranteeKind::Guarantor,
        holder_name: Some("Karim Bouzid".into()),
        reference: None,
        phone: Some("0550 12 34 56".into()),
        note: None,
    };
    let g = shop
        .db
        .write(move |tx| ins::add_guarantee(tx, &ctx, &id, guarantor))
        .unwrap();
    assert_eq!(
        (g.kind, g.holder_name.as_deref()),
        (GuaranteeKind::Guarantor, Some("Karim Bouzid"))
    );
    let id = s.plan.id.clone();
    assert_eq!(
        shop.db
            .read(move |c| ins::get(c, &id, "2026-09-28"))
            .unwrap()
            .guarantees
            .len(),
        2
    );
    let ctx = shop.owner_till();
    let id = s.plan.id.clone();
    let err = shop
        .db
        .write(move |tx| ins::note_reminder(tx, &ctx, &id, 7, None))
        .unwrap_err();
    assert_eq!(err.code(), "instalment_due_unknown");
    let ctx = shop.owner_till();
    let mut q = quote_input(&fridge, &customer);
    q.down = Money(12_000_000);
    let err = shop.db.read(move |c| ins::quote(c, &ctx, &q)).unwrap_err();
    assert_eq!(err.code(), "instalment_nothing_financed");
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[]))
        .unwrap();
    let ctx = shop.owner_till();
    let q = quote_input(&fridge, &customer);
    assert_eq!(
        shop.db
            .read(move |c| ins::quote(c, &ctx, &q))
            .unwrap_err()
            .code(),
        "module_disabled"
    );
}

#[test]
fn the_instalment_price_level_is_seeded_once() {
    let shop = shop();
    let actor = shop.owner.actor();
    let created = shop
        .db
        .write(move |tx| Ok((ins::seed_level(tx, &actor)?, ins::seed_level(tx, &actor)?)))
        .unwrap();
    assert_eq!(created, (1, 0));
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM price_levels WHERE code = 'instalment' AND sort_order = 5"
        ),
        1
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'instalments.level_seeded'"),
        1
    );
}

proptest! {
    #[test]
    fn the_schedule_sums_to_the_credit_part_and_the_last_due_takes_the_rest(
        financed in 1_i64..1_000_000_000,
        count in 1_u8..=60,
    ) {
        let dues = ins::schedule(Money(financed), count, day("2026-10-15")).unwrap();
        prop_assert_eq!(dues.len(), usize::from(count));
        prop_assert_eq!(dues.iter().map(|d| d.amount.0).sum::<i64>(), financed);
        prop_assert!(dues.iter().all(|d| d.amount.is_positive()));
        let first = dues[0].amount.0;
        prop_assert!(dues[..dues.len() - 1].iter().all(|d| d.amount.0 == first));
        prop_assert!(dues.windows(2).all(|w| w[0].due_on < w[1].due_on));
    }
}
