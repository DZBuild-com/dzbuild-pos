use super::templates::{PackRelease, TEMPLATES, template};
use super::*;
use crate::company::ShopActivity;
use crate::lang::Lang;
use crate::testkit::shop;
use std::collections::BTreeSet;

fn count(shop: &crate::testkit::Shop, sql: &'static str) -> i64 {
    shop.count(sql)
}

#[test]
fn every_activity_has_one_template_with_names_in_three_languages() {
    assert_eq!(TEMPLATES.len(), ShopActivity::ALL.len());
    let activities: BTreeSet<&str> = TEMPLATES.iter().map(|t| t.activity.as_str()).collect();
    assert_eq!(activities.len(), 25);
    let mut codes = BTreeSet::new();
    for t in &TEMPLATES {
        assert!(
            !t.categories.is_empty(),
            "{:?} seeds categories",
            t.activity
        );
        assert!(t.modules.contains(&Module::Staff) && t.modules.contains(&Module::Zakat));
        for name in t
            .categories
            .iter()
            .chain(t.services.iter().map(|s| &s.name))
        {
            assert!(
                !name.ar.is_empty() && !name.fr.is_empty() && !name.en.is_empty(),
                "{name:?}"
            );
            assert!(
                name.ar
                    .chars()
                    .any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
                "{name:?} is Arabic"
            );
        }
        for s in t.services {
            assert!(s.code.starts_with("SVC-"), "{}", s.code);
            codes.insert(s.code);
        }
    }
    assert_eq!(
        template(ShopActivity::FoodService).till_mode,
        TillMode::Restaurant
    );
    assert_eq!(
        template(ShopActivity::Wholesale).till_mode,
        TillMode::Wholesale
    );
    assert_eq!(template(ShopActivity::Pharmacy).release, PackRelease::V2);
    assert!(
        template(ShopActivity::General)
            .modules
            .contains(&Module::RegulatedPrices)
    );
    assert!(
        template(ShopActivity::Phones)
            .modules
            .contains(&Module::Serials)
    );
}

#[test]
fn a_shop_without_choices_follows_its_template() {
    let shop = shop();
    let state = shop.db.read(state).unwrap();
    assert_eq!(state.activity, ShopActivity::General);
    assert_eq!(
        state.modules,
        template(ShopActivity::General).modules.to_vec()
    );
    assert_eq!(state.till_mode, TillMode::Retail);
    assert!(!state.customised);
}

#[test]
fn modules_switch_on_and_off_and_gate_their_features() {
    let shop = shop();
    let actor = shop.owner.actor();
    let after = shop
        .db
        .write(move |tx| set_modules(tx, &actor, &[Module::Restaurant, Module::Staff]))
        .unwrap();
    assert!(after.customised);
    assert!(shop.db.read(|c| enabled(c, Module::Restaurant)).unwrap());
    let err = shop.db.read(|c| require(c, Module::Loyalty)).unwrap_err();
    assert_eq!(err.code(), "module_disabled");
    assert_eq!(err.payload().details.unwrap()["module"], "loyalty");
    let actor = shop.owner.actor();
    let back = shop.db.write(move |tx| reset_modules(tx, &actor)).unwrap();
    assert_eq!(
        back.modules,
        template(ShopActivity::General).modules.to_vec()
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'packs.modules_set'"
        ),
        2
    );
}

