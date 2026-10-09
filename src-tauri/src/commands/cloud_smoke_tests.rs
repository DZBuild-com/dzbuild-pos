//! A day of a linked shop against the DZBuild stand-in, and the promise that the link never
//! slows or blocks the till.

use super::cloud::{self, PublishChange};
use super::cloud_mock::{Dz, cloud_shop, created, ok, one_store, settled};
use super::returns::ReturnRequest;
use super::sales::SaleRequest;
use pos_core::cash::closing::{CloseInput, CountLine};
use pos_core::catalog::UNIT_PIECE_ID;
use pos_core::catalog::products::ProductInput;
use pos_core::cloud::engine::Timing;
use pos_core::cloud::link::AttemptState;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::returns::{ReturnInput, ReturnLineInput, ReturnReason};
use pos_core::sales::{Cart, CartLine, SaleInput};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use wiremock::Mock;
use wiremock::matchers::{method, path};

fn wait(what: &str, secs: u64, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !done() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn sale(product_id: &str, pieces: i64, cash: i64) -> SaleRequest {
    SaleRequest {
        sale: SaleInput {
            cart: Cart::new(
                Lang::Fr,
                vec![CartLine::product(product_id, Qty(pieces * 1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(cash))],
        },
        print_receipt: false,
        extras: Default::default(),
    }
}

#[test]
fn a_shop_day_with_dzbuild_end_to_end() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    // specific answers first: wiremock uses the first mounted mock that matches
    dz.mount(
        Mock::given(method("GET"))
            .and(path("/v1/orders"))
            .respond_with(ok(json!({ "items": [{
        "id": 9001, "order_number": 1042, "status": "confirmed", "payment_method": "cod", "delivery_type": "pickup",
        "customer_name": "Samira B.", "customer_phone": "0550123456", "subtotal": 110, "total": 110,
        "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
        "items": [{ "product_id": 88121, "sku": "LAIT-1L", "product_name": "Lait", "price": 110, "quantity": 1, "total": 110 }]
    }], "next_cursor": null, "has_more": false }))),
    );
    dz.mount(
        Mock::given(method("POST"))
            .and(path("/v1/orders/9001/claim"))
            .respond_with(ok(json!({ "claimed": true }))),
    );
    dz.mount(
        Mock::given(method("PATCH"))
            .and(path("/v1/orders/9001"))
            .respond_with(ok(json!({ "id": 9001 }))),
    );
    dz.mount_sync();
    let (shop, _) = cloud_shop(&dz);
    shop.owner();
    // 1. link in the browser
    cloud::link_start(&shop.st, Lang::Fr).unwrap();
    assert!(matches!(settled(&shop).state, AttemptState::Linked { .. }));
    let rt = shop.st.cloud().unwrap();
    rt.start(crate::cloud::EngineHooks::silent(), Timing::fast())
        .unwrap();
    // 2. publish the milk, sell one at the till, close the day
    let mut milk = ProductInput::simple("Lait", UNIT_PIECE_ID, &shop.rate_id(1_900), Money(11_000));
    milk.reference = Some("LAIT-1L".into());
    let milk = shop.product_with(milk);
    shop.stock_in(&milk.id, 10);
    cloud::publish_set(
        &shop.st,
        PublishChange {
            product_ids: vec![milk.id.clone()],
            publish: true,
            draft: false,
        },
    )
    .unwrap();
    shop.open_till(0);
    let ticket = super::sales::create(&shop.st, sale(&milk.id, 1, 11_000)).unwrap();
    wait("the product and the sale reach DZBuild", 20, || {
        dz.calls("POST", "/v1/products/batch") >= 1 && dz.calls("POST", "/v1/pos/sales") >= 1
    });
    let sale_request = dz
        .requests()
        .into_iter()
        .find(|r| r.url.path() == "/v1/pos/sales")
        .unwrap();
    let body: Value = serde_json::from_slice(&sale_request.body).unwrap();
    assert_eq!(body["external_id"], ticket.sale.document.id);
    assert_eq!(body["apply_stock"], false);
    // 3. the online order arrives by the date pull, is claimed and sold at the till
    wait("the online order", 20, || {
        shop.st
            .db
            .read(|c| {
                Ok(c.query_row("SELECT COUNT(*) FROM online_orders", [], |r| {
                    r.get::<_, i64>(0)
                })?)
            })
            .unwrap()
            == 1
    });
    let id: String = shop
        .st
        .db
        .read(|c| Ok(c.query_row("SELECT id FROM online_orders", [], |r| r.get(0))?))
        .unwrap();
    let claimed = super::cloud_orders::claim(&shop.st, id.clone()).unwrap();
    assert!(claimed.claim_confirmed);
    super::cloud_orders::sell(
        &shop.st,
        super::cloud_orders::OrderSaleRequest {
            id,
            shipping_label: "Livraison".into(),
            payments: vec![PaymentInput::cash(Money(11_000))],
            print_receipt: false,
            extras: Default::default(),
        },
    )
    .unwrap();
    wait("DZBuild told the order was delivered", 20, || {
        dz.calls("PATCH", "/v1/orders/9001") >= 1
    });
    // 4. the Z closure carries its anchors to DZBuild
    super::cash::close_session(
        &shop.st,
        CloseInput {
            counts: vec![
                CountLine {
                    denomination: Money(20_000),
                    count: 1,
                },
                CountLine {
                    denomination: Money(2_000),
                    count: 1,
                },
            ],
            notes: None,
        },
    )
    .unwrap();
    wait("the closure", 20, || {
        dz.calls("POST", "/v1/pos/closures") >= 1
    });
    let closure = dz
        .requests()
        .into_iter()
        .find(|r| r.url.path() == "/v1/pos/closures")
        .unwrap();
    let body: Value = serde_json::from_slice(&closure.body).unwrap();
    assert!(
        body["anchors"]["audit"]["last_hash"]
            .as_str()
            .is_some_and(|h| h.len() == 64)
    );
    // 5. every request carried a key, and no token left the Authorization header
    for r in dz.requests() {
        if matches!(r.method.as_str(), "POST" | "PATCH" | "DELETE")
            && r.url.path().starts_with("/v1/")
        {
            assert!(
                r.headers.get("idempotency-key").is_some(),
                "{}",
                r.url.path()
            );
        }
        let text = String::from_utf8_lossy(&r.body);
        assert!(
            !text.contains("at_1") && !text.contains("rt_1")
                || r.url.path() == "/oauth/revoke"
                || r.url.path() == "/oauth/token"
        );
    }
    rt.stop();
}

#[test]
fn selling_never_waits_for_dzbuild() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    let (shop, _) = cloud_shop(&dz);
    shop.owner();
    cloud::link_start(&shop.st, Lang::Fr).unwrap();
    assert!(matches!(settled(&shop).state, AttemptState::Linked { .. }));
    // from now on DZBuild answers every API call after 10 s
    dz.mount(
        Mock::given(wiremock::matchers::path_regex("^/v1/.*"))
            .respond_with(created(json!({ "id": 1 })).set_delay(Duration::from_secs(10))),
    );
    let rt = shop.st.cloud().unwrap();
    rt.start(crate::cloud::EngineHooks::silent(), Timing::fast())
        .unwrap();
    let milk = shop.product("Lait", 11_000, 1_900, None);
    shop.stock_in(&milk.id, 10);
    shop.open_till(0);
    for _ in 0..5 {
        let started = Instant::now();
        super::sales::create(&shop.st, sale(&milk.id, 1, 11_000)).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(1_500),
            "a sale took {:?}",
            started.elapsed()
        );
    }
    let started = Instant::now();
    rt.stop();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "stopping never waits for a slow request"
    );
    assert!(
        cloud::status(&shop.st).unwrap().linked,
        "a slow DZBuild changes nothing to the link"
    );
    assert!(
        dz.calls("POST", "/v1/pos/sales") <= 1,
        "one slow request at a time, never one per sale"
    );
}

