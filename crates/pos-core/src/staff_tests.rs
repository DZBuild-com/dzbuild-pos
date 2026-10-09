//! Employees lite and zakat (Task 15).

use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleInput, create_sale, price_cart};
use crate::staff::{self, AdvanceInput, ProfileInput};
use crate::testkit::{Shop, shop};
use crate::zakat::{self, Valuation, ZakatInput};

fn office() -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Staff, Module::Zakat]))
        .unwrap();
    shop.open_till(100_000);
    shop
}

/// The shop of these tests (staff and zakat modules on, the cashier's till open), for the
/// report tests.
pub(crate) fn office_for_reports() -> Shop {
    office()
}

/// The cashier sells `qty` milli-units of `product`, paid by card, for the report tests.
pub(crate) fn sell_for_reports(shop: &Shop, product: &str, qty: i64) {
    let ctx = shop.till();
    let cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(qty))]);
    let c = cart.clone();
    let total = shop
        .db
        .read(move |conn| price_cart(conn, &ctx, &c))
        .unwrap()
        .totals
        .total_ttc;
    let ctx = shop.till();
    let card = PaymentInput::electronic(PaymentMethod::CardCib, total, Some("A7"));
    shop.db
        .write(move |tx| {
            create_sale(
                tx,
                &ctx,
                SaleInput {
                    cart,
                    payments: vec![card],
                },
            )
        })
        .unwrap();
}

#[test]
fn an_advance_leaves_the_drawer_as_a_wages_expense_and_is_owed_until_repaid_or_deducted() {
    let shop = office();
    let cashier = shop.cashier.user_id.clone();
    let actor = shop.owner.actor();
    let c = cashier.clone();
    let profile = shop
        .db
        .write(move |tx| {
            staff::set_profile(
                tx,
                &actor,
                &c,
                ProfileInput {
                    phone: None,
                    hire_date: Some("2025-03-01".into()),
                    monthly_salary: Some(Money(4_500_000)),
                    commission_bp: 100,
                    note: None,
                },
            )
        })
        .unwrap();
    assert_eq!((profile.commission_bp, profile.owed), (100, Money::ZERO));
    let drawer_before = shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements");
    let (ctx, c) = (shop.owner_till(), cashier.clone());
    let given = shop
        .db
        .write(move |tx| {
            staff::advance(
                tx,
                &ctx,
                AdvanceInput {
                    user_id: c,
                    amount: Money(500_000),
                    method: PaymentMethod::Cash,
                    from_till: true,
                    note: Some("Avance septembre".into()),
                },
            )
        })
        .unwrap();
    assert!(given.expense_id.is_some());
    assert_eq!(
        shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements") - drawer_before,
        -500_000
    );
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM expenses e JOIN expense_categories c ON c.id = e.category_id WHERE c.code = 'wages'"
        ),
        1
    );
    let (ctx, c) = (shop.owner_till(), cashier.clone());
    shop.db
        .write(move |tx| staff::deduct(tx, &ctx, &c, Money(300_000), "2026-09", None))
        .unwrap();
    let (ctx, c) = (shop.owner_till(), cashier.clone());
    assert_eq!(
        shop.db
            .write(move |tx| staff::deduct(tx, &ctx, &c, Money(1), "2026-13", None))
            .unwrap_err()
            .code(),
        "month_invalid"
    );
    let (ctx, c) = (shop.owner_till(), cashier.clone());
    let err = shop
        .db
        .write(move |tx| staff::repay(tx, &ctx, &c, Money(300_000), PaymentMethod::Cash, None))
        .unwrap_err();
    assert_eq!(err.code(), "advance_repay_too_much");
    let (ctx, c) = (shop.owner_till(), cashier.clone());
    shop.db
        .write(move |tx| staff::repay(tx, &ctx, &c, Money(200_000), PaymentMethod::Cash, None))
        .unwrap();
    let c = cashier.clone();
    assert_eq!(
        shop.db.read(move |conn| staff::owed(conn, &c)).unwrap(),
        Money::ZERO
    );
    assert_eq!(
        shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements") - drawer_before,
        -300_000
    );
}

#[test]
fn shifts_come_from_the_cash_sessions() {
    let shop = office();
    let bread = shop.product("Pain", Money(2_500), &shop.exempt).id;
    let ctx = shop.till();
    let sale = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&bread, Qty(4_000))]),
        payments: vec![PaymentInput::cash(Money(10_000))],
    };
    shop.db
        .write(move |tx| create_sale(tx, &ctx, sale))
        .unwrap();
    let rows = shop
        .db
        .read(|c| staff::shifts(c, "2026-09-28", "2026-09-28", None))
        .unwrap();
    let mine = rows
        .iter()
        .find(|r| r.user_id == shop.cashier.user_id)
        .unwrap();
    assert_eq!(
        (mine.tickets, mine.sales_ttc, mine.closed_at),
        (1, Money(10_000), None)
    );
}

#[test]
fn zakat_is_two_and_a_half_percent_of_the_base_above_the_nisab() {
    let shop = office();
    let flour = shop.product("Farine 25 kg", Money(300_000), &shop.tva9).id;
    shop.stock_in(&flour, 500_000);
    let customer = shop.customer("Épicerie Amine").id;
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&flour, Qty(1_000))]);
    cart.customer_id = Some(customer);
    let credit = PaymentInput {
        method: PaymentMethod::Credit,
        amount: Money(300_000),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    };
    shop.db
        .write(move |tx| {
            create_sale(
                tx,
                &ctx,
                SaleInput {
                    cart,
                    payments: vec![credit],
                },
            )
        })
        .unwrap();
    let input = ZakatInput {
        day: Some("2026-09-28".into()),
        valuation: Valuation::SalePrice,
        bank: Money(100_000_000),
        other_assets: Money::ZERO,
        other_debts: Money(5_000_000),
        note: None,
    };
    let ctx = shop.owner_till();
    let i = input.clone();
    let p = shop.db.read(move |c| zakat::preview(c, &ctx, &i)).unwrap();
    assert_eq!(
        (p.stock_value, p.cash, p.receivables, p.payables),
        (
            Money(149_700_000),
            Money(100_000),
            Money(300_000),
            Money::ZERO
        )
    );
    assert_eq!(
        (p.base, p.nisab, p.rate_bp),
        (Money(245_100_000), Money(229_500_000), 250)
    );
    assert_eq!((p.zakat_due, p.below_nisab), (Money(6_127_500), false));
    assert_eq!(
        (p.hijri_date.year, p.hijri_date.month, p.hijri_date.day),
        (1448, 4, 17)
    );
    let ctx = shop.owner_till();
    let saved = shop
        .db
        .write(move |tx| zakat::save(tx, &ctx, input))
        .unwrap();
    assert_eq!(saved.number, "ZK2026-000001");
    assert!(
        shop.db
            .write(|tx| Ok(tx.execute_batch("UPDATE zakat_statements SET zakat_due = 0")?))
            .is_err()
    );
    let ctx = shop.owner_till();
    let poor = ZakatInput {
        day: None,
        valuation: Valuation::SalePrice,
        bank: Money::ZERO,
        other_assets: Money::ZERO,
        other_debts: Money(5_000_000),
        note: None,
    };
    let p = shop
        .db
        .read(move |c| zakat::preview(c, &ctx, &poor))
        .unwrap();
    assert_eq!(
        (p.base, p.zakat_due, p.below_nisab),
        (Money(145_100_000), Money::ZERO, true)
    );
}
