//! An events page whose detail DZBuild answers in a shape this version cannot
//! read is tried again once per cycle; the 100th try in a row records that event `failed` in the
//! inbox with one notice, and the feed moves on past it (the events behind it apply, the cursor
//! moves).

use super::engine::{EngineDeps, SyncHandle, Timing};
use super::http::{Http, ManualClock};
use super::link::{DeviceInfo, LinkContext};
use super::secrets::{MemorySecrets, SecretStore, refresh_account};
use super::store::{self, CloudNoticeKind};
use super::testkit::{MockDz, created, linked_shop, ok, one_store};
use crate::testkit::Shop;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn empty_page() -> ResponseTemplate {
    ok(json!({ "items": [], "next_cursor": null, "has_more": false }))
}

fn requests_to(dz: &MockDz, wanted: &str) -> usize {
    dz.requests()
        .iter()
        .filter(|r| r.url.path() == wanted)
        .count()
}

fn inbox(shop: &Shop) -> Vec<(String, String)> {
    shop.db
        .read(|c| {
            let mut stmt =
                c.prepare("SELECT event_id, outcome FROM cloud_inbox ORDER BY event_id")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<(String, String)>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

/// The engine of the linked test shop on `dz_uri`, a cycle every 5 ms. Its clock is moved by
/// hand: the rate limiter's pauses (120 calls a minute) advance it instead of waiting.
fn start(shop: &Shop, dz_uri: &str) -> SyncHandle {
    let secrets = Arc::new(MemorySecrets::default());
    secrets
        .set(
            &refresh_account("00000000-0000-7000-8000-00000000c001"),
            "rt_1",
        )
        .unwrap();
    let config = super::config::CloudConfig::loopback(dz_uri);
    let clock = ManualClock::at(crate::time::now_ms());
    let http = Http::new(&config, clock, "t".into()).unwrap();
    let link = Arc::new(LinkContext {
        db: shop.db.clone(),
        http: Arc::new(http),
        config: Arc::new(config),
        secrets,
        device: DeviceInfo {
            name: "C1".into(),
            os: "Windows".into(),
            app_version: "0.7.0".into(),
            terminal_code: "C1".into(),
            terminal_id: shop.terminal.id.clone(),
        },
        on_change: Arc::new(|| {}),
    });
    SyncHandle::start(EngineDeps {
        link,
        media_dir: None,
        timing: Timing {
            cycle: Duration::from_millis(5),
            ..Timing::fast()
        },
        on_status: Arc::new(|_| {}),
        on_orders: Arc::new(|_| {}),
    })
    .unwrap()
}

#[test]
fn an_unreadable_detail_is_recorded_failed_after_100_tries_and_the_feed_moves_on() {
    let dz = MockDz::start();
    dz.mount_link(one_store());
    dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
    dz.mount(Mock::given(path("/v1/orders")).respond_with(empty_page()));
    dz.mount(Mock::given(path("/v1/products")).respond_with(empty_page()));
    // the feed: e1 (an order whose detail this version cannot read), then e2, then nothing new
    dz.mount(
        Mock::given(path("/v1/events"))
            .and(query_param("cursor", "c2"))
            .respond_with(
                ok(json!({ "items": [], "next_cursor": "c2", "has_more": false }))
                    .set_delay(Duration::from_millis(200)),
            ),
    );
    dz.mount(
        Mock::given(path("/v1/events"))
            .and(query_param("cursor", "c1"))
            .respond_with(ok(json!({
                "items": [{ "id": "e2", "type": "store.updated", "data": {} }],
                "next_cursor": "c2", "has_more": false
            }))),
    );
    dz.mount(Mock::given(path("/v1/events")).respond_with(ok(json!({
        "items": [{ "id": "e1", "type": "order.created", "data": { "id": 9001 } }],
        "next_cursor": "c1", "has_more": false
    }))));
    dz.mount(
        Mock::given(path("/v1/orders/9001")).respond_with(ok(json!({ "id": 9001, "status": 7 }))),
    );
    let (shop, _) = linked_shop();
    let handle = start(&shop, &dz.uri());
    let deadline = Instant::now() + Duration::from_secs(60);
    while inbox(&shop).len() < 2 {
        assert!(
            Instant::now() < deadline,
            "timed out: the feed is still held up after {} tries of the detail; inbox {:?}",
            requests_to(&dz, "/v1/orders/9001"),
            inbox(&shop)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(handle);
    assert_eq!(
        requests_to(&dz, "/v1/orders/9001"),
        100,
        "tried once per cycle, 100 times in a row, then no more"
    );
    assert_eq!(
        inbox(&shop),
        [
            ("e1".to_owned(), "failed".to_owned()),
            ("e2".to_owned(), "ignored".to_owned())
        ],
        "the unreadable event is recorded failed and the one behind it applies"
    );
    let cursor = shop
        .db
        .read(|c| store::remote_cursor(c, "cloud:events"))
        .unwrap();
    assert_eq!(cursor.as_deref(), Some("c2"), "the feed moved on");
    let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
    assert_eq!(notices.len(), 1, "one notice: {notices:?}");
    assert_eq!(
        (notices[0].kind, notices[0].code.as_str(), notices[0].count),
        (CloudNoticeKind::FailedItems, "cloud_event_unreadable", 1)
    );
}
