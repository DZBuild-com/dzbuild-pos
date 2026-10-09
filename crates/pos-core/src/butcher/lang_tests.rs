//! The cut names of the preview, the run and the run history come in the reader's language with
//! the fr → ar → en fallback, so a cut named only in Arabic (common at the butcher's) is never
//! blank, and the history sorts by the name the reader sees.

use super::yields::{self, CostMethod, RunCut, RunInput};
use crate::auth::Permission;
use crate::catalog::UNIT_KG_ID;
use crate::catalog::products::ProductInput;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::testkit::{Shop, shop};

fn by_kg(shop: &Shop, (ar, fr, en): (&str, &str, &str), price: i64, cost: i64) -> String {
    let mut input = ProductInput::simple(fr, UNIT_KG_ID, &shop.tva19, Money(price));
    input.name_ar = ar.into();
    input.name_en = en.into();
    input.is_weighed = true;
    input.cost_price = Money(cost);
    shop.product_with(input).id
}

fn names(cuts: &[yields::CutResult]) -> Vec<&str> {
    cuts.iter().map(|c| c.name.as_str()).collect()
}

#[test]
fn an_arabic_only_cut_keeps_its_name_in_the_preview_the_run_and_the_history() {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Yield]))
        .unwrap();
    let carcass = by_kg(
        &shop,
        ("ذبيحة خروف", "Carcasse d'agneau", "Lamb carcass"),
        0,
        150_000,
    );
    shop.stock_in(&carcass, 20_000);
    let gigot = by_kg(&shop, ("فخذ", "Gigot", "Leg"), 280_000, 0);
    let kebda = by_kg(&shop, ("كبدة", "", ""), 180_000, 0);
    let input = RunInput {
        template_id: None,
        source_product_id: carcass,
        location_id: None,
        input_qty: Qty(20_000),
        input_cost: None,
        method: CostMethod::Weight,
        cuts: vec![
            RunCut {
                product_id: gigot,
                qty: Qty(6_000),
            },
            RunCut {
                product_id: kebda,
                qty: Qty(1_000),
            },
        ],
        note: None,
    };

    let preview_input = input.clone();
    let preview = shop
        .db
        .read(move |c| yields::preview(c, &preview_input, Lang::Ar))
        .unwrap();
    assert_eq!(names(&preview.cuts), ["فخذ", "كبدة"]);
    let preview_input = input.clone();
    let preview = shop
        .db
        .read(move |c| yields::preview(c, &preview_input, Lang::En))
        .unwrap();
    assert_eq!(
        names(&preview.cuts),
        ["Leg", "كبدة"],
        "falls back on Arabic"
    );

    let ctx = shop.approved_till(Permission::StockAdjust);
    let run = shop
        .db
        .write(move |tx| yields::run(tx, &ctx, input, Lang::Ar))
        .unwrap();
    assert_eq!(names(&run.cuts), ["فخذ", "كبدة"]);

    for (lang, expected) in [
        (Lang::Fr, ["Gigot", "كبدة"]),
        (Lang::Ar, ["فخذ", "كبدة"]),
        (Lang::En, ["Leg", "كبدة"]),
    ] {
        let history = shop
            .db
            .read(move |c| yields::runs(c, "2026-09-28", "2026-09-28", lang))
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(
            names(&history[0].cuts),
            expected,
            "{lang:?}: localized, sorted by the localized name"
        );
    }
}
