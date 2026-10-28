//! A variant family goes online as one product with its `variants` block (wire contract v1.12):
//! sent once, sent again when a child changes, its children mapped by key from DZBuild's answer;
//! each child's stock travels as a `target: "variant"` item under its parent; an online order
//! line names the child it sold.

use super::derive::derive;
use super::http::ManualClock;
use super::orders::{RemoteOrder, claim, get, sell, upsert_remote};
use super::push::{PushContext, PushReport, push_due};
use super::store::{self, CloudLink, CloudNoticeKind, MapEntity, cloud_settings};
use super::testkit::{MockDz, linked_shop, ok, publish, remote_for_test};
use super::variants::{self, VariantMapRow, variant_key};
use crate::catalog::products::{ProductInput, ProductKind};
use crate::catalog::variants::{VariantAxis, VariantsInput, generate};
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::{NOON, Shop};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wiremock::matchers::path;
use wiremock::{Mock, Request, Respond, ResponseTemplate};

/// A T-shirt family made at the till: sizes S and M by colours Noir and Blanc, 1 500 DA, the
/// children with internal barcodes. Returns the parent id and the children in creation order.
fn family(shop: &Shop) -> (String, Vec<crate::catalog::products::Product>) {
    let mut input = ProductInput::simple(
        "T-shirt",
        crate::catalog::UNIT_PIECE_ID,
        &shop.tva19,
        Money(150_000),
    );
    input.kind = ProductKind::VariantParent;
    input.track_stock = false;
    input.reference = Some("TS01".into());
    let parent = shop.product_with(input).id;
    let (actor, id) = (shop.owner.actor(), parent.clone());
    let children = shop
        .db
        .write(move |tx| {
            generate(
                tx,
                &actor,
                &id,
                VariantsInput {
                    axes: vec![
                        VariantAxis {
                            name: "Taille".into(),
                            values: vec!["S".into(), "M".into()],
                        },
                        VariantAxis {
                            name: "Couleur".into(),
                            values: vec!["Noir".into(), "Blanc".into()],
                        },
                    ],
                    sell_price: None,
                    cost_price: None,
                    internal_barcodes: true,
                },
            )
        })
        .unwrap();
    (parent, children)
}

fn derive_now(shop: &Shop, link: &CloudLink) -> super::derive::DeriveReport {
    let link = link.clone();
    shop.db
        .write(move |tx| {
            let settings = cloud_settings(tx)?;
            derive(tx, &link, &settings, NOON)
        })
        .unwrap()
}

fn push(shop: &Shop, link: &CloudLink, dz: &MockDz, now: i64) -> PushReport {
    let remote = remote_for_test(dz, link, ManualClock::at(NOON));
    let settings = shop.db.read(cloud_settings).unwrap();
    push_due(&PushContext {
        remote: &remote,
        db: &shop.db,
        link,
        settings: &settings,
        media_dir: None,
        now,
    })
    .unwrap()
}

/// DZBuild v1.12 answering a products batch: every item `updated` as product 88121, each of its
/// variants `ok` under the key of its attributes, online at the price sent.
struct FamilyAccepted;

impl Respond for FamilyAccepted {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let answers: Vec<Value> = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                let variants: Vec<Value> = item["variants"]["items"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|v| {
                        let attributes: BTreeMap<String, String> =
                            serde_json::from_value(v["attributes"].clone()).unwrap();
                        json!({ "external_id": v["external_id"], "key": variant_key(&attributes),
                                "status": "ok", "price_online": v["price"], "error": null })
                    })
                    .collect();
                json!({ "external_id": item["external_id"], "id": 88121, "status": "updated",
                        "error": null, "variants": variants })
            })
            .collect();
        ok(json!({ "items": answers }))
    }
}

