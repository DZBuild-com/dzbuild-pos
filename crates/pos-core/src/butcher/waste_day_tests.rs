//! The day's waste screen (Task 25): the rows of the day come with their cost total, summed in
//! Rust so the screen never adds money up.

use super::waste::{self, WasteInput, WasteLine, WasteReason};
use crate::auth::Permission;
use crate::catalog::UNIT_KG_ID;
use crate::catalog::products::ProductInput;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::testkit::shop;

#[test]
fn the_days_waste_comes_with_its_cost_total() {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Waste]))
        .unwrap();
    let mut input = ProductInput::simple("Tomates", UNIT_KG_ID, &shop.tva19, Money(12_000));
    input.is_weighed = true;
    input.cost_price = Money(8_000);
    let tomates = shop.product_with(input).id;
    let pain = shop.product("Baguette", Money(1_500), &shop.exempt).id;
    shop.stock_in(&tomates, 30_000);
    shop.stock_in(&pain, 50_000);
    let ctx = shop.approved_till(Permission::StockAdjust);
    let lines = vec![
        WasteLine {
            product_id: tomates.clone(),
            qty: Qty(2_500),
            reason: WasteReason::Damage,
        },
        WasteLine {
            product_id: pain.clone(),
            qty: Qty(12_000),
            reason: WasteReason::OwnUse,
        },
    ];
    shop.db
        .write(move |tx| {
            waste::record(
                tx,
                &ctx,
                WasteInput {
                    location_id: None,
                    lines,
                    note: None,
                },
            )
        })
        .unwrap();
    let day = shop
        .db
        .read(|c| waste::day_summary(c, "2026-09-28", Lang::Fr))
        .unwrap();
    assert_eq!(day.rows.len(), 2);
    let costs: Vec<Money> = day.rows.iter().map(|r| r.cost).collect();
    assert!(
        costs.contains(&Money(20_000)),
        "2,5 kg of tomatoes at 80 DA"
    );
    assert_eq!(day.total_cost, Money(costs.iter().map(|c| c.0).sum()));
    let empty = shop
        .db
        .read(|c| waste::day_summary(c, "2026-09-27", Lang::Fr))
        .unwrap();
    assert_eq!((empty.rows.len(), empty.total_cost), (0, Money::ZERO));
}
