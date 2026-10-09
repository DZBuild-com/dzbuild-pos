//! Loyalty points, gift, exchange and loyalty vouchers (Task 13).

use crate::gift_vouchers::{self as gv, SellInput, VoucherKind, VoucherStatus};
use crate::lang::Lang;
use crate::loyalty;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::returns::{ReturnInput, ReturnLineInput, ReturnReason, create_return_with};
use crate::sales::extensions::ReturnExtras;
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, ValidatedSale, create_sale_with};
use crate::testkit::{Shop, shop};
use proptest::prelude::*;

fn club() -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(
                tx,
                &actor,
                &[
                    Module::Loyalty,
                    Module::GiftVouchers,
                    Module::DepartmentKeys,
                ],
            )
        })
        .unwrap();
    shop.open_till(100_000);
    shop
}

fn enrolled(shop: &Shop, name: &str) -> String {
    let id = shop.customer(name).id;
    let (ctx, c) = (shop.owner_till(), id.clone());
    shop.db
        .write(move |tx| loyalty::enrol(tx, &ctx, &c, None))
        .unwrap();
    id
}

fn sell(
    shop: &Shop,
    product: &str,
    qty: i64,
    customer: Option<&str>,
    payments: Vec<PaymentInput>,
) -> Result<ValidatedSale, crate::error::AppError> {
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(qty))]);
    cart.customer_id = customer.map(str::to_owned);
    shop.db.write(move |tx| {
        create_sale_with(
            tx,
            &ctx,
            SaleInput { cart, payments },
            &SaleExtras::default(),
        )
    })
}

fn voucher_tender(code: &str, amount: i64) -> PaymentInput {
    PaymentInput {
        method: PaymentMethod::Voucher,
        amount: Money(amount),
        reference: Some(code.into()),
        cheque_bank: None,
        cheque_date: None,
    }
}

/// Cash for goods of `amount`, with room for the droit de timbre charged on cash (the shop's
/// default mode): the change goes back.
fn cash(amount: i64) -> PaymentInput {
    PaymentInput::cash(Money(amount + amount / 50 + 1_000))
}

fn balance(shop: &Shop, customer: &str) -> i64 {
    let c = customer.to_owned();
    shop.db
        .read(move |conn| loyalty::account(conn, &c, "2026-09-28"))
        .unwrap()
        .balance
}

fn back(line: &str, qty: i64) -> ReturnLineInput {
    ReturnLineInput {
        origin_line_id: Some(line.into()),
        product_id: None,
        qty: Qty(qty),
        unit_price: None,
        restock: true,
        reason: ReturnReason::Other,
        note: None,
    }
}

fn sell_gift(shop: &Shop, amount: i64) -> gv::Voucher {
    let ctx = shop.till();
    let input = SellInput {
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        customer_id: None,
        buyer_name: Some("Karim".into()),
    };
    shop.db.write(move |tx| gv::sell(tx, &ctx, input)).unwrap()
}

/// The shop of these tests, for the vouchers report test (`reports::packs`).
pub(crate) fn club_for_reports() -> Shop {
    club()
}

/// A cash gift voucher of `amount`, for the vouchers report test.
pub(crate) fn gift_for_reports(shop: &Shop, amount: i64) -> gv::Voucher {
    sell_gift(shop, amount)
}

/// One piece of `product` paid by `amount` of the voucher `code`, for the vouchers report test.
pub(crate) fn spend_for_reports(shop: &Shop, product: &str, code: &str, amount: i64) {
    sell(
        shop,
        product,
        1_000,
        None,
        vec![voucher_tender(code, amount)],
    )
    .unwrap();
}

