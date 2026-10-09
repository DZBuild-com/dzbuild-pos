//! The till's engine (spec §4.6): one thread, one current-thread Tokio runtime. Each cycle
//! checks the main PC (identity, clock, schema) when due, replays the outbox in order, pulls the
//! feed until caught up, then waits for a poke: the main PC's WebSocket `/feed` when it writes,
//! the outbox hook when this till queued an item, `sync_now`. Without a live socket it polls
//! every 5 s, with one every 30 s; errors back off 1, 2, 5, 10, 30 s. Selling never goes through
//! here, and the database is only touched on blocking tasks.

use super::api::HostApi;
use crate::host::routes::blocking;
use crate::wire::{FeedMessage, RpcGrant};
use pos_core::db::Db;
use pos_core::error::AppError;
use pos_core::sync::apply::{ApplyReport, apply_page};
use pos_core::sync::outbox::{self, ItemResult, ItemStatus, OutboxDepth};
use pos_core::sync::status::{ClientStatus, LinkState, client_status};
use pos_core::sync::{role, skew};
use pos_core::time::now_ms;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, watch};

/// Items per replay request (the main PC takes up to `wire::MAX_REPLAY_ITEMS`).
const REPLAY_BATCH: usize = 50;
/// How often the main PC's identity, clock and schema are checked while all goes well; the
/// cycle after an error, and the one a `sync_now` starts, check again. The cycles pokes start in
/// between only replay and pull, so a busy shop never runs into the main PC's limit on `/health`.
const CHECK_EVERY: Duration = Duration::from_secs(30);
/// The wait between two cycles with a live WebSocket (it pokes on every change), and without.
const POLL_WITH_SOCKET: Duration = Duration::from_secs(30);
const POLL_WITHOUT_SOCKET: Duration = Duration::from_secs(5);
/// Opening the WebSocket gives up after this long.
const SOCKET_CONNECT: Duration = Duration::from_secs(5);
/// A WebSocket silent this long is dead (the main PC pings every 20 s) and is opened again.
const SOCKET_SILENCE: Duration = Duration::from_secs(45);
/// How long a host service waits for the outbox to empty, then for its answer to arrive.
const RPC_SETTLE: Duration = Duration::from_secs(5);
/// After a stop, how long the runtime waits for a database task still running.
const RUNTIME_SHUTDOWN: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct ClientConfig {
    pub base: String,
    pub host_id: String,
    pub terminal_id: String,
    pub token: String,
    pub app_version: String,
    pub schema_version: i64,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("base", &self.base)
            .field("terminal_id", &self.terminal_id)
            .field("token", &"<hidden>")
            .finish_non_exhaustive()
    }
}

/// The till's outbox is empty, so a host service sees what the till did; `replay_rejected` when
/// the line is stopped at a refused item (the items after it never leave).
fn outbox_drained(db: &Db) -> Result<bool, AppError> {
    let depth = db.read(outbox::depth)?;
    if depth.rejected > 0 {
        return Err(AppError::conflict("replay_rejected"));
    }
    Ok(depth.pending == 0)
}

/// A host service's answer ([`LanClient::rpc_blocking`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RpcReply {
    pub output: Value,
    /// The main PC's change is in the till's copy: a caller that reads it back there may.
    pub caught_up: bool,
}

/// What the shell hears from the engine: the link changed, a feed page was applied.
pub trait ClientEvents: Send + Sync + 'static {
    fn status(&self, status: &ClientStatus);
    fn applied(&self, report: &ApplyReport);
}

/// Retry delays after an error: 1, 2, 5, 10, then 30 s.
pub fn backoff_ms(attempt: u32) -> u64 {
    crate::host::backoff_ms(attempt)
}

fn offline() -> AppError {
    AppError::Sync {
        code: "lan_offline".into(),
    }
}

#[derive(Debug, Clone)]
struct EngineState {
    link: LinkState,
    last_error: Option<String>,
    head: Option<i64>,
    skew_ms: Option<i64>,
    socket: bool,
    /// When the main PC was last checked; `None` until then and after an error.
    checked_at: Option<Instant>,
}

