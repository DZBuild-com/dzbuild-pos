//! Sales and returns on a customer's account (CRD-001/002, PAY-004/011/013/017/020, TAX-007,
//! CRM-005, SAL-037, RET-007).

use super::cart::{Cart, CartLine, price_cart};
use super::sale::{SaleInput, ValidatedSale, create_sale};
use crate::auth::{OverrideGrant, Permission};
use crate::customers::{self, Customer, CustomerInput};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::reports::{self, ReportKind, ReportRequest};
use crate::returns::{self, ReturnInput, ReturnLineInput, ReturnReason};
use crate::table::Cell;
use crate::testkit::{NOON, Shop, shop};
use crate::till::TillContext;
use serde_json::json;
use std::sync::Arc;

fn oil(shop: &Shop) -> String {
    let p = shop.product("Huile Elio 5 L", Money(500_000), &shop.tva19);
    shop.stock_in(&p.id, 20_000);
    p.id
}

fn with_limit(shop: &Shop, name: &str, limit: Option<i64>) -> Customer {
    let mut input = CustomerInput::named(name);
    input.credit_limit = limit.map(Money);
    let pack = Arc::clone(&shop.pack);
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| customers::create(tx, &actor, &pack, input))
        .unwrap()
}

fn sell(
    shop: &Shop,
    ctx: TillContext,
    customer: Option<&str>,
    product: &str,
    tenders: Vec<PaymentInput>,
) -> Result<ValidatedSale, AppError> {
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(1_000))]);
    cart.customer_id = customer.map(str::to_owned);
    let input = SaleInput {
        cart,
        payments: tenders,
    };
    shop.db.write(move |tx| create_sale(tx, &ctx, input))
}

fn credit(amount: i64) -> PaymentInput {
    PaymentInput::electronic(PaymentMethod::Credit, Money(amount), None)
}

fn voucher(amount: i64) -> PaymentInput {
    PaymentInput::electronic(PaymentMethod::Voucher, Money(amount), None)
}

fn balance(shop: &Shop, id: &str) -> Money {
    let id = id.to_owned();
    shop.db
        .read(move |c| customers::get(c, &id))
        .unwrap()
        .balance
}

fn manager_approval(shop: &Shop) -> OverrideGrant {
    OverrideGrant {
        approver_id: shop.owner.user_id.clone(),
        approver_name: "Amine".into(),
        permission: Permission::CreditLimitOverride,
        granted_at: NOON,
        discount_limit_bp: 10_000,
    }
}

/// The whole first line of the ticket comes back, refunded with `method`.
fn return_all(sale: &ValidatedSale, method: PaymentMethod) -> ReturnInput {
    ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(sale.document.lines[0].id.clone()),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: true,
            reason: ReturnReason::ChangedMind,
            note: None,
        }],
        refund_method: method,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    }
}

#[test]
fn a_cash_and_credit_split_charges_duty_only_on_the_cash_part_and_books_the_rest() {
    let shop = shop();
    let product = oil(&shop);
    let karim = with_limit(&shop, "Karim", None);
    shop.open_till(0);
    // 5 000 DA: 3 000 DA on kridi, 2 000 DA in cash → 20 tranches × 1 DA = 20 DA of duty.
    let sale = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &product,
        vec![PaymentInput::cash(Money(202_000)), credit(300_000)],
    )
    .unwrap();
    let doc = &sale.document;
    assert_eq!(
        (doc.stamp_duty, doc.total_due, doc.credit_amount),
        (Money(2_000), Money(502_000), Money(300_000))
    );
    assert_eq!(doc.customer_id.as_deref(), Some(karim.id.as_str()));
    assert_eq!(sale.customer_balance, Some(Money(300_000)));
    assert_eq!(balance(&shop, &karim.id), Money(300_000));
    assert_eq!(
        shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'sale'"),
        202_000
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM customer_ledger WHERE kind = 'sale_credit' AND ref_type = 'document'"),
        1
    );
}