#[test]
fn points_are_earned_on_money_paid_by_enrolled_customers() {
    let shop = club();
    let dates = shop
        .product("Dattes Deglet Nour 1 kg", Money(235_000), &shop.tva9)
        .id;
    let amina = enrolled(&shop, "Amina");
    sell(&shop, &dates, 1_000, Some(&amina), vec![cash(235_000)]).unwrap();
    assert_eq!(balance(&shop, &amina), 23, "one point per whole 100 DA");
    let card = sell_gift(&shop, 100_000);
    sell(
        &shop,
        &dates,
        1_000,
        Some(&amina),
        vec![voucher_tender(&card.code, 100_000), cash(135_000)],
    )
    .unwrap();
    assert_eq!(
        balance(&shop, &amina),
        36,
        "no points on the part a voucher paid"
    );
    let walk_in = shop.customer("Sans carte").id;
    sell(&shop, &dates, 1_000, Some(&walk_in), vec![cash(235_000)]).unwrap();
    assert_eq!(balance(&shop, &walk_in), 0, "not enrolled, no points");
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM customer_consents WHERE channel = 'loyalty' AND granted = 1"
        ),
        1
    );
}

#[test]
fn points_earned_on_returned_goods_come_back() {
    let shop = club();
    let oil = shop
        .product("Huile d'olive 1 L", Money(125_000), &shop.tva19)
        .id;
    let amina = enrolled(&shop, "Amina");
    let sale = sell(&shop, &oil, 4_000, Some(&amina), vec![cash(500_000)]).unwrap();
    assert_eq!(balance(&shop, &amina), 50);
    let ctx = shop.owner_till();
    let line = sale.document.lines[0].id.clone();
    let input = ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![back(&line, 2_000)],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    shop.db
        .write(move |tx| create_return_with(tx, &ctx, input, None, &ReturnExtras::default()))
        .unwrap();
    assert_eq!(
        balance(&shop, &amina),
        25,
        "the points of the returned half came back"
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM loyalty_ledger WHERE kind = 'reversal' AND points = -25"),
        1
    );
}

#[test]
fn rewards_are_vouchers_and_nothing_is_drawn() {
    let shop = club();
    let amina = enrolled(&shop, "Amina");
    let (ctx, c) = (shop.owner_till(), amina.clone());
    shop.db
        .write(move |tx| loyalty::adjust(tx, &ctx, &c, 600, "Reprise du carnet papier"))
        .unwrap();
    let (ctx, c) = (shop.owner_till(), amina.clone());
    assert_eq!(
        shop.db
            .write(move |tx| loyalty::redeem(tx, &ctx, &c, 400))
            .unwrap_err()
            .code(),
        "points_below_minimum"
    );
    let (ctx, c) = (shop.owner_till(), amina.clone());
    let reward = shop
        .db
        .write(move |tx| loyalty::redeem(tx, &ctx, &c, 500))
        .unwrap();
    assert_eq!(
        (
            reward.kind,
            reward.initial_amount,
            reward.customer_id.as_deref()
        ),
        (VoucherKind::Loyalty, Money(50_000), Some(amina.as_str()))
    );
    assert_eq!(balance(&shop, &amina), 100);
    let names: Vec<String> = shop
        .db
        .read(|c| {
            let mut s = c.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
            Ok(s.query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?)
        })
        .unwrap();
    assert!(names.iter().all(|n| {
        !["draw", "lottery", "tombola", "raffle", "prize"]
            .iter()
            .any(|w| n.contains(w))
    }));
    assert!(Module::ALL.iter().all(|m| !m.as_str().contains("draw")));
    assert!(
        shop.db
            .write(|tx| Ok(tx.execute_batch(
                "INSERT INTO loyalty_ledger (id, customer_id, kind, points, business_date, created_at, user_id, terminal_id)
             SELECT '00000000-0000-7000-8000-00000000d001', id, 'draw', 1, '2026-09-28', 1, 'u', 't' FROM customers LIMIT 1"
            )?))
            .is_err(),
        "the ledger has no draw entry kind"
    );
}

