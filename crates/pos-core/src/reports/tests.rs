//! Reports agree with the books: the Z of the day, the payments, the P&L (Review Focus 5).

use super::*;
use crate::cash::ExpenseCategory;
use crate::cash::closing::x_report;
use crate::catalog::UNIT_PIECE_ID;
use crate::catalog::products::ProductInput;
use crate::expenses::{self, NewExpense};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::returns::{self, ReturnInput, ReturnLineInput, ReturnReason};
use crate::sales::cart::{Cart, CartLine};
use crate::sales::sale::{SaleInput, ValidatedSale, create_sale};
use crate::table::Cell;
use crate::testkit::{Shop, shop};
use serde_json::json;

const DAY: &str = "2026-09-28";
const ELECTRICITY: &str = "00000000-0000-7000-8000-000000000502";

/// Huile 5 L: 1 190 DA TTC at 19 % (1 000 DA HT), cost 800 DA. Sucre 1 kg: 130 DA TTC at 9 %,
/// cost 100 DA. Both stocked. The shop absorbs the droit de timbre, so the tenders equal the
/// TTC (the duty is recorded on each ticket and comes back in the P&L).
fn goods(shop: &Shop) -> (String, String) {
    shop.set("stamp_duty.mode", json!("absorb"));
    let mut oil = ProductInput::simple("Huile 5 L", UNIT_PIECE_ID, &shop.tva19, Money(119_000));
    oil.cost_price = Money(80_000);
    let mut sugar = ProductInput::simple("Sucre 1 kg", UNIT_PIECE_ID, &shop.tva9, Money(13_000));
    sugar.cost_price = Money(10_000);
    let (oil, sugar) = (shop.product_with(oil), shop.product_with(sugar));
    shop.stock_in(&oil.id, 50_000);
    shop.stock_in(&sugar.id, 50_000);
    (oil.id, sugar.id)
}

fn sell(shop: &Shop, product: &str, qty: i64, tenders: Vec<PaymentInput>) -> ValidatedSale {
    let cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(qty))]);
    let ctx = shop.till();
    shop.db
        .write(move |tx| {
            create_sale(
                tx,
                &ctx,
                SaleInput {
                    cart,
                    payments: tenders,
                },
            )
        })
        .unwrap()
}

fn give_back_one(shop: &Shop, sale: &ValidatedSale) {
    let input = ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(sale.document.lines[0].id.clone()),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: true,
            reason: ReturnReason::Defective,
            note: None,
        }],
        refund_method: PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    let ctx = shop.owner_till();
    shop.db
        .write(move |tx| returns::create_return(tx, &ctx, input))
        .unwrap();
}

fn report(shop: &Shop, req: ReportRequest) -> Report {
    shop.db.read(move |c| run(c, &req)).unwrap()
}

fn column(report: &Report, key: &str) -> Vec<Cell> {
    let i = report.table.column(key).unwrap();
    report.table.rows.iter().map(|r| r[i].clone()).collect()
}

fn total(report: &Report, key: &str) -> i64 {
    let i = report.table.column(key).unwrap();
    report.table.totals.as_ref().unwrap()[i].as_i64().unwrap()
}

#[test]
fn the_daily_summary_equals_the_z_reports_of_the_day() {
    let shop = shop();
    let (oil, sugar) = goods(&shop);
    let session = shop.open_till(100_000);
    let a = sell(&shop, &oil, 2_000, vec![PaymentInput::cash(Money(238_000))]);
    sell(
        &shop,
        &sugar,
        1_000,
        vec![PaymentInput::electronic(
            PaymentMethod::CardCib,
            Money(13_000),
            None,
        )],
    );
    give_back_one(&shop, &a);
    let id = session.id.clone();
    let x = shop.db.read(move |c| x_report(c, &id, true)).unwrap();

    let summary = report(
        &shop,
        ReportRequest::new(ReportKind::SalesSummary, DAY, DAY),
    );
    assert_eq!(column(&summary, "period"), vec![Cell::text(DAY)]);
    assert_eq!(total(&summary, "netTtc"), x.net_ttc.0);
    assert_eq!(total(&summary, "salesTtc"), x.sales_ttc.0);
    assert_eq!(total(&summary, "returnsTtc"), x.returns_ttc.0);
    assert_eq!(
        (total(&summary, "tickets"), total(&summary, "returns")),
        (x.tickets, x.returns)
    );
    assert_eq!(total(&summary, "netTtc"), 238_000 + 13_000 - 119_000);
    assert_eq!(
        summary.chart,
        Some(Chart {
            kind: ChartKind::Bar,
            label: "period".into(),
            value: "netTtc".into()
        })
    );

    let methods = report(
        &shop,
        ReportRequest::new(ReportKind::SalesByMethod, DAY, DAY),
    );
    let net_of = |method: &str| -> i64 {
        let i = methods.table.column("method").unwrap();
        let n = methods.table.column("net").unwrap();
        methods
            .table
            .rows
            .iter()
            .find(|r| r[i] == Cell::text(method))
            .and_then(|r| r[n].as_i64())
            .unwrap_or(0)
    };
    let x_net = |m: PaymentMethod| {
        x.by_method
            .iter()
            .find(|t| t.method == m)
            .map_or(0, |t| t.net.0)
    };
    assert_eq!(
        net_of("cash"),
        x_net(PaymentMethod::Cash) + x_net(PaymentMethod::Change)
    );
    assert_eq!(net_of("card_cib"), x_net(PaymentMethod::CardCib));
    assert_eq!(
        methods.table.money_total("net").0,
        x.net_ttc.0,
        "payments add up to the net sales"
    );
}

#[test]
fn method_totals_net_the_change_into_cash() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    shop.open_till(0);
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(200_000))]);
    let methods = report(
        &shop,
        ReportRequest::new(ReportKind::SalesByMethod, DAY, DAY),
    );
    assert_eq!(column(&methods, "method"), vec![Cell::text("cash")]);
    assert_eq!(column(&methods, "received"), vec![Cell::int(200_000)]);
    assert_eq!(column(&methods, "paidBack"), vec![Cell::int(-81_000)]);
    assert_eq!(column(&methods, "net"), vec![Cell::int(119_000)]);
}