#[test]
fn credit_above_the_limit_needs_a_manager_approval() {
    let shop = shop();
    let product = oil(&shop);
    let karim = with_limit(&shop, "Karim", Some(100_000));
    shop.open_till(0);
    let err = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &product,
        vec![credit(500_000)],
    )
    .unwrap_err();
    assert_eq!(err.code(), "credit_limit_exceeded");
    assert_eq!(
        err.payload().details,
        Some(json!({ "customerId": karim.id, "balance": 0, "limit": 100_000, "after": 500_000 }))
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM documents"),
        0,
        "nothing was written"
    );
    let approved = sell(
        &shop,
        shop.till().with_approval(manager_approval(&shop)),
        Some(&karim.id),
        &product,
        vec![credit(500_000)],
    )
    .unwrap();
    assert_eq!(approved.customer_balance, Some(Money(500_000)));
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'sale.created' AND payload LIKE '%credit.limit_override%'"),
        1,
        "the approval is journaled with the sale"
    );
}

#[test]
fn store_credit_spent_with_kridi_on_one_ticket_counts_against_the_limit() {
    let shop = shop();
    let couscous = shop.product("Couscous 10 kg", Money(200_000), &shop.tva9);
    shop.stock_in(&couscous.id, 10_000);
    let karim = with_limit(&shop, "Karim", Some(100_000));
    let ctx = shop.owner_till();
    let id = karim.id.clone();
    shop.db
        .write(move |tx| {
            customers::credit::set_opening(tx, &ctx, &id, Money(-50_000), Some("avoir"))
        })
        .unwrap();
    shop.open_till(0);
    // 2 000 DA: the 500 DA of store credit first, the rest on kridi. The account ends at
    // -500 + 500 + 1 500 = 1 500 DA owed, above the 1 000 DA limit.
    let tenders = || vec![voucher(50_000), credit(150_000)];
    let err = sell(&shop, shop.till(), Some(&karim.id), &couscous.id, tenders()).unwrap_err();
    assert_eq!(err.code(), "credit_limit_exceeded");
    assert_eq!(
        err.payload().details,
        Some(
            json!({ "customerId": karim.id, "balance": -50_000, "limit": 100_000, "after": 150_000 })
        )
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM documents"),
        0,
        "nothing was written"
    );
    assert_eq!(balance(&shop, &karim.id), Money(-50_000));
    // More store credit than he has is refused as such, not as a limit to override.
    let err = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &couscous.id,
        vec![voucher(60_000), credit(140_000)],
    )
    .unwrap_err();
    assert_eq!(err.code(), "store_credit_insufficient");
    assert_eq!(err.payload().details, Some(json!({ "available": 50_000 })));
    let approved = sell(
        &shop,
        shop.till().with_approval(manager_approval(&shop)),
        Some(&karim.id),
        &couscous.id,
        tenders(),
    )
    .unwrap();
    assert_eq!(approved.customer_balance, Some(Money(150_000)));
    assert_eq!(balance(&shop, &karim.id), Money(150_000));
}

#[test]
fn a_blocked_customer_cannot_buy_on_credit_but_can_pay() {
    let shop = shop();
    let product = oil(&shop);
    let karim = with_limit(&shop, "Karim", None);
    let actor = shop.owner.actor();
    let id = karim.id.clone();
    shop.db
        .write(move |tx| customers::set_blocked(tx, &actor, &id, true))
        .unwrap();
    shop.open_till(0);
    let err = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &product,
        vec![credit(500_000)],
    )
    .unwrap_err();
    assert_eq!(err, AppError::conflict("customer_blocked"));
    sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &product,
        vec![PaymentInput::cash(Money(505_000))],
    )
    .unwrap();
}

