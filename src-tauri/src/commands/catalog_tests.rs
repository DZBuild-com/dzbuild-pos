//! Catalogue, products and stock through the commands: guards, cost redaction, scan lookup.

use super::testshop::shop;
use super::{catalog, products, stock};
use pos_core::catalog::categories::CategoryInput;
use pos_core::catalog::search::SearchQuery;
use pos_core::money::{Money, Qty};
use pos_core::stock::{AdjustInput, AdjustReason};

fn category(name: &str) -> CategoryInput {
    CategoryInput {
        name_fr: name.into(),
        ..CategoryInput::default()
    }
}

#[test]
fn a_cashier_reads_the_catalogue_but_cannot_edit_it() {
    let shop = shop();
    // The first run seeded the activity template's categories (M8).
    let seeded = catalog::categories_list(&shop.st, false).unwrap().len();
    catalog::categories_create(&shop.st, category("Crèmerie")).unwrap();
    shop.cashier();
    assert_eq!(
        catalog::categories_list(&shop.st, false).unwrap().len(),
        seeded + 1
    );
    assert!(!catalog::units_list(&shop.st).unwrap().is_empty());
    assert!(!catalog::tax_rates_list(&shop.st).unwrap().is_empty());
    assert!(!catalog::price_levels_list(&shop.st).unwrap().is_empty());
    let refused = catalog::categories_create(&shop.st, category("Boissons")).unwrap_err();
    assert_eq!(refused.code(), "insufficient_permission");
    let level = catalog::price_levels_list(&shop.st).unwrap().remove(0);
    assert_eq!(
        catalog::price_levels_set_default(&shop.st, level.id)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn a_scan_finds_the_product_and_hides_its_cost_from_a_cashier() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, Some("6130000000017"));
    assert_eq!(
        products::get(&shop.st, milk.id.clone()).unwrap().cost_price,
        Money(9_000),
        "the owner sees the cost"
    );
    shop.cashier();
    let hit = products::find_by_code(&shop.st, "6130000000017".into())
        .unwrap()
        .unwrap();
    assert_eq!(hit.product.id, milk.id);
    assert_eq!(hit.pack_qty, Qty(1_000));
    assert_eq!(
        hit.product.cost_price,
        Money::ZERO,
        "no reports.profit, no cost"
    );
    assert_eq!(
        products::get(&shop.st, milk.id).unwrap().cost_price,
        Money::ZERO
    );
    assert!(
        products::find_by_code(&shop.st, "0000".into())
            .unwrap()
            .is_none()
    );
}

#[test]
fn search_and_grid_answer_the_till() {
    let shop = shop();
    // The first run seeded the General template's staples and department keys (M8).
    let seeded = products::grid(&shop.st, None, None).unwrap().len();
    shop.product("Lait Candia 1 L", 15_000, 900, None);
    shop.product("Huile Elio 5 L", 65_000, 1_900, None);
    shop.cashier();
    let hits = products::search(
        &shop.st,
        SearchQuery {
            text: "candia".into(),
            ..SearchQuery::default()
        },
    )
    .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name_fr, "Lait Candia 1 L");
    assert_eq!(
        products::grid(&shop.st, None, None).unwrap().len(),
        seeded + 2
    );
    assert_eq!(products::grid(&shop.st, None, Some(1)).unwrap().len(), 1);
}

