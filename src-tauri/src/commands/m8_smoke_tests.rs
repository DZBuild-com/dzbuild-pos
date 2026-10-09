//! M8 acceptance on the Rust side (Task 31): every activity template sets up a working shop; a day
//! in every pack reconciles the drawer, the ledgers, the stock, the reports and the fiscal chains
//! to the centime; every report runs in three languages; and the same day at a till lands where
//! it belongs (its cash in its own drawer, the registers and the ledgers on the main PC).

use super::testshop::{TestShop, column_total, counted, every_module_shop, first_run_as};
use pos_core::company::ShopActivity;
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::packs::templates::{self, PackRelease};
use pos_core::reports::ReportKind;

/// The float every drawer of these tests opens with.
const FLOAT: Money = Money(1_000_000);

#[test]
fn every_template_sets_up_a_working_shop() {
    for activity in ShopActivity::ALL {
        let t: TestShop = first_run_as(activity);
        let st = &t.st;
        let template = templates::template(activity);
        let state = super::packs::state(st).unwrap();
        let mut modules = template.modules.to_vec();
        modules.sort();
        assert_eq!(
            state.modules, modules,
            "{activity:?}: the template's modules"
        );
        assert_eq!(
            state.till_mode, template.till_mode,
            "{activity:?}: the till mode"
        );
        let categories = t.categories();
        assert!(
            categories.len() >= template.categories.len(),
            "{activity:?}: seeded categories"
        );
        for c in &categories {
            assert!(
                !c.name_ar.is_empty() && !c.name_fr.is_empty() && !c.name_en.is_empty(),
                "{activity:?}: {c:?} named in three languages"
            );
        }
        for service in template.services {
            let product = t.product_by_reference(service.code);
            assert!(
                !product.name_ar.is_empty()
                    && !product.name_fr.is_empty()
                    && !product.name_en.is_empty(),
                "{activity:?}: the service {} named in three languages",
                service.code
            );
        }
        if template.release == PackRelease::Mvp {
            t.open_till(10_000);
            let product = t.product("Article témoin", 10_000, 1_900, None);
            t.stock_in(&product.id, 5);
            let sale =
                super::sales::create(st, crate::lan::two_pcs::cash_sale(&product.id, 1, 10_000))
                    .unwrap_or_else(|e| panic!("{activity:?}: {e:?}"));
            assert_eq!(
                sale.sale.document.total_ttc,
                Money(10_000),
                "{activity:?}: a sale works in the template's shop"
            );
        }
    }
}

/// A day in every pack at `t`'s open drawer: what the drawer should hold (the float, what the
/// packs took in, less what they paid out) and the salary advance paid out of it.
fn a_day_in_every_pack(t: &TestShop) -> (Money, Money) {
    let mut cash_in = FLOAT;
    cash_in = cash_in.checked_add(t.day_restaurant()).unwrap(); // kitchen tickets, a split table, one seat in cash
    cash_in = cash_in.checked_add(t.day_flexy()).unwrap(); // a top-up
    cash_in = cash_in.checked_add(t.day_serials()).unwrap(); // a phone with its IMEI, by Edahabia: no cash
    cash_in = cash_in.checked_add(t.day_repair()).unwrap(); // deposit with duty, pickup balance
    cash_in = cash_in.checked_add(t.day_instalment()).unwrap(); // down payment with duty, one collection
    cash_in = cash_in.checked_add(t.day_vouchers()).unwrap(); // a gift voucher sold (paid-in with duty), redeemed on a loyalty ticket
    cash_in = cash_in.checked_add(t.day_layaway()).unwrap(); // two deposits, the sale by voucher only
    cash_in = cash_in.checked_add(t.day_round()).unwrap(); // a round closed with a cash collection
    let advance = t.day_staff_advance(); // an advance from the till
    cash_in = cash_in.checked_sub(advance).unwrap();
    cash_in = cash_in.checked_add(t.day_zakat()).unwrap(); // the zakat statement: no cash
    cash_in = cash_in.checked_add(t.day_jewellery()).unwrap(); // a ring with the buyer, card for the rest
    cash_in = cash_in.checked_add(t.day_pharmacy()).unwrap(); // a controlled medicine with its prescription
    cash_in = cash_in.checked_add(t.day_butcher()).unwrap(); // a cutting run, the waste, a weighed sale
    cash_in = cash_in.checked_add(t.day_payment_type()).unwrap(); // a custom payment type: no cash
    (cash_in, advance)
}