#[test]
fn store_credit_is_spent_once() {
    let shop = shop();
    let small = shop.product("Sucre 1 kg", Money(40_000), &shop.tva9);
    shop.stock_in(&small.id, 10_000);
    let karim = with_limit(&shop, "Karim", None);
    let ctx = shop.owner_till();
    let id = karim.id.clone();
    shop.db
        .write(move |tx| {
            customers::credit::set_opening(tx, &ctx, &id, Money(-50_000), Some("avoir"))
        })
        .unwrap();
    shop.open_till(0);
    let first = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &small.id,
        vec![voucher(40_000)],
    )
    .unwrap();
    assert_eq!(
        first.document.stamp_duty,
        Money::ZERO,
        "store credit carries no duty"
    );
    assert_eq!(balance(&shop, &karim.id), Money(-10_000));
    let err = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &small.id,
        vec![voucher(40_000)],
    )
    .unwrap_err();
    assert_eq!(err.code(), "store_credit_insufficient");
    assert_eq!(err.payload().details, Some(json!({ "available": 10_000 })));
}

#[test]
fn account_tenders_need_a_customer_and_cheques_may_too() {
    let shop = shop();
    let product = oil(&shop);
    shop.open_till(0);
    let err = sell(&shop, shop.till(), None, &product, vec![credit(500_000)]).unwrap_err();
    assert_eq!(err, AppError::validation("customerId", "customer_required"));
    shop.set("payments.cheque_requires_customer", json!(true));
    let cheque = PaymentInput {
        method: PaymentMethod::Cheque,
        amount: Money(500_000),
        reference: Some("0012345".into()),
        cheque_bank: Some("CPA".into()),
        cheque_date: Some("2026-09-28".into()),
    };
    let err = sell(&shop, shop.till(), None, &product, vec![cheque]).unwrap_err();
    assert_eq!(err, AppError::validation("customerId", "customer_required"));
}

#[test]
fn a_change_above_the_shop_limit_is_refused() {
    let shop = shop();
    let small = shop.product("Pain", Money(1_500), &shop.exempt);
    shop.stock_in(&small.id, 10_000);
    shop.set("sales.max_change", json!(100_000));
    shop.open_till(0);
    let err = sell(
        &shop,
        shop.till(),
        None,
        &small.id,
        vec![PaymentInput::cash(Money(200_000))],
    )
    .unwrap_err();
    assert_eq!(err, AppError::validation("payments", "change_too_large"));
    sell(
        &shop,
        shop.till(),
        None,
        &small.id,
        vec![PaymentInput::cash(Money(50_000))],
    )
    .unwrap();
}

#[test]
fn the_cart_shows_the_customer_and_uses_his_price_level() {
    let shop = shop();
    let product = shop.product("Huile Elio 5 L", Money(500_000), &shop.tva19);
    let ctx = shop.owner_till();
    let pid = product.id.clone();
    shop.db
        .write(move |tx| {
            crate::catalog::products::set_price(
                tx,
                &ctx.actor(),
                &pid,
                crate::catalog::WHOLESALE_LEVEL_ID,
                Money(460_000),
            )
        })
        .unwrap();
    let mut input = CustomerInput::named("Superette Nour");
    input.price_level_id = Some(crate::catalog::WHOLESALE_LEVEL_ID.into());
    input.credit_limit = Some(Money(2_000_000));
    let pack = Arc::clone(&shop.pack);
    let actor = shop.owner.actor();
    let nour = shop
        .db
        .write(move |tx| customers::create(tx, &actor, &pack, input))
        .unwrap();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&product.id, Qty(1_000))]);
    cart.customer_id = Some(nour.id.clone());
    let ctx = shop.till();
    let priced = shop.db.read(move |c| price_cart(c, &ctx, &cart)).unwrap();
    assert_eq!(priced.lines[0].unit_price, Money(460_000));
    let c = priced.customer.unwrap();
    assert_eq!(
        (
            c.balance,
            c.credit_limit,
            c.available_credit,
            c.store_credit
        ),
        (
            Money::ZERO,
            Some(Money(2_000_000)),
            Some(Money(2_000_000)),
            Money::ZERO
        )
    );
}