#[test]
fn sales_by_product_nets_the_returns_and_gives_the_margin_on_the_ht() {
    let shop = shop();
    let (oil, sugar) = goods(&shop);
    shop.open_till(100_000);
    let a = sell(&shop, &oil, 2_000, vec![PaymentInput::cash(Money(238_000))]);
    sell(
        &shop,
        &sugar,
        3_000,
        vec![PaymentInput::cash(Money(39_000))],
    );
    give_back_one(&shop, &a);
    let by_product = report(
        &shop,
        ReportRequest::new(ReportKind::SalesByProduct, DAY, DAY),
    );
    assert_eq!(
        column(&by_product, "name"),
        vec![Cell::text("Huile 5 L"), Cell::text("Sucre 1 kg")]
    );
    assert_eq!(
        column(&by_product, "qty"),
        vec![Cell::int(1_000), Cell::int(3_000)]
    );
    assert_eq!(
        column(&by_product, "netTtc"),
        vec![Cell::int(119_000), Cell::int(39_000)]
    );
    // Oil: 1 000 DA HT − 800 DA; sugar: 390 DA TTC is 357.80 DA HT at 9 %, − 300 DA.
    assert_eq!(
        column(&by_product, "cost"),
        vec![Cell::int(80_000), Cell::int(30_000)]
    );
    assert_eq!(
        column(&by_product, "margin"),
        vec![Cell::int(20_000), Cell::int(5_780)]
    );
    assert_eq!(
        column(&by_product, "marginRate"),
        vec![Cell::int(2_000), Cell::int(1_615)]
    );
    assert_eq!(total(&by_product, "netTtc"), 158_000);

    let hidden = by_product.hide_costs();
    for key in COST_COLUMNS {
        assert!(hidden.table.column(key).is_none(), "{key} is hidden");
    }
    assert!(
        hidden
            .table
            .rows
            .iter()
            .all(|r| r.len() == hidden.table.columns.len())
    );
    assert_eq!(
        hidden.table.totals.as_ref().map(Vec::len),
        Some(hidden.table.columns.len())
    );
    let by_period = report(
        &shop,
        ReportRequest::new(ReportKind::ProfitByPeriod, DAY, DAY),
    );
    assert!(by_period.chart.is_some());
    assert_eq!(
        by_period.hide_costs().chart,
        None,
        "a margin chart goes with the margin"
    );
}

#[test]
fn the_pnl_is_sales_minus_cost_of_goods_minus_expenses() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    shop.open_till(100_000);
    sell(&shop, &oil, 2_000, vec![PaymentInput::cash(Money(238_000))]);
    // An M1-style till expense (no `expenses` row) and a back-office bank expense.
    let ctx = shop.till();
    shop.db
        .write(move |tx| {
            crate::cash::expense(
                tx,
                &ctx,
                ExpenseCategory::Transport,
                Money(5_000),
                Some("taxi"),
            )
        })
        .unwrap();
    let ctx = shop.owner_till();
    let bill = NewExpense {
        category_id: ELECTRICITY.into(),
        amount: Money(15_000),
        method: PaymentMethod::Transfer,
        from_till: false,
        reference: Some("CCP 7894561".into()),
        supplier_id: None,
        business_date: None,
        note: None,
    };
    shop.db
        .write(move |tx| expenses::record(tx, &ctx, bill))
        .unwrap();

    let pnl = report(
        &shop,
        ReportRequest::new(ReportKind::ProfitAndLoss, "2026-09-01", "2026-09-30"),
    );
    let lines: Vec<(String, i64)> = pnl
        .table
        .rows
        .iter()
        .map(|r| (r[0].as_str().unwrap().to_owned(), r[2].as_i64().unwrap()))
        .collect();
    assert_eq!(
        lines,
        vec![
            ("revenue".to_owned(), 200_000),
            ("cost_of_goods".to_owned(), -160_000),
            ("gross_margin".to_owned(), 40_000),
            ("stock_losses".to_owned(), 0),
            ("credit_adjustments".to_owned(), 0),
            // 2 380 DA in cash: 24 tranches of 100 DA at 1 DA, paid by the shop.
            ("stamp_absorbed".to_owned(), -2_400),
            ("expense".to_owned(), -15_000),
            ("expense".to_owned(), -5_000),
            ("expenses_total".to_owned(), -20_000),
            ("net_profit".to_owned(), 17_600),
        ]
    );
    assert_eq!(column(&pnl, "label")[6], Cell::text("Électricité et gaz"));
    assert_eq!(column(&pnl, "label")[7], Cell::text("Transport"));
}

/// The P&L of September 2026 as `(line, label, amount)`.
fn pnl_of_september(shop: &Shop) -> Vec<(String, Option<String>, i64)> {
    let pnl = report(
        shop,
        ReportRequest::new(ReportKind::ProfitAndLoss, "2026-09-01", "2026-09-30"),
    );
    pnl.table
        .rows
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap().to_owned(),
                r[1].as_str().map(str::to_owned),
                r[2].as_i64().unwrap(),
            )
        })
        .collect()
}

fn pnl_amount(lines: &[(String, Option<String>, i64)], code: &str) -> i64 {
    lines.iter().find(|(line, _, _)| line == code).unwrap().2
}

#[test]
fn a_till_expense_stays_in_the_pnl_by_its_cash_group_whatever_the_category_code() {
    const TRANSPORT: &str = "00000000-0000-7000-8000-000000000505";
    let shop = shop();
    shop.open_till(100_000);
    // An M1/M1b till expense (a bare cash movement of the `transport` group, no `expenses` row)
    // and a back-office expense booked on the same category.
    let ctx = shop.till();
    shop.db
        .write(move |tx| {
            crate::cash::expense(
                tx,
                &ctx,
                ExpenseCategory::Transport,
                Money(5_000),
                Some("taxi"),
            )
        })
        .unwrap();
    let ctx = shop.owner_till();
    let fuel = NewExpense {
        category_id: TRANSPORT.into(),
        amount: Money(7_000),
        method: PaymentMethod::Transfer,
        from_till: false,
        reference: Some("CCP 1234".into()),
        supplier_id: None,
        business_date: None,
        note: None,
    };
    shop.db
        .write(move |tx| expenses::record(tx, &ctx, fuel))
        .unwrap();
    let before = pnl_of_september(&shop);
    assert_eq!(pnl_amount(&before, "expenses_total"), -12_000);

    // The owner renames the seeded `transport` code: both expenses stay on its one line.
    let rename = |cash_group: ExpenseCategory| expenses::ExpenseCategoryInput {
        code: "taxi".into(),
        name_ar: "طاكسي".into(),
        name_fr: "Taxi".into(),
        name_en: "Taxi".into(),
        cash_group,
        sort_order: 5,
    };
    let (actor, input) = (shop.owner.actor(), rename(ExpenseCategory::Transport));
    shop.db
        .write(move |tx| expenses::update_category(tx, &actor, TRANSPORT, input))
        .unwrap();
    let renamed = pnl_of_september(&shop);
    assert_eq!(
        pnl_amount(&renamed, "expenses_total"),
        -12_000,
        "the till expense is not dropped"
    );
    let spent: Vec<_> = renamed
        .iter()
        .filter(|(line, _, _)| line == "expense")
        .cloned()
        .collect();
    assert_eq!(
        spent,
        vec![("expense".to_owned(), Some("Taxi".to_owned()), -12_000)]
    );
    assert_eq!(pnl_amount(&renamed, "net_profit"), -12_000);

    // No category left in the `transport` group: the till expense still counts, under its group.
    let (actor, input) = (shop.owner.actor(), rename(ExpenseCategory::Other));
    shop.db
        .write(move |tx| expenses::update_category(tx, &actor, TRANSPORT, input))
        .unwrap();
    let orphan = pnl_of_september(&shop);
    assert_eq!(pnl_amount(&orphan, "expenses_total"), -12_000);
    let mut spent: Vec<_> = orphan
        .iter()
        .filter(|(line, _, _)| line == "expense")
        .cloned()
        .collect();
    spent.sort();
    assert_eq!(
        spent,
        vec![
            ("expense".to_owned(), Some("Taxi".to_owned()), -7_000),
            ("expense".to_owned(), Some("transport".to_owned()), -5_000),
        ]
    );
}