fn product_bodies(dz: &MockDz) -> Vec<Value> {
    dz.requests()
        .iter()
        .filter(|r| r.url.path() == "/v1/products/batch")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

fn stock_bodies(dz: &MockDz) -> Vec<Value> {
    dz.requests()
        .iter()
        .filter(|r| r.url.path() == "/v1/inventory/adjustments/batch")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

fn unit_barcode(p: &crate::catalog::products::Product) -> String {
    p.barcodes[0].barcode.clone()
}

fn mapped_children(shop: &Shop, parent: &str) -> Vec<VariantMapRow> {
    let parent = parent.to_owned();
    shop.db
        .read(move |c| variants::map_children(c, "141", &parent))
        .unwrap()
}

fn set_price(shop: &Shop, product_id: &str, price: Money) {
    let (actor, id) = (shop.owner.actor(), product_id.to_owned());
    shop.db
        .write(move |tx| {
            let level = crate::catalog::price_levels::default_level(tx)?;
            crate::catalog::products::set_price(tx, &actor, &id, &level.id, price).map(drop)
        })
        .unwrap();
}

#[test]
fn a_published_family_is_sent_once_with_its_variants_and_again_when_a_child_changes() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let (parent, children) = family(&shop);
    publish(&shop, &parent);
    let report = derive_now(&shop, &link);
    assert_eq!(
        report.products, 1,
        "the parent only, its children travel inside it"
    );
    dz.mount(Mock::given(path("/v1/products/batch")).respond_with(FamilyAccepted));
    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch"))
            .respond_with(ok(json!({ "items": [] }))),
    );
    assert_eq!(
        push(&shop, &link, &dz, NOON).sent,
        2,
        "the family and its stock"
    );
    let bodies = product_bodies(&dz);
    assert_eq!(bodies.len(), 1);
    let item = |p: &crate::catalog::products::Product, colour: &str, size: &str| {
        json!({ "external_id": p.id, "attributes": { "Couleur": colour, "Taille": size },
                "sku": p.reference, "barcode": unit_barcode(p), "price": "1500.00", "track_stock": true })
    };
    assert_eq!(
        bodies[0]["items"][0]["variants"],
        json!({
            "axes": [
                { "name": "Couleur", "values": ["Noir", "Blanc"] },
                { "name": "Taille", "values": ["S", "M"] }
            ],
            "items": [
                item(&children[0], "Noir", "S"),
                item(&children[1], "Blanc", "S"),
                item(&children[2], "Noir", "M"),
                item(&children[3], "Blanc", "M")
            ]
        })
    );
    assert_eq!(bodies[0]["items"][0]["external_id"], parent);
    assert_eq!(bodies[0]["items"][0]["sku"], "TS01");
    assert!(
        bodies[0]["items"][0].get("id").is_none(),
        "never mapped: no online product to name"
    );
    let mapped = mapped_children(&shop, &parent);
    assert_eq!(mapped.len(), 4);
    assert!(mapped.iter().all(|r| r.remote_product_id == "88121"
        && r.parent_id == parent
        && r.price_online == Some(Money(150_000))));
    assert_eq!(
        shop.db
            .read(|c| variants::map_local(c, "141", "88121", "couleur=blanc|taille=m"))
            .unwrap()
            .as_deref(),
        Some(children[3].id.as_str())
    );

    // every product of the family touched, nothing changed: the family is not sent again
    shop.db
        .write(|tx| {
            tx.execute("UPDATE products SET updated_at = updated_at + 1", [])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        derive_now(&shop, &link).products,
        1,
        "one row for the family"
    );
    let again = push(&shop, &link, &dz, NOON + 1);
    assert_eq!((again.sent, again.unchanged), (0, 1));
    assert_eq!(product_bodies(&dz).len(), 1);

    // a child's price: the parent goes again with it
    set_price(&shop, &children[2].id, Money(165_000));
    derive_now(&shop, &link);
    assert_eq!(push(&shop, &link, &dz, NOON + 2).sent, 1);
    let bodies = product_bodies(&dz);
    assert_eq!(bodies.len(), 2);
    assert_eq!(
        bodies[1]["items"][0]["id"], "88121",
        "the product it answered"
    );
    assert_eq!(
        bodies[1]["items"][0]["variants"]["items"][2]["price"],
        "1650.00"
    );
    assert_eq!(
        bodies[1]["items"][0]["variants"]["items"][0]["price"],
        "1500.00"
    );
    let id = children[2].id.clone();
    let row = shop
        .db
        .read(move |c| variants::map_get(c, "141", &id))
        .unwrap();
    assert_eq!(row.and_then(|r| r.price_online), Some(Money(165_000)));
}

#[test]
fn an_imported_family_names_its_online_product_on_its_first_push() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let (parent, _) = family(&shop);
    let p = parent.clone();
    shop.db
        .write(move |tx| {
            store::map_put(tx, "141", MapEntity::Product, &p, "70412", None, None, NOON)
        })
        .unwrap();
    publish(&shop, &parent);
    derive_now(&shop, &link);
    dz.mount(Mock::given(path("/v1/products/batch")).respond_with(FamilyAccepted));
    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch"))
            .respond_with(ok(json!({ "items": [] }))),
    );
    push(&shop, &link, &dz, NOON);
    let bodies = product_bodies(&dz);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["items"][0]["external_id"], parent);
    assert_eq!(bodies[0]["items"][0]["id"], "70412");
}