#[test]
fn a_return_goes_back_to_the_customer_account() {
    let shop = shop();
    let product = oil(&shop);
    let karim = with_limit(&shop, "Karim", None);
    shop.open_till(0);
    let sale = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &product,
        vec![credit(500_000)],
    )
    .unwrap();
    let input = return_all(&sale, PaymentMethod::Credit);
    let ctx = shop.till();
    let back = shop
        .db
        .write(move |tx| returns::create_return(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        back.document.customer_id.as_deref(),
        Some(karim.id.as_str())
    );
    assert_eq!(balance(&shop, &karim.id), Money::ZERO);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM cash_movements WHERE kind = 'refund'"),
        0,
        "no cash left the drawer"
    );
}

/// M2 Task 29 (the month): a return refunded to the kridi carries what it takes off the account,
/// as the ticket carries what it put on it, so "on credit" in the sales reports nets the returns
/// like every other column; store credit given back is not kridi.
#[test]
fn a_return_to_the_kridi_takes_its_amount_off_the_credit_sales() {
    let shop = shop();
    let product = oil(&shop);
    let karim = with_limit(&shop, "Karim", None);
    shop.open_till(0);
    let on_credit = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &product,
        vec![credit(500_000)],
    )
    .unwrap();
    assert_eq!(on_credit.document.credit_amount, Money(500_000));
    let input = return_all(&on_credit, PaymentMethod::Credit);
    let ctx = shop.till();
    let back = shop
        .db
        .write(move |tx| returns::create_return(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        back.document.credit_amount,
        Money(-500_000),
        "the kridi given back"
    );
    let in_cash = sell(
        &shop,
        shop.till(),
        Some(&karim.id),
        &product,
        vec![PaymentInput::cash(Money(505_000))],
    )
    .unwrap();
    let input = return_all(&in_cash, PaymentMethod::Voucher);
    let ctx = shop.till();
    let voucher = shop
        .db
        .write(move |tx| returns::create_return(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        voucher.document.credit_amount,
        Money::ZERO,
        "store credit is not kridi"
    );
    let request = ReportRequest::new(ReportKind::SalesSummary, "2026-09-28", "2026-09-28");
    let summary = shop.db.read(move |c| reports::run(c, &request)).unwrap();
    let on_credit_column = summary.table.column("onCredit").unwrap();
    assert_eq!(
        summary.table.totals.unwrap()[on_credit_column],
        Cell::int(0),
        "sold on credit, then returned to it: nothing left on credit"
    );
}

#[test]
fn a_cash_ticket_can_be_returned_as_store_credit_for_a_customer_named_at_the_counter() {
    let shop = shop();
    let product = oil(&shop);
    let karim = with_limit(&shop, "Karim", None);
    shop.open_till(0);
    let sale = sell(
        &shop,
        shop.till(),
        None,
        &product,
        vec![PaymentInput::cash(Money(505_000))],
    )
    .unwrap();
    assert_eq!(sale.document.customer_id, None);
    let input = return_all(&sale, PaymentMethod::Voucher);
    let ctx = shop.till();
    let id = karim.id.clone();
    let back = shop
        .db
        .write(move |tx| returns::create_return_for(tx, &ctx, input, Some(&id)))
        .unwrap();
    assert_eq!(
        back.document.customer_id.as_deref(),
        Some(karim.id.as_str())
    );
    assert_eq!(back.refund, Money(500_000));
    assert_eq!(
        balance(&shop, &karim.id),
        Money(-500_000),
        "500 DA of store credit"
    );
    let id = karim.id.clone();
    let entries = shop
        .db
        .read(move |c| customers::credit::entries(c, &id, &Default::default()))
        .unwrap()
        .entries;
    assert_eq!(entries.len(), 1);
    let refund = &entries[0];
    assert_eq!(
        (
            refund.kind,
            refund.amount,
            refund.method,
            refund.ref_type.as_deref(),
            refund.ref_id.as_deref()
        ),
        (
            crate::ledger::EntryKind::Refund,
            Money(-500_000),
            Some(PaymentMethod::Voucher),
            Some("document"),
            Some(back.document.id.as_str())
        )
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM cash_movements WHERE kind = 'refund'"),
        0,
        "no cash left the drawer"
    );
}

#[test]
fn a_return_for_an_unknown_or_archived_customer_is_refused_and_writes_nothing() {
    let shop = shop();
    let product = oil(&shop);
    let gone = with_limit(&shop, "Ancien client", None);
    let actor = shop.owner.actor();
    let id = gone.id.clone();
    shop.db
        .write(move |tx| customers::archive(tx, &actor, &id))
        .unwrap();
    shop.open_till(0);
    let sale = sell(
        &shop,
        shop.till(),
        None,
        &product,
        vec![PaymentInput::cash(Money(505_000))],
    )
    .unwrap();
    let movements = shop.count("SELECT COUNT(*) FROM stock_movements");
    let unknown = "01920000-0000-7000-8000-000000000000".to_owned();
    for (customer, method) in [
        (unknown.clone(), PaymentMethod::Cash),
        (unknown, PaymentMethod::Voucher),
        (gone.id.clone(), PaymentMethod::Cash),
        (gone.id.clone(), PaymentMethod::Credit),
    ] {
        let input = return_all(&sale, method);
        let ctx = shop.till();
        let err = shop
            .db
            .write(move |tx| returns::create_return_for(tx, &ctx, input, Some(&customer)))
            .unwrap_err();
        assert_eq!(
            err,
            AppError::validation("customerId", "customer_not_found"),
            "{method:?}"
        );
    }
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM documents"),
        1,
        "only the ticket"
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM stock_movements"),
        movements
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM customer_ledger"), 0);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM cash_movements WHERE kind = 'refund'"),
        0
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'return.created'"),
        0
    );
}