#[test]
fn an_absorbed_versement_duty_is_a_cost_in_the_pnl() {
    use crate::customers::credit::{CollectInput, collect, set_opening};
    let shop = shop();
    shop.set("stamp_duty.mode", json!("absorb"));
    let karim = shop.customer("Karim");
    let ctx = shop.owner_till();
    let id = karim.id.clone();
    shop.db
        .write(move |tx| set_opening(tx, &ctx, &id, Money(5_000_000), None))
        .unwrap();
    shop.open_till(0);
    let versement = |amount: i64| CollectInput {
        customer_id: karim.id.clone(),
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    };
    // 10 000 DA in cash, absorbed: 100 tranches of 100 DA at 1 DA, recorded and paid by the shop.
    let (ctx, input) = (shop.till(), versement(1_000_000));
    let done = shop.db.write(move |tx| collect(tx, &ctx, input)).unwrap();
    assert_eq!(
        (done.entry.stamp_duty, done.entry.stamp_charged),
        (Money(10_000), Money::ZERO)
    );
    let lines = pnl_of_september(&shop);
    assert_eq!(pnl_amount(&lines, "stamp_absorbed"), -10_000);
    assert_eq!(pnl_amount(&lines, "net_profit"), -10_000);

    // A versement whose duty the customer pays costs the shop nothing.
    shop.set("stamp_duty.mode", json!("charge_customer"));
    let (ctx, input) = (shop.till(), versement(2_000_000));
    let done = shop.db.write(move |tx| collect(tx, &ctx, input)).unwrap();
    assert_eq!(done.entry.stamp_charged, done.entry.stamp_duty);
    assert!(done.entry.stamp_duty.is_positive());
    let lines = pnl_of_september(&shop);
    assert_eq!(pnl_amount(&lines, "stamp_absorbed"), -10_000);
    assert_eq!(pnl_amount(&lines, "net_profit"), -10_000);
}

#[test]
fn a_range_is_a_valid_span_of_at_most_five_years() {
    let shop = shop();
    let err = |from: &'static str, to: &'static str| {
        shop.db
            .read(move |c| run(c, &ReportRequest::new(ReportKind::SalesSummary, from, to)))
            .unwrap_err()
    };
    assert_eq!(
        err("2026-09-30", "2026-09-01"),
        AppError::validation("to", "report_range_invalid")
    );
    assert_eq!(
        err("2020-01-01", "2026-09-30"),
        AppError::validation("to", "report_range_invalid")
    );
    assert_eq!(
        err("2026-02-30", "2026-03-01"),
        AppError::validation("from", "date_invalid")
    );
    assert!(
        shop.db
            .read(|c| run(
                c,
                &ReportRequest::new(ReportKind::SalesSummary, "2021-10-01", "2026-09-30")
            ))
            .is_ok()
    );
}

#[test]
fn a_period_is_compared_with_the_same_length_just_before() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    shop.open_till(0);
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);
    let cmp = report(
        &shop,
        ReportRequest::new(ReportKind::PeriodComparison, "2026-09-22", "2026-09-28"),
    );
    let i = cmp.table.column("metric").unwrap();
    let row = cmp
        .table
        .rows
        .iter()
        .find(|r| r[i] == Cell::text("net_sales"))
        .unwrap()
        .clone();
    let at = |key: &str| row[cmp.table.column(key).unwrap()].clone();
    assert_eq!(
        (at("current"), at("previous"), at("change")),
        (Cell::int(119_000), Cell::int(0), Cell::int(119_000))
    );
    assert_eq!(at("changeRate"), Cell::NULL, "no rate against nothing");
    assert_eq!(cmp.table.rows.len(), 7);
}

#[test]
fn a_line_journal_says_when_it_was_cut() {
    let shop = shop();
    let (oil, sugar) = goods(&shop);
    shop.open_till(0);
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);
    sell(
        &shop,
        &sugar,
        1_000,
        vec![PaymentInput::cash(Money(13_000))],
    );
    let mut req = ReportRequest::new(ReportKind::SalesLines, DAY, DAY);
    req.limit = Some(1);
    let cut = report(&shop, req);
    assert!(cut.truncated);
    assert_eq!(cut.table.rows.len(), 1);
    let whole = report(&shop, ReportRequest::new(ReportKind::SalesLines, DAY, DAY));
    assert!(!whole.truncated);
    assert_eq!(
        column(&whole, "docType"),
        vec![Cell::text("ticket"), Cell::text("ticket")]
    );
}

