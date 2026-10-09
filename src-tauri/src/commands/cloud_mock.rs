//! The DZBuild stand-in for the shell's tests: OAuth (discovery, authorize redirect to the
//! loopback, token, device grant, revoke), the link-time API, and the sync endpoints, on a local
//! wiremock server. Tests drive it from plain `#[test]` threads.

use crate::cloud::UrlOpener;
use crate::commands::testshop::{TestShop, first_run_shop};
use crate::state::AppState;
use pos_core::cloud::config::CloudConfig;
use pos_core::cloud::secrets::MemorySecrets;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use wiremock::matchers::{body_string_contains, header, method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub(crate) struct Dz {
    rt: tokio::runtime::Runtime,
    server: MockServer,
}

pub(crate) fn ok(data: Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({ "data": data, "meta": { "request_id": "req" } }))
}

pub(crate) fn created(data: Value) -> ResponseTemplate {
    ResponseTemplate::new(201).set_body_json(json!({ "data": data }))
}

pub(crate) fn empty_page() -> ResponseTemplate {
    ok(json!({ "items": [], "next_cursor": null, "has_more": false }))
}

pub(crate) fn one_store() -> Value {
    json!([{ "store_id": 141, "store_name": "Boutique Amel", "plan": "pro", "role": "owner",
             "public_url": "https://amel.dzbuild.app" }])
}

struct Authorize {
    issuer: String,
}

impl Respond for Authorize {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let q: HashMap<String, String> = request.url.query_pairs().into_owned().collect();
        let Some(mut target) = q.get("redirect_uri").and_then(|u| url::Url::parse(u).ok()) else {
            return ResponseTemplate::new(400);
        };
        target
            .query_pairs_mut()
            .append_pair("code", "code_1")
            .append_pair("state", q.get("state").map_or("", String::as_str))
            .append_pair("iss", &self.issuer);
        ResponseTemplate::new(302).insert_header("Location", target.as_str())
    }
}

impl Dz {
    pub fn start() -> Dz {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
        Dz { rt, server }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    pub fn config(&self) -> CloudConfig {
        CloudConfig::loopback(&self.uri())
    }

    pub fn mount(&self, mock: Mock) {
        self.rt.block_on(mock.mount(&self.server));
    }

    pub fn requests(&self) -> Vec<Request> {
        self.rt
            .block_on(self.server.received_requests())
            .unwrap_or_default()
    }

    pub fn calls(&self, verb: &str, route: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.method.as_str() == verb && r.url.path() == route)
            .count()
    }