use crate::catalog::breaks::{self, PriceBreakInput};
use crate::catalog::extras::{self as product_extras, ProductExtrasInput};
use crate::promotions::{self, Campaign, PromoKind, PromoScope, PromotionInput, TriggerMode};

fn price_one(
    shop: &Shop,
    ctx: TillContext,
    cart: Cart,
) -> Result<super::cart::PricedCart, AppError> {
    shop.db.read(move |c| price_cart(c, &ctx, &cart))
}

#[test]
fn a_promotion_discounts_the_line_and_is_recorded_with_the_sale() {
    let shop = shop();
    let yoghurt = shop.product("Yaourt Soummam", Money(3_000), &shop.tva9);
    shop.stock_in(&yoghurt.id, 50_000);
    let actor = shop.owner.actor();
    let input = PromotionInput {
        name_ar: String::new(),
        name_fr: "2 achetés, 1 offert".into(),
        name_en: String::new(),
        kind: PromoKind::BuyXGetY,
        campaign: Campaign::Promotion,
        trigger_mode: TriggerMode::Auto,
        scope: PromoScope::Products {
            ids: vec![yoghurt.id.clone()],
        },
        value: 0,
        buy_qty: Some(Qty(2_000)),
        get_qty: Some(Qty(1_000)),
        min_qty: Qty::ZERO,
        priority: 0,
        starts_on: None,
        ends_on: None,
        active: true,
    };
    let promo = shop
        .db
        .write(move |tx| promotions::create(tx, &actor, input))
        .unwrap();
    let cart = Cart::new(Lang::Fr, vec![CartLine::product(&yoghurt.id, Qty(6_000))]);
    let priced = price_one(&shop, shop.till(), cart.clone()).unwrap();
    let line = &priced.lines[0];
    assert_eq!(line.amounts.discount_amount, Money(6_000));
    assert_eq!(
        line.auto_discount.as_ref().map(|d| d.promotion_id.clone()),
        Some(Some(promo.id.clone()))
    );
    assert_eq!(
        priced.totals.total_ttc,
        Money(12_000),
        "a cashier without sales.discount gets the promotion"
    );
    shop.open_till(0);
    let ctx = shop.till();
    let input = SaleInput {
        cart,
        payments: vec![PaymentInput::cash(Money(12_000))],
    };
    let sale = shop
        .db
        .write(move |tx| create_sale(tx, &ctx, input))
        .unwrap();
    assert_eq!(
        shop.count("SELECT SUM(amount) FROM sale_line_promotions"),
        6_000
    );
    assert_eq!(sale.document.discount_total, Money(6_000));
    let doc_id = sale.document.id.clone();
    let (line_id, recorded): (String, Option<String>) = shop
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT line_id, promotion_id FROM sale_line_promotions WHERE document_id = ?1",
                [&doc_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(
        line_id, sale.document.lines[0].id,
        "stored on the ticket line"
    );
    assert_eq!(recorded.as_deref(), Some(promo.id.as_str()));
    let doc_id = sale.document.id.clone();
    let payload: serde_json::Value = shop
        .db
        .read(move |c| {
            let text: String = c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'sale.created' AND entity_id = ?1",
                [&doc_id],
                |r| r.get(0),
            )?;
            Ok(serde_json::from_str(&text)?)
        })
        .unwrap();
    assert_eq!(
        payload["discounts"][0],
        json!({
            "lineNo": 1,
            "lineId": line_id,
            "amount": 6_000,
            "source": "promotion",
            "promotionId": promo.id,
        }),
        "the applied promotion is journaled with the sale"
    );
}