#[test]
fn the_dashboard_shows_today_and_hides_what_the_viewer_may_not_see() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    shop.open_till(0);
    sell(&shop, &oil, 2_000, vec![PaymentInput::cash(Money(238_000))]);
    let blind = DashboardView {
        costs: false,
        credit: false,
        purchases: false,
        stock: false,
    };
    let d = shop
        .db
        .read(move |c| dashboard(c, DAY, Lang::Fr, blind))
        .unwrap();
    assert_eq!(
        (d.net_ttc, d.tickets, d.average_ticket),
        (Money(238_000), 1, Money(238_000))
    );
    assert_eq!(
        (d.margin, d.receivables, d.payables, d.low_stock, d.expiring),
        (None, None, None, None, None)
    );
    assert_eq!(d.open_sessions, 1);
    assert_eq!(d.last_days.len(), 14);
    assert_eq!(d.last_days[0].day, "2026-09-15");
    assert_eq!(
        d.last_days[13],
        DayTotal {
            day: DAY.into(),
            net_ttc: Money(238_000)
        }
    );
    assert_eq!(d.top_products[0].name, "Huile 5 L");
    assert_eq!(
        d.by_method,
        vec![MethodNet {
            method: PaymentMethod::Cash,
            net: Money(238_000)
        }]
    );
    let all = DashboardView {
        costs: true,
        credit: true,
        purchases: true,
        stock: true,
    };
    let d = shop
        .db
        .read(move |c| dashboard(c, DAY, Lang::Fr, all))
        .unwrap();
    assert_eq!(d.margin, Some(Money(40_000)));
    assert_eq!(
        (d.receivables, d.payables, d.low_stock, d.expiring),
        (Some(Money::ZERO), Some(Money::ZERO), Some(0), Some(0))
    );
}

#[test]
fn every_kind_runs_on_an_empty_shop_and_is_described() {
    let shop = shop();
    let actor = shop.owner.actor();
    let promo = shop
        .db
        .write(move |tx| crate::promotions::create(tx, &actor, solde_input(PromoScope::All)))
        .unwrap();
    let ctx = shop.owner_till();
    let count = shop
        .db
        .write(move |tx| crate::stock::counts::start(tx, &ctx, full_count()))
        .unwrap();
    // The pack kinds run while their module is on.
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| crate::packs::set_modules(tx, &actor, &crate::packs::Module::ALL))
        .unwrap();
    for kind in ReportKind::ALL {
        let mut req = ReportRequest::new(kind, "2026-09-01", "2026-09-30");
        req.promotion_id = Some(promo.id.clone());
        req.count_id = Some(count.id.clone());
        let r = shop.db.read(move |c| run(c, &req));
        let r = r.unwrap_or_else(|e| panic!("{} fails: {e:?}", kind.as_str()));
        assert!(!r.table.columns.is_empty(), "{}", kind.as_str());
        assert!(
            r.table
                .rows
                .iter()
                .all(|row| row.len() == r.table.columns.len()),
            "{}",
            kind.as_str()
        );
        assert!(
            r.table
                .columns
                .iter()
                .all(|c| (c.kind == ColumnKind::Tag) == c.tag.is_some()),
            "{}: tag columns name their domain",
            kind.as_str()
        );
        if let Some(chart) = &r.chart {
            assert!(
                r.table.column(&chart.label).is_some() && r.table.column(&chart.value).is_some(),
                "{}",
                kind.as_str()
            );
        }
        assert_eq!(
            ReportKind::ALL
                .iter()
                .filter(|k| k.as_str() == kind.as_str())
                .count(),
            1
        );
    }
}

use crate::customers::{self, credit::CollectInput};
use crate::promotions::{Campaign, PromoKind, PromoScope, PromotionInput, TriggerMode};
use crate::stock::SHOP_LOCATION_ID;
use crate::stock::counts::{self, CountInput, CountMode, CountScope};
use crate::stock::minimums::{self, LocationMinimum};
use crate::suppliers;

fn solde_input(scope: PromoScope) -> PromotionInput {
    PromotionInput {
        name_ar: "تخفيضات الشتاء".into(),
        name_fr: "Soldes d'hiver".into(),
        name_en: "Winter sales".into(),
        kind: PromoKind::Percent,
        campaign: Campaign::Solde,
        trigger_mode: TriggerMode::Auto,
        scope,
        value: 3_000,
        buy_qty: None,
        get_qty: None,
        min_qty: Qty::ZERO,
        priority: 0,
        starts_on: Some(DAY.into()),
        ends_on: Some("2026-10-10".into()),
        active: true,
    }
}

fn full_count() -> CountInput {
    CountInput {
        location_id: None,
        scope: CountScope::Full,
        category_id: None,
        product_ids: Vec::new(),
        blind: false,
        note: None,
    }
}

fn opening(shop: &Shop, customer_id: &str, amount: i64) {
    let ctx = shop.owner_till();
    let id = customer_id.to_owned();
    shop.db
        .write(move |tx| customers::credit::set_opening(tx, &ctx, &id, Money(amount), None))
        .unwrap();
}

#[test]
fn stock_levels_and_valuation_agree_with_the_ledger() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    let levels = report(&shop, ReportRequest::new(ReportKind::StockLevels, DAY, DAY));
    assert_eq!(
        column(&levels, "name"),
        vec![Cell::text("Huile 5 L"), Cell::text("Sucre 1 kg")]
    );
    assert_eq!(
        column(&levels, "location"),
        vec![Cell::text("Magasin"), Cell::text("Magasin")]
    );
    assert_eq!(
        column(&levels, "qty"),
        vec![Cell::int(50_000), Cell::int(50_000)]
    );
    assert_eq!(
        column(&levels, "costValue"),
        vec![Cell::int(4_000_000), Cell::int(500_000)]
    );
    assert_eq!(
        column(&levels, "retailValue"),
        vec![Cell::int(5_950_000), Cell::int(650_000)]
    );
    let valuation = report(
        &shop,
        ReportRequest::new(ReportKind::StockValuation, DAY, DAY),
    );
    assert_eq!(
        (
            total(&valuation, "costValue"),
            total(&valuation, "retailValue")
        ),
        (4_500_000, 6_600_000)
    );
    assert_eq!(total(&valuation, "costValue"), total(&levels, "costValue"));
    shop.open_till(0);
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);
    let valuation = report(
        &shop,
        ReportRequest::new(ReportKind::StockValuation, DAY, DAY),
    );
    assert_eq!(
        total(&valuation, "costValue"),
        4_420_000,
        "the sale took one oil at 800 DA out"
    );
    assert!(!ReportKind::StockValuation.dated());
}

