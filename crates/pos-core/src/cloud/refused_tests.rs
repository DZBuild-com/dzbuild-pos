//! Task 11 carry-over: a DZBuild item this PC refuses (a rule of the catalogue, a check of the
//! order mirror) no longer goes in silence. The feed still moves on without it, and one notice
//! names the item and carries the refusal's code, so the owner reads which one and why in
//! Settings › DZBuild. A dated pull that is sent the same refused item again (its
//! `updated_since` is inclusive) does not count it again; an edit online that is refused again
//! does.

use super::api::Remote;
use super::http::{ManualClock, Page};
use super::payloads::ProductSnapshot;
use super::pull::{EventItem, PullContext, apply_events, pull_orders};
use super::store::{
    self, CloudLink, CloudNotice, CloudNoticeKind, CloudSettings, MapEntity, cloud_settings,
};
use super::testkit::{MockDz, linked_shop, ok, remote_for_test};
use crate::money::Money;
use crate::testkit::{NOON, Shop};
use serde_json::{Value, json};
use wiremock::matchers::path;
use wiremock::{Mock, ResponseTemplate};

fn pull<'a>(
    shop: &'a Shop,
    link: &'a CloudLink,
    remote: &'a Remote,
    settings: &'a CloudSettings,
) -> PullContext<'a> {
    PullContext {
        remote,
        db: &shop.db,
        link,
        settings,
        actor: shop.owner.actor(),
        now: NOON,
    }
}

fn events(items: Value, next: &str) -> Page<EventItem> {
    // `data` is kept as raw JSON text: parse text, as the transport does
    serde_json::from_str(
        &json!({ "items": items, "next_cursor": next, "has_more": false }).to_string(),
    )
    .unwrap()
}

fn open_notices(shop: &Shop) -> Vec<CloudNotice> {
    let mut notices = shop.db.read(|c| store::notices(c, false)).unwrap();
    notices.sort_by(|a, b| a.code.cmp(&b.code));
    notices
}

/// `(kind, code, subject, details, count)` of a notice.
fn summary(n: &CloudNotice) -> (CloudNoticeKind, &str, &str, &Value, i64) {
    (
        n.kind,
        n.code.as_str(),
        n.subject.as_str(),
        &n.details,
        n.count,
    )
}

fn order(id: i64, shipping: i64, updated: &str) -> Value {
    json!({ "id": id, "order_number": id, "status": "pending", "subtotal": 100, "shipping_cost": shipping,
            "total": 100, "created_at": "2026-09-28 10:00:00", "updated_at": updated, "items": [] })
}

#[test]
fn a_refused_event_names_its_item_and_the_reason_in_one_notice() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let bag = shop.product("Sac", Money(450_000), &shop.tva19);
    // the product as the POS last pushed it
    let snapshot = serde_json::to_value(ProductSnapshot {
        name: "Sac".into(),
        price: Money(450_000),
        status: "active".into(),
    })
    .unwrap();
    let id = bag.id.clone();
    shop.db
        .write(move |tx| {
            store::map_put(
                tx,
                "141",
                MapEntity::Product,
                &id,
                "88121",
                Some(&snapshot),
                None,
                NOON,
            )
        })
        .unwrap();
    // an online name longer than the catalogue takes (120 characters)
    let long = "حقيبة جلد ".repeat(20);
    dz.mount(Mock::given(path("/v1/products/88121")).respond_with(
        ResponseTemplate::new(200).set_body_json(json!({ "data": {
                "id": 88121, "external_id": bag.id, "name": long, "pricing": { "price": 4_700 },
                "inventory": { "track_stock": true, "stock_quantity": 10 }, "status": "active",
                "has_variants": false } })),
    ));
    // an order with a negative delivery fee (the mirror's CHECK refuses it)
    dz.mount(Mock::given(path("/v1/orders/9001")).respond_with(ok(order(
        9001,
        -1,
        "2026-09-28 10:00:00",
    ))));
    // a stock move for a shop with no default location to put it in
    shop.db
        .write(|tx| {
            tx.execute("UPDATE stock_locations SET is_default = 0", [])?;
            Ok(())
        })
        .unwrap();
    let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
    let settings = shop.db.read(cloud_settings).unwrap();
    let page = events(
        json!([
            { "id": "evt_a", "type": "product.updated", "data": { "id": 88121, "external_id": bag.id } },
            { "id": "evt_b", "type": "order.created", "data": { "id": 9001 } },
            { "id": "evt_c", "type": "inventory.level_changed",
              "data": { "external_id": bag.id, "delta": 2, "source": "dashboard" } }
        ]),
        "c5",
    );
    let report = apply_events(&pull(&shop, &link, &remote, &settings), &page).unwrap();
    assert_eq!(report.applied, 0, "all three are refused here");
    // the name as DZBuild shows it, cut to 60 characters
    let cut: String = long.trim().chars().take(60).collect();
    let shown = format!("{}…", cut.trim_end());
    let notices = open_notices(&shop);
    assert_eq!(
        notices.iter().map(summary).collect::<Vec<_>>(),
        [
            (
                CloudNoticeKind::FailedItems,
                "cloud_event_refused",
                "inventory.level_changed",
                &json!({ "reason": "stock_location_not_found" }),
                1
            ),
            (
                CloudNoticeKind::FailedItems,
                "cloud_order_refused",
                "9001",
                &json!({ "reason": "constraint_violation" }),
                1
            ),
            (
                CloudNoticeKind::FailedItems,
                "cloud_product_refused",
                shown.as_str(),
                &json!({ "reason": "too_long" }),
                1
            ),
        ],
        "one notice per refused item, with what it is and why"
    );
    let failed = shop.count("SELECT COUNT(*) FROM cloud_inbox WHERE outcome = 'failed'");
    assert_eq!(failed, 3, "each one is still recorded failed in the inbox");
    assert_eq!(
        shop.db
            .read(|c| store::remote_cursor(c, "cloud:events"))
            .unwrap()
            .as_deref(),
        Some("c5"),
        "and the feed moved on"
    );
}