#[test]
fn quantity_breaks_floor_prices_and_the_customer_discount() {
    let shop = shop();
    let water = shop.product("Eau Ifri 1,5 L", Money(5_000), &shop.tva9);
    let actor = shop.owner.actor();
    let id = water.id.clone();
    shop.db
        .write(move |tx| {
            breaks::replace(
                tx,
                &actor,
                &id,
                vec![PriceBreakInput {
                    price_level_id: crate::catalog::RETAIL_LEVEL_ID.into(),
                    min_qty: Qty(6_000),
                    price: Money(4_500),
                }],
            )
        })
        .unwrap();
    let pack = Cart::new(Lang::Fr, vec![CartLine::product(&water.id, Qty(6_000))]);
    assert_eq!(
        price_one(&shop, shop.till(), pack).unwrap().lines[0].unit_price,
        Money(4_500)
    );
    let actor = shop.owner.actor();
    let id = water.id.clone();
    shop.db
        .write(move |tx| {
            product_extras::set(
                tx,
                &actor,
                &id,
                ProductExtrasInput {
                    min_price: Some(Money(4_800)),
                    ..ProductExtrasInput::default()
                },
            )
        })
        .unwrap();
    let pack = Cart::new(Lang::Fr, vec![CartLine::product(&water.id, Qty(6_000))]);
    assert_eq!(
        price_one(&shop, shop.till(), pack.clone()).unwrap_err(),
        AppError::validation("lines.0.unitPrice", "price_below_floor"),
        "the floor holds on the catalogue's own break for a cashier (PRD-061)"
    );
    assert_eq!(
        price_one(&shop, shop.owner_till(), pack).unwrap().lines[0].unit_price,
        Money(4_500),
        "the owner sets prices and sells at the break"
    );
    let mut cheap = CartLine::product(&water.id, Qty(1_000));
    cheap.unit_price = Some(Money(4_000));
    let manager = shop.owner_till();
    let err = price_one(
        &shop,
        shop.till().with_approval(OverrideGrant {
            approver_id: "nobody".into(),
            approver_name: "x".into(),
            permission: Permission::SalesDiscount,
            granted_at: NOON,
            discount_limit_bp: 10_000,
        }),
        Cart::new(Lang::Fr, vec![cheap.clone()]),
    )
    .unwrap_err();
    assert_eq!(
        err.code(),
        "price_override_not_allowed",
        "a cashier cannot change prices at all"
    );
    assert!(
        price_one(&shop, manager, Cart::new(Lang::Fr, vec![cheap.clone()])).is_ok(),
        "the owner may sell below the floor"
    );
    let granted = shop.till().with_approval(OverrideGrant {
        approver_id: shop.owner.user_id.clone(),
        approver_name: "Amine".into(),
        permission: Permission::SalesPriceOverride,
        granted_at: NOON,
        discount_limit_bp: 10_000,
    });
    assert!(
        !price_one(&shop, granted, Cart::new(Lang::Fr, vec![cheap]))
            .unwrap()
            .lines[0]
            .below_cost,
        "a manager's approval passes the floor"
    );
    let fidele = {
        let mut input = CustomerInput::named("Client fidèle");
        input.discount_bp = crate::money::Rate(500);
        let pack = Arc::clone(&shop.pack);
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| customers::create(tx, &actor, &pack, input))
            .unwrap()
    };
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&water.id, Qty(2_000))]);
    cart.customer_id = Some(fidele.id);
    let line = price_one(&shop, shop.till(), cart).unwrap().lines.remove(0);
    assert_eq!(
        (
            line.amounts.discount_amount,
            line.auto_discount.map(|d| d.source)
        ),
        (Money(500), Some(promotions::DiscountSource::Customer))
    );
}