/// Stock an archived or untracked product still holds (data from before archiving required zero
/// stock) is listed, flagged, and valued: never dropped from the reports.
#[test]
fn stock_an_archived_product_holds_stays_in_the_levels_and_the_valuation() {
    let shop = shop();
    let (oil, sugar) = goods(&shop);
    shop.db
        .write(move |tx| {
            tx.execute("UPDATE products SET archived_at = 1 WHERE id = ?1", [&oil])?;
            tx.execute(
                "UPDATE products SET track_stock = 0 WHERE id = ?1",
                [&sugar],
            )?;
            Ok(())
        })
        .unwrap();
    let levels = report(&shop, ReportRequest::new(ReportKind::StockLevels, DAY, DAY));
    assert_eq!(
        column(&levels, "name"),
        vec![Cell::text("Huile 5 L"), Cell::text("Sucre 1 kg")]
    );
    assert_eq!(
        column(&levels, "archived"),
        vec![Cell::text("yes"), Cell::NULL]
    );
    assert_eq!(total(&levels, "costValue"), 4_500_000);
    let valuation = report(
        &shop,
        ReportRequest::new(ReportKind::StockValuation, DAY, DAY),
    );
    assert_eq!(total(&valuation, "costValue"), 4_500_000);
}

#[test]
fn low_stock_lists_the_product_minimum_and_the_location_minimum() {
    let shop = shop();
    let mut input = ProductInput::simple("Café 250 g", UNIT_PIECE_ID, &shop.tva19, Money(45_000));
    input.min_stock = Qty(10_000);
    let coffee = shop.product_with(input);
    shop.stock_in(&coffee.id, 4_000);
    let actor = shop.owner.actor();
    let id = coffee.id.clone();
    shop.db
        .write(move |tx| {
            minimums::set(
                tx,
                &actor,
                &id,
                vec![LocationMinimum {
                    location_id: SHOP_LOCATION_ID.into(),
                    min_qty: Qty(6_000),
                }],
            )
        })
        .unwrap();
    let low = report(&shop, ReportRequest::new(ReportKind::LowStock, DAY, DAY));
    assert_eq!(
        column(&low, "location"),
        vec![Cell::NULL, Cell::text("Magasin")]
    );
    assert_eq!(
        column(&low, "minQty"),
        vec![Cell::int(10_000), Cell::int(6_000)]
    );
    assert_eq!(
        column(&low, "missing"),
        vec![Cell::int(6_000), Cell::int(2_000)]
    );
    let mut at_shop = ReportRequest::new(ReportKind::LowStock, DAY, DAY);
    at_shop.location_id = Some(SHOP_LOCATION_ID.into());
    assert_eq!(
        column(&report(&shop, at_shop), "minQty"),
        vec![Cell::int(6_000)]
    );
}

#[test]
fn a_count_report_values_the_differences_at_cost() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    let err = shop
        .db
        .read(|c| run(c, &ReportRequest::new(ReportKind::CountVariance, DAY, DAY)))
        .unwrap_err();
    assert_eq!(
        err,
        AppError::validation("countId", "report_filter_required")
    );
    let ctx = shop.owner_till();
    let only_oil = CountInput {
        scope: CountScope::Products,
        product_ids: vec![oil.clone()],
        ..full_count()
    };
    let count = shop
        .db
        .write(move |tx| counts::start(tx, &ctx, only_oil))
        .unwrap();
    let ctx = shop.owner_till();
    let (id, product) = (count.id.clone(), oil.clone());
    shop.db
        .write(move |tx| counts::set_counted(tx, &ctx, &id, &product, Qty(48_000), CountMode::Set))
        .unwrap();
    let mut req = ReportRequest::new(ReportKind::CountVariance, DAY, DAY);
    req.count_id = Some(count.id.clone());
    let variance = report(&shop, req);
    assert_eq!(column(&variance, "expected"), vec![Cell::int(50_000)]);
    assert_eq!(column(&variance, "counted"), vec![Cell::int(48_000)]);
    assert_eq!(column(&variance, "difference"), vec![Cell::int(-2_000)]);
    assert_eq!(total(&variance, "costValue"), -160_000);
}

#[test]
fn balances_and_ageing_match_the_ledgers() {
    let shop = shop();
    let karim = shop.customer("Karim");
    opening(&shop, &karim.id, 150_000);
    let supplier = shop.supplier("Cevital");
    let ctx = shop.owner_till();
    let id = supplier.id.clone();
    shop.db
        .write(move |tx| suppliers::set_opening(tx, &ctx, &id, Money(300_000), None))
        .unwrap();

    let balances = report(
        &shop,
        ReportRequest::new(ReportKind::CustomerBalances, DAY, DAY),
    );
    assert_eq!(column(&balances, "name"), vec![Cell::text("Karim")]);
    assert_eq!(total(&balances, "balance"), 150_000);
    assert!(
        balances.table.column("phone").is_none(),
        "no contact data in reports"
    );
    let aging = report(
        &shop,
        ReportRequest::new(ReportKind::CustomerAging, DAY, DAY),
    );
    assert_eq!(total(&aging, "balance"), 150_000);
    let buckets: i64 = ["days0To30", "days31To60", "days61To90", "over90"]
        .iter()
        .map(|k| total(&aging, k))
        .sum();
    assert_eq!(buckets, 150_000);
    let payables = report(
        &shop,
        ReportRequest::new(ReportKind::SupplierBalances, DAY, DAY),
    );
    assert_eq!(total(&payables, "balance"), 300_000);
    let activity = report(
        &shop,
        ReportRequest::new(ReportKind::CreditActivity, DAY, DAY),
    );
    assert_eq!(column(&activity, "adjustments"), vec![Cell::int(150_000)]);
    assert_eq!(column(&activity, "balance"), vec![Cell::int(150_000)]);
}

#[test]
fn daily_cash_in_adds_the_kridi_collections_to_the_sales() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    shop.open_till(0);
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);
    let karim = shop.customer("Karim");
    opening(&shop, &karim.id, 80_000);
    let ctx = shop.owner_till();
    let versement = CollectInput {
        customer_id: karim.id.clone(),
        amount: Money(50_000),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    };
    shop.db
        .write(move |tx| customers::credit::collect(tx, &ctx, versement))
        .unwrap();

    let daily = report(
        &shop,
        ReportRequest::new(ReportKind::DailyByMethod, DAY, DAY),
    );
    assert_eq!(column(&daily, "method"), vec![Cell::text("cash")]);
    assert_eq!(column(&daily, "sales"), vec![Cell::int(119_000)]);
    assert_eq!(column(&daily, "collections"), vec![Cell::int(50_000)]);
    // The shop absorbs the duty in this test: nothing is added to the drawer for it.
    assert_eq!(column(&daily, "total"), vec![Cell::int(169_000)]);
    let sessions = report(
        &shop,
        ReportRequest::new(ReportKind::CashSessions, DAY, DAY),
    );
    assert_eq!(column(&sessions, "closedAt"), vec![Cell::NULL]);
    let moves = report(
        &shop,
        ReportRequest::new(ReportKind::CashMovements, DAY, DAY),
    );
    let kinds = column(&moves, "kind");
    assert!(kinds.contains(&Cell::text("sale")) && kinds.contains(&Cell::text("credit_payment")));
}

