//! The main PC's routes. `/health` and `/pair` are public (rate-limited per address); every
//! other route needs a till's bearer token.

use super::Shared;
use super::guard::{self, error_response};
use crate::wire::*;
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use pos_core::error::AppError;
use pos_core::sync::devices::{self, Device};
use pos_core::sync::{feed, replay};
use std::net::SocketAddr;
use std::sync::Arc;

pub fn router(shared: Arc<Shared>) -> Router {
    let protected = Router::new()
        .route("/whoami", get(whoami))
        .route("/snapshot", get(snapshot))
        .route("/changes", get(changes))
        .route("/feed", get(feed_socket))
        .route("/replay", post(replay))
        .route("/rpc/{service}", post(rpc))
        .route("/updates/manifest", get(update_manifest))
        .route("/updates/setup", get(update_setup))
        .route_layer(middleware::from_fn_with_state(shared.clone(), guard::auth));
    let schema = shared.config.schema_version;
    let clock = shared.services.clock.clone();
    Router::new()
        .route("/health", get(health))
        .route("/pair", post(pair))
        .merge(protected)
        .layer(middleware::from_fn(guard::log))
        .layer(middleware::map_response(move |mut response: Response| {
            let clock = clock.clone();
            async move {
                let headers = response.headers_mut();
                if let Ok(v) = HeaderValue::from_str(&clock().to_string()) {
                    headers.insert(H_HOST_TIME, v);
                }
                if let Ok(v) = HeaderValue::from_str(&schema.to_string()) {
                    headers.insert(H_SCHEMA, v);
                }
                response
            }
        }))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .with_state(shared)
}

pub(crate) async fn blocking<T, F>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("LAN task: {e}")))?
}

pub(crate) fn limited(
    shared: &Shared,
    peer: SocketAddr,
    route: &str,
    per_minute: u32,
) -> Option<Response> {
    let key = format!("{}:{route}", peer.ip());
    (!shared
        .limiter
        .allow(&key, per_minute, per_minute, (shared.services.clock)()))
    .then(|| error_response(&AppError::conflict("lan_rate_limited")))
}

async fn health(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if let Some(refused) = limited(&shared, peer, "/health", 60) {
        return refused;
    }
    let now = (shared.services.clock)();
    let pairing_open = shared
        .services
        .desk
        .lock()
        .map(|d| d.open_until(now).is_some())
        .unwrap_or(false);
    let db = shared.services.db.clone();
    let config = shared.config.clone();
    match blocking(move || db.read(devices::suggest_code)).await {
        Ok(suggested_code) => Json(HealthInfo {
            host_id: config.host_id,
            shop_name: config.shop_name,
            app_version: config.app_version,
            schema_version: config.schema_version,
            host_time: now,
            pairing_open,
            suggested_code,
        })
        .into_response(),
        Err(error) => error_response(&error),
    }
}

/// A pairing code for a token: the code is redeemed first (spent even when the till is refused
/// afterwards), then the till is registered and journaled with the main PC's terminal as actor.
async fn pair(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<devices::PairRequest>,
) -> Response {
    if let Some(refused) = limited(&shared, peer, "/pair", 10) {
        return refused;
    }
    let (db, desk, clock) = (
        shared.services.db.clone(),
        shared.services.desk.clone(),
        shared.services.clock.clone(),
    );
    let actor = pos_core::audit::Actor::system(Some(shared.services.host_terminal_id.clone()));
    let schema = shared.config.schema_version;
    let paired =
        blocking(move || devices::pair(&db, &desk, request, schema, &actor, clock())).await;
    match paired {
        Ok(paired) => Json(PairResponse {
            host_id: shared.config.host_id.clone(),
            shop_name: shared.config.shop_name.clone(),
            terminal: paired.terminal,
            token: paired.token,
            host_time: (shared.services.clock)(),
        })
        .into_response(),
        Err(error) => error_response(&error),
    }
}

