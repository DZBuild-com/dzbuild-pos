use super::waste::{self, WasteInput, WasteLine, WasteReason};
use super::yields::{self, CostMethod, RunCut, RunInput, TemplateCut, YieldTemplateInput};
use crate::auth::Permission;
use crate::catalog::UNIT_KG_ID;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::testkit::{Shop, shop};
use proptest::prelude::*;

fn butcher() -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(tx, &actor, &[Module::Yield, Module::Waste, Module::Staff])
        })
        .unwrap();
    shop
}

fn by_kg(shop: &Shop, name: &str, price: i64, cost: i64) -> String {
    let mut input =
        crate::catalog::products::ProductInput::simple(name, UNIT_KG_ID, &shop.tva19, Money(price));
    input.is_weighed = true;
    input.cost_price = Money(cost);
    shop.product_with(input).id
}

fn lamb(shop: &Shop) -> (String, String, String, String) {
    let carcass = by_kg(shop, "Carcasse d'agneau", 0, 150_000);
    shop.stock_in(&carcass, 20_000);
    (
        carcass,
        by_kg(shop, "Gigot", 280_000, 0),
        by_kg(shop, "Côtelettes", 260_000, 0),
        by_kg(shop, "Épaule", 220_000, 0),
    )
}

fn input(carcass: &str, cuts: &[(&str, i64)], cost: Option<i64>, method: CostMethod) -> RunInput {
    RunInput {
        template_id: None,
        source_product_id: carcass.into(),
        location_id: None,
        input_qty: Qty(20_000),
        input_cost: cost.map(Money),
        method,
        cuts: cuts
            .iter()
            .map(|(p, q)| RunCut {
                product_id: (*p).into(),
                qty: Qty(*q),
            })
            .collect(),
        note: None,
    }
}

#[test]
fn a_run_splits_the_carcass_cost_by_value_and_moves_the_cuts_cmup() {
    let shop = butcher();
    let (carcass, gigot, cotes, epaule) = lamb(&shop);
    let ctx = shop.approved_till(Permission::StockAdjust);
    let run_input = input(
        &carcass,
        &[(&gigot, 6_000), (&cotes, 5_000), (&epaule, 7_000)],
        None,
        CostMethod::Value,
    );
    let run = shop
        .db
        .write(move |tx| yields::run(tx, &ctx, run_input, Lang::Fr))
        .unwrap();
    assert_eq!(run.number, "DC2026-000001");
    assert_eq!(
        (run.input_cost, run.output_qty, run.loss_qty),
        (Money(3_000_000), Qty(18_000), Qty(2_000)),
        "20 kg at 1 500 DA/kg, 2 kg of bone and fat"
    );
    assert_eq!(run.cuts.iter().map(|c| c.cost.0).sum::<i64>(), 3_000_000);
    let gigot_cut = run.cuts.iter().find(|c| c.product_id == gigot).unwrap();
    // weights: 6 × 2800 = 16 800, 5 × 2600 = 13 000, 7 × 2200 = 15 400; total 45 200
    assert_eq!(gigot_cut.cost, Money(1_115_044));
    assert_eq!(gigot_cut.unit_cost, Money(185_841), "per kg");
    let g = gigot.clone();
    assert_eq!(
        shop.db
            .read(move |c| crate::catalog::products::get(c, &g))
            .unwrap()
            .cost_price,
        Money(185_841)
    );
    let c = carcass.clone();
    assert_eq!(
        shop.db
            .read(move |conn| crate::stock::on_hand(conn, &c))
            .unwrap(),
        Qty::ZERO
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM stock_movements WHERE reason = 'production'"),
        4
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'butcher.yield_run'"),
        1
    );
}