#[test]
fn applying_a_template_seeds_once_and_journals_once() {
    let shop = shop();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    let first = shop
        .db
        .write(move |tx| {
            apply_template(tx, &actor, &pack, ShopActivity::FoodService, Lang::Fr, true)
        })
        .unwrap();
    assert_eq!(first.categories, 6);
    assert_eq!(
        first.extra,
        9 + 4 + 2,
        "the room with its 8 tables, the 3 modifier groups, the cheese supplement, the kitchen and the bar"
    );
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    let again = shop
        .db
        .write(move |tx| {
            apply_template(tx, &actor, &pack, ShopActivity::FoodService, Lang::Fr, true)
        })
        .unwrap();
    assert_eq!(
        (again.categories, again.services, again.extra),
        (0, 0, 0),
        "nothing twice"
    );
    assert_eq!(count(&shop, "SELECT COUNT(*) FROM dining_tables"), 8);
    assert_eq!(count(&shop, "SELECT COUNT(*) FROM categories"), 6);
    assert_eq!(
        count(&shop, "SELECT COUNT(*) FROM category_stations"),
        6,
        "every seeded category goes to the kitchen or the bar"
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'packs.template_applied'"
        ),
        2
    );
    let state = shop.db.read(state).unwrap();
    assert_eq!(
        (state.activity, state.till_mode),
        (ShopActivity::FoodService, TillMode::Restaurant)
    );
    assert!(
        !state.customised,
        "a template resets the owner's module choices"
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM categories WHERE name_ar = 'مشروبات ساخنة' AND name_en = 'Hot drinks'"
        ),
        1
    );
}

#[test]
fn seeded_services_are_services_and_open_price_where_asked() {
    let shop = shop();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| apply_template(tx, &actor, &pack, ShopActivity::Phones, Lang::Fr, true))
        .unwrap();
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM products WHERE reference = 'SVC-REPAIR' AND kind = 'service' AND track_stock = 0"
        ),
        1
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM open_price_products o JOIN products p ON p.id = o.product_id WHERE p.reference = 'SVC-REPAIR'"
        ),
        1
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM open_price_products o JOIN products p ON p.id = o.product_id WHERE p.reference = 'SVC-DIAG'"
        ),
        0,
        "a diagnosis has its price"
    );
}

/// Final review: the restaurant template applied in Arabic seeds the counter's buttons and the
/// kitchen tickets' language in Arabic (no French "Comptoir" on an Arabic floor); what the owner
/// set stays when a template is applied again.
#[test]
fn the_restaurant_template_seeds_its_counter_and_kitchen_in_the_shops_language() {
    let shop = shop();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| {
            apply_template(
                tx,
                &actor,
                &pack,
                ShopActivity::FoodService,
                Lang::Ar,
                false,
            )
        })
        .unwrap();
    let read = |key: &'static str| {
        shop.db
            .read(move |c| crate::settings::get_value(c, key))
            .unwrap()
    };
    assert_eq!(
        read("restaurant.predefined_tickets"),
        serde_json::json!(["الكونتوار", "التراس"])
    );
    assert_eq!(read("restaurant.kitchen_lang"), serde_json::json!("ar"));
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            crate::settings::set_value(
                tx,
                &actor,
                "restaurant.kitchen_lang",
                serde_json::json!("fr"),
            )
            .map(drop)
        })
        .unwrap();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| {
            apply_template(
                tx,
                &actor,
                &pack,
                ShopActivity::FoodService,
                Lang::En,
                false,
            )
        })
        .unwrap();
    assert_eq!(
        read("restaurant.kitchen_lang"),
        serde_json::json!("fr"),
        "the owner's choice stays"
    );
}

#[test]
fn a_template_without_seeds_changes_only_the_switches() {
    let shop = shop();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    let report = shop
        .db
        .write(move |tx| apply_template(tx, &actor, &pack, ShopActivity::Gym, Lang::Fr, false))
        .unwrap();
    assert_eq!(
        (report.categories, report.services, report.extra),
        (0, 0, 0)
    );
    assert_eq!(count(&shop, "SELECT COUNT(*) FROM categories"), 0);
}