#[test]
fn the_expenses_report_and_the_pnl_count_the_same_expenses() {
    let shop = shop();
    shop.open_till(100_000);
    let ctx = shop.till();
    shop.db
        .write(move |tx| {
            crate::cash::expense(tx, &ctx, ExpenseCategory::Transport, Money(5_000), None)
        })
        .unwrap();
    let ctx = shop.owner_till();
    let bill = NewExpense {
        category_id: ELECTRICITY.into(),
        amount: Money(15_000),
        method: PaymentMethod::Transfer,
        from_till: false,
        reference: Some("CCP 7894561".into()),
        supplier_id: None,
        business_date: None,
        note: None,
    };
    shop.db
        .write(move |tx| expenses::record(tx, &ctx, bill))
        .unwrap();
    let spent = report(&shop, ReportRequest::new(ReportKind::Expenses, DAY, DAY));
    assert_eq!(
        column(&spent, "category"),
        vec![Cell::text("Électricité et gaz"), Cell::text("Transport")]
    );
    assert_eq!(
        (total(&spent, "amount"), total(&spent, "fromTill")),
        (20_000, 5_000)
    );
    let pnl = report(
        &shop,
        ReportRequest::new(ReportKind::ProfitAndLoss, DAY, DAY),
    );
    let at = pnl
        .table
        .rows
        .iter()
        .find(|r| r[0] == Cell::text("expenses_total"))
        .unwrap();
    assert_eq!(at[2], Cell::int(-total(&spent, "amount")));
}

#[test]
fn the_soldes_declaration_is_the_promotion_s_own_table() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    let err = shop
        .db
        .read(|c| {
            run(
                c,
                &ReportRequest::new(ReportKind::PromotionDeclaration, DAY, DAY),
            )
        })
        .unwrap_err();
    assert_eq!(
        err,
        AppError::validation("promotionId", "report_filter_required")
    );
    let actor = shop.owner.actor();
    let promo = shop
        .db
        .write(move |tx| {
            crate::promotions::create(
                tx,
                &actor,
                solde_input(PromoScope::Products { ids: vec![oil] }),
            )
        })
        .unwrap();
    let mut req = ReportRequest::new(ReportKind::PromotionDeclaration, DAY, DAY);
    req.promotion_id = Some(promo.id.clone());
    let declared = report(&shop, req);
    let id = promo.id.clone();
    let expected = shop
        .db
        .read(move |c| crate::promotions::declaration(c, &id, Lang::Fr))
        .unwrap();
    assert_eq!(declared.table, expected);
    assert_eq!(declared.table.rows.len(), 1);
}

use crate::purchases::lines::PurchaseLineInput;
use crate::purchases::receipts::{ReceiptInput, receive};
use crate::purchases::returns::{SupplierReturnInput, SupplierReturnLine, return_to_supplier};
use crate::stock::counts::Uncounted;
use crate::tax::tva::Discount;
use crate::testkit::NOON;
use crate::till::TillContext;
use std::sync::Arc;

const DAY_BEFORE: &str = "2026-09-27";
const TWO_DAYS_BEFORE: &str = "2026-09-26";
const DAY_MS: i64 = 86_400_000;

/// The owner's till context at noon on the day before `DAY`.
fn day_before(shop: &Shop) -> TillContext {
    TillContext::new(shop.owner.clone(), Arc::clone(&shop.pack), NOON - DAY_MS)
}

fn purchase_line(product: &str, qty: i64, cost: i64) -> PurchaseLineInput {
    PurchaseLineInput {
        product_id: product.into(),
        qty: Qty(qty),
        unit_cost: Money(cost),
        pack_qty: None,
        tax_rate_id: None,
        discount: Discount::None,
        origin_line_id: None,
        lot_number: None,
        expiry_date: None,
        serials: Vec::new(),
        note: None,
    }
}

/// A goods receipt that is not invoiced (no supplier ledger entry).
fn bring(shop: &Shop, ctx: TillContext, supplier: &str, lines: Vec<PurchaseLineInput>) {
    let receipt = ReceiptInput {
        supplier_id: supplier.into(),
        order_id: None,
        location_id: None,
        lines,
        supplier_ref: None,
        note: None,
        invoice: None,
    };
    shop.db.write(move |tx| receive(tx, &ctx, receipt)).unwrap();
}

fn on(shop: &Shop, kind: ReportKind, day: &str) -> Report {
    report(shop, ReportRequest::new(kind, day, day))
}

#[test]
fn stock_reports_on_a_past_day_read_the_movements_up_to_that_day() {
    let shop = shop();
    shop.set("stamp_duty.mode", json!("absorb"));
    let mut oil = ProductInput::simple("Huile 5 L", UNIT_PIECE_ID, &shop.tva19, Money(119_000));
    oil.cost_price = Money(80_000);
    oil.min_stock = Qty(50_000);
    let oil = shop.product_with(oil).id;
    let input = crate::stock::AdjustInput {
        product_id: oil.clone(),
        location_id: None,
        qty_delta: Qty(50_000),
        reason: crate::stock::AdjustReason::Opening,
        unit_cost: None,
        note: None,
    };
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| crate::stock::adjust(tx, &actor, input, NOON - DAY_MS))
        .unwrap();
    shop.open_till(0);
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);

    // The day before the sale: 50 in stock, above the minimum.
    let levels = on(&shop, ReportKind::StockLevels, DAY_BEFORE);
    assert_eq!(column(&levels, "qty"), vec![Cell::int(50_000)]);
    assert_eq!(column(&levels, "costValue"), vec![Cell::int(4_000_000)]);
    let valuation = on(&shop, ReportKind::StockValuation, DAY_BEFORE);
    assert_eq!(total(&valuation, "costValue"), 4_000_000);
    assert!(
        on(&shop, ReportKind::LowStock, DAY_BEFORE)
            .table
            .rows
            .is_empty()
    );
    // The day of the sale: 49, below the minimum.
    let levels = on(&shop, ReportKind::StockLevels, DAY);
    assert_eq!(column(&levels, "qty"), vec![Cell::int(49_000)]);
    let valuation = on(&shop, ReportKind::StockValuation, DAY);
    assert_eq!(total(&valuation, "costValue"), 3_920_000);
    let low = on(&shop, ReportKind::LowStock, DAY);
    assert_eq!(column(&low, "missing"), vec![Cell::int(1_000)]);
    // Before the opening stock there was nothing.
    assert!(
        on(&shop, ReportKind::StockLevels, TWO_DAYS_BEFORE)
            .table
            .rows
            .is_empty()
    );
    let valuation = on(&shop, ReportKind::StockValuation, TWO_DAYS_BEFORE);
    assert_eq!(total(&valuation, "costValue"), 0);
}