#[test]
fn a_map_row_without_a_remote_id_names_no_online_product() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let milk = shop.product("Lait", Money(10_000), &shop.tva19);
    let id = milk.id.clone();
    shop.db
        .write(move |tx| store::map_put(tx, "141", MapEntity::Product, &id, "", None, None, NOON))
        .unwrap();
    publish(&shop, &milk.id);
    derive_now(&shop, &link);
    dz.mount(Mock::given(path("/v1/products/batch")).respond_with(FamilyAccepted));
    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch"))
            .respond_with(ok(json!({ "items": [] }))),
    );
    push(&shop, &link, &dz, NOON);
    assert!(product_bodies(&dz)[0]["items"][0].get("id").is_none());
}

#[test]
fn a_childs_new_size_resends_its_family_once() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let (parent, children) = family(&shop);
    publish(&shop, &parent);
    derive_now(&shop, &link);
    dz.mount(Mock::given(path("/v1/products/batch")).respond_with(FamilyAccepted));
    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch"))
            .respond_with(ok(json!({ "items": [] }))),
    );
    push(&shop, &link, &dz, NOON);

    let (actor, id) = (shop.owner.actor(), children[3].id.clone());
    shop.db
        .write(move |tx| {
            let mut input = crate::catalog::extras::ProductExtrasInput::from(
                &crate::catalog::extras::get(tx, &id)?,
            );
            input.attributes.insert("Taille".into(), "L".into());
            crate::catalog::extras::set(tx, &actor, &id, input).map(drop)
        })
        .unwrap();
    assert_eq!(derive_now(&shop, &link).products, 1);
    let p = parent.clone();
    let queued: i64 = shop
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM sync_outbox
                 WHERE kind = 'product_upsert' AND status = 'pending' AND entity_id = ?1",
                [&p],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(queued, 1, "one row for the family");
    assert_eq!(push(&shop, &link, &dz, NOON + 1).sent, 1);
    let bodies = product_bodies(&dz);
    assert_eq!(bodies.len(), 2);
    assert_eq!(
        bodies[1]["items"][0]["variants"]["items"][3]["attributes"],
        json!({ "Couleur": "Blanc", "Taille": "L" })
    );
}

#[test]
fn a_family_without_live_children_stays_home_and_an_archived_child_leaves_the_block() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let mut input = ProductInput::simple(
        "Robe",
        crate::catalog::UNIT_PIECE_ID,
        &shop.tva19,
        Money(300_000),
    );
    input.kind = ProductKind::VariantParent;
    input.track_stock = false;
    let empty = shop.product_with(input).id;
    publish(&shop, &empty);
    assert_eq!(
        derive_now(&shop, &link).products,
        0,
        "no child, nothing to sell online"
    );

    let (parent, children) = family(&shop);
    publish(&shop, &parent);
    derive_now(&shop, &link);
    dz.mount(Mock::given(path("/v1/products/batch")).respond_with(FamilyAccepted));
    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch"))
            .respond_with(ok(json!({ "items": [] }))),
    );
    push(&shop, &link, &dz, NOON);
    let (actor, gone) = (shop.owner.actor(), children[0].id.clone());
    shop.db
        .write(move |tx| crate::catalog::products::archive(tx, &actor, &gone))
        .unwrap();
    derive_now(&shop, &link);
    push(&shop, &link, &dz, NOON + 1);
    let bodies = product_bodies(&dz);
    let sent: Vec<&str> = bodies[1]["items"][0]["variants"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["external_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        sent,
        [
            children[1].id.as_str(),
            children[2].id.as_str(),
            children[3].id.as_str()
        ]
    );
    let mapped: Vec<String> = mapped_children(&shop, &parent)
        .into_iter()
        .map(|r| r.local_id)
        .collect();
    assert!(!mapped.contains(&children[0].id), "its key is gone online");
    assert_eq!(mapped.len(), 3);
}