#[test]
fn a_day_in_every_pack_reconciles_the_drawer_and_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let t = every_module_shop(dir.path());
    let st = &t.st;
    assert!(
        crate::license::verdict("sales_create", Some(st)).is_ok(),
        "the day runs with a licence"
    );
    t.open_till(FLOAT.0);
    let (cash_in, advance) = a_day_in_every_pack(&t);
    let x = super::cash::x_report(st).unwrap();
    assert_eq!(
        x.expected_cash,
        Some(cash_in),
        "the drawer holds exactly what the packs took in and paid out"
    );
    // The cash reports read the same drawer: its movements add up to the X, and the duty
    // statement's cash base is every dinar received (each cash amount of the day is dutiable),
    // the float and the advance aside.
    let movements = t.report(ReportKind::CashMovements);
    assert_eq!(Money(column_total(&movements, "amount")), cash_in);
    let stamp = t.report(ReportKind::StampDutyStatement);
    assert_eq!(
        column_total(&stamp, "cashBase"),
        cash_in.0 - FLOAT.0 + advance.0
    );
    t.packs_reconcile();
    let chain = super::audit::verify(st).unwrap();
    assert!(
        chain.first_broken_seq.is_none(),
        "the journal's hash chain holds after a day of packs: {chain:?}"
    );
    t.fiscal_chains_hold();
    assert!(
        t.numbering_is_gap_free(),
        "every series (TK, AC, RP, CT, TR, BA, DC, ZK, the registers) is gap-free"
    );
    let closed = super::cash::close_session(st, counted(cash_in)).unwrap();
    assert_eq!(closed.z.variance, Money::ZERO);
    assert_eq!(closed.z.summary.expected_cash, Some(cash_in));
    t.fiscal_chains_hold();
    assert!(t.numbering_is_gap_free(), "the Z took the next number");
}

#[test]
fn every_report_runs_in_three_languages() {
    let dir = tempfile::tempdir().unwrap();
    let t = every_module_shop(dir.path());
    let (promotion_id, count_id) = super::testshop::promotion_and_count(&t);
    t.open_till(100_000);
    t.day_restaurant();
    t.day_vouchers();
    let day = super::services::business_day(&t.st);
    let kinds = ReportKind::ALL;
    assert!(kinds.len() >= 50);
    for lang in [Lang::Ar, Lang::Fr, Lang::En] {
        for kind in kinds {
            let mut request = pos_core::reports::ReportRequest::new(kind, &day, &day);
            request.lang = Some(lang);
            match kind.required_filter() {
                Some(pos_core::reports::ReportFilter::Promotion) => {
                    request.promotion_id = Some(promotion_id.clone());
                }
                Some(pos_core::reports::ReportFilter::Count) => {
                    request.count_id = Some(count_id.clone());
                }
                _ => {}
            }
            let view = super::reports::run(&t.st, request)
                .unwrap_or_else(|e| panic!("{kind:?} in {lang:?}: {e:?}"));
            assert!(
                !view.title.is_empty(),
                "{kind:?} in {lang:?}: the report has its title"
            );
        }
    }
}

/// Two PCs: the same day in every pack, at a till. The main PC
/// numbers the pack documents and books their ledgers (the serial, the float, the vouchers and
/// deposits, the registers against the till's own tickets), the till's drawer takes every dinar
/// (its X and its Z), and the main PC's drawer is never opened.
#[test]
fn a_tills_day_in_every_pack_keeps_its_cash_and_the_main_pc_its_books() {
    let pcs = crate::lan::two_pcs::TwoPcs::new();
    pcs.host.licensed();
    pcs.host.stock_every_pack();
    pcs.wait_caught_up();
    let till = pcs.till();
    till.licensed();
    till.owner();
    super::cash::open_session(&till.st, FLOAT).unwrap();
    let (cash_in, _) = a_day_in_every_pack(till);
    let x = super::cash::x_report(&till.st).unwrap();
    assert_eq!(
        x.expected_cash,
        Some(cash_in),
        "the till's drawer holds every dinar its pack day moved"
    );
    let host_terminal = pcs.host.st.terminal.id.clone();
    assert_eq!(
        pcs.host
            .st
            .db
            .read(move |c| pos_core::cash::current(c, &host_terminal))
            .unwrap(),
        None,
        "the main PC never opened a drawer"
    );
    assert!(
        till.st
            .db
            .read(pos_core::sales::extensions::pending_host_registers)
            .unwrap()
            .is_empty(),
        "the main PC took everything the till's tickets left to it"
    );
    pcs.wait_sent();
    pcs.wait_caught_up();
    let till_terminal = till.st.terminal.id.clone();
    let (controlled, jewels): (i64, i64) = pcs
        .host
        .st
        .db
        .read(move |c| {
            let controlled = c.query_row(
                "SELECT COUNT(*) FROM controlled_register r JOIN documents d ON d.id = r.document_id
                 WHERE d.terminal_id = ?1",
                [&till_terminal],
                |r| r.get(0),
            )?;
            let jewels = c.query_row(
                "SELECT COUNT(*) FROM jeweller_register r JOIN documents d ON d.id = r.document_id
                 WHERE d.terminal_id = ?1",
                [&till_terminal],
                |r| r.get(0),
            )?;
            Ok((controlled, jewels))
        })
        .unwrap();
    assert_eq!(
        (controlled, jewels),
        (1, 1),
        "the registers are the main PC's, written against the till's tickets"
    );
    pcs.host.packs_reconcile();
    let closed = super::cash::close_session(&till.st, counted(cash_in)).unwrap();
    assert_eq!(closed.z.variance, Money::ZERO);
    for shop in [&pcs.host, till] {
        shop.owner();
        let chain = super::audit::verify(&shop.st).unwrap();
        assert!(chain.first_broken_seq.is_none(), "{chain:?}");
        shop.fiscal_chains_hold();
    }
    assert!(
        pcs.host.numbering_is_gap_free(),
        "the main PC's series are gap-free"
    );
}