#[test]
fn product_edits_need_products_edit_and_prices_need_products_price() {
    let shop = shop();
    let oil = shop.product("Huile Elio 5 L", 65_000, 1_900, None);
    shop.cashier();
    assert_eq!(
        products::archive(&shop.st, oil.id.clone())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    let levels = catalog::price_levels_list(&shop.st).unwrap();
    let wholesale = levels.iter().find(|l| !l.is_default).unwrap().id.clone();
    assert_eq!(
        products::set_price(&shop.st, oil.id.clone(), wholesale.clone(), Money(60_000))
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    shop.owner();
    let priced =
        products::set_price(&shop.st, oil.id.clone(), wholesale.clone(), Money(60_000)).unwrap();
    assert!(
        priced
            .prices
            .iter()
            .any(|p| p.price_level_id == wholesale && p.price == Money(60_000))
    );
    products::archive(&shop.st, oil.id.clone()).unwrap();
    assert!(
        products::search(
            &shop.st,
            SearchQuery {
                text: "elio".into(),
                ..SearchQuery::default()
            }
        )
        .unwrap()
        .is_empty()
    );
    products::restore(&shop.st, oil.id).unwrap();
}

#[test]
fn an_editor_who_cannot_see_costs_never_erases_them() {
    let shop = shop();
    let oil = shop.product("Huile Elio 5 L", 65_000, 1_900, None);
    let storekeeper = shop.user("Karim", pos_core::auth::STOREKEEPER_ROLE_ID, "5173", 0);
    shop.sign_in(&storekeeper, "5173");
    let seen = products::get(&shop.st, oil.id.clone()).unwrap();
    assert_eq!(seen.cost_price, Money::ZERO, "redacted for a storekeeper");
    let mut input = pos_core::catalog::products::ProductInput::simple(
        "Huile Elio 5 L (bidon)",
        &seen.unit_id,
        &seen.tax_rate_id,
        seen.sell_price,
    );
    input.cost_price = seen.cost_price;
    products::update(&shop.st, oil.id.clone(), input).unwrap();
    shop.owner();
    assert_eq!(
        products::get(&shop.st, oil.id).unwrap().cost_price,
        Money(39_000),
        "the stored cost is kept"
    );
}

#[test]
fn an_editor_without_products_price_cannot_reprice_through_an_update() {
    let shop = shop();
    let oil = shop.product("Huile Elio 5 L", 65_000, 1_900, None);
    let levels = catalog::price_levels_list(&shop.st).unwrap();
    let wholesale = levels.iter().find(|l| !l.is_default).unwrap().id.clone();
    products::set_price(&shop.st, oil.id.clone(), wholesale.clone(), Money(60_000)).unwrap();
    let wholesale_of = |p: &pos_core::catalog::products::Product| p.price_for(&wholesale);
    let storekeeper = shop.user("Karim", pos_core::auth::STOREKEEPER_ROLE_ID, "5173", 0);
    shop.sign_in(&storekeeper, "5173");
    let seen = products::get(&shop.st, oil.id.clone()).unwrap();
    let mut input = pos_core::catalog::products::ProductInput::simple(
        "Huile Elio 5 L (bidon)",
        &seen.unit_id,
        &seen.tax_rate_id,
        Money(50_000),
    );
    input.prices = vec![pos_core::catalog::products::LevelPrice {
        price_level_id: wholesale.clone(),
        price: Money(40_000),
    }];
    let answered = products::update(&shop.st, oil.id.clone(), input.clone()).unwrap();
    assert_eq!(
        answered.name_fr, "Huile Elio 5 L (bidon)",
        "the edit itself went through"
    );
    assert_eq!(answered.sell_price, Money(65_000));
    shop.owner();
    let stored = products::get(&shop.st, oil.id.clone()).unwrap();
    assert_eq!(stored.name_fr, "Huile Elio 5 L (bidon)");
    assert_eq!(stored.sell_price, Money(65_000), "the retail price is kept");
    assert_eq!(
        wholesale_of(&stored),
        Money(60_000),
        "the wholesale price is kept"
    );
    let repriced = products::update(&shop.st, oil.id, input).unwrap();
    assert_eq!(
        (repriced.sell_price, wholesale_of(&repriced)),
        (Money(50_000), Money(40_000)),
        "products.price may reprice through an update"
    );
}

#[test]
fn stock_moves_only_with_stock_adjust_and_costs_stay_hidden() {
    let shop = shop();
    let milk = shop.product("Lait Candia 1 L", 15_000, 900, None);
    let adjust = |pieces: i64, cost: Option<i64>| AdjustInput {
        product_id: milk.id.clone(),
        location_id: None,
        qty_delta: Qty(pieces * 1_000),
        reason: AdjustReason::Opening,
        unit_cost: cost.map(Money),
        note: None,
    };
    shop.cashier();
    assert_eq!(
        stock::adjust(&shop.st, adjust(10, Some(9_000)))
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    shop.owner();
    let movement = stock::adjust(&shop.st, adjust(10, Some(9_000))).unwrap();
    assert_eq!(movement.unit_cost, Some(Money(9_000)));
    let levels = stock::levels(&shop.st, milk.id.clone()).unwrap();
    assert_eq!(levels.iter().map(|l| l.qty.0).sum::<i64>(), 10_000);
    shop.cashier();
    let seen = stock::movements(&shop.st, milk.id, 10).unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].unit_cost, None,
        "a cashier does not see purchase costs"
    );
    assert_eq!(
        stock::rebuild_levels(&shop.st).unwrap_err().code(),
        "insufficient_permission"
    );
}
