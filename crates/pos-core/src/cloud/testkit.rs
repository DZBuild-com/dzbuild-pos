//! A DZBuild stand-in for the tests: a wiremock server on 127.0.0.1 driven from plain `#[test]`
//! threads (the blocking client must never run inside the mock's async runtime).

use super::api::Remote;
use super::config::CloudConfig;
use super::http::{Clock, Http, SystemClock};
use super::oauth::TokenResponse;
use super::secrets::MemorySecrets;
use super::store::{self, CloudLink, NewLink};
use super::tokens::TokenManager;
use crate::lang::Lang;
use crate::sync::changelog;
use crate::testkit::{NOON, Shop, shop};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wiremock::matchers::{body_string_contains, method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub(crate) struct MockDz {
    pub rt: tokio::runtime::Runtime,
    pub server: MockServer,
}

impl MockDz {
    pub fn start() -> MockDz {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let server = rt.block_on(MockServer::start());
        MockDz { rt, server }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    pub fn config(&self) -> CloudConfig {
        CloudConfig::loopback(&self.uri())
    }

    pub fn http(&self) -> Http {
        self.http_at(Arc::new(SystemClock))
    }

    pub fn http_at(&self, clock: Arc<dyn Clock>) -> Http {
        Http::new(&self.config(), clock, "DZBuildPOS/test (Windows)".into()).unwrap()
    }

    pub fn mount(&self, mock: Mock) {
        self.rt.block_on(mock.mount(&self.server));
    }

    pub fn requests(&self) -> Vec<Request> {
        self.rt
            .block_on(self.server.received_requests())
            .unwrap_or_default()
    }

    pub fn verify(&self) {
        self.rt.block_on(self.server.verify());
    }
}

pub(crate) fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// The authorization endpoint as a browser sees it: a 302 back to the loopback with a code (or
/// an error), the request's `state` and the issuer (RFC 9207).
pub(crate) struct AuthorizeRedirect {
    pub issuer: String,
    pub error: Option<&'static str>,
}

impl Respond for AuthorizeRedirect {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let q: HashMap<String, String> = request.url.query_pairs().into_owned().collect();
        let mut target = url::Url::parse(&q["redirect_uri"]).unwrap();
        {
            let mut pairs = target.query_pairs_mut();
            match self.error {
                Some(error) => pairs.append_pair("error", error),
                None => pairs.append_pair("code", "code_1"),
            };
            pairs
                .append_pair("state", &q["state"])
                .append_pair("iss", &self.issuer);
        }
        ResponseTemplate::new(302).insert_header("Location", target.as_str())
    }
}

pub(crate) fn ok(data: Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({ "data": data, "meta": { "request_id": "req" } }))
}

pub(crate) fn created(data: Value) -> ResponseTemplate {
    ResponseTemplate::new(201).set_body_json(json!({ "data": data }))
}

/// `POST /v1/devices` behind DZBuild's idempotency store (research 06 §3.1.5; the 24 h expiry
/// never comes in a test): a key seen before replays its first answer (`Idempotency-Replay: 1`)
/// when the body is the same and gets `422 idempotency_key_reuse` when it differs; a new key
/// makes the next device (`dev_1`, `dev_2`, …). A deleted device is still replayed, as DZBuild
/// would.
#[derive(Default)]
pub(crate) struct DeviceRegistry {
    seen: Mutex<HashMap<String, (Vec<u8>, String)>>,
}

impl Respond for DeviceRegistry {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let key = header(request, "idempotency-key").unwrap_or_default();
        let mut seen = self.seen.lock().unwrap();
        let next = format!("dev_{}", seen.len() + 1);
        let replay = seen.contains_key(&key);
        let (body, id) = seen
            .entry(key)
            .or_insert_with(|| (request.body.clone(), next))
            .clone();
        if body != request.body {
            return ResponseTemplate::new(422).set_body_json(json!({
                "error": { "code": "idempotency_key_reuse", "message": "key reused with another request" }
            }));
        }
        let answer = created(json!({ "id": id }));
        if replay {
            answer.insert_header("Idempotency-Replay", "1")
        } else {
            answer
        }
    }
}