#[test]
fn a_run_never_outputs_more_than_the_carcass_nor_takes_what_is_not_there() {
    let shop = butcher();
    let (carcass, gigot, cotes, _) = lamb(&shop);
    let ctx = shop.approved_till(Permission::StockAdjust);
    let too_much = input(
        &carcass,
        &[(&gigot, 15_000), (&cotes, 6_000)],
        None,
        CostMethod::Weight,
    );
    assert_eq!(
        shop.db
            .write(move |tx| yields::run(tx, &ctx, too_much, Lang::Fr))
            .unwrap_err()
            .code(),
        "yield_output_exceeds_input"
    );
    let ctx = shop.approved_till(Permission::StockAdjust);
    let mut short = input(&carcass, &[(&gigot, 5_000)], None, CostMethod::Weight);
    short.input_qty = Qty(25_000);
    assert_eq!(
        shop.db
            .write(move |tx| yields::run(tx, &ctx, short, Lang::Fr))
            .unwrap_err()
            .code(),
        "yield_source_short"
    );
    let ctx = shop.till();
    let cashier = input(&carcass, &[(&gigot, 5_000)], None, CostMethod::Weight);
    assert_eq!(
        shop.db
            .write(move |tx| yields::run(tx, &ctx, cashier, Lang::Fr))
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn a_template_holds_expected_shares_and_the_preview_compares_them() {
    let shop = butcher();
    let (carcass, gigot, cotes, epaule) = lamb(&shop);
    let actor = shop.owner.actor();
    let (c, g, co, e) = (
        carcass.clone(),
        gigot.clone(),
        cotes.clone(),
        epaule.clone(),
    );
    let bad = YieldTemplateInput {
        source_product_id: c.clone(),
        name: "Agneau".into(),
        cuts: vec![
            TemplateCut {
                product_id: g.clone(),
                expected_bp: 6_000,
            },
            TemplateCut {
                product_id: co.clone(),
                expected_bp: 5_000,
            },
        ],
    };
    assert_eq!(
        shop.db
            .write(move |tx| yields::save_template(tx, &actor, None, bad))
            .unwrap_err()
            .code(),
        "yield_shares_invalid"
    );
    let actor = shop.owner.actor();
    let good = YieldTemplateInput {
        source_product_id: c,
        name: "Agneau".into(),
        cuts: vec![
            TemplateCut {
                product_id: g,
                expected_bp: 3_000,
            },
            TemplateCut {
                product_id: co,
                expected_bp: 2_500,
            },
            TemplateCut {
                product_id: e,
                expected_bp: 3_500,
            },
        ],
    };
    let template = shop
        .db
        .write(move |tx| yields::save_template(tx, &actor, None, good))
        .unwrap();
    let mut with_template = input(
        &carcass,
        &[(&gigot, 6_000), (&cotes, 5_000), (&epaule, 7_000)],
        Some(2_900_000),
        CostMethod::Weight,
    );
    with_template.template_id = Some(template.id);
    let preview = shop
        .db
        .read(move |c| yields::preview(c, &with_template, Lang::Fr))
        .unwrap();
    let gigot_row = preview.cuts.iter().find(|c| c.product_id == gigot).unwrap();
    assert_eq!(
        (gigot_row.expected_qty, gigot_row.yield_bp),
        (Qty(6_000), 3_000)
    );
    assert_eq!((preview.loss_qty, preview.loss_bp), (Qty(2_000), 1_000));
    assert_eq!(
        preview.cuts.iter().map(|c| c.cost.0).sum::<i64>(),
        2_900_000
    );
}

/// Archiving a template is a pack write: refused while the yield module is off (Task 7 review).
#[test]
fn a_template_is_archived_only_while_the_yield_module_is_on() {
    let shop = butcher();
    let (carcass, gigot, ..) = lamb(&shop);
    let actor = shop.owner.actor();
    let input = YieldTemplateInput {
        source_product_id: carcass,
        name: "Agneau".into(),
        cuts: vec![TemplateCut {
            product_id: gigot,
            expected_bp: 9_000,
        }],
    };
    let template = shop
        .db
        .write(move |tx| yields::save_template(tx, &actor, None, input))
        .unwrap();
    let modules = |on: &'static [Module]| {
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| packs::set_modules(tx, &actor, on))
            .unwrap();
    };
    modules(&[Module::Waste]);
    let (actor, id) = (shop.owner.actor(), template.id.clone());
    assert_eq!(
        shop.db
            .write(move |tx| yields::archive_template(tx, &actor, &id))
            .unwrap_err()
            .code(),
        "module_disabled"
    );
    modules(&[Module::Yield]);
    let (actor, id) = (shop.owner.actor(), template.id);
    shop.db
        .write(move |tx| yields::archive_template(tx, &actor, &id))
        .unwrap();
}

#[test]
fn the_waste_log_adjusts_stock_per_reason_and_reads_back_by_day() {
    let shop = butcher();
    let tomates = by_kg(&shop, "Tomates", 12_000, 8_000);
    let pain = shop.product("Baguette", Money(1_500), &shop.exempt).id;
    shop.stock_in(&tomates, 30_000);
    shop.stock_in(&pain, 50_000);
    let ctx = shop.approved_till(Permission::StockAdjust);
    let input = WasteInput {
        location_id: None,
        lines: vec![
            WasteLine {
                product_id: tomates.clone(),
                qty: Qty(2_500),
                reason: WasteReason::Damage,
            },
            WasteLine {
                product_id: pain.clone(),
                qty: Qty(12_000),
                reason: WasteReason::Expiry,
            },
        ],
        note: Some("Invendus du soir".into()),
    };
    let moved = shop
        .db
        .write(move |tx| waste::record(tx, &ctx, input))
        .unwrap();
    assert_eq!(moved.len(), 2);
    let t = tomates.clone();
    assert_eq!(
        shop.db.read(move |c| crate::stock::on_hand(c, &t)).unwrap(),
        Qty(27_500)
    );
    let rows = shop
        .db
        .read(|c| waste::day(c, "2026-09-28", crate::lang::Lang::Fr))
        .unwrap();
    let tomato = rows.iter().find(|r| r.product_id == tomates).unwrap();
    assert_eq!(
        (tomato.qty, tomato.reason, tomato.cost),
        (Qty(2_500), WasteReason::Damage, Money(20_000))
    );
}

proptest! {
    #[test]
    fn a_yield_run_conserves_the_carcass_cost_to_the_centime(
        total in 0_i64..50_000_000,
        weights in proptest::collection::vec(0_i64..10_000_000, 1..12),
    ) {
        let w: Vec<i128> = weights.iter().map(|x| i128::from(*x)).collect();
        let shares = yields::allocate(Money(total), &w);
        prop_assert_eq!(shares.len(), w.len());
        prop_assert_eq!(shares.iter().map(|m| m.0).sum::<i64>(), total);
        prop_assert!(shares.iter().all(|m| m.0 >= 0));
    }
}