#[test]
fn a_gift_voucher_is_sold_with_its_duty_and_spent_as_a_tender_until_empty() {
    let shop = club();
    let cash_before = shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements");
    let ctx = shop.till();
    let input = SellInput {
        amount: Money(500_000),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        customer_id: None,
        buyer_name: None,
    };
    let q = shop
        .db
        .read(move |c| gv::quote_sale(c, &ctx, &input))
        .unwrap();
    assert_eq!(
        (q.stamp_duty, q.to_receive),
        (Money(5_000), Money(505_000)),
        "50 tranches of 100 DA at 1 DA"
    );
    let v = sell_gift(&shop, 500_000);
    assert_eq!(
        (v.code.as_str(), v.barcode.len(), v.expires_on.as_str()),
        ("BA2026-000001", 13, "2027-09-28")
    );
    let code = v.code.clone();
    let read = shop.db.read(move |c| gv::get(c, &code)).unwrap();
    assert_eq!(
        (
            v.stamp_duty,
            v.stamp_charged,
            read.stamp_duty,
            read.stamp_charged
        ),
        (Money(5_000), Money(5_000), Money(5_000), Money(5_000)),
        "the voucher carries the duty of its issue row, for its slip"
    );
    assert_eq!(
        shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements") - cash_before,
        505_000
    );
    let item = shop.product("Parfum", Money(300_000), &shop.tva19).id;
    let small = shop.product("Savon", Money(250_000), &shop.tva19).id;
    sell(
        &shop,
        &item,
        1_000,
        None,
        vec![voucher_tender(&v.barcode, 300_000)],
    )
    .unwrap();
    let err = sell(
        &shop,
        &small,
        1_000,
        None,
        vec![voucher_tender(&v.code, 250_000)],
    )
    .unwrap_err();
    assert_eq!(err.code(), "voucher_balance_insufficient");
    sell(
        &shop,
        &small,
        1_000,
        None,
        vec![voucher_tender(&v.code, 200_000), cash(50_000)],
    )
    .unwrap();
    let code = v.code.clone();
    let after = shop.db.read(move |c| gv::get(c, &code)).unwrap();
    assert_eq!(
        (after.balance, after.status),
        (Money::ZERO, VoucherStatus::Spent)
    );
}

#[test]
fn a_return_without_customer_gives_an_exchange_voucher() {
    let shop = club();
    let shirt = shop.product("Chemise", Money(250_000), &shop.tva19).id;
    let sale = sell(&shop, &shirt, 1_000, None, vec![cash(250_000)]).unwrap();
    let ctx = shop.till();
    let line = sale.document.lines[0].id.clone();
    let input = ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![back(&line, 1_000)],
        refund_method: PaymentMethod::Voucher,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    let cash_before = shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements");
    let no_extras = input.clone();
    let c2 = shop.till();
    let refused = shop
        .db
        .write(move |tx| create_return_with(tx, &c2, no_extras, None, &ReturnExtras::default()))
        .unwrap_err();
    assert_eq!(
        refused.code(),
        "customer_required",
        "M2's store credit still needs a customer"
    );
    let extras = ReturnExtras {
        exchange_voucher: true,
        ..ReturnExtras::default()
    };
    let done = shop
        .db
        .write(move |tx| create_return_with(tx, &ctx, input, None, &extras))
        .unwrap();
    let voucher = done.exchange_voucher.clone().unwrap();
    assert_eq!(
        (voucher.kind, voucher.initial_amount),
        (VoucherKind::Exchange, Money(250_000))
    );
    assert_eq!(
        shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements"),
        cash_before,
        "no cash left the drawer"
    );
    sell(
        &shop,
        &shirt,
        1_000,
        None,
        vec![voucher_tender(&voucher.barcode, 250_000)],
    )
    .unwrap();
}

#[test]
fn expired_vouchers_and_points_leave_the_balance() {
    let shop = club();
    let v = sell_gift(&shop, 100_000);
    let amina = enrolled(&shop, "Amina");
    let (ctx, c) = (shop.owner_till(), amina.clone());
    shop.db
        .write(move |tx| loyalty::adjust(tx, &ctx, &c, 80, "Bienvenue"))
        .unwrap();
    let dates = shop.product("Dattes", Money(100_000), &shop.tva9).id;
    sell(&shop, &dates, 1_000, Some(&amina), vec![cash(100_000)]).unwrap();
    let actor = shop.owner.actor();
    let expired = shop
        .db
        .write(move |tx| {
            Ok((
                gv::expire_due(tx, &actor, "2027-09-29")?,
                loyalty::expire_due(tx, &actor, "2027-09-29")?,
            ))
        })
        .unwrap();
    assert_eq!(expired, (1, 1));
    let code = v.code.clone();
    assert_eq!(
        shop.db.read(move |c| gv::get(c, &code)).unwrap().status,
        VoucherStatus::Expired
    );
    assert_eq!(
        balance(&shop, &amina),
        80,
        "the adjustment never expires; the earned point did"
    );
}