impl MockDz {
    /// Discovery, authorize, token (code and refresh), device grant, revoke; `/v1/me` with
    /// `stores`; the store profile (store 141), the device and the location of a first link.
    pub fn mount_link(&self, stores: Value) {
        let issuer = self.uri();
        self.mount(
            Mock::given(path("/.well-known/oauth-authorization-server")).respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(crate::cloud::oauth::tests::metadata_json(&issuer)),
            ),
        );
        self.mount(
            Mock::given(path("/oauth/authorize")).respond_with(AuthorizeRedirect {
                issuer,
                error: None,
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
            Mock::given(path("/oauth/device_authorization")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "dc_1", "user_code": "DZKM-4Q7P", "verification_uri": "https://dzbuild.com/device",
                "verification_uri_complete": "https://dzbuild.com/device?code=DZKM-4Q7P", "expires_in": 600, "interval": 1
            }))),
        );
        self.mount(Mock::given(path("/oauth/revoke")).respond_with(ResponseTemplate::new(200)));
        self.mount(Mock::given(path("/v1/me")).respond_with(ok(
            json!({ "sub": "84211", "name": "Amel", "stores": stores }),
        )));
        // Store 141's profile only: another store's `GET /v1/store` is a bare 404 (not shipped),
        // so its link keeps the name the account's store list gave.
        self.mount(
            Mock::given(path("/v1/store"))
                .and(wiremock::matchers::header("x-dz-store", "141"))
                .respond_with(ok(json!({
                    "name": "Boutique Amel", "language": "ar", "public_url": "https://amel.dzbuild.app",
                    "stock_deduction": "on_confirm", "currency": "DZD", "plan": "pro",
                    "plan_limits": { "active_products": 300, "orders_per_month": null }
                }))),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/devices"))
                .respond_with(DeviceRegistry::default()),
        );
        self.mount(
            Mock::given(method("DELETE"))
                .and(path_regex(r"^/v1/devices/dev_\d+$"))
                .respond_with(ResponseTemplate::new(204)),
        );
        self.mount(
            Mock::given(method("GET"))
                .and(path("/v1/locations"))
                .respond_with(ok(
                    json!({ "items": [], "next_cursor": null, "has_more": false }),
                )),
        );
        self.mount(
            Mock::given(method("POST"))
                .and(path("/v1/locations"))
                .respond_with(created(json!({ "id": "loc_1" }))),
        );
    }
}

/// A `Remote` for `link`'s store on the mock, holding `at_1`/`rt_1` in memory.
pub(crate) fn remote_for_test(dz: &MockDz, link: &CloudLink, clock: Arc<dyn Clock>) -> Remote {
    remote_at(&dz.uri(), link, clock)
}

/// No DZBuild at all: nothing listens on 127.0.0.1:9 (discard), so every call is `Offline`. A
/// dropped `MockDz` does not do: wiremock hands its server back to a pool, still listening.
pub(crate) const UNREACHABLE: &str = "http://127.0.0.1:9";

/// A one-shot HTTP/1.1 server on 127.0.0.1 for bodies wiremock cannot serve. It answers the
/// first request with a 200 that declares `declared` bytes, then sends each chunk after its
/// pause and closes. A slow line is long pauses; a body that breaks off declares more than it
/// sends. Returns the base URL.
pub(crate) fn trickle(steps: Vec<(std::time::Duration, Vec<u8>)>, declared: usize) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let Ok((mut socket, _)) = listener.accept() else {
            return;
        };
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match socket.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => return,
            }
        }
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n"
        );
        if socket.write_all(head.as_bytes()).is_err() {
            return;
        }
        for (pause, chunk) in steps {
            std::thread::sleep(pause);
            if socket
                .write_all(&chunk)
                .and_then(|()| socket.flush())
                .is_err()
            {
                return;
            }
        }
    });
    url
}

/// [`remote_for_test`] against any base URL.
pub(crate) fn remote_at(uri: &str, link: &CloudLink, clock: Arc<dyn Clock>) -> Remote {
    let config = CloudConfig::loopback(uri);
    let http = Http::new(&config, clock.clone(), "DZBuildPOS/test (Windows)".into()).unwrap();
    let tokens = Arc::new(TokenManager::new(
        &link.id,
        Arc::new(MemorySecrets::default()),
        clock.clone(),
    ));
    tokens
        .store(&TokenResponse {
            access_token: "at_1".into(),
            token_type: "Bearer".into(),
            expires_in: Some(900),
            refresh_token: Some("rt_1".into()),
            scope: None,
            stores: Vec::new(),
        })
        .unwrap();
    Remote::new(
        Arc::new(http),
        tokens,
        Arc::new(crate::cloud::oauth::tests::metadata(uri)),
        Arc::new(config),
        Some(link.store_id.clone()),
    )
}

