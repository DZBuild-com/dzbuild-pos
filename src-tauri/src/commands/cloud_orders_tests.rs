//! Online orders through the commands, against the DZBuild stand-in.

use super::cloud;
use super::cloud_mock::{Dz, cloud_shop, ok, one_store, settled};
use super::cloud_orders::{self, OrderSaleRequest};
use super::testshop::{MANAGER_PIN, TestShop};
use pos_core::cloud::link::AttemptState;
use pos_core::cloud::orders::{OrderFilter, OrderQuery, OrderState, RefuseReason};
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::payments::PaymentInput;
use serde_json::json;
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Linked through the browser flow, the engine running on fast timings.
fn linked(dz: &Dz) -> TestShop {
    dz.mount_link(one_store());
    dz.mount_sync();
    let (shop, _) = cloud_shop(dz);
    shop.owner();
    cloud::link_start(&shop.st, Lang::Fr).unwrap();
    assert!(matches!(settled(&shop).state, AttemptState::Linked { .. }));
    let cloud_rt = shop.st.cloud().unwrap();
    cloud_rt
        .start(
            crate::cloud::EngineHooks::silent(),
            pos_core::cloud::engine::Timing::fast(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while cloud_rt.remote_now().is_none() {
        assert!(
            Instant::now() < deadline,
            "the engine never built its client"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    shop
}

fn order(shop: &TestShop, external_id: &str, price_dinars: i64) -> String {
    let link = shop
        .st
        .db
        .read(pos_core::cloud::store::active_link)
        .unwrap()
        .unwrap();
    let text = json!({
        "id": 9001, "order_number": 1042, "status": "confirmed", "payment_method": "cod", "delivery_type": "pickup",
        "customer_name": "Samira B.", "customer_phone": "0550123456", "wilaya_id": 16,
        "subtotal": price_dinars * 2, "total": price_dinars * 2,
        "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
        "items": [{ "product_id": 88121, "external_id": external_id, "product_name": "Lait",
                    "price": price_dinars, "quantity": 2, "total": price_dinars * 2 }]
    })
    .to_string();
    let remote: pos_core::cloud::orders::RemoteOrder = serde_json::from_str(&text).unwrap();
    shop.st
        .db
        .write(move |tx| {
            pos_core::cloud::orders::upsert_remote(tx, &link, &remote, pos_core::time::now_ms())?;
            Ok(tx.query_row(
                "SELECT id FROM online_orders WHERE remote_id = '9001'",
                [],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap()
}

#[test]
fn a_claim_asks_dzbuild_first_and_is_refused_when_another_device_has_it() {
    let dz = Dz::start();
    let shop = linked(&dz);
    let milk = shop.product("Lait", 10_000, 1_900, None);
    let id = order(&shop, &milk.id, 100);
    dz.mount(
        Mock::given(method("POST"))
            .and(path("/v1/orders/9001/claim"))
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                "error": { "code": "already_claimed", "message": "claimed by another device" }
            }))),
    );
    shop.cashier();
    assert_eq!(
        cloud_orders::claim(&shop.st, id.clone())
            .unwrap_err()
            .code(),
        "cloud_order_claimed_elsewhere"
    );
    let summary = cloud_orders::list(
        &shop.st,
        OrderQuery {
            filter: OrderFilter::Open,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!(
        summary[0].local_state,
        OrderState::New,
        "nothing changed locally"
    );
}

#[test]
fn offline_the_claim_is_queued_and_the_order_is_sold_at_the_online_price_with_an_approval() {
    let dz = Dz::start();
    let shop = linked(&dz);
    let milk = shop.product("Lait", 10_000, 1_900, None);
    shop.stock_in(&milk.id, 10);
    let id = order(&shop, &milk.id, 90);
    // DZBuild has not shipped the claim endpoint: the claim waits in the outbox
    dz.mount(
        Mock::given(method("POST"))
            .and(path("/v1/orders/9001/claim"))
            .respond_with(ResponseTemplate::new(501)),
    );
    shop.cashier();
    let claimed = cloud_orders::claim(&shop.st, id.clone()).unwrap();
    assert!(claimed.claimed_by_me && !claimed.claim_confirmed);
    shop.open_till(0);
    let request = || OrderSaleRequest {
        id: id.clone(),
        shipping_label: "Livraison".into(),
        payments: vec![PaymentInput::cash(Money(18_000))],
        print_receipt: false,
        extras: Default::default(),
    };
    assert_eq!(
        cloud_orders::sell(&shop.st, request()).unwrap_err().code(),
        "price_override_not_allowed"
    );
    let manager_id = shop.manager().user_id; // signs the manager in to create the tile
    shop.cashier(); // the cashier asks
    super::auth::manager_override(
        &shop.st,
        &super::auth::OverrideInput {
            approver_id: manager_id,
            pin: MANAGER_PIN.into(),
            permission: pos_core::auth::Permission::SalesPriceOverride,
        },
    )
    .unwrap();
    let done = cloud_orders::sell(&shop.st, request()).unwrap();
    assert_eq!(done.sale.document.total_ttc, Money(18_000));
    let got = cloud_orders::get(&shop.st, id).unwrap();
    assert_eq!(got.local_state, OrderState::Fulfilled);
    assert_eq!(got.local_document_number, done.sale.document.number);
}

#[test]
fn shipping_needs_the_delivery_permission_and_refusing_cancels_online() {
    let dz = Dz::start();
    let shop = linked(&dz);
    let milk = shop.product("Lait", 10_000, 1_900, None);
    shop.stock_in(&milk.id, 10);
    let id = order(&shop, &milk.id, 100);
    dz.mount(
        Mock::given(method("POST"))
            .and(path("/v1/orders/9001/claim"))
            .respond_with(ok(json!({ "claimed": true }))),
    );
    shop.cashier();
    cloud_orders::claim(&shop.st, id.clone()).unwrap();
    assert_eq!(
        cloud_orders::ship(&shop.st, id.clone(), "Livraison".into(), None)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    let refused = cloud_orders::refuse(&shop.st, id, RefuseReason::OutOfStock, None).unwrap();
    assert_eq!(refused.local_state, OrderState::Refused);
}

/// An online order is a sale: a custom role holding the delivery note but not the sale cannot
/// ship one (M7 design: online orders `sales.create`, shipping with a BL also
/// `documents.delivery`). Nothing is written: no BL, no consignee, the order stays claimed.
#[test]
fn shipping_also_needs_the_sale_permission() {
    let dz = Dz::start();
    let shop = linked(&dz);
    let milk = shop.product("Lait", 10_000, 1_900, None);
    shop.stock_in(&milk.id, 10);
    let id = order(&shop, &milk.id, 100);
    dz.mount(
        Mock::given(method("POST"))
            .and(path("/v1/orders/9001/claim"))
            .respond_with(ok(json!({ "claimed": true }))),
    );
    shop.cashier();
    cloud_orders::claim(&shop.st, id.clone()).unwrap();
    let delivery_notes = "SELECT COUNT(*) FROM documents WHERE doc_type = 'delivery_note'";
    let local_state = |shop: &TestShop| {
        let id = id.clone();
        shop.st
            .db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT local_state FROM online_orders WHERE id = ?1",
                    [&id],
                    |r| r.get::<_, String>(0),
                )?)
            })
            .unwrap()
    };
    let consignee = |shop: &TestShop| {
        shop.st
            .db
            .read(|c| pos_core::customers::find_by_phone(c, "0550123456"))
            .unwrap()
    };

    shop.with_permissions(&[pos_core::auth::Permission::DocumentsDelivery]);
    assert_eq!(
        cloud_orders::ship(&shop.st, id.clone(), "Livraison".into(), None)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    assert_eq!(shop.count(delivery_notes), 0, "no delivery note");
    assert!(consignee(&shop).is_none(), "no customer from the buyer");
    assert_eq!(local_state(&shop), "claimed", "the order stays claimed");

    shop.manager(); // both permissions: the same order ships
    let bl = cloud_orders::ship(&shop.st, id.clone(), "Livraison".into(), None).unwrap();
    assert_eq!(bl.doc_type, pos_core::documents::DocType::DeliveryNote);
    assert_eq!(shop.count(delivery_notes), 1);
    assert_eq!(local_state(&shop), "fulfilled");
}

#[test]
fn the_blacklist_lookup_sends_the_phone_only_when_the_owner_allows_it() {
    let dz = Dz::start();
    let shop = linked(&dz);
    dz.mount(
        Mock::given(path("/v1/customers")).respond_with(ok(json!({ "items": [
            { "id": 7, "is_banned": true, "fraud_score": 90 }
        ], "next_cursor": null, "has_more": false }))),
    );
    shop.cashier();
    assert_eq!(
        cloud_orders::customer_flag(&shop.st, "0550 12 34 56".into()).unwrap(),
        None
    );
    assert_eq!(
        dz.calls("GET", "/v1/customers"),
        0,
        "cloud.customers.sync is off by default"
    );
    shop.owner();
    super::settings::set(&shop.st, "cloud.customers.sync".into(), json!(true)).unwrap();
    shop.cashier();
    let flag = cloud_orders::customer_flag(&shop.st, "0550123456".into())
        .unwrap()
        .unwrap();
    assert!(flag.is_banned);
    assert_eq!(dz.calls("GET", "/v1/customers"), 1);
}