#[test]
fn the_receipt_footer_is_seeded_only_when_empty() {
    let shop = shop();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| apply_template(tx, &actor, &pack, ShopActivity::Clothing, Lang::Fr, true))
        .unwrap();
    let company = shop.db.read(crate::company::get).unwrap().unwrap();
    assert_eq!(
        company.receipt_footer_fr.as_deref(),
        Some("Échange sous 7 jours avec ce ticket")
    );
    assert_eq!(
        company.receipt_footer_ar.as_deref(),
        Some("الاستبدال خلال 7 أيام بهذه التذكرة")
    );
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| apply_template(tx, &actor, &pack, ShopActivity::Pharmacy, Lang::Fr, true))
        .unwrap();
    let company = shop.db.read(crate::company::get).unwrap().unwrap();
    assert_eq!(
        company.receipt_footer_fr.as_deref(),
        Some("Échange sous 7 jours avec ce ticket"),
        "the owner's (here the first template's) text stays"
    );
}

#[test]
fn a_till_mode_is_chosen_apart_from_the_template() {
    let shop = shop();
    let actor = shop.owner.actor();
    let state = shop
        .db
        .write(move |tx| set_till_mode(tx, &actor, TillMode::Mixed))
        .unwrap();
    assert_eq!(state.till_mode, TillMode::Mixed);
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'packs.till_mode_set'"
        ),
        1
    );
}

#[test]
fn the_superette_template_seeds_its_staples_and_department_keys_once() {
    let shop = shop();
    let apply = || {
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        shop.db
            .write(move |tx| {
                apply_template(tx, &actor, &pack, ShopActivity::General, Lang::Fr, true)
            })
            .unwrap()
    };
    assert_eq!(
        apply().extra,
        14 + 3,
        "the 14 regulated staples and 3 department keys"
    );
    assert_eq!(apply().extra, 0, "nothing twice");
    assert_eq!(count(&shop, "SELECT COUNT(*) FROM product_regulations"), 14);
    assert_eq!(count(&shop, "SELECT COUNT(*) FROM department_keys"), 3);
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    let kiosk = shop
        .db
        .write(move |tx| apply_template(tx, &actor, &pack, ShopActivity::Kiosk, Lang::Fr, true))
        .unwrap();
    assert_eq!(
        kiosk.extra, 3,
        "its Divers key exists and the staples are the superette's: only the 3 top-up operators"
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM open_price_products o JOIN products p ON p.id = o.product_id
             WHERE p.reference LIKE 'SVC-FLEXY-%' AND p.kind = 'service' AND p.track_stock = 0"
        ),
        3
    );
}

/// ONB-008 (Task 31): a module the owner turns on in the settings works at once. A supérette
/// turning flexy on had no top-up product (`flexy_operator_unknown` at the float purchase and the
/// till): the modules turned on get the module's own seeds their template gives, once; turned off
/// and on again, nothing is created twice, and a module that stays on is not seeded again.
#[test]
fn a_module_turned_on_by_hand_gets_its_own_seeds_once() {
    let shop = shop();
    let switch = |modules: &'static [Module]| {
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        shop.db
            .write(move |tx| switch_modules(tx, &actor, &pack, modules))
            .unwrap()
    };
    let on = switch(&[
        Module::Flexy,
        Module::Instalments,
        Module::Restaurant,
        Module::Staff,
    ]);
    assert!(on.customised);
    let pack = shop.pack.clone();
    let operators = shop
        .db
        .read(move |c| crate::flexy::operators(c, &pack))
        .unwrap();
    assert!(
        !operators.is_empty() && operators.iter().all(|o| o.product_id.is_some()),
        "every operator has its top-up product: {operators:?}"
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM price_levels WHERE code = 'instalment'"
        ),
        1
    );
    assert_eq!(count(&shop, "SELECT COUNT(*) FROM dining_tables"), 8);
    switch(&[Module::Staff]);
    switch(&[Module::Flexy, Module::Instalments, Module::Restaurant]);
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM products WHERE reference LIKE 'SVC-FLEXY-%'"
        ),
        i64::try_from(operators.len()).unwrap(),
        "nothing twice"
    );
    assert_eq!(count(&shop, "SELECT COUNT(*) FROM dining_tables"), 8);
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'flexy.operators_seeded'"
        ),
        1
    );
    assert_eq!(
        count(
            &shop,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'packs.modules_set'"
        ),
        3
    );
}