#[test]
fn a_refused_variant_raises_one_notice_and_the_family_stays_mapped() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let (parent, children) = family(&shop);
    publish(&shop, &parent);
    derive_now(&shop, &link);
    dz.mount(
        Mock::given(path("/v1/products/batch"))
            .respond_with(FamilyAccepted)
            .up_to_n_times(1),
    );
    let refused = |c: &crate::catalog::products::Product| {
        json!({ "external_id": c.id, "key": null, "status": "error", "price_online": null,
                "error": { "code": "validation_error", "message": "invalid", "details": { "field": "variants.items.0.sku" } } })
    };
    dz.mount(
        Mock::given(path("/v1/products/batch")).respond_with(ok(json!({ "items": [{
        "external_id": parent, "error": { "code": "validation_error", "message": "invalid" },
        "variants": [refused(&children[0]), refused(&children[1])]
    }] }))),
    );
    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch"))
            .respond_with(ok(json!({ "items": [] }))),
    );
    push(&shop, &link, &dz, NOON);
    set_price(&shop, &children[0].id, Money(1));
    derive_now(&shop, &link);
    let report = push(&shop, &link, &dz, NOON + 1);
    assert_eq!(report.failed, 1);
    let notices: Vec<_> = shop
        .db
        .read(|c| store::notices(c, false))
        .unwrap()
        .into_iter()
        .filter(|n| n.code == "cloud_variant_refused")
        .collect();
    assert_eq!(notices.len(), 1, "one notice for the family");
    assert_eq!(
        (
            notices[0].kind,
            notices[0].subject.as_str(),
            notices[0].count
        ),
        (CloudNoticeKind::FailedItems, parent.as_str(), 1)
    );
    assert_eq!(
        notices[0].details,
        json!({ "product": "T-shirt", "count": 2, "reason": "validation_error" })
    );
    let p = parent.clone();
    assert!(
        shop.db
            .read(move |c| store::map_get(c, "141", MapEntity::Product, &p))
            .unwrap()
            .is_some()
    );
    assert_eq!(mapped_children(&shop, &parent).len(), 4);
}

fn sell_one(shop: &Shop, product_id: &str) {
    let till = shop.till();
    let input = SaleInput {
        cart: Cart::new(
            crate::lang::Lang::Fr,
            vec![CartLine::product(product_id, Qty(1_000))],
        ),
        payments: vec![PaymentInput::cash(Money(1_000_000))],
    };
    shop.db
        .write(move |tx| create_sale(tx, &till, input))
        .unwrap();
}

fn cloud_stock(shop: &Shop, product_id: &str) -> (Option<i64>, i64, bool) {
    let id = product_id.to_owned();
    shop.db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT confirmed, in_flight, reset_requested FROM cloud_stock WHERE product_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .unwrap()
}

#[test]
fn a_sale_of_a_child_moves_its_variant_online_and_a_refusal_releases_it() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let (parent, children) = family(&shop);
    let child = children[1].id.clone();
    publish(&shop, &parent);
    shop.stock_in(&child, 5_000);
    let report = derive_now(&shop, &link);
    assert_eq!(report.stock_items, 4, "a set for every child");
    dz.mount(Mock::given(path("/v1/products/batch")).respond_with(FamilyAccepted));
    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch"))
            .respond_with(ok(json!({ "items": [] })))
            .up_to_n_times(2),
    );
    push(&shop, &link, &dz, NOON);
    let first = &stock_bodies(&dz)[0]["items"];
    let set = first
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["variant_external_id"] == child.as_str())
        .unwrap();
    assert_eq!(
        (&set["product_external_id"], &set["target"], &set["set"]),
        (&json!(parent), &json!("variant"), &json!(5))
    );
    assert_eq!(cloud_stock(&shop, &child), (Some(5), 0, false));
    let p = parent.clone();
    let tracked: i64 = shop
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM cloud_stock WHERE product_id = ?1",
                [p],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(tracked, 0, "the parent holds no stock");

    shop.open_till(0);
    sell_one(&shop, &child);
    assert_eq!(derive_now(&shop, &link).stock_items, 1);
    push(&shop, &link, &dz, NOON + 1);
    let bodies = stock_bodies(&dz);
    let item = &bodies[1]["items"][0];
    assert_eq!(
        item,
        &json!({ "product_external_id": parent, "target": "variant", "variant_external_id": child,
                 "delta": -1, "reason": "pos_sale", "ref": item["ref"], "location_id": "loc_1" })
    );
    assert_eq!(
        cloud_stock(&shop, &child),
        (Some(4), 0, false),
        "the answer settles the child"
    );

    dz.mount(
        Mock::given(path("/v1/inventory/adjustments/batch")).respond_with(ok(json!({ "items": [
            { "ref": "r", "status": "error", "error": { "code": "variant_gone" } }
        ] }))),
    );
    sell_one(&shop, &child);
    derive_now(&shop, &link);
    push(&shop, &link, &dz, NOON + 2);
    assert_eq!(
        cloud_stock(&shop, &child),
        (Some(4), 0, true),
        "released, a set next time"
    );
}