/// The raw text of every number of a JSON body, strings skipped: what DZBuild's parser reads.
/// JSON's structural characters are ASCII and never a UTF-8 continuation byte, so bytes do.
fn numbers(body: &[u8]) -> Vec<String> {
    let (mut out, mut i) = (Vec::new(), 0);
    let (mut in_string, mut escaped) = (false, false);
    while i < body.len() {
        let b = body[i];
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            i += 1;
        } else if b == b'"' {
            in_string = true;
            i += 1;
        } else if b == b'-' || b.is_ascii_digit() {
            let start = i;
            while i < body.len()
                && (body[i].is_ascii_digit() || matches!(body[i], b'-' | b'+' | b'.' | b'e' | b'E'))
            {
                i += 1;
            }
            out.push(String::from_utf8_lossy(&body[start..i]).into_owned());
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn the_number_scanner_sees_a_float_and_skips_strings() {
    let body = r#"{"a":"1.5 \" 2","b":119.0,"c":[-81,0],"d":1.19e2,"e":"Lait 1,5 L thé"}"#;
    assert_eq!(numbers(body.as_bytes()), ["119.0", "-81", "0", "1.19e2"]);
}

/// Every amount DZBuild receives is exact: dinars as a two-decimal string (`wire::money_wire`) in
/// every body (the products, the sales, the refund, the closure), and any JSON number left (a
/// quantity, a count) is never a float such as `119.0` or `1.19e2` that a pass through `f64`
/// would write. Each amount is the shop's own.
#[test]
fn every_amount_dzbuild_receives_is_an_exact_decimal_string() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    // a refund is posted under its sale's DZBuild id (the 99120 the stand-in gives every sale)
    dz.mount(
        Mock::given(method("POST"))
            .and(path("/v1/pos/sales/99120/refunds"))
            .respond_with(created(json!({ "id": 99121 }))),
    );
    dz.mount_sync();
    let (shop, _) = cloud_shop(&dz);
    shop.owner();
    cloud::link_start(&shop.st, Lang::Fr).unwrap();
    assert!(matches!(settled(&shop).state, AttemptState::Linked { .. }));
    let rt = shop.st.cloud().unwrap();
    rt.start(crate::cloud::EngineHooks::silent(), Timing::fast())
        .unwrap();
    // 119 DA TTC at 19 %: 100 DA HT and 19 DA of TVA; under 300 DA, no stamp duty
    let mut sugar = ProductInput::simple(
        "Sucre 1 kg",
        UNIT_PIECE_ID,
        &shop.rate_id(1_900),
        Money(11_900),
    );
    sugar.reference = Some("SUC-1KG".into());
    let sugar = shop.product_with(sugar);
    shop.stock_in(&sugar.id, 10);
    cloud::publish_set(
        &shop.st,
        PublishChange {
            product_ids: vec![sugar.id.clone()],
            publish: true,
            draft: false,
        },
    )
    .unwrap();
    shop.open_till(100_000);
    let first = super::sales::create(&shop.st, sale(&sugar.id, 1, 11_900)).unwrap();
    // paid with a 200 DA note: 81 DA of change
    super::sales::create(&shop.st, sale(&sugar.id, 1, 20_000)).unwrap();
    let line_id = first.sale.document.lines[0].id.clone();
    super::returns::create(
        &shop.st,
        ReturnRequest {
            input: ReturnInput {
                original_id: Some(first.sale.document.id.clone()),
                lines: vec![ReturnLineInput {
                    origin_line_id: Some(line_id),
                    product_id: None,
                    qty: Qty(1_000),
                    unit_price: None,
                    restock: true,
                    reason: ReturnReason::ChangedMind,
                    note: None,
                }],
                refund_method: PaymentMethod::Cash,
                refund_reference: None,
                lang: Lang::Fr,
                note: None,
            },
            print_receipt: false,
            extras: Default::default(),
        },
    )
    .unwrap();
    // 1 000 DA of float + 238 DA sold - 119 DA refunded = 1 119 DA in the drawer
    let count = |dinars: i64, count: i64| CountLine {
        denomination: Money(dinars * 100),
        count,
    };
    super::cash::close_session(
        &shop.st,
        CloseInput {
            counts: vec![
                count(1_000, 1),
                count(100, 1),
                count(10, 1),
                count(5, 1),
                count(2, 2),
            ],
            notes: None,
        },
    )
    .unwrap();
    let refunds = "/v1/pos/sales/99120/refunds";
    wait("the products, sales, refund and closure", 20, || {
        dz.calls("POST", "/v1/products/batch") >= 1
            && dz.calls("POST", "/v1/pos/sales") >= 2
            && dz.calls("POST", refunds) >= 1
            && dz.calls("POST", "/v1/pos/closures") >= 1
    });
    rt.stop();
    let bodies: Vec<(String, Vec<u8>)> = dz
        .requests()
        .into_iter()
        .filter(|r| {
            [
                "/v1/products/batch",
                "/v1/pos/sales",
                refunds,
                "/v1/pos/closures",
            ]
            .contains(&r.url.path())
        })
        .map(|r| (r.url.path().to_owned(), r.body))
        .collect();
    for (route, body) in &bodies {
        for n in numbers(body) {
            let digits = n.strip_prefix('-').unwrap_or(&n);
            assert!(
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
                "{route} sent the number {n}: not an integer"
            );
        }
    }
    let body = |route: &str, nth: usize| -> Value {
        let (_, bytes) = bodies.iter().filter(|(r, _)| r == route).nth(nth).unwrap();
        serde_json::from_slice(bytes).unwrap()
    };
    let product = body("/v1/products/batch", 0);
    assert_eq!(product["items"][0]["pricing"]["price"], json!("119.00"));
    let ticket = body("/v1/pos/sales", 0);
    assert_eq!(ticket["external_id"], first.sale.document.id);
    assert_eq!(
        (
            &ticket["lines"][0]["unit_price"],
            &ticket["totals"]["tax"],
            &ticket["totals"]["total"],
        ),
        (&json!("119.00"), &json!("19.00"), &json!("119.00"))
    );
    let change = body("/v1/pos/sales", 1);
    assert!(
        change["payments"]
            .as_array()
            .unwrap()
            .contains(&json!({ "method": "change", "amount": "-81.00" })),
        "{change}"
    );
    assert_eq!(body(refunds, 0)["total"], json!("119.00"));
    // the Z report's own signs: returns count against the sales (net = sales + returns)
    let z = body("/v1/pos/closures", 0);
    assert_eq!(
        (
            &z["totals"]["sales"],
            &z["totals"]["returns"],
            &z["totals"]["net"],
            &z["cash"]["expected"],
            &z["cash"]["counted"],
        ),
        (
            &json!("238.00"),
            &json!("-119.00"),
            &json!("119.00"),
            &json!("1119.00"),
            &json!("1119.00")
        )
    );
}