#[test]
fn expiry_counts_the_lots_as_they_stood_on_the_to_day() {
    let shop = shop();
    let syrup = shop.product("Sirop Toplexil", Money(40_000), &shop.tva9);
    let actor = shop.owner.actor();
    let id = syrup.id.clone();
    shop.db
        .write(move |tx| {
            crate::catalog::extras::set(
                tx,
                &actor,
                &id,
                crate::catalog::extras::ProductExtrasInput {
                    track_lots: true,
                    ..crate::catalog::extras::ProductExtrasInput::default()
                },
            )
        })
        .unwrap();
    let supplier = shop.supplier("Biopharm");
    let lot = |number: &str, expiry: &str, qty: i64| {
        let mut l = purchase_line(&syrup.id, qty, 20_000);
        l.lot_number = Some(number.into());
        l.expiry_date = Some(expiry.into());
        l
    };
    bring(
        &shop,
        day_before(&shop),
        &supplier.id,
        vec![
            lot("L-OLD", "2026-09-27", 5_000),
            lot("L-SOON", "2026-10-05", 10_000),
        ],
    );
    // On DAY four of the soon lot go back to the supplier.
    let back = SupplierReturnInput {
        supplier_id: supplier.id.clone(),
        location_id: None,
        lines: vec![SupplierReturnLine {
            product_id: syrup.id.clone(),
            qty: Qty(4_000),
            unit_cost: None,
            lot_number: Some("L-SOON".into()),
            serials: Vec::new(),
            note: None,
        }],
        credit_supplier: false,
        note: None,
    };
    let ctx = shop.owner_till();
    shop.db
        .write(move |tx| return_to_supplier(tx, &ctx, back))
        .unwrap();

    let today = on(&shop, ReportKind::Expiry, DAY);
    assert_eq!(
        column(&today, "lot"),
        vec![Cell::text("L-OLD"), Cell::text("L-SOON")]
    );
    assert_eq!(
        column(&today, "qty"),
        vec![Cell::int(5_000), Cell::int(6_000)]
    );
    assert_eq!(
        column(&today, "daysLeft"),
        vec![Cell::int(-1), Cell::int(7)]
    );
    assert_eq!(
        column(&today, "status"),
        vec![Cell::text("expired"), Cell::text("soon")]
    );
    let before = on(&shop, ReportKind::Expiry, DAY_BEFORE);
    assert_eq!(
        column(&before, "qty"),
        vec![Cell::int(5_000), Cell::int(10_000)],
        "the return came the day after"
    );
    assert_eq!(
        column(&before, "daysLeft"),
        vec![Cell::int(0), Cell::int(8)]
    );
    assert_eq!(
        column(&before, "status"),
        vec![Cell::text("soon"), Cell::text("soon")]
    );
    assert!(
        on(&shop, ReportKind::Expiry, TWO_DAYS_BEFORE)
            .table
            .rows
            .is_empty(),
        "the lots came in on the day before"
    );
}

#[test]
fn balances_and_ageing_on_a_past_day_read_the_ledgers_up_to_that_day() {
    let shop = shop();
    let karim = shop.customer("Karim");
    let sidi = shop.customer("Sidi");
    let supplier = shop.supplier("Cevital");
    // The day before: Karim's notebook opens at 1 500 DA.
    let (ctx, id) = (day_before(&shop), karim.id.clone());
    shop.db
        .write(move |tx| customers::credit::set_opening(tx, &ctx, &id, Money(150_000), None))
        .unwrap();
    // DAY: Karim pays it all by card, Sidi's notebook opens at 400 DA, the shop owes Cevital.
    let ctx = shop.owner_till();
    let versement = CollectInput {
        customer_id: karim.id.clone(),
        amount: Money(150_000),
        method: PaymentMethod::Edahabia,
        reference: Some("TX-1".into()),
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    };
    shop.db
        .write(move |tx| customers::credit::collect(tx, &ctx, versement))
        .unwrap();
    opening(&shop, &sidi.id, 40_000);
    let (ctx, id) = (shop.owner_till(), supplier.id.clone());
    shop.db
        .write(move |tx| suppliers::set_opening(tx, &ctx, &id, Money(300_000), None))
        .unwrap();

    let before = on(&shop, ReportKind::CustomerBalances, DAY_BEFORE);
    assert_eq!(column(&before, "name"), vec![Cell::text("Karim")]);
    assert_eq!(column(&before, "balance"), vec![Cell::int(150_000)]);
    assert_eq!(
        column(&before, "lastPayment"),
        vec![Cell::NULL],
        "he paid the day after"
    );
    let today = on(&shop, ReportKind::CustomerBalances, DAY);
    assert_eq!(column(&today, "name"), vec![Cell::text("Sidi")]);
    assert_eq!(column(&today, "balance"), vec![Cell::int(40_000)]);

    let aging = on(&shop, ReportKind::CustomerAging, DAY_BEFORE);
    assert_eq!(column(&aging, "name"), vec![Cell::text("Karim")]);
    assert_eq!(column(&aging, "balance"), vec![Cell::int(150_000)]);
    assert_eq!(column(&aging, "days0To30"), vec![Cell::int(150_000)]);
    assert_eq!(column(&aging, "advance"), vec![Cell::int(0)]);
    let aging = on(&shop, ReportKind::CustomerAging, DAY);
    assert_eq!(column(&aging, "name"), vec![Cell::text("Sidi")]);
    assert_eq!(column(&aging, "days0To30"), vec![Cell::int(40_000)]);

    let payables = on(&shop, ReportKind::SupplierBalances, DAY_BEFORE);
    assert!(payables.table.rows.is_empty());
    let payables = on(&shop, ReportKind::SupplierBalances, DAY);
    assert_eq!(total(&payables, "balance"), 300_000);

    let activity = on(&shop, ReportKind::CreditActivity, DAY_BEFORE);
    assert_eq!(column(&activity, "name"), vec![Cell::text("Karim")]);
    assert_eq!(column(&activity, "balance"), vec![Cell::int(150_000)]);
    let activity = on(&shop, ReportKind::CreditActivity, DAY);
    assert_eq!(
        column(&activity, "name"),
        vec![Cell::text("Karim"), Cell::text("Sidi")]
    );
    assert_eq!(
        column(&activity, "collected"),
        vec![Cell::int(150_000), Cell::int(0)]
    );
    assert_eq!(
        column(&activity, "balance"),
        vec![Cell::int(0), Cell::int(40_000)],
        "the balance at the end of the range"
    );

    let bought = on(&shop, ReportKind::PurchasesBySupplier, DAY_BEFORE);
    assert!(bought.table.rows.is_empty(), "nothing owed to Cevital yet");
    let bought = on(&shop, ReportKind::PurchasesBySupplier, DAY);
    assert_eq!(column(&bought, "balance"), vec![Cell::int(300_000)]);
}