/// A 48 DA minimum price on `product_id` (PRD-061).
fn floor_48(shop: &Shop, product_id: &str) {
    let actor = shop.owner.actor();
    let id = product_id.to_owned();
    shop.db
        .write(move |tx| {
            product_extras::set(
                tx,
                &actor,
                &id,
                ProductExtrasInput {
                    min_price: Some(Money(4_800)),
                    ..ProductExtrasInput::default()
                },
            )
        })
        .unwrap();
}

fn price_override_approval(shop: &Shop) -> OverrideGrant {
    OverrideGrant {
        approver_id: shop.owner.user_id.clone(),
        approver_name: "Amine".into(),
        permission: Permission::SalesPriceOverride,
        granted_at: NOON,
        discount_limit_bp: 10_000,
    }
}

#[test]
fn a_price_typed_below_the_floor_needs_products_price_or_an_approval() {
    let shop = shop();
    let water = shop.product("Eau Ifri 1,5 L", Money(5_000), &shop.tva9);
    floor_48(&shop, &water.id);
    let mut supervisor = shop.till();
    supervisor
        .session
        .permissions
        .push(Permission::SalesPriceOverride);
    assert!(
        !supervisor.session.has(Permission::ProductsPrice),
        "may change a price on the ticket, not set the catalogue's prices"
    );
    let typed = |price: i64| {
        let mut line = CartLine::product(&water.id, Qty(1_000));
        line.unit_price = Some(Money(price));
        Cart::new(Lang::Fr, vec![line])
    };
    assert_eq!(
        price_one(&shop, supervisor.clone(), typed(4_000)).unwrap_err(),
        AppError::validation("lines.0.unitPrice", "price_below_floor"),
        "40 DA under a 48 DA floor"
    );
    for ok in [4_900, 4_800] {
        let line = price_one(&shop, supervisor.clone(), typed(ok))
            .unwrap()
            .lines
            .remove(0);
        assert_eq!(
            (line.unit_price, line.price_overridden),
            (Money(ok), true),
            "{ok} centimes is not under the floor"
        );
    }
    let approved = supervisor.with_approval(price_override_approval(&shop));
    assert_eq!(
        price_one(&shop, approved, typed(4_000)).unwrap().lines[0].unit_price,
        Money(4_000),
        "a manager's approval of sales.price_override passes the floor"
    );
}