fn order_with(item: Value) -> RemoteOrder {
    let value = json!({
        "id": 9001, "order_number": 1042, "status": "confirmed", "payment_method": "cod", "delivery_type": "pickup",
        "customer_name": "Samira B.", "customer_phone": "0550 12 34 56", "wilaya_id": 16, "commune": "Bab Ezzouar",
        "address": "Cité 5 juillet", "customer_notes": null,
        "subtotal": 1500, "shipping_cost": 0, "discount": 0, "total": 1500,
        "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
        "items": [item]
    });
    // amounts are read from the raw JSON text (ApiMoney)
    serde_json::from_str(&value.to_string()).unwrap()
}

/// The order mirrored and the product of its first line.
fn mirrored_line(shop: &Shop, link: &CloudLink, order: RemoteOrder) -> (String, Option<String>) {
    let l = link.clone();
    let id: String = shop
        .db
        .write(move |tx| {
            upsert_remote(tx, &l, &order, NOON)?;
            Ok(tx.query_row(
                "SELECT id FROM online_orders WHERE remote_id = '9001'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let (terminal, i) = (shop.terminal.id.clone(), id.clone());
    let order = shop.db.read(move |c| get(c, &i, &terminal)).unwrap();
    (id, order.lines[0].product_id.clone())
}

#[test]
fn an_order_line_naming_its_variant_is_the_child_and_selling_it_acknowledges_the_childs_stock() {
    let (shop, link) = linked_shop();
    let (parent, children) = family(&shop);
    let child = children[2].id.clone();
    shop.stock_in(&child, 5_000);
    shop.db
        .write({
            let child = child.clone();
            move |tx| {
                tx.execute(
                    "INSERT INTO cloud_stock (store_id, product_id, confirmed, updated_at) VALUES ('141', ?1, 5, 0)",
                    [child],
                )?;
                Ok(())
            }
        })
        .unwrap();
    let (id, product) = mirrored_line(
        &shop,
        &link,
        order_with(json!({
            "product_id": 88121, "external_id": parent, "product_name": "T-shirt", "variant_name": "M / Noir",
            "variant_key": "couleur=noir|taille=m", "variant_external_id": child,
            "price": 1500, "quantity": 1, "total": 1500
        })),
    );
    assert_eq!(product.as_deref(), Some(child.as_str()));
    shop.open_till(0);
    let (till, l, i) = (shop.till(), link.clone(), id.clone());
    shop.db
        .write(move |tx| {
            claim(tx, &till, &l, &i, true)?;
            sell(
                tx,
                &till,
                &l,
                &i,
                "Retrait".into(),
                vec![PaymentInput::cash(Money(1_000_000))],
            )
        })
        .unwrap();
    assert_eq!(
        cloud_stock(&shop, &child).0,
        Some(4),
        "DZBuild took the child's piece"
    );
}

#[test]
fn an_order_line_with_only_the_variant_key_resolves_through_the_map() {
    let (shop, link) = linked_shop();
    let (parent, children) = family(&shop);
    let child = children[3].id.clone();
    let (p, c) = (parent.clone(), child.clone());
    shop.db
        .write(move |tx| {
            store::map_put(tx, "141", MapEntity::Product, &p, "88121", None, None, NOON)?;
            variants::map_put(
                tx,
                "141",
                &VariantMapRow {
                    local_id: c,
                    parent_id: p,
                    remote_product_id: "88121".into(),
                    variant_key: "couleur=blanc|taille=m".into(),
                    price_online: None,
                    updated_at: NOON,
                },
            )
        })
        .unwrap();
    let (_, product) = mirrored_line(
        &shop,
        &link,
        order_with(json!({
            "product_id": 88121, "product_name": "T-shirt", "variant_name": "M / Blanc",
            "variant_key": "couleur=blanc|taille=m", "price": 1500, "quantity": 1, "total": 1500
        })),
    );
    assert_eq!(product.as_deref(), Some(child.as_str()));
}