#[test]
fn a_refused_order_sent_again_unchanged_is_not_counted_again() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    let first = ok(json!({
        "items": [order(9001, -1, "2026-09-28 11:30:00"), order(9002, 0, "2026-09-28 11:00:00")],
        "next_cursor": null, "has_more": false
    }));
    dz.mount(
        Mock::given(path("/v1/orders"))
            .respond_with(first)
            .up_to_n_times(2),
    );
    // edited online, still with a negative fee
    dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(json!({
        "items": [order(9001, -2, "2026-09-28 12:00:00")],
        "next_cursor": null, "has_more": false
    }))));
    let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
    let settings = shop.db.read(cloud_settings).unwrap();
    let ctx = pull(&shop, &link, &remote, &settings);
    let refused = |count: i64| {
        vec![(
            CloudNoticeKind::FailedItems,
            "cloud_order_refused".to_owned(),
            "9001".to_owned(),
            json!({ "reason": "constraint_violation" }),
            count,
        )]
    };
    let seen = || {
        open_notices(&shop)
            .into_iter()
            .map(|n| (n.kind, n.code, n.subject, n.details, n.count))
            .collect::<Vec<_>>()
    };
    assert_eq!(pull_orders(&ctx).unwrap().orders, 1, "9002 comes in");
    assert_eq!(seen(), refused(1));
    pull_orders(&ctx).unwrap();
    assert_eq!(
        seen(),
        refused(1),
        "the same refused order, sent again unchanged, is not counted again"
    );
    pull_orders(&ctx).unwrap();
    assert_eq!(seen(), refused(2), "an edit online refused again is");
}

/// The detail of an order the store's plan refuses (403 `app_plan_required`) or DZBuild refuses
/// (404 with a code): each event is recorded failed with a notice, never held forever or
/// `ignored` in silence, and the feed moves on.
#[test]
fn a_refused_or_plan_capped_detail_is_recorded_with_a_notice() {
    let dz = MockDz::start();
    let (shop, link) = linked_shop();
    dz.mount(
        Mock::given(path("/v1/orders/9001")).respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({ "error": { "code": "app_plan_required" } })),
        ),
    );
    dz.mount(Mock::given(path("/v1/orders/9002")).respond_with(
        ResponseTemplate::new(404).set_body_json(json!({ "error": { "code": "order_not_found" } })),
    ));
    let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
    let settings = shop.db.read(cloud_settings).unwrap();
    let page = events(
        json!([
            { "id": "evt_p", "type": "order.created", "data": { "id": 9001 } },
            { "id": "evt_r", "type": "order.updated", "data": { "id": 9002 } }
        ]),
        "c9",
    );
    let report = apply_events(&pull(&shop, &link, &remote, &settings), &page).unwrap();
    assert_eq!(report.applied, 0);
    let notices = open_notices(&shop);
    assert_eq!(
        notices.iter().map(summary).collect::<Vec<_>>(),
        [
            (
                CloudNoticeKind::FailedItems,
                "cloud_order_refused",
                "9002",
                &json!({ "reason": "order_not_found", "type": "order.updated" }),
                1
            ),
            (
                CloudNoticeKind::PlanCap,
                "cloud_plan_cap",
                "",
                &json!({ "code": "app_plan_required", "type": "order.created" }),
                1
            ),
        ]
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM cloud_inbox WHERE outcome = 'failed'"),
        2,
        "recorded failed, never ignored"
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM online_orders"), 0);
    assert_eq!(
        shop.db
            .read(|c| store::remote_cursor(c, "cloud:events"))
            .unwrap()
            .as_deref(),
        Some("c9"),
        "the feed moved on"
    );
}