#[test]
fn an_expired_voucher_is_refused_as_a_tender_and_its_duty_reaches_the_stamp_statement() {
    let shop = club();
    let v = sell_gift(&shop, 500_000);
    let req = crate::reports::ReportRequest::new(
        crate::reports::ReportKind::StampDutyStatement,
        "2026-09-28",
        "2026-09-28",
    );
    let report = shop.db.read(move |c| crate::reports::run(c, &req)).unwrap();
    let totals = report.table.totals.clone().unwrap();
    assert_eq!(
        totals[6].as_i64(),
        Some(5_000),
        "the voucher's duty is due sur état"
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| gv::expire_due(tx, &actor, "2027-09-29"))
        .unwrap();
    let item = shop.product("Parfum", Money(100_000), &shop.tva19).id;
    let err = sell(
        &shop,
        &item,
        1_000,
        None,
        vec![voucher_tender(&v.code, 100_000)],
    )
    .unwrap_err();
    assert_eq!(err.code(), "voucher_expired");
}

#[test]
fn returns_never_take_back_more_than_the_ticket_earned_nor_points_already_spent() {
    let shop = club();
    let oil = shop.product("Huile", Money(125_000), &shop.tva19).id;
    let amina = enrolled(&shop, "Amina");
    let sale = sell(&shop, &oil, 4_000, Some(&amina), vec![cash(500_000)]).unwrap();
    let (ctx, c) = (shop.owner_till(), amina.clone());
    shop.db
        .write(move |tx| loyalty::adjust(tx, &ctx, &c, -20, "Correction"))
        .unwrap();
    let give_back = |qty: i64| {
        let ctx = shop.owner_till();
        let input = ReturnInput {
            original_id: Some(sale.document.id.clone()),
            lines: vec![back(&sale.document.lines[0].id, qty)],
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            lang: Lang::Fr,
            note: None,
        };
        shop.db
            .write(move |tx| create_return_with(tx, &ctx, input, None, &ReturnExtras::default()))
            .unwrap();
    };
    give_back(1_000);
    assert_eq!(balance(&shop, &amina), 18, "a quarter of the 50 points");
    give_back(3_000);
    assert_eq!(
        balance(&shop, &amina),
        0,
        "the rest is capped by the balance: the 20 adjusted away stay gone"
    );
    assert_eq!(
        shop.count("SELECT -SUM(points) FROM loyalty_ledger WHERE kind = 'reversal'"),
        30
    );
}