    /// OAuth and the link-time API, for an account with `stores`. Only store 141 has a profile:
    /// another store's `GET /v1/store` is a bare 404 (not shipped), so its link keeps the name
    /// the account's store list gave.
    pub fn mount_link(&self, stores: Value) {
        let issuer = self.uri();
        self.mount(
            Mock::given(path("/.well-known/oauth-authorization-server")).respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "issuer": issuer,
                    "authorization_endpoint": format!("{issuer}/oauth/authorize"),
                    "token_endpoint": format!("{issuer}/oauth/token"),
                    "device_authorization_endpoint": format!("{issuer}/oauth/device_authorization"),
                    "revocation_endpoint": format!("{issuer}/oauth/revoke"),
                    "userinfo_endpoint": format!("{issuer}/oauth/oidc/userinfo"),
                    "code_challenge_methods_supported": ["S256"],
                    "grant_types_supported": ["authorization_code", "refresh_token",
                                              "urn:ietf:params:oauth:grant-type:device_code"],
                    "authorization_response_iss_parameter_supported": true
                })),
            ),
        );
        self.mount(
            Mock::given(path("/oauth/authorize")).respond_with(Authorize {
                issuer: issuer.clone(),
            }),
        );
        let tokens = json!({ "access_token": "at_1", "token_type": "Bearer", "expires_in": 900,
                             "refresh_token": "rt_1", "scope": "offline_access store:read", "stores": stores });
        for grant in [
            "grant_type=authorization_code",
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
        ] {
            self.mount(
                Mock::given(path("/oauth/token"))
                    .and(body_string_contains(grant))
                    .respond_with(ResponseTemplate::new(200).set_body_json(tokens.clone())),
            );
        }
        self.mount(
            Mock::given(path("/oauth/token"))
                .and(body_string_contains("grant_type=refresh_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "at_2", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_2"
                }))),
        );
        self.mount(
            Mock::given(path("/oauth/device_authorization")).respond_with(
                ResponseTemplate::new(200).set_body_json(json!({
                    "device_code": "dc_1", "user_code": "DZKM-4Q7P", "verification_uri": "https://dzbuild.com/device",
                    "verification_uri_complete": "https://dzbuild.com/device?code=DZKM-4Q7P", "expires_in": 600, "interval": 1
                })),
            ),
        );
        self.mount(Mock::given(path("/oauth/revoke")).respond_with(ResponseTemplate::new(200)));
        self.mount(Mock::given(path("/v1/me")).respond_with(ok(
            json!({ "sub": "84211", "name": "Amel", "stores": stores }),
        )));
        self.mount(
            Mock::given(path("/v1/store"))
                .and(header("x-dz-store", "141"))
                .respond_with(ok(json!({
                    "name": "Boutique Amel", "language": "ar", "public_url": "https://amel.dzbuild.app",
                    "stock_deduction": "on_confirm", "currency": "DZD", "plan": "pro",
                    "plan_limits": { "active_products": 300, "orders_per_month": null }
                }))),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/devices"))
                .respond_with(created(json!({ "id": "dev_1" }))),
        );
        self.mount(
            Mock::given(method("DELETE"))
                .and(path("/v1/devices/dev_1"))
                .respond_with(ResponseTemplate::new(204)),
        );
        self.mount(
            Mock::given(method("GET"))
                .and(path("/v1/devices"))
                .respond_with(ok(json!({ "items": [
                    { "id": "dev_1", "name": "CAISSE-1 · C1", "app_version": "0.7.0", "last_seen_at": "2026-09-28T11:00:00+01:00" },
                    { "id": "dev_2", "name": "BUREAU · C2", "app_version": "0.7.0", "last_seen_at": null }
                ], "next_cursor": null, "has_more": false }))),
        );
        self.mount(
            Mock::given(method("GET"))
                .and(path("/v1/locations"))
                .respond_with(empty_page()),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/locations"))
                .respond_with(created(json!({ "id": "loc_1" }))),
        );
    }

    /// The sync endpoints of the Backend Brief (the events feed not shipped yet: pulls by date).
    #[allow(
        dead_code,
        reason = "the engine tests (online orders, backups, the offline week) mount it"
    )]
    pub fn mount_sync(&self) {
        self.mount(Mock::given(path("/v1/events")).respond_with(ResponseTemplate::new(404)));
        self.mount(
            Mock::given(method("GET"))
                .and(path("/v1/orders"))
                .respond_with(empty_page()),
        );
        self.mount(
            Mock::given(method("GET"))
                .and(path("/v1/products"))
                .respond_with(empty_page()),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/products/batch"))
                .respond_with(ok(json!({ "items": [] }))),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/inventory/adjustments"))
                .respond_with(ok(json!({ "items": [] }))),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/sales"))
                .respond_with(created(json!({ "id": 99120 }))),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/refunds"))
                .respond_with(created(json!({ "id": 99121 }))),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/closures"))
                .respond_with(created(json!({ "id": 501 }))),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path_regex("^/v1/devices/dev_1/heartbeat$"))
                .respond_with(created(json!({}))),
        );
    }
}

/// The owner's browser: follows the authorization URL to the loopback, like Edge would.
pub(crate) fn browser() -> UrlOpener {
    Arc::new(|url: &str| {
        let url = url.to_owned();
        std::thread::spawn(move || {
            let _ = reqwest::blocking::get(url).and_then(|r| r.text());
        });
        Ok(())
    })
}

/// A first-run shop whose DZBuild runtime talks to `dz`, with the tokens in memory.
pub(crate) fn cloud_shop(dz: &Dz) -> (TestShop, Arc<MemorySecrets>) {
    let secrets = Arc::new(MemorySecrets::default());
    let st = AppState::for_tests_cloud(dz.config(), secrets.clone());
    let shop = first_run_shop(st);
    shop.st.cloud().unwrap().set_opener(browser());
    (shop, secrets)
}

/// Polls the link attempt until it leaves `Waiting`/`Finishing`.
pub(crate) fn settled(shop: &TestShop) -> pos_core::cloud::link::AttemptView {
    use pos_core::cloud::link::AttemptState;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let view = super::cloud::link_poll(&shop.st)
            .unwrap()
            .expect("an attempt");
        if !matches!(view.state, AttemptState::Waiting | AttemptState::Finishing) {
            return view;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the attempt never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