/// The sanitised copy made for the calling till, one export at a time, streamed from the file
/// with its feed `seq` and SHA-256; the export records the till's journal anchor.
async fn snapshot(
    State(shared): State<Arc<Shared>>,
    Extension(device): Extension<Device>,
) -> Response {
    let Ok(_one_at_a_time) = shared.snapshots.acquire().await else {
        return error_response(&AppError::Internal("snapshot gate closed".into()));
    };
    let (db, dir, terminal, now) = (
        shared.services.db.clone(),
        shared.config.snapshot_dir.clone(),
        device.terminal_id.clone(),
        (shared.services.clock)(),
    );
    let file =
        match blocking(move || pos_core::sync::snapshot::export(&db, &dir, &terminal, now)).await {
            Ok(file) => file,
            Err(error) => return error_response(&error),
        };
    let opened = match tokio::fs::File::open(&file.path).await {
        Ok(opened) => opened,
        Err(error) => return error_response(&AppError::from(error)),
    };
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(opened));
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    for (name, value) in [
        (
            header::CONTENT_TYPE.as_str(),
            "application/octet-stream".to_owned(),
        ),
        (header::CONTENT_LENGTH.as_str(), file.size.to_string()),
        (H_SEQ, file.seq.to_string()),
        (H_SHA256, file.sha256.clone()),
    ] {
        if let (Ok(name), Ok(value)) = (
            axum::http::HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            headers.insert(name, value);
        }
    }
    response.extensions_mut().insert(device);
    response
}

/// The manifest of the update this main PC downloaded and verified, for its tills; no
/// path comes from the request (a fixed file of `update_dir`). `update_not_available` when none.
async fn update_manifest(State(shared): State<Arc<Shared>>) -> Response {
    let path = shared.config.update_dir.join(super::UPDATE_MANIFEST);
    match tokio::fs::read(&path).await {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(manifest) => Json(manifest).into_response(),
            Err(_) => error_response(&AppError::conflict("update_not_available")),
        },
        Err(_) => error_response(&AppError::conflict("update_not_available")),
    }
}

/// The setup of that update, streamed from the fixed file; the till checks its signature and
/// digest itself, so a main PC never vouches for it.
async fn update_setup(State(shared): State<Arc<Shared>>) -> Response {
    let path = shared.config.update_dir.join(super::UPDATE_SETUP);
    let (opened, size) = match tokio::fs::File::open(&path).await {
        Ok(file) => {
            let size = file.metadata().await.map(|m| m.len()).ok();
            (file, size)
        }
        Err(_) => return error_response(&AppError::conflict("update_not_available")),
    };
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(opened));
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Some(size) = size
        && let Ok(value) = HeaderValue::from_str(&size.to_string())
    {
        headers.insert(header::CONTENT_LENGTH, value);
    }
    response
}

async fn whoami(Extension(device): Extension<Device>) -> Response {
    let mut response = Json(device.code.clone()).into_response();
    response.extensions_mut().insert(device);
    response
}

#[derive(Debug, serde::Deserialize)]
struct ChangesQuery {
    since: i64,
    limit: Option<usize>,
}

fn till_schema(headers: &HeaderMap) -> Option<i64> {
    headers.get(H_SCHEMA)?.to_str().ok()?.parse().ok()
}

/// The till's code for the request's log line.
fn with_device(mut response: Response, device: Device) -> Response {
    response.extensions_mut().insert(device);
    response
}

/// A page of the feed after `since`, which the till holds already: its reader moves there first
/// (the feed is pruned behind every reader). Only a till of the same schema gets the feed.
async fn changes(
    State(shared): State<Arc<Shared>>,
    Extension(device): Extension<Device>,
    headers: HeaderMap,
    Query(query): Query<ChangesQuery>,
) -> Response {
    if till_schema(&headers).is_some_and(|s| s != shared.config.schema_version) {
        return with_device(
            error_response(&AppError::conflict("lan_version_mismatch")),
            device,
        );
    }
    let (db, id, now) = (
        shared.services.db.clone(),
        device.terminal_id.clone(),
        (shared.services.clock)(),
    );
    let limit = query.limit.unwrap_or(feed::PAGE_ROWS);
    let served = blocking(move || {
        let ack = id.clone();
        db.write(move |tx| feed::acknowledge(tx, &ack, query.since, now).map(drop))?;
        db.read(move |c| feed::page(c, query.since, &id, limit))
    })
    .await;
    with_device(
        match served {
            Ok(page) => Json(page).into_response(),
            Err(error) => error_response(&error),
        },
        device,
    )
}