#[test]
fn a_count_expects_the_level_when_counted_and_a_closed_one_what_it_posted_against() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    shop.open_till(0);
    let ctx = shop.owner_till();
    let only_oil = CountInput {
        scope: CountScope::Products,
        product_ids: vec![oil.clone()],
        ..full_count()
    };
    let count = shop
        .db
        .write(move |tx| counts::start(tx, &ctx, only_oil))
        .unwrap();
    let ctx = shop.owner_till();
    let (id, product) = (count.id.clone(), oil.clone());
    shop.db
        .write(move |tx| counts::set_counted(tx, &ctx, &id, &product, Qty(48_000), CountMode::Set))
        .unwrap();
    // A sale after the oil was counted: the count compares the 48 with the 50 held when it was
    // counted, and the sale is not undone by the close.
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);
    let mut req = ReportRequest::new(ReportKind::CountVariance, DAY, DAY);
    req.count_id = Some(count.id.clone());
    let open = report(&shop, req.clone());
    assert_eq!(column(&open, "expected"), vec![Cell::int(50_000)]);
    assert_eq!(column(&open, "difference"), vec![Cell::int(-2_000)]);
    assert_eq!(total(&open, "costValue"), -160_000);

    let ctx = shop.owner_till();
    let id = count.id.clone();
    shop.db
        .write(move |tx| counts::close(tx, &ctx, &id, Uncounted::Ignore))
        .unwrap();
    // A sale after the close does not change what the count posted against.
    sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);
    let closed = report(&shop, req);
    assert_eq!(column(&closed, "expected"), vec![Cell::int(50_000)]);
    assert_eq!(column(&closed, "counted"), vec![Cell::int(48_000)]);
    assert_eq!(column(&closed, "difference"), vec![Cell::int(-2_000)]);
    assert_eq!(total(&closed, "costValue"), -160_000);
}

#[test]
fn the_movement_log_lists_the_day_s_moves_with_their_document() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    shop.open_till(0);
    let sale = sell(&shop, &oil, 1_000, vec![PaymentInput::cash(Money(119_000))]);
    let log = on(&shop, ReportKind::StockMovements, DAY);
    let reasons = column(&log, "reason");
    assert_eq!(reasons.len(), 3, "two openings and the sale");
    assert_eq!(
        reasons
            .iter()
            .filter(|r| **r == Cell::text("opening"))
            .count(),
        2
    );
    let at = reasons
        .iter()
        .position(|r| *r == Cell::text("sale"))
        .unwrap();
    let row = &log.table.rows[at];
    let cell = |key: &str| row[log.table.column(key).unwrap()].clone();
    assert_eq!(
        cell("document"),
        Cell::opt_text(sale.document.number.clone())
    );
    assert_eq!(cell("name"), Cell::text("Huile 5 L"));
    assert_eq!(cell("qty"), Cell::int(-1_000));
    assert!(
        on(&shop, ReportKind::StockMovements, DAY_BEFORE)
            .table
            .rows
            .is_empty()
    );
}

#[test]
fn purchases_by_supplier_lists_a_supplier_with_receipts_only() {
    let shop = shop();
    let (oil, _) = goods(&shop);
    let supplier = shop.supplier("Cevital");
    bring(
        &shop,
        shop.owner_till(),
        &supplier.id,
        vec![purchase_line(&oil, 2_000, 60_000)],
    );
    let bought = on(&shop, ReportKind::PurchasesBySupplier, DAY);
    assert_eq!(column(&bought, "name"), vec![Cell::text("Cevital")]);
    assert_eq!(column(&bought, "receipts"), vec![Cell::int(1)]);
    assert_eq!(
        column(&bought, "purchases"),
        vec![Cell::int(0)],
        "not invoiced yet"
    );
    assert_eq!(column(&bought, "balance"), vec![Cell::int(0)]);
}

#[test]
fn purchases_by_product_average_the_cost_over_the_receipts() {
    let shop = shop();
    let (oil, sugar) = goods(&shop);
    let (cevital, sim) = (shop.supplier("Cevital"), shop.supplier("Sim"));
    bring(
        &shop,
        shop.owner_till(),
        &cevital.id,
        vec![
            purchase_line(&oil, 1_000, 60_000),
            purchase_line(&sugar, 1_000, 7_500),
        ],
    );
    bring(
        &shop,
        shop.owner_till(),
        &sim.id,
        vec![
            purchase_line(&oil, 2_000, 80_000),
            purchase_line(&sugar, 1_000, 7_501),
        ],
    );
    let bought = on(&shop, ReportKind::PurchasesByProduct, DAY);
    assert_eq!(
        column(&bought, "name"),
        vec![Cell::text("Huile 5 L"), Cell::text("Sucre 1 kg")]
    );
    assert_eq!(
        column(&bought, "qty"),
        vec![Cell::int(3_000), Cell::int(2_000)]
    );
    assert_eq!(
        column(&bought, "purchasedHt"),
        vec![Cell::int(220_000), Cell::int(15_001)]
    );
    // 220 000 / 3 = 73 333.33 and 15 001 / 2 = 7 500.5, rounded half up.
    assert_eq!(
        column(&bought, "averageCost"),
        vec![Cell::int(73_333), Cell::int(7_501)]
    );
    assert_eq!(
        column(&bought, "suppliers"),
        vec![Cell::int(2), Cell::int(2)]
    );
    assert_eq!(total(&bought, "purchasedHt"), 235_001);
    let mut from_sim = ReportRequest::new(ReportKind::PurchasesByProduct, DAY, DAY);
    from_sim.supplier_id = Some(sim.id.clone());
    let from_sim = report(&shop, from_sim);
    assert_eq!(
        column(&from_sim, "averageCost"),
        vec![Cell::int(80_000), Cell::int(7_501)]
    );
}