pub(crate) fn one_store() -> Value {
    json!([{ "store_id": 141, "store_name": "Boutique Amel", "plan": "pro", "role": "owner",
             "public_url": "https://amel.dzbuild.app" }])
}

/// What a browser does with the authorize URL: follows the redirect to the loopback and reads
/// the page.
pub(crate) fn browse(url: String) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        reqwest::blocking::get(url)
            .and_then(|r| r.text())
            .unwrap_or_default()
    })
}

/// A test shop linked to store 141 (the change-feed reader registered at the head).
pub(crate) fn linked_shop() -> (Shop, CloudLink) {
    link_shop(shop())
}

/// [`linked_shop`] for a shop that shows its prices HT.
pub(crate) fn linked_ht_shop() -> (Shop, CloudLink) {
    link_shop(crate::testkit::shop_with(
        crate::company::TaxRegime::Reel,
        crate::company::PriceMode::Ht,
    ))
}

fn link_shop(shop: Shop) -> (Shop, CloudLink) {
    let link = shop
        .db
        .write(|tx| {
            let link = store::insert_link(
                tx,
                &NewLink {
                    id: "00000000-0000-7000-8000-00000000c001".into(),
                    account_sub: "84211".into(),
                    account_name: None,
                    store_id: "141".into(),
                    store_name: "Boutique Amel".into(),
                    store_language: Lang::Ar,
                    store_plan: Some("pro".into()),
                    public_url: Some("https://amel.dzbuild.app".into()),
                    location_id: Some("loc_1".into()),
                    device_id: Some("dev_1".into()),
                    scopes: vec!["offline_access".into()],
                    stock_deduction: Some("on_confirm".into()),
                    active_products_cap: Some(300),
                },
                NOON,
            )?;
            changelog::register(tx, changelog::CLOUD_CONSUMER, NOON)?;
            Ok(link)
        })
        .unwrap();
    (shop, link)
}

/// Publishes `product_id` (the owner's "Publier en ligne").
pub(crate) fn publish(shop: &Shop, product_id: &str) {
    let id = product_id.to_owned();
    shop.db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO cloud_publish (product_id, publish, updated_at) VALUES (?1, 1, 0)
                 ON CONFLICT (product_id) DO UPDATE SET publish = 1",
                [id],
            )?;
            Ok(())
        })
        .unwrap();
}

/// Returns every line of `original` in cash, back to the shelf (the owner's till: the cashier
/// role has no `sales.return`).
pub(crate) fn return_all(shop: &Shop, original: &str) -> crate::documents::Document {
    return_all_to(shop, original, true)
}

/// [`return_all`], back to the shelf when `restock`, to the damaged-goods location otherwise.
pub(crate) fn return_all_to(
    shop: &Shop,
    original: &str,
    restock: bool,
) -> crate::documents::Document {
    let ctx = crate::till::TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
    let original = original.to_owned();
    shop.db
        .write(move |tx| {
            let lines = crate::returns::returnable(tx, &original)?
                .into_iter()
                .map(|l| crate::returns::ReturnLineInput {
                    origin_line_id: Some(l.line.id.clone()),
                    product_id: None,
                    qty: l.remaining,
                    unit_price: None,
                    restock,
                    reason: crate::returns::ReturnReason::ChangedMind,
                    note: None,
                })
                .collect();
            let input = crate::returns::ReturnInput {
                original_id: Some(original.clone()),
                lines,
                refund_method: crate::payments::PaymentMethod::Cash,
                refund_reference: None,
                lang: Lang::Fr,
                note: None,
            };
            crate::returns::create_return(tx, &ctx, input).map(|r| r.document)
        })
        .unwrap()
}

pub(crate) fn outbox_rows(shop: &Shop) -> Vec<(String, String, String)> {
    shop.db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT kind, entity_id, status FROM sync_outbox ORDER BY created_at, rowid",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}