#[test]
fn a_card_is_scanned_blocked_and_replaced_and_the_account_shows_what_expires_soon() {
    let shop = club();
    let amina = enrolled(&shop, "Amina");
    let a = amina.clone();
    let card = shop
        .db
        .read(move |c| loyalty::account(c, &a, "2026-09-28"))
        .unwrap()
        .card
        .unwrap()
        .code;
    assert!(card.len() == 13 && card.starts_with("297"));
    let code = card.clone();
    assert_eq!(
        shop.db
            .read(move |c| loyalty::customer_of_card(c, &code))
            .unwrap(),
        Some(amina.clone())
    );
    let (ctx, karim, code) = (shop.owner_till(), shop.customer("Karim").id, card.clone());
    let taken = shop
        .db
        .write(move |tx| loyalty::enrol(tx, &ctx, &karim, Some(code)))
        .unwrap_err();
    assert_eq!(
        taken.code(),
        "loyalty_card_invalid",
        "a card has one holder"
    );
    let (ctx, code) = (shop.owner_till(), card.clone());
    let blocked = shop
        .db
        .write(move |tx| loyalty::block(tx, &ctx, &code))
        .unwrap();
    assert!(blocked.blocked_at.is_some());
    let (ctx, code) = (shop.owner_till(), card.clone());
    assert_eq!(
        shop.db
            .write(move |tx| loyalty::block(tx, &ctx, &code))
            .unwrap_err()
            .code(),
        "loyalty_card_invalid"
    );
    let code = card.clone();
    assert_eq!(
        shop.db
            .read(move |c| loyalty::customer_of_card(c, &code))
            .unwrap(),
        None
    );
    let (ctx, c) = (shop.owner_till(), amina.clone());
    assert_eq!(
        shop.db
            .write(move |tx| loyalty::redeem(tx, &ctx, &c, 500))
            .unwrap_err()
            .code(),
        "loyalty_not_enrolled"
    );
    let (ctx, c) = (shop.owner_till(), amina.clone());
    let new_card = shop
        .db
        .write(move |tx| loyalty::enrol(tx, &ctx, &c, Some("fid00042".into())))
        .unwrap();
    assert_eq!(new_card.code, "FID00042");
    let dates = shop.product("Dattes", Money(235_000), &shop.tva9).id;
    let sale = sell(&shop, &dates, 1_000, Some(&amina), vec![cash(235_000)]).unwrap();
    let a = amina.clone();
    let soon = shop
        .db
        .read(move |c| loyalty::account(c, &a, "2027-09-01"))
        .unwrap();
    assert_eq!(
        (
            soon.balance,
            soon.expiring_soon,
            soon.next_expiry.as_deref()
        ),
        (23, 23, Some("2027-09-28"))
    );
    assert!(!soon.redeemable, "23 points are under the 500 minimum");
    let a = amina.clone();
    let today = shop
        .db
        .read(move |c| loyalty::account(c, &a, "2026-09-28"))
        .unwrap();
    assert_eq!((today.expiring_soon, today.next_expiry), (0, None));
    let a = amina.clone();
    let entries = shop.db.read(move |c| loyalty::entries(c, &a)).unwrap();
    assert_eq!(
        (
            entries[0].kind,
            entries[0].points,
            entries[0].document_number.clone()
        ),
        (loyalty::PointsKind::Earn, 23, sale.document.number.clone())
    );
}

#[test]
fn vouchers_are_listed_by_kind_status_and_text_and_described_for_the_payment_screen() {
    let shop = club();
    let gift = sell_gift(&shop, 100_000);
    let amina = enrolled(&shop, "Amina");
    let (ctx, c) = (shop.owner_till(), amina.clone());
    shop.db
        .write(move |tx| loyalty::adjust(tx, &ctx, &c, 500, "Reprise du carnet papier"))
        .unwrap();
    let (ctx, c) = (shop.owner_till(), amina.clone());
    let reward = shop
        .db
        .write(move |tx| loyalty::redeem(tx, &ctx, &c, 500))
        .unwrap();
    let list = |filter: gv::VoucherFilter, today: &'static str| {
        shop.db
            .read(move |c| gv::list(c, &filter, today))
            .unwrap()
            .into_iter()
            .map(|v| v.code)
            .collect::<Vec<_>>()
    };
    let gifts = gv::VoucherFilter {
        kind: Some(VoucherKind::Gift),
        ..gv::VoucherFilter::default()
    };
    assert_eq!(list(gifts, "2026-09-28"), [gift.code.as_str()]);
    let scanned = gv::VoucherFilter {
        text: gift.barcode.clone(),
        ..gv::VoucherFilter::default()
    };
    assert_eq!(list(scanned, "2026-09-28"), [gift.code.as_str()]);
    let expired = gv::VoucherFilter {
        status: Some(VoucherStatus::Expired),
        ..gv::VoucherFilter::default()
    };
    assert!(list(expired.clone(), "2026-09-28").is_empty());
    assert_eq!(
        list(expired, "2027-09-29"),
        [reward.code.clone(), gift.code.clone()],
        "past their validity, newest first"
    );
    let barcode = gift.barcode.clone();
    let info = shop
        .db
        .read(move |c| crate::vouchers::describe(c, &barcode))
        .unwrap()
        .unwrap();
    assert_eq!(
        (info.target, info.balance),
        (
            crate::vouchers::VoucherTarget::GiftVoucher {
                code: gift.code.clone()
            },
            Money(100_000)
        )
    );
    assert_eq!(
        shop.db
            .read(|c| crate::vouchers::describe(c, "BA2026-999999"))
            .unwrap(),
        None
    );
    let soap = shop.product("Savon", Money(1_000), &shop.tva19).id;
    let unknown = sell(
        &shop,
        &soap,
        1_000,
        None,
        vec![voucher_tender("BA2026-999999", 1_000)],
    );
    assert_eq!(
        unknown.unwrap_err().field(),
        Some("payments.0.reference"),
        "an unknown voucher is refused on its tender"
    );
}

