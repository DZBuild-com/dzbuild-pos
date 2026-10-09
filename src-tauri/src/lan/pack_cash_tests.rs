//! The loyalty and staff packs on a till: the cash a till's cashier takes for a gift voucher, pays out as a
//! salary advance from the drawer or takes back as a repayment goes through the main PC, which
//! numbers the voucher and writes the ledgers, while the money lands in the till's own drawer and
//! Z, never in the main PC's.

use super::two_pcs::{TwoPcs, close_approved};
use crate::commands::{cash as cash_cmd, loyalty, staff};
use pos_core::cash::{self, MovementKind};
use pos_core::money::Money;
use pos_core::packs::Module;
use pos_core::payments::PaymentMethod;
use pos_core::staff::AdvanceInput;

/// `(kind, amount)` of the drawer movements naming `ref_type` on `st`.
fn moves(st: &crate::state::AppState, ref_type: &'static str) -> Vec<(MovementKind, Money)> {
    st.db
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

#[test]
fn a_tills_cash_voucher_advance_and_repayment_land_in_its_own_drawer_and_z() {
    let pcs = TwoPcs::new();
    let actor = pcs.host.owner.actor();
    pcs.host
        .st
        .db
        .write(move |tx| {
            pos_core::packs::set_modules(tx, &actor, &[Module::GiftVouchers, Module::Staff])
        })
        .unwrap();
    pcs.wait_caught_up();
    let till = pcs.till();
    let cashier = till.cashier();
    cash_cmd::open_session(&till.st, Money(1_000_000)).unwrap();

    // a gift voucher sold for cash: numbered by the main PC, its cash in the till's drawer
    let sold = loyalty::voucher_sell(
        &till.st,
        loyalty::voucher_sale(500_000, PaymentMethod::Cash),
    )
    .unwrap();
    assert!(sold.voucher.code.starts_with("BA2026-"));
    let received = Money(500_000 + sold.voucher.stamp_charged.0);
    assert_eq!(
        moves(&till.st, "gift_voucher"),
        [(MovementKind::PaidIn, received)],
        "the voucher's cash and duty are in the till's drawer"
    );

    // an advance paid from the drawer, then part of it repaid in cash
    let advance = staff::advance(
        &till.st,
        AdvanceInput {
            user_id: cashier.user_id.clone(),
            amount: Money(200_000),
            method: PaymentMethod::Cash,
            from_till: true,
            note: None,
        },
    )
    .unwrap();
    assert_eq!(advance.expense_id, None);
    staff::repay(
        &till.st,
        cashier.user_id.clone(),
        Money(50_000),
        PaymentMethod::Cash,
        None,
    )
    .unwrap();
    assert_eq!(
        moves(&till.st, "staff_ledger"),
        [
            (MovementKind::Expense, Money(-200_000)),
            (MovementKind::PaidIn, Money(50_000))
        ],
        "the advance and the repayment moved the till's drawer"
    );

    // the main PC wrote the ledgers on the till's session and never opened a drawer
    let host_terminal = pcs.host.st.terminal.id.clone();
    assert_eq!(
        pcs.host
            .st
            .db
            .read(move |c| cash::current(c, &host_terminal))
            .unwrap(),
        None
    );
    let till_terminal = till.st.terminal.id.clone();
    let till_session = till
        .st
        .db
        .read(move |c| cash::require_open(c, &till_terminal))
        .unwrap()
        .id;
    let booked: Vec<Option<String>> = pcs
        .host
        .st
        .db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT cash_session_id FROM gift_voucher_ledger
                 UNION ALL SELECT cash_session_id FROM staff_ledger",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(booked, vec![Some(till_session); 3]);

    // the till's Z: every dinar of its drawer
    pcs.wait_sent();
    pcs.wait_caught_up();
    let z = close_approved(till).z.summary;
    assert_eq!(
        (z.paid_in, z.expenses_total),
        (Money(received.0 + 50_000), Money(-200_000))
    );
    assert_eq!(
        z.expected_cash,
        Some(Money(1_000_000 + received.0 + 50_000 - 200_000))
    );
}