/// The till's WebSocket: the feed head at once and each time the main PC commits feed rows.
async fn feed_socket(
    State(shared): State<Arc<Shared>>,
    Extension(device): Extension<Device>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let (head, stop) = (shared.head.clone(), shared.stop.clone());
    with_device(
        upgrade.on_upgrade(move |socket| run_feed(socket, head, stop)),
        device,
    )
}

async fn run_feed(
    mut socket: WebSocket,
    mut head: tokio::sync::watch::Receiver<i64>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let text = |n: i64| Message::Text(serde_json::json!({ "head": n }).to_string().into());
    let first = *head.borrow_and_update();
    if socket.send(text(first)).await.is_err() {
        return;
    }
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(20));
    ping.tick().await;
    loop {
        tokio::select! {
            changed = head.changed() => {
                if changed.is_err() {
                    break;
                }
                let n = *head.borrow_and_update();
                if socket.send(text(n)).await.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                if socket.send(Message::Ping(axum::body::Bytes::new())).await.is_err() {
                    break;
                }
            }
            message = socket.recv() => match message {
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            () = stopped(&mut stop) => {
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
        }
    }
}

/// Ends when the server stops (or is gone). The `watch::Ref` of `wait_for` is dropped inside, so
/// the socket's future stays `Send`.
async fn stopped(stop: &mut tokio::sync::watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}

/// A till's outbox items, in order, one host transaction each (`replay::apply_batch`), with the
/// host's reception time and the till's measured clock offset.
async fn replay(
    State(shared): State<Arc<Shared>>,
    Extension(device): Extension<Device>,
    headers: HeaderMap,
    Json(request): Json<ReplayRequest>,
) -> Response {
    if request.items.len() > MAX_REPLAY_ITEMS {
        return with_device(
            error_response(&AppError::validation("items", "value_invalid")),
            device,
        );
    }
    if till_schema(&headers).is_some_and(|s| s > shared.config.schema_version) {
        return with_device(
            error_response(&AppError::conflict("lan_version_mismatch")),
            device,
        );
    }
    let received = replay::Received {
        at: (shared.services.clock)(),
        skew_ms: headers
            .get(H_SKEW)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok()),
    };
    let sender = replay::Sender {
        terminal_id: device.terminal_id.clone(),
        code: device.code.clone(),
    };
    let db = shared.services.db.clone();
    let applied =
        blocking(move || replay::apply_batch(&db, &sender, request.items, &received)).await;
    with_device(
        match applied {
            Ok(results) => Json(ReplayResponse { results }).into_response(),
            Err(error) => error_response(&error),
        },
        device,
    )
}

/// A host service for the till and its signed-in user, answered with the feed head after it.
async fn rpc(
    State(shared): State<Arc<Shared>>,
    Extension(device): Extension<Device>,
    Path(service): Path<String>,
    Json(request): Json<RpcRequest>,
) -> Response {
    let caller = crate::host::RpcCaller {
        terminal_id: device.terminal_id.clone(),
        terminal_code: device.code.clone(),
        user_id: request.user_id,
        approvals: request.approvals,
        idempotency_key: request.idempotency_key,
    };
    let (handler, db) = (shared.services.rpc.clone(), shared.services.db.clone());
    let answered = blocking(move || {
        let output = handler.call(caller, &service, request.input)?;
        let host_seq = db.read(feed::head)?;
        Ok(RpcResponse { output, host_seq })
    })
    .await;
    with_device(
        match answered {
            Ok(answer) => Json(answer).into_response(),
            Err(error) => error_response(&error),
        },
        device,
    )
}