struct Engine {
    db: Db,
    api: HostApi,
    config: ClientConfig,
    state: Arc<Mutex<EngineState>>,
    events: Arc<dyn ClientEvents>,
    wake: Arc<Notify>,
}

/// Ends when `stop` turns `true` (or its sender is gone). The `watch::Ref` is dropped inside,
/// so the futures that wait on it stay `Send`.
async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}

impl Engine {
    fn update(&self, change: impl FnOnce(&mut EngineState)) {
        if let Ok(mut state) = self.state.lock() {
            change(&mut state);
        }
    }

    fn get<T>(&self, read: impl FnOnce(&EngineState) -> T) -> Option<T> {
        self.state.lock().ok().map(|state| read(&state))
    }

    /// Changes the link; the shell hears of it when the link or its error changed.
    async fn set(&self, link: LinkState, error: Option<String>) {
        let changed = self.state.lock().is_ok_and(|mut s| {
            let changed = s.link != link || s.last_error != error;
            s.link = link;
            s.last_error = error;
            changed
        });
        if changed {
            let (db, state) = (self.db.clone(), self.state.clone());
            if let Ok(status) = blocking(move || snapshot_status(&db, &state)).await {
                self.events.status(&status);
            }
        }
    }

    async fn depth(&self) -> Result<OutboxDepth, AppError> {
        let db = self.db.clone();
        blocking(move || db.read(outbox::depth)).await
    }

    /// The main PC's identity, its clock (recorded, `lan.clock_skew` journaled once per
    /// episode) and its schema.
    async fn check(&self) -> Result<(), AppError> {
        let (info, skew_ms) = self.api.health().await?;
        if info.host_id != self.config.host_id {
            // Another main PC answers at this address: this till's token means nothing there.
            return Err(AppError::auth("lan_token_invalid"));
        }
        self.update(|s| s.skew_ms = Some(skew_ms));
        let (db, terminal, schema) = (
            self.db.clone(),
            self.config.terminal_id.clone(),
            info.schema_version,
        );
        blocking(move || {
            db.write(move |tx| {
                role::set_contact(tx, now_ms(), Some(skew_ms), Some(schema))?;
                skew::note(tx, &terminal, skew_ms, now_ms()).map(drop)
            })
        })
        .await?;
        if info.schema_version != self.config.schema_version {
            return Err(AppError::conflict("lan_version_mismatch"));
        }
        self.update(|s| s.checked_at = Some(Instant::now()));
        Ok(())
    }

