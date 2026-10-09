//! What the pharmacy and butcher screens need from their commands (Task 25): the expiry board's
//! lots carry their value at cost (zero for whoever does not see costs), the day's waste comes
//! with its cost total, and the controlled register, kept on the main PC only, is printed and
//! exported there.

use super::exports::{self, ExportTarget};
use super::lan_rules_tests::{as_till, link};
use super::testshop::shop;
use super::{butcher, packs, pharmacy, reports};
use pos_core::auth::Permission;
use pos_core::catalog::extras::{self, ProductExtrasInput};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::packs::Module;
use pos_core::reports::ReportRequest;
use pos_core::stock::lots::LotInput;
use pos_core::stock::{ReceiveLine, SHOP_LOCATION_ID, receive};
use pos_core::time::now_ms;

#[test]
fn the_expiry_board_values_each_lot_for_whoever_sees_costs() {
    let t = shop();
    let st = &t.st;
    packs::set_modules(st, vec![Module::Pharmacy, Module::Lots]).unwrap();
    let id = t.product("Amoxicilline 500 mg", 45_000, 0, None).id;
    let (actor, pid) = (t.owner.actor(), id.clone());
    st.db
        .write(move |tx| {
            extras::set_quiet(
                tx,
                &pid,
                ProductExtrasInput {
                    track_lots: true,
                    ..Default::default()
                },
            )?;
            receive(
                tx,
                &actor,
                &ReceiveLine {
                    product_id: pid.clone(),
                    location_id: SHOP_LOCATION_ID.into(),
                    qty: Qty(2_000),
                    unit_cost: Money(25_000),
                    lot: Some(LotInput {
                        lot_number: "A1".into(),
                        expiry_date: Some("2020-01-31".into()),
                    }),
                    serials: Vec::new(),
                    supplier_id: None,
                    ref_type: "test".into(),
                    ref_id: "t".into(),
                    note: None,
                },
                now_ms(),
            )
            .map(drop)
        })
        .unwrap();
    let board = pharmacy::expiry_board(st, Some(30)).unwrap();
    let lot = &board.expired[0];
    assert_eq!(
        (lot.lot.lot_number.as_str(), lot.cost, board.expired_cost),
        ("A1", Money(50_000), Money(50_000))
    );
    t.with_permissions(&[Permission::StockView]);
    let blind = pharmacy::expiry_board(st, Some(30)).unwrap();
    assert_eq!(
        (blind.expired[0].cost, blind.expired_cost),
        (Money::ZERO, Money::ZERO),
        "no cost without reports.profit"
    );
}

#[test]
fn the_days_waste_comes_with_its_total_at_cost() {
    let t = shop();
    let st = &t.st;
    packs::set_modules(st, vec![Module::Waste]).unwrap();
    let pain = t.product("Baguette", 1_500, 0, None).id;
    t.stock_in(&pain, 50);
    t.open_till(0);
    let input = pos_core::butcher::waste::WasteInput {
        location_id: None,
        lines: vec![pos_core::butcher::waste::WasteLine {
            product_id: pain,
            qty: Qty(12_000),
            reason: pos_core::butcher::waste::WasteReason::OwnUse,
        }],
        note: None,
    };
    butcher::record_waste(st, input).unwrap();
    let today = pos_core::time::business_date(st.now())
        .format("%Y-%m-%d")
        .to_string();
    let day = butcher::waste_of_day(st, today.clone()).unwrap();
    assert_eq!(day.rows.len(), 1);
    assert_eq!(day.total_cost, Money(10_800), "12 baguettes at 9 DA");
    t.with_permissions(&[Permission::StockView]);
    let blind = butcher::waste_of_day(st, today).unwrap();
    assert_eq!(
        (blind.rows[0].cost, blind.total_cost),
        (Money::ZERO, Money::ZERO)
    );
}

#[test]
fn the_controlled_register_is_printed_and_exported_on_the_main_pc() {
    let t = shop();
    let st = &t.st;
    packs::set_modules(st, vec![Module::Pharmacy]).unwrap();
    let request: ReportRequest = serde_json::from_value(serde_json::json!({
        "kind": "controlled_register",
        "from": "2026-09-01",
        "to": "2026-09-30",
    }))
    .unwrap();
    assert!(
        reports::run(st, request.clone()).is_ok(),
        "the main PC runs it"
    );
    as_till(&t);
    link(&t, true, None);
    assert_eq!(
        reports::run(st, request.clone()).unwrap_err().code(),
        "host_required",
        "a till's copy has no register"
    );
    let export = exports::table_of(st, ExportTarget::Report { request }, Lang::Fr);
    assert_eq!(
        export.err().map(|e| e.code()),
        Some("host_required".to_owned())
    );
}