#[test]
fn a_level_picked_on_the_cart_does_not_sell_under_the_floor() {
    use crate::catalog::products::{LevelPrice, ProductInput};
    use crate::catalog::{UNIT_PIECE_ID, WHOLESALE_LEVEL_ID};
    let shop = shop();
    let water = shop.product_with(ProductInput {
        prices: vec![LevelPrice {
            price_level_id: WHOLESALE_LEVEL_ID.into(),
            price: Money(4_000),
        }],
        ..ProductInput::simple("Eau Ifri 1,5 L", UNIT_PIECE_ID, &shop.tva9, Money(5_000))
    });
    floor_48(&shop, &water.id);
    let mut gros = Cart::new(Lang::Fr, vec![CartLine::product(&water.id, Qty(1_000))]);
    gros.price_level_id = Some(WHOLESALE_LEVEL_ID.into());
    assert_eq!(
        price_one(&shop, shop.till(), gros.clone()).unwrap_err(),
        AppError::validation("lines.0.unitPrice", "price_below_floor"),
        "a cashier picking Gros at 40 DA cannot pass a 48 DA floor"
    );
    let approved = shop.till().with_approval(price_override_approval(&shop));
    assert_eq!(
        price_one(&shop, approved, gros.clone()).unwrap().lines[0].unit_price,
        Money(4_000),
        "a manager's approval passes"
    );
    assert_eq!(
        price_one(&shop, shop.owner_till(), gros).unwrap().lines[0].unit_price,
        Money(4_000),
        "the owner sets prices"
    );
    let retail = Cart::new(Lang::Fr, vec![CartLine::product(&water.id, Qty(1_000))]);
    assert_eq!(
        price_one(&shop, shop.till(), retail).unwrap().lines[0].unit_price,
        Money(5_000),
        "the retail price is above the floor"
    );
}

#[test]
fn a_whole_order_discount_in_dinars_is_spread_over_the_lines() {
    let shop = shop();
    let a = shop.product("Café", Money(31_000), &shop.tva19);
    let b = shop.product("Sucre", Money(9_000), &shop.tva9);
    let cart = Cart::new(
        Lang::Fr,
        vec![
            CartLine::product(&a.id, Qty(1_000)),
            CartLine::product(&b.id, Qty(1_000)),
        ],
    );
    let ctx = shop.owner_till();
    let spread = shop
        .db
        .read(move |c| super::cart::spread_discount(c, &ctx, &cart, Money(4_000)))
        .unwrap();
    use crate::tax::tva::Discount;
    assert_eq!(
        spread.lines.iter().map(|l| l.discount).collect::<Vec<_>>(),
        vec![Discount::Amount(Money(3_100)), Discount::Amount(Money(900))]
    );
}

#[test]
fn valuing_a_cart_ignores_the_floor_and_a_customer_archived_since() {
    // The journal of a void values the cart as it stands (AUD-001): a price below the floor
    // or a customer archived meanwhile stop a sale, never the valuation.
    let shop = shop();
    let water = shop.product("Eau Ifri 1,5 L", Money(5_000), &shop.tva9);
    floor_48(&shop, &water.id);
    let mut cheap = CartLine::product(&water.id, Qty(1_000));
    cheap.unit_price = Some(Money(4_000));
    let cart = Cart::new(Lang::Fr, vec![cheap]);
    let value = |cart: Cart| {
        let ctx = shop.till();
        shop.db
            .read(move |c| super::cart::value_cart(c, &ctx, &cart))
    };
    assert_eq!(
        value(cart.clone()).unwrap().totals.total_ttc,
        Money(4_000),
        "below the floor"
    );
    let karim = shop.customer("Karim");
    let (actor, id) = (shop.owner.actor(), karim.id.clone());
    shop.db
        .write(move |tx| customers::archive(tx, &actor, &id))
        .unwrap();
    let with_karim = Cart {
        customer_id: Some(karim.id.clone()),
        ..cart
    };
    let (ctx, sold) = (shop.owner_till(), with_karim.clone());
    assert_eq!(
        shop.db
            .read(move |c| price_cart(c, &ctx, &sold))
            .unwrap_err(),
        AppError::validation("customerId", "customer_not_found"),
        "a sale refuses the archived customer"
    );
    let valued = value(with_karim).unwrap();
    assert_eq!(valued.totals.total_ttc, Money(4_000));
    assert_eq!(valued.customer.map(|c| c.id), Some(karim.id));
}