#[cfg(test)]
mod tests {
    use crate::testing::{self, http};
    use crate::wire::{FeedMessage, ReplayRequest, ReplayResponse, RpcRequest, RpcResponse};
    use crate::wire::{H_SCHEMA, H_SEQ, H_SHA256, PairResponse};
    use pos_core::db::migrations::LATEST_VERSION;
    use pos_core::sync::devices::PairRequest;
    use pos_core::sync::feed::FeedPage;
    use pos_core::sync::outbox::{self, ItemStatus};

    fn sell(till: &testing::Till, ctx: &pos_core::till::TillContext, product: &str) {
        let (ctx, product) = (ctx.clone(), product.to_owned());
        till.db
            .write(move |tx| {
                pos_core::sales::create_sale(
                    tx,
                    &ctx,
                    pos_core::sales::SaleInput {
                        cart: pos_core::sales::Cart::new(
                            pos_core::lang::Lang::Fr,
                            vec![pos_core::sales::CartLine::product(
                                &product,
                                pos_core::money::Qty(1_000),
                            )],
                        ),
                        payments: vec![pos_core::payments::PaymentInput::cash(
                            pos_core::money::Money(10_500),
                        )],
                    },
                )
            })
            .unwrap();
    }

    #[test]
    fn changes_serve_the_feed_and_move_the_tills_reader() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let till = testing::till(dir.path(), &host);
        let since: i64 = till
            .db
            .read(|c| Ok(c.query_row("SELECT applied_seq FROM lan_local", [], |r| r.get(0))?))
            .unwrap();
        host.product("Lait", 10_000);
        let page: FeedPage = http()
            .get(format!("{}/changes?since={since}", host.base()))
            .bearer_auth(&till.token)
            .header(crate::wire::H_SCHEMA, LATEST_VERSION.to_string())
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert!(
            page.rows
                .iter()
                .any(|r| r.table == "products" && r.image["name_fr"] == "Lait")
        );
        let next = page.next_since;
        http()
            .get(format!("{}/changes?since={next}", host.base()))
            .bearer_auth(&till.token)
            .header(crate::wire::H_SCHEMA, LATEST_VERSION.to_string())
            .send()
            .unwrap();
        let consumer = pos_core::sync::changelog::lan_consumer(&till.terminal.id);
        let position = host
            .db
            .read(move |c| pos_core::sync::changelog::position(c, &consumer))
            .unwrap();
        assert_eq!(position, Some(next));
        let old = http()
            .get(format!("{}/changes?since={next}", host.base()))
            .bearer_auth(&till.token)
            .header(crate::wire::H_SCHEMA, (LATEST_VERSION - 1).to_string())
            .send()
            .unwrap();
        assert_eq!(old.status(), 409, "another schema does not get the feed");
    }

    #[test]
    fn replay_applies_in_order_and_answers_a_resend_from_the_inbox() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let milk = host.product("Lait", 10_000);
        let till = testing::till(dir.path(), &host);
        let since: i64 = till
            .db
            .read(|c| Ok(c.query_row("SELECT applied_seq FROM lan_local", [], |r| r.get(0))?))
            .unwrap();
        let page: FeedPage = http()
            .get(format!("{}/changes?since={since}", host.base()))
            .bearer_auth(&till.token)
            .header(crate::wire::H_SCHEMA, LATEST_VERSION.to_string())
            .send()
            .unwrap()
            .json()
            .unwrap();
        let me = till.terminal.id.clone();
        till.db
            .write_replica(move |tx| pos_core::sync::apply::apply_page(tx, &me, &page, 0))
            .unwrap();
        let ctx = till.open_session();
        sell(&till, &ctx, &milk);
        let items = till.db.read(|c| outbox::pending(c, 100)).unwrap();
        assert_eq!(items.len(), 3, "paired, session, sale");
        let post = |items: Vec<pos_core::sync::outbox::WireItem>| {
            http()
                .post(format!("{}/replay", host.base()))
                .bearer_auth(&till.token)
                .header(crate::wire::H_SKEW, "250")
                .json(&ReplayRequest { items })
                .send()
                .unwrap()
        };
        let answer: ReplayResponse = post(items.clone()).json().unwrap();
        assert!(
            answer
                .results
                .iter()
                .all(|r| r.status == ItemStatus::Applied),
            "{answer:?}"
        );
        let again: ReplayResponse = post(items.clone()).json().unwrap();
        assert!(
            again
                .results
                .iter()
                .all(|r| r.status == ItemStatus::Duplicate)
        );
        assert_eq!(
            host.db
                .read(|c| Ok(c.query_row(
                    "SELECT COUNT(*) FROM documents WHERE series = 'C2'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?))
                .unwrap(),
            1
        );
        let skew: i64 = host
            .db
            .read(|c| Ok(c.query_row("SELECT skew_ms FROM lan_inbox LIMIT 1", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(skew, 250);
    }

    #[test]
    fn an_item_after_a_gap_answers_409_with_the_expected_seq() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let till = testing::till(dir.path(), &host);
        till.open_session();
        let items = till.db.read(|c| outbox::pending(c, 100)).unwrap();
        http()
            .post(format!("{}/replay", host.base()))
            .bearer_auth(&till.token)
            .json(&ReplayRequest {
                items: vec![items[0].clone()],
            })
            .send()
            .unwrap();
        let mut late = items[1].clone();
        late.local_seq += 5;
        late.bundle.key = format!("tx:{}", pos_core::ids::new_id());
        let response = http()
            .post(format!("{}/replay", host.base()))
            .bearer_auth(&till.token)
            .json(&ReplayRequest { items: vec![late] })
            .send()
            .unwrap();
        assert_eq!(response.status(), 409);
        let body: pos_core::error::ErrorPayload = response.json().unwrap();
        assert_eq!(body.code, "replay_out_of_order");
        assert_eq!(body.details.unwrap()["expected"], items[1].local_seq);
    }

    #[test]
    fn the_feed_socket_pokes_when_the_main_pc_writes() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let token = host.pair("C2");
        let url = format!("ws://{}/feed", host.server.as_ref().unwrap().local_addr());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            use futures_util::StreamExt;
            use tokio_tungstenite::tungstenite::client::IntoClientRequest;
            let mut request = url.into_client_request().unwrap();
            request.headers_mut().insert(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {token}").parse().unwrap(),
            );
            let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
            let first = socket.next().await.unwrap().unwrap();
            let first: FeedMessage = serde_json::from_str(first.to_text().unwrap()).unwrap();
            let db = host.db.clone();
            tokio::task::spawn_blocking(move || {
                let actor = pos_core::audit::Actor::default();
                db.write(move |tx| {
                    pos_core::settings::set_value(
                        tx,
                        &actor,
                        "stock.negative_policy",
                        serde_json::json!("block"),
                    )
                })
                .unwrap();
            })
            .await
            .unwrap();
            let poke = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) =
                        socket.next().await
                    {
                        return serde_json::from_str::<FeedMessage>(&text).unwrap();
                    }
                }
            })
            .await
            .unwrap();
            assert!(poke.head > first.head);
        });
    }

    struct Echo;

    impl crate::host::RpcHandler for Echo {
        fn call(
            &self,
            caller: crate::host::RpcCaller,
            service: &str,
            input: serde_json::Value,
        ) -> Result<serde_json::Value, pos_core::error::AppError> {
            Ok(
                serde_json::json!({ "service": service, "till": caller.terminal_code, "user": caller.user_id, "input": input }),
            )
        }
    }

    #[test]
    fn rpc_calls_the_handler_as_the_till_and_its_user() {
        let dir = tempfile::tempdir().unwrap();
        let mut host = testing::host(dir.path());
        host.server.take().unwrap().stop();
        let mut services = testing::services(&host.db, &host.desk, &host.terminal_id);
        services.rpc = std::sync::Arc::new(Echo);
        host.server = Some(crate::host::HostServer::start(host.config.clone(), services).unwrap());
        let token = host.pair("C2");
        let answer: RpcResponse = http()
            .post(format!("{}/rpc/products.update", host.base()))
            .bearer_auth(&token)
            .json(&RpcRequest {
                user_id: "u-owner".into(),
                input: serde_json::json!({ "id": "p1" }),
                approvals: Vec::new(),
                idempotency_key: None,
            })
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(answer.output["service"], "products.update");
        assert_eq!(answer.output["till"], "C2");
        assert_eq!(answer.output["user"], "u-owner");
        assert!(answer.host_seq >= 0);
    }

    struct Grants;

    impl crate::host::RpcHandler for Grants {
        fn call(
            &self,
            caller: crate::host::RpcCaller,
            _: &str,
            _: serde_json::Value,
        ) -> Result<serde_json::Value, pos_core::error::AppError> {
            Ok(serde_json::to_value(caller.approvals)?)
        }
    }

    /// Task 20 review: the manager approvals pending for the till's user travel with the call,
    /// for the main PC to confirm; a call from an older till carries none.
    #[test]
    fn rpc_hands_the_tills_approvals_to_the_handler() {
        use crate::wire::RpcGrant;
        use pos_core::auth::Permission;
        let dir = tempfile::tempdir().unwrap();
        let mut host = testing::host(dir.path());
        host.server.take().unwrap().stop();
        let mut services = testing::services(&host.db, &host.desk, &host.terminal_id);
        services.rpc = std::sync::Arc::new(Grants);
        host.server = Some(crate::host::HostServer::start(host.config.clone(), services).unwrap());
        let token = host.pair("C2");
        let grant = RpcGrant {
            approver_id: "u-manager".into(),
            permission: Permission::SalesReturn,
            granted_at: 1_790_593_200_000,
        };
        let answer: RpcResponse = http()
            .post(format!("{}/rpc/deposits.refund", host.base()))
            .bearer_auth(&token)
            .json(&RpcRequest {
                user_id: "u-cashier".into(),
                input: serde_json::json!({}),
                approvals: vec![grant.clone()],
                idempotency_key: None,
            })
            .send()
            .unwrap()
            .json()
            .unwrap();
        let carried: Vec<RpcGrant> = serde_json::from_value(answer.output).unwrap();
        assert_eq!(carried, vec![grant]);
        let older: RpcResponse = http()
            .post(format!("{}/rpc/deposits.refund", host.base()))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "userId": "u-cashier", "input": {} }))
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(older.output, serde_json::json!([]));
    }

    fn request(code: &str) -> PairRequest {
        PairRequest {
            code: code.into(),
            terminal_code: "C2".into(),
            terminal_name: "Caisse 2".into(),
            app_version: "0.5.0".into(),
            schema_version: LATEST_VERSION,
            install_id: pos_core::ids::new_id(),
            terminal_id: None,
        }
    }

    #[test]
    fn a_code_becomes_a_token_once() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let code = host
            .desk
            .lock()
            .unwrap()
            .issue(pos_core::time::now_ms())
            .unwrap()
            .code;
        let url = format!("{}/pair", host.base());
        let body = request(&code);
        let ok = http().post(&url).json(&body).send().unwrap();
        assert_eq!(ok.status(), 200);
        let paired: PairResponse = ok.json().unwrap();
        assert_eq!(paired.terminal.code, "C2");
        assert_eq!(paired.token.len(), 64);
        assert_eq!(paired.host_id, host.config.host_id);
        let actor: Option<String> = host
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT terminal_id FROM audit_log WHERE action = 'lan.till_paired'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            actor.as_deref(),
            Some(host.terminal_id.as_str()),
            "journaled with the main PC's terminal as actor"
        );
        let replay = http().post(&url).json(&body).send().unwrap();
        assert_eq!(replay.status(), 409);
        let err: pos_core::error::ErrorPayload = replay.json().unwrap();
        assert_eq!(
            err.code, "pairing_code_invalid",
            "a captured request finds its code spent"
        );
    }

    #[test]
    fn pair_is_rate_limited_per_address() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let url = format!("{}/pair", host.base());
        let mut statuses = Vec::new();
        for _ in 0..11 {
            statuses.push(
                http()
                    .post(&url)
                    .json(&request("000000"))
                    .send()
                    .unwrap()
                    .status()
                    .as_u16(),
            );
        }
        assert!(statuses[..10].iter().all(|s| *s == 409), "{statuses:?}");
        assert_eq!(statuses[10], 429);
    }

    #[test]
    fn the_snapshot_streams_with_its_seq_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let token = host.pair("C2");
        let response = http()
            .get(format!("{}/snapshot", host.base()))
            .bearer_auth(&token)
            .send()
            .unwrap();
        assert_eq!(response.status(), 200);
        let seq: i64 = response.headers()[H_SEQ].to_str().unwrap().parse().unwrap();
        let sha = response.headers()[H_SHA256].to_str().unwrap().to_owned();
        assert_eq!(
            response.headers()[H_SCHEMA].to_str().unwrap(),
            LATEST_VERSION.to_string()
        );
        let length = response.content_length();
        let bytes = response.bytes().unwrap();
        assert_eq!(length, Some(u64::try_from(bytes.len()).unwrap()));
        let file = dir.path().join("download.db");
        std::fs::write(&file, &bytes).unwrap();
        assert_eq!(
            pos_core::sync::snapshot::verify_download(&file, &sha, seq).unwrap(),
            LATEST_VERSION
        );
        assert!(seq >= 0);
        let anchored: bool = host
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT d.journal_anchor_seq = a.seq AND d.journal_anchor_hash = a.hash
                     FROM lan_devices d,
                          (SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1) a",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert!(anchored, "the export records the till's journal anchor");
        let unauthorised = http()
            .get(format!("{}/snapshot", host.base()))
            .send()
            .unwrap();
        assert_eq!(unauthorised.status(), 401);
    }

    #[test]
    fn a_revoked_token_is_refused_on_every_route() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let token = host.pair("C2");
        let id: String = host
            .db
            .read(|c| Ok(c.query_row("SELECT terminal_id FROM lan_devices", [], |r| r.get(0))?))
            .unwrap();
        host.db
            .write(move |tx| {
                pos_core::sync::devices::revoke(tx, &pos_core::audit::Actor::default(), &id, 1)
            })
            .unwrap();
        for route in [
            "/whoami",
            "/snapshot",
            "/changes?since=0",
            "/updates/manifest",
            "/updates/setup",
        ] {
            let response = http()
                .get(format!("{}{route}", host.base()))
                .bearer_auth(&token)
                .send()
                .unwrap();
            assert_eq!(response.status(), 401, "{route}");
        }
    }

    /// The main PC serves the update it verified to its tills (their token only), from
    /// its fixed cache; nothing before it has one.
    #[test]
    fn the_main_pc_mirrors_its_verified_update_to_its_tills() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let token = host.pair("C2");
        let api = crate::client::api::HostApi::new(
            &host.base(),
            Some(token.clone()),
            "0.5.0",
            pos_core::db::migrations::LATEST_VERSION,
        )
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            runtime.block_on(api.update_manifest()).unwrap(),
            None,
            "nothing cached yet"
        );
        let cache = &host.config.update_dir;
        std::fs::create_dir_all(cache).unwrap();
        let manifest = serde_json::json!({ "version": "0.8.0", "platforms": {} });
        std::fs::write(
            cache.join(super::super::UPDATE_MANIFEST),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(
            cache.join(super::super::UPDATE_SETUP),
            b"the verified setup",
        )
        .unwrap();
        assert_eq!(
            runtime.block_on(api.update_manifest()).unwrap(),
            Some(manifest)
        );
        let mut seen = Vec::new();
        let bytes = runtime
            .block_on(api.update_setup(&mut |received, total| seen.push((received, total))))
            .unwrap();
        assert_eq!(bytes, b"the verified setup");
        assert_eq!(seen.last(), Some(&(18, Some(18))));
        let anonymous = http()
            .get(format!("{}/updates/setup", host.base()))
            .send()
            .unwrap();
        assert_eq!(anonymous.status(), 401, "a till's token only");
    }
}