/// The whole of `ticket` back, swapped for one `product` paid by the returned value and `payments`.
fn swap(
    shop: &Shop,
    ticket: &crate::documents::Document,
    product: &str,
    customer: &str,
    payments: Vec<PaymentInput>,
) -> crate::returns::Exchange {
    let ctx = shop.owner_till();
    let ret = ReturnInput {
        original_id: Some(ticket.id.clone()),
        lines: vec![back(&ticket.lines[0].id, 1_000)],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(1_000))]);
    cart.customer_id = Some(customer.to_owned());
    shop.db
        .write(move |tx| crate::returns::exchange(tx, &ctx, ret, SaleInput { cart, payments }))
        .unwrap()
}

#[test]
fn an_exchange_earns_points_only_on_the_money_paid_on_top() {
    let shop = club();
    let shirt = shop.product("Chemise", Money(250_000), &shop.tva19).id;
    let jacket = shop.product("Veste", Money(300_000), &shop.tva19).id;
    let amina = enrolled(&shop, "Amina");
    let sale = sell(&shop, &shirt, 1_000, Some(&amina), vec![cash(250_000)]).unwrap();
    assert_eq!(balance(&shop, &amina), 25);
    let like_for_like = swap(&shop, &sale.document, &shirt, &amina, vec![]);
    assert_eq!(
        balance(&shop, &amina),
        25,
        "a like-for-like exchange earns nothing: the store credit is a voucher"
    );
    let dearer = swap(
        &shop,
        &like_for_like.sale.document,
        &jacket,
        &amina,
        vec![cash(50_000)],
    );
    assert_eq!(
        balance(&shop, &amina),
        30,
        "only the 500 DA paid on top earns points"
    );
    let ctx = shop.owner_till();
    let ticket = dearer.sale.document.clone();
    let input = ReturnInput {
        original_id: Some(ticket.id.clone()),
        lines: vec![back(&ticket.lines[0].id, 1_000)],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    shop.db
        .write(move |tx| create_return_with(tx, &ctx, input, None, &ReturnExtras::default()))
        .unwrap();
    assert_eq!(
        balance(&shop, &amina),
        0,
        "the goods came back for cash: the points carried through both exchanges come back too"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn a_gift_voucher_is_never_spent_beyond_its_balance(attempts in proptest::collection::vec(1_i64..=60_000, 1..8)) {
        let shop = club();
        let v = sell_gift(&shop, 100_000);
        let key = shop.department_key("Divers");
        let mut left = 100_000_i64;
        for a in attempts {
            let ctx = shop.till();
            let mut line = CartLine::product(&key, Qty(1_000));
            line.unit_price = Some(Money(a));
            let input = SaleInput { cart: Cart::new(Lang::Fr, vec![line]), payments: vec![voucher_tender(&v.code, a)] };
            let result = shop.db.write(move |tx| create_sale_with(tx, &ctx, input, &SaleExtras::default()));
            if a <= left {
                prop_assert!(result.is_ok());
                left -= a;
            } else {
                prop_assert_eq!(result.unwrap_err().code(), "voucher_balance_insufficient");
            }
            let code = v.code.clone();
            let now = shop.db.read(move |c| gv::get(c, &code)).unwrap().balance;
            prop_assert_eq!(now, Money(left));
            prop_assert!(!now.is_negative());
        }
    }
}