    /// Sends the pending items in order until none is left or the line stops at a refusal.
    /// A main PC that lost items (`replay_out_of_order`) gets them again from the seq it expects.
    async fn replay(&self) -> Result<(), AppError> {
        let mut resend_from: Option<i64> = None;
        loop {
            let db = self.db.clone();
            let items = blocking(move || match resend_from {
                Some(seq) => db.read(move |c| outbox::from_seq(c, seq, REPLAY_BATCH)),
                None => db.read(|c| outbox::pending(c, REPLAY_BATCH)),
            })
            .await?;
            if items.is_empty() {
                return Ok(());
            }
            self.set(LinkState::Replaying, None).await;
            let pending = self.depth().await?.pending;
            let results = match self.api.replay(items, pending, self.skew()).await {
                Ok(results) => results,
                Err(error) if error.code() == "replay_out_of_order" => {
                    let expected = error.payload().details.and_then(|d| d["expected"].as_i64());
                    match expected {
                        Some(seq) if resend_from != Some(seq) => {
                            resend_from = Some(seq);
                            continue;
                        }
                        _ => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            };
            if results.is_empty() {
                return Err(AppError::Internal(
                    "the main PC answered a replay without results".into(),
                ));
            }
            resend_from = None;
            let (db, terminal, host) = (
                self.db.clone(),
                self.config.terminal_id.clone(),
                self.config.host_id.clone(),
            );
            blocking(move || record(&db, &terminal, &host, results)).await?;
        }
    }

    /// Applies the feed page by page (one replica transaction each) until caught up.
    async fn pull(&self) -> Result<(), AppError> {
        loop {
            let db = self.db.clone();
            let since = blocking(move || db.read(|c| Ok(role::current(c)?.applied_seq))).await?;
            let pending = self.depth().await?.pending;
            let page = self.api.changes(since, pending, self.skew()).await?;
            self.update(|s| s.head = Some(page.head));
            let (more, next) = (page.more, page.next_since);
            if next > since || !page.rows.is_empty() {
                let (db, terminal) = (self.db.clone(), self.config.terminal_id.clone());
                let report = blocking(move || {
                    db.write_replica(move |tx| apply_page(tx, &terminal, &page, now_ms()))
                })
                .await?;
                self.events.applied(&report);
            }
            if !more || next <= since {
                return Ok(());
            }
            self.set(LinkState::Behind, None).await;
        }
    }

    fn skew(&self) -> Option<i64> {
        self.get(|s| s.skew_ms).flatten()
    }

    async fn cycle(&self) -> Result<(), AppError> {
        let due = self
            .get(|s| s.checked_at.is_none_or(|at| at.elapsed() >= CHECK_EVERY))
            .unwrap_or(true);
        if due {
            match self.check().await {
                Ok(()) => {}
                Err(error) if error.code() == "lan_version_mismatch" => {
                    // A till never newer than its main PC still sends what it did; the feed
                    // waits for the update.
                    self.replay().await?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
        }
        self.replay().await?;
        self.pull().await?;
        if self.depth().await?.rejected > 0 {
            self.set(LinkState::Attention, Some("replay_rejected".into()))
                .await;
        } else {
            self.set(LinkState::Online, None).await;
        }
        Ok(())
    }

    async fn failed(&self, error: &AppError) {
        let code = error.code();
        let link = match code.as_str() {
            "lan_offline" => LinkState::Offline,
            "lan_version_mismatch" => LinkState::VersionMismatch,
            _ => LinkState::Attention,
        };
        if link != LinkState::Offline {
            tracing::warn!(code = %code, "the LAN link needs attention");
        }
        self.update(|s| s.checked_at = None);
        self.set(link, Some(code)).await;
        if link == LinkState::Offline {
            // The head of the line waited through this attempt: it is counted there.
            let db = self.db.clone();
            let counted = blocking(move || {
                let first = db.read(|c| outbox::pending(c, 1))?;
                match first.first().map(|item| item.local_seq) {
                    Some(seq) => db.write(move |tx| outbox::record_failure(tx, seq, "lan_offline")),
                    None => Ok(()),
                }
            })
            .await;
            if let Err(error) = counted {
                tracing::warn!(code = %error.code(), "a failed attempt could not be counted");
            }
        }
    }

    async fn run(self: Arc<Self>, mut stop: watch::Receiver<bool>) {
        let socket = tokio::spawn(feed_socket(self.clone(), stop.clone()));
        let mut attempt = 0_u32;
        loop {
            // A stop never waits for a call in flight: a replay cut short is sent again and
            // answered as a duplicate.
            let outcome = tokio::select! {
                outcome = self.cycle() => outcome,
                () = stopped(&mut stop) => break,
            };
            let wait = match outcome {
                Ok(()) => {
                    attempt = 0;
                    if self.get(|s| s.socket).unwrap_or(false) {
                        POLL_WITH_SOCKET
                    } else {
                        POLL_WITHOUT_SOCKET
                    }
                }
                Err(error) => {
                    self.failed(&error).await;
                    let delay = backoff_ms(attempt);
                    attempt = attempt.saturating_add(1);
                    Duration::from_millis(delay)
                }
            };
            tokio::select! {
                () = self.wake.notified() => {}
                () = tokio::time::sleep(wait) => {}
                () = stopped(&mut stop) => break,
            }
        }
        socket.abort();
    }
}

/// Takes the main PC's answers in order: its corrections first (`reconcile`, triggers off like
/// every host image), then the items marked sent in one transaction. A refused item is kept as
/// `rejected`; `outbox::pending` never goes past it, so the line stops there.
fn record(
    db: &Db,
    terminal_id: &str,
    host_id: &str,
    results: Vec<ItemResult>,
) -> Result<(), AppError> {
    let (mut sent, mut refused) = (Vec::new(), None);
    for result in results {
        if result.status == ItemStatus::Rejected {
            let code = result.code.unwrap_or_else(|| "replay_invalid".into());
            refused = Some((result.local_seq, code));
            break;
        }
        if !result.adjustments.is_empty() {
            let (me, adjustments) = (terminal_id.to_owned(), result.adjustments);
            db.write_replica(move |tx| {
                outbox::reconcile(tx, &me, &adjustments, now_ms()).map(drop)
            })?;
        }
        sent.push(result.local_seq);
    }
    let host = host_id.to_owned();
    db.write(move |tx| {
        for seq in sent {
            outbox::mark_sent(tx, seq, &host, now_ms())?;
        }
        match refused {
            Some((seq, code)) => outbox::mark_rejected(tx, seq, &code),
            None => Ok(()),
        }
    })
}

/// The main PC's WebSocket: each `{"head"}` pokes the engine. Opened again with backoff when it
/// fails, closes, or stays silent past the main PC's pings.
async fn feed_socket(engine: Arc<Engine>, mut stop: watch::Receiver<bool>) {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut attempt = 0_u32;
    loop {
        let connect = async {
            let mut request = engine.api.feed_url().into_client_request().ok()?;
            let bearer = format!("Bearer {}", engine.api.token()?);
            request
                .headers_mut()
                .insert("authorization", bearer.parse().ok()?);
            tokio_tungstenite::connect_async(request)
                .await
                .ok()
                .map(|(socket, _)| socket)
        };
        let connected = tokio::select! {
            connected = tokio::time::timeout(SOCKET_CONNECT, connect) => connected.ok().flatten(),
            () = stopped(&mut stop) => return,
        };
        if let Some(mut socket) = connected {
            attempt = 0;
            engine.update(|s| s.socket = true);
            loop {
                tokio::select! {
                    message = tokio::time::timeout(SOCKET_SILENCE, socket.next()) => match message {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            if let Ok(message) = serde_json::from_str::<FeedMessage>(&text) {
                                engine.update(|s| s.head = Some(message.head));
                                engine.wake.notify_one();
                            }
                        }
                        Ok(Some(Ok(_))) => {}
                        Ok(Some(Err(_)) | None) | Err(_) => break,
                    },
                    () = stopped(&mut stop) => return,
                }
            }
            // The main PC may be gone: a cycle now says so, instead of after the 30 s of a socket.
            engine.update(|s| s.socket = false);
            engine.wake.notify_one();
        }
        let delay = Duration::from_millis(backoff_ms(attempt));
        attempt = attempt.saturating_add(1);
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = stopped(&mut stop) => return,
        }
    }
}

fn snapshot_status(db: &Db, state: &Mutex<EngineState>) -> Result<ClientStatus, AppError> {
    let s = state
        .lock()
        .map(|s| s.clone())
        .map_err(|_| AppError::Internal("LAN state poisoned".into()))?;
    let measured = s.skew_ms;
    let mut status = db.read(move |c| client_status(c, s.link, s.head, s.last_error))?;
    if status.skew_ms.is_none() {
        status.skew_ms = measured;
    }
    Ok(status)
}

/// The engine of a till, running on its own thread until `stop`.
pub struct LanClient {
    handle: tokio::runtime::Handle,
    wake: Arc<Notify>,
    stop: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
    state: Arc<Mutex<EngineState>>,
    api: HostApi,
    db: Db,
}

impl LanClient {
    /// Starts the engine for this till. `wake` is the one its outbox hook pokes.
    pub fn start(
        db: Db,
        config: ClientConfig,
        wake: Arc<Notify>,
        events: Arc<dyn ClientEvents>,
    ) -> Result<LanClient, AppError> {
        let api = HostApi::new(
            &config.base,
            Some(config.token.clone()),
            &config.app_version,
            config.schema_version,
        )?;
        let state = Arc::new(Mutex::new(EngineState {
            link: LinkState::Connecting,
            last_error: None,
            head: None,
            skew_ms: None,
            socket: false,
            checked_at: None,
        }));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let handle = runtime.handle().clone();
        let (stop, stop_rx) = watch::channel(false);
        let engine = Arc::new(Engine {
            db: db.clone(),
            api: api.clone(),
            config,
            state: state.clone(),
            events,
            wake: wake.clone(),
        });
        let thread = std::thread::Builder::new()
            .name("pos-lan-till".into())
            .spawn(move || {
                runtime.block_on(engine.run(stop_rx));
                runtime.shutdown_timeout(RUNTIME_SHUTDOWN);
            })?;
        Ok(LanClient {
            handle,
            wake,
            stop,
            thread: Some(thread),
            state,
            api,
            db,
        })
    }

    pub fn status(&self) -> Result<ClientStatus, AppError> {
        snapshot_status(&self.db, &self.state)
    }

    pub fn is_online(&self) -> bool {
        self.state.lock().is_ok_and(|s| {
            matches!(
                s.link,
                LinkState::Online | LinkState::Replaying | LinkState::Behind
            )
        })
    }

    pub fn skew_ms(&self) -> Option<i64> {
        self.state.lock().ok().and_then(|s| s.skew_ms)
    }

    /// The main PC's WebSocket is open (tests).
    #[cfg(test)]
    pub(crate) fn has_socket(&self) -> bool {
        self.state.lock().is_ok_and(|s| s.socket)
    }

    /// A whole cycle now, the check of the main PC (identity, clock, schema) included: someone
    /// asked for it, unlike the pokes of the outbox hook and the WebSocket, which only replay and
    /// pull.
    pub fn sync_now(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.checked_at = None;
        }
        self.wake.notify_one();
    }

    /// Pokes the engine once (a whole cycle starts after the poke), then waits for `done`.
    fn settle(&self, done: impl Fn(&Db) -> Result<bool, AppError>) -> Result<(), AppError> {
        if done(&self.db)? {
            return Ok(());
        }
        self.wake.notify_one();
        let deadline = Instant::now() + RPC_SETTLE;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            if done(&self.db)? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(offline());
            }
        }
    }

    /// A host service for this till (catalogue edits, a customer, an invoice for a ticket): the
    /// outbox goes first so the main PC sees what the till did, and the call waits a moment for
    /// its effect to reach the till's copy. `host_required` while the link is not up;
    /// `replay_rejected` at once when the till's line is stopped at a refused item (its items
    /// after it never leave, so the main PC would not see them). Once the main PC has answered,
    /// its answer is returned even when the till's copy is still catching up
    /// ([`RpcReply::caught_up`]): the write is done there, and a retry would make it again.
    /// Blocking: call it from a blocking task, never from the engine's thread.
    pub fn rpc_blocking(
        &self,
        service: &str,
        user_id: &str,
        approvals: Vec<RpcGrant>,
        input: Value,
        timeout: Duration,
        idempotency_key: Option<String>,
    ) -> Result<RpcReply, AppError> {
        if !self.is_online() {
            return Err(AppError::conflict("host_required"));
        }
        self.settle(outbox_drained)?;
        let (tx, rx) = crossbeam_channel::bounded(1);
        let (api, service, user) = (self.api.clone(), service.to_owned(), user_id.to_owned());
        self.handle.spawn(async move {
            let _ = tx.send(
                api.rpc(&service, &user, approvals, input, idempotency_key)
                    .await,
            );
        });
        let answer = rx.recv_timeout(timeout).map_err(|_| offline())??;
        let host_seq = answer.host_seq;
        let caught_up = match self
            .settle(move |db| Ok(db.read(|c| Ok(role::current(c)?.applied_seq))? >= host_seq))
        {
            Ok(()) => true,
            Err(error) => {
                tracing::info!(code = %error.code(), host_seq, "the main PC answered; this till's copy is still catching up");
                false
            }
        };
        Ok(RpcReply {
            output: answer.output,
            caught_up,
        })
    }

    /// Stops the engine and waits for its thread (a call in flight is dropped).
    pub fn stop(mut self) {
        self.stop.send_replace(true);
        self.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for LanClient {
    fn drop(&mut self) {
        self.stop.send_replace(true);
        self.wake.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, Till};
    use std::time::{Duration, Instant};

    struct Quiet;

    impl ClientEvents for Quiet {
        fn status(&self, _: &ClientStatus) {}
        fn applied(&self, _: &ApplyReport) {}
    }

    fn engine(host: &testing::Host, till: &Till) -> (LanClient, Arc<Notify>) {
        let wake = Arc::new(Notify::new());
        let poke = wake.clone();
        till.db
            .set_write_hook(Some(Arc::new(pos_core::sync::outbox::OutboxHook::new(
                till.terminal.id.clone(),
                Arc::new(move || poke.notify_one()),
            ))));
        let client = LanClient::start(
            till.db.clone(),
            ClientConfig {
                base: host.base(),
                host_id: host.config.host_id.clone(),
                terminal_id: till.terminal.id.clone(),
                token: till.token.clone(),
                app_version: "0.5.0".into(),
                schema_version: pos_core::db::migrations::LATEST_VERSION,
            },
            wake.clone(),
            Arc::new(Quiet),
        )
        .unwrap();
        (client, wake)
    }

    fn eventually(what: &str, within: Duration, check: impl Fn() -> bool) {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if check() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("{what} did not happen within {within:?}");
    }

    fn count(db: &pos_core::db::Db, sql: &'static str) -> i64 {
        db.read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    fn sell(till: &Till, ctx: &pos_core::till::TillContext, product: &str) -> String {
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
            .unwrap()
            .document
            .number
            .unwrap_or_default()
    }

    #[test]
    fn the_engine_replays_then_pulls_and_reports_its_status() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let milk = host.product("Lait", 10_000);
        let till = testing::till(dir.path(), &host);
        let (client, _) = engine(&host, &till);
        eventually("the catalogue arrives", Duration::from_secs(5), || {
            count(
                &till.db,
                "SELECT COUNT(*) FROM products WHERE name_fr = 'Lait'",
            ) == 1
        });
        host.product("Pain", 1_000);
        eventually(
            "a poke brings the new product",
            Duration::from_secs(3),
            || {
                count(
                    &till.db,
                    "SELECT COUNT(*) FROM products WHERE name_fr = 'Pain'",
                ) == 1
            },
        );
        let ctx = till.open_session();
        sell(&till, &ctx, &milk);
        eventually(
            "the sale reaches the main PC",
            Duration::from_secs(3),
            || {
                count(
                    &host.db,
                    "SELECT COUNT(*) FROM documents WHERE series = 'C2'",
                ) == 1
            },
        );
        eventually("the till is up to date", Duration::from_secs(3), || {
            client
                .status()
                .is_ok_and(|s| s.link == LinkState::Online && s.pending == 0)
        });
        assert!(client.is_online());
        assert!(client.skew_ms().is_some_and(|s| s.abs() < 5_000));
        client.stop();
    }

    #[test]
    fn a_sale_commits_locally_when_the_host_drops_mid_push() {
        // A "main PC" that accepts connections and drops them: every push fails half-way.
        let dropper = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = dropper.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in dropper.incoming().flatten() {
                drop(stream);
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let mut host = testing::host(dir.path());
        let milk = host.product("Lait", 10_000);
        let till = testing::till(dir.path(), &host);
        let real = host.base();
        let flaky = testing::Host {
            config: crate::host::HostConfig {
                bind: address,
                ..host.config.clone()
            },
            ..testing::Host::detached(&host)
        };
        let (client, _) = engine(&flaky, &till);
        let ctx = till.open_session();
        let number = sell(&till, &ctx, &milk);
        assert_eq!(
            number, "TK2026-C2-000001",
            "the sale is committed on the till whatever the network does"
        );
        eventually(
            "the items are kept after the failed push",
            Duration::from_secs(5),
            || {
                till.db
                    .read(|c| pos_core::sync::outbox::get(c, 1))
                    .is_ok_and(|i| i.attempts >= 1)
            },
        );
        assert_eq!(
            till.db.read(pos_core::sync::outbox::depth).unwrap().pending,
            3,
            "paired, session, sale: all kept"
        );
        assert_eq!(client.status().unwrap().link, LinkState::Offline);
        client.stop();

        // The real main PC stops, the till sells again, the main PC comes back: nothing is lost.
        let (client, _) = engine(&host, &till);
        let addr = host.server.as_ref().unwrap().local_addr();
        host.server.take().unwrap().stop();
        sell(&till, &ctx, &milk);
        eventually("offline", Duration::from_secs(5), || {
            client.status().is_ok_and(|s| s.link == LinkState::Offline)
        });
        let config = crate::host::HostConfig {
            bind: addr,
            ..host.config.clone()
        };
        host.server = Some(
            crate::host::HostServer::start(
                config,
                testing::services(&host.db, &host.desk, &host.terminal_id),
            )
            .unwrap(),
        );
        client.sync_now();
        eventually(
            "both sales reach the main PC once",
            Duration::from_secs(15),
            || {
                count(
                    &host.db,
                    "SELECT COUNT(*) FROM documents WHERE series = 'C2'",
                ) == 2
            },
        );
        eventually("nothing left to send", Duration::from_secs(5), || {
            client.status().is_ok_and(|s| s.pending == 0)
        });
        let _ = real;
        client.stop();
    }

    #[test]
    fn the_engine_retries_with_backoff_and_keeps_the_order() {
        assert_eq!(
            (0..6).map(backoff_ms).collect::<Vec<_>>(),
            [1_000, 2_000, 5_000, 10_000, 30_000, 30_000]
        );
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let milk = host.product("Lait", 10_000);
        let till = testing::till(dir.path(), &host);
        let ctx = till.open_session();
        for _ in 0..3 {
            sell(&till, &ctx, &milk);
        }
        let (client, _) = engine(&host, &till);
        eventually("all replayed", Duration::from_secs(5), || {
            count(
                &host.db,
                "SELECT COUNT(*) FROM documents WHERE series = 'C2'",
            ) == 3
        });
        let seqs: Vec<i64> = host
            .db
            .read(|c| {
                let mut stmt =
                    c.prepare("SELECT local_seq FROM lan_inbox ORDER BY received_at, local_seq")?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<i64>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        let mut sorted = seqs.clone();
        sorted.sort_unstable();
        assert_eq!(seqs, sorted, "received in the till's order");
        client.stop();
    }

    #[test]
    fn a_refused_item_stops_the_line_and_asks_for_attention_while_the_till_sells() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let milk = host.product("Lait", 10_000);
        let till = testing::till(dir.path(), &host);
        let (client, _) = engine(&host, &till);
        eventually("online", Duration::from_secs(5), || {
            client
                .status()
                .is_ok_and(|s| s.link == LinkState::Online && s.pending == 0)
        });
        // An item the main PC must refuse: a till never writes the shared catalogue.
        let me = till.terminal.id.clone();
        till.db
            .write(move |tx| {
                let image = serde_json::json!({ "id": pos_core::ids::new_id(), "name_fr": "Faux" });
                pos_core::sync::outbox::enqueue(
                    tx,
                    &me,
                    vec![pos_core::sync::outbox::BundleRow {
                        table: "products".into(),
                        op: pos_core::sync::feed::Op::Insert,
                        image: image.as_object().cloned().unwrap_or_default(),
                    }],
                    Vec::new(),
                    pos_core::time::now_ms(),
                )
                .map(drop)
            })
            .unwrap();
        client.sync_now();
        eventually("attention", Duration::from_secs(5), || {
            client.status().is_ok_and(|s| {
                s.link == LinkState::Attention
                    && s.last_error.as_deref() == Some("replay_rejected")
                    && s.rejected == 1
            })
        });
        assert!(!client.is_online());
        let ctx = till.open_session();
        assert_eq!(sell(&till, &ctx, &milk), "TK2026-C2-000001");
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(
            count(
                &host.db,
                "SELECT COUNT(*) FROM documents WHERE series = 'C2'"
            ),
            0,
            "the line waits behind the refused item"
        );
        let status = client.status().unwrap();
        // waiting behind the refused item: its own journal row (lan.item_rejected, chained in
        // the till's audit log and sent like every row), the session opened and the ticket
        assert_eq!(
            (status.link, status.pending, status.rejected),
            (LinkState::Attention, 3, 1)
        );
        host.product("Pain", 1_000);
        eventually(
            "the main PC's changes still arrive",
            Duration::from_secs(5),
            || {
                count(
                    &till.db,
                    "SELECT COUNT(*) FROM products WHERE name_fr = 'Pain'",
                ) == 1
            },
        );
        client.stop();
    }

    #[test]
    fn a_revoked_till_stops_and_says_so_but_keeps_selling() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let milk = host.product("Lait", 10_000);
        let till = testing::till(dir.path(), &host);
        let (client, _) = engine(&host, &till);
        let id = till.terminal.id.clone();
        host.db
            .write(move |tx| {
                pos_core::sync::devices::revoke(tx, &pos_core::audit::Actor::default(), &id, 0)
            })
            .unwrap();
        client.sync_now();
        eventually("attention", Duration::from_secs(5), || {
            client.status().is_ok_and(|s| {
                s.link == LinkState::Attention
                    && s.last_error.as_deref() == Some("lan_token_invalid")
            })
        });
        let ctx = till.open_session();
        assert_eq!(sell(&till, &ctx, &milk), "TK2026-C2-000001");
        client.stop();
    }

    #[test]
    fn host_services_wait_for_the_outbox_and_for_their_answer_to_arrive() {
        let dir = tempfile::tempdir().unwrap();
        let mut host = testing::host(dir.path());
        host.server.take().unwrap().stop();
        let mut services = testing::services(&host.db, &host.desk, &host.terminal_id);
        services.rpc = Arc::new(testing::CreatesProduct {
            db: host.db.clone(),
        });
        host.server = Some(crate::host::HostServer::start(host.config.clone(), services).unwrap());
        let head = host.server.as_ref().unwrap().head_watch();
        host.db
            .set_write_hook(Some(Arc::new(pos_core::sync::feed::HostHook::new(
                Arc::new(move |h| {
                    head.send_replace(h);
                }),
            ))));
        let till = testing::till(dir.path(), &host);
        let (client, _) = engine(&host, &till);
        eventually("online", Duration::from_secs(5), || client.is_online());
        let reply = client
            .rpc_blocking(
                "products.create",
                &till.user_id,
                Vec::new(),
                serde_json::json!({ "name": "Sucre" }),
                Duration::from_secs(5),
                None,
            )
            .unwrap();
        assert!(reply.caught_up);
        assert_eq!(reply.output["name"], "Sucre");
        assert_eq!(
            count(
                &till.db,
                "SELECT COUNT(*) FROM products WHERE name_fr = 'Sucre'"
            ),
            1,
            "already here when the call returns"
        );
        client.stop();
    }

    /// A host service waits for the outbox to drain, except when the line is stopped at a
    /// refused item: it says so at once (`replay_rejected`), not "the main PC does not answer".
    #[test]
    fn a_stopped_line_is_named_before_a_host_service() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let till = testing::till(dir.path(), &host);
        assert!(
            !outbox_drained(&till.db).unwrap(),
            "the pairing's journal row waits"
        );
        let first = till
            .db
            .read(|c| outbox::pending(c, 10))
            .unwrap()
            .first()
            .map(|i| i.local_seq)
            .unwrap();
        till.db
            .write(move |tx| outbox::mark_rejected(tx, first, "replay_invalid"))
            .unwrap();
        assert_eq!(
            outbox_drained(&till.db).unwrap_err().code(),
            "replay_rejected"
        );
    }
}
