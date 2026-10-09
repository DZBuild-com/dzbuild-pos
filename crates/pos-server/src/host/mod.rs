//! The main PC's server (spec §4.6): axum on port 7345 in its own thread and Tokio runtime, so
//! it neither depends on Tauri's runtime nor slows the till UI; every database call goes through
//! `Db::read`/`Db::write` on a blocking task. Started when the owner makes this PC the main PC,
//! stopped when it goes back to a single PC or the app exits.

pub mod guard;
pub mod routes;
#[cfg(test)]
mod touch_tests;

use pos_core::db::Db;
use pos_core::error::AppError;
use pos_core::sync::pairing::PairingDesk;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    pub bind: SocketAddr,
    pub host_id: String,
    pub shop_name: String,
    pub app_version: String,
    pub schema_version: i64,
    pub snapshot_dir: PathBuf,
    /// Where the main PC keeps the update it downloaded and verified (`latest.json`
    /// and `setup.exe`, see [`UPDATE_MANIFEST`] and [`UPDATE_SETUP`]), served to its tills.
    pub update_dir: PathBuf,
}

/// The manifest of the update cached for the tills, in [`HostConfig::update_dir`].
pub const UPDATE_MANIFEST: &str = "latest.json";
/// The setup of that update, its bytes as the main PC verified them.
pub const UPDATE_SETUP: &str = "setup.exe";

/// Who calls a host service from a till: the till (its token) and the user signed in there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcCaller {
    pub terminal_id: String,
    pub terminal_code: String,
    pub user_id: String,
    /// The manager approvals the user brings from the till, as the till sent them: the handler
    /// confirms each one before using it.
    pub approvals: Vec<crate::wire::RpcGrant>,
    /// The till's key for this call ([`crate::wire::RpcRequest::idempotency_key`]).
    pub idempotency_key: Option<String>,
}

/// The host-only operations a till may ask for (Task 19 implements it in the app).
pub trait RpcHandler: Send + Sync + 'static {
    fn call(&self, caller: RpcCaller, service: &str, input: Value) -> Result<Value, AppError>;
}

#[derive(Clone)]
pub struct HostServices {
    pub db: Db,
    pub desk: Arc<Mutex<PairingDesk>>,
    pub rpc: Arc<dyn RpcHandler>,
    pub host_terminal_id: String,
    pub clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

pub struct Shared {
    pub config: HostConfig,
    pub services: HostServices,
    pub limiter: guard::Limiter,
    pub head: watch::Receiver<i64>,
    /// Turns `true` when the server stops: the tills' WebSockets close.
    pub stop: watch::Receiver<bool>,
    pub snapshots: tokio::sync::Semaphore,
    touched: Mutex<HashMap<String, i64>>,
}

impl Shared {
    /// A till's contact is recorded at most every 10 s of this PC's clock, and again at once when
    /// that clock was set back (the last record is then in its future).
    fn should_touch(&self, terminal_id: &str, now: i64) -> bool {
        let Ok(mut touched) = self.touched.lock() else {
            return true;
        };
        let due = touched
            .get(terminal_id)
            .is_none_or(|at| now < *at || now - at >= 10_000);
        if due {
            touched.insert(terminal_id.to_owned(), now);
        }
        due
    }
}

#[derive(Debug)]
pub struct HostServer {
    addr: SocketAddr,
    head: watch::Sender<i64>,
    stop: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}

/// After a stop, how long in-flight requests get to finish before the remaining connections are
/// dropped (a till that lost power mid-request never finishes its own).
const DRAIN: Duration = Duration::from_secs(4);
/// Then how long the runtime waits for its blocking tasks (a write queued behind a backup);
/// past that they end on their own, off the stop path.
const RUNTIME_SHUTDOWN: Duration = Duration::from_millis(500);
/// What `stop` promises: it returns within 5 s whatever the connections or the writer do.
const STOP_DEADLINE: Duration = Duration::from_secs(5);

/// Restart delays of the serve loop: 1, 2, 5, 10, then 30 s.
pub fn backoff_ms(attempt: u32) -> u64 {
    match attempt {
        0 => 1_000,
        1 => 2_000,
        2 => 5_000,
        3 => 10_000,
        _ => 30_000,
    }
}

impl HostServer {
    pub fn start(config: HostConfig, services: HostServices) -> Result<HostServer, AppError> {
        let listener = std::net::TcpListener::bind(config.bind).map_err(|e| {
            tracing::warn!(error = %e, "the LAN port could not be opened");
            AppError::conflict("lan_port_busy")
        })?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let initial = services.db.read(pos_core::sync::feed::head)?;
        let (head, head_rx) = watch::channel(initial);
        let (stop, stop_rx) = watch::channel(false);
        let shared = Arc::new(Shared {
            config,
            services,
            limiter: guard::Limiter::default(),
            head: head_rx,
            stop: stop_rx.clone(),
            snapshots: tokio::sync::Semaphore::new(1),
            touched: Mutex::new(HashMap::new()),
        });
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("pos-lan-host")
            .enable_all()
            .build()?;
        let thread = std::thread::Builder::new()
            .name("pos-lan-host".into())
            .spawn(move || {
                runtime.block_on(supervise(listener, shared, stop_rx));
                // Connections still open are dropped; blocking tasks are not waited for past this.
                runtime.shutdown_timeout(RUNTIME_SHUTDOWN);
            })?;
        Ok(HostServer {
            addr,
            head,
            stop,
            thread: Some(thread),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The feed moved: the tills' WebSockets hear it.
    pub fn poke(&self, head: i64) {
        self.head.send_replace(head);
    }

    pub fn head_watch(&self) -> watch::Sender<i64> {
        self.head.clone()
    }

    /// Graceful stop: no new connection, in-flight requests get up to 4 s to finish, then the
    /// rest are dropped and the port is released. Returns within 5 s; a thread still busy past
    /// that is left to end on its own.
    pub fn stop(mut self) {
        self.stop.send_replace(true);
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + STOP_DEADLINE;
            while !thread.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if thread.is_finished() {
                let _ = thread.join();
            } else {
                tracing::warn!("the LAN server thread did not end within 5 s; left behind");
            }
        }
    }
}

impl Drop for HostServer {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

/// Serves until stopped; a serve loop that ends on its own (an accept error, a panic in the
/// runtime) is restarted with backoff on a freshly bound listener (PLT-021).
async fn supervise(
    listener: std::net::TcpListener,
    shared: Arc<Shared>,
    mut stop: watch::Receiver<bool>,
) {
    let mut listener = tokio::net::TcpListener::from_std(listener)
        .inspect_err(|error| tracing::error!(%error, "the LAN listener could not start"))
        .ok();
    let mut attempt = 0_u32;
    loop {
        if let Some(current) = listener.take() {
            let app = routes::router(shared.clone());
            let mut until = stop.clone();
            let serving = axum::serve(
                current,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = until.wait_for(|stopped| *stopped).await;
            });
            tokio::select! {
                served = serving.into_future() => {
                    if *stop.borrow() {
                        return;
                    }
                    tracing::error!(error = ?served.err(), attempt, "the LAN server stopped; restarting");
                }
                // The drain is bounded: past it the serve future is dropped with its listener.
                () = drained(stop.clone()) => return,
            }
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(backoff_ms(attempt))) => {}
            _ = stop.wait_for(|stopped| *stopped) => return,
        }
        attempt = attempt.saturating_add(1);
        listener = tokio::net::TcpListener::bind(shared.config.bind)
            .await
            .inspect_err(|error| tracing::error!(%error, "the LAN port could not be reopened"))
            .ok();
    }
}

/// Ends `DRAIN` after the stop signal.
async fn drained(mut stop: watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
    tokio::time::sleep(DRAIN).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, http};
    use crate::wire::{H_HOST_TIME, H_SCHEMA, HealthInfo};

    #[test]
    fn health_answers_without_a_token_and_says_who_the_main_pc_is() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let response = http()
            .get(format!("{}/health", host.base()))
            .send()
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.headers().get(H_HOST_TIME).is_some());
        assert_eq!(
            response.headers().get(H_SCHEMA).unwrap().to_str().unwrap(),
            pos_core::db::migrations::LATEST_VERSION.to_string()
        );
        let info: HealthInfo = response.json().unwrap();
        assert_eq!(info.shop_name, "Supérette El Amel");
        assert_eq!(info.host_id, host.config.host_id);
        assert_eq!(info.suggested_code, "C2");
        assert!(!info.pairing_open);
    }

    #[test]
    fn a_protected_route_needs_a_live_token() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let url = format!("{}/whoami", host.base());
        let missing = http().get(&url).send().unwrap();
        assert_eq!(missing.status(), 401);
        let body: pos_core::error::ErrorPayload = missing.json().unwrap();
        assert_eq!(body.code, "lan_token_invalid");
        let token = host.pair("C2");
        let ok = http().get(&url).bearer_auth(&token).send().unwrap();
        assert_eq!(ok.status(), 200);
        assert_eq!(ok.text().unwrap(), "\"C2\"");
        let wrong = http().get(&url).bearer_auth("0".repeat(64)).send().unwrap();
        assert_eq!(wrong.status(), 401);
    }

    #[test]
    fn stop_is_graceful_and_frees_the_port() {
        let dir = tempfile::tempdir().unwrap();
        let mut host = testing::host(dir.path());
        let addr = host.server.as_ref().unwrap().local_addr();
        let started = std::time::Instant::now();
        host.server.take().unwrap().stop();
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert!(http().get(format!("http://{addr}/health")).send().is_err());
        let config = HostConfig {
            bind: addr,
            ..host.config.clone()
        };
        let again = HostServer::start(
            config,
            testing::services(&host.db, &host.desk, &host.terminal_id),
        )
        .unwrap();
        assert_eq!(again.local_addr(), addr, "the port was released");
        again.stop();
    }

    #[test]
    fn stop_returns_within_five_seconds_with_a_stalled_connection_open() {
        use std::io::Write;
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let mut host = testing::host(dir.path());
        let addr = host.server.as_ref().unwrap().local_addr();
        // A till lost power mid-request: its request line arrived, the rest never will.
        let mut stalled = std::net::TcpStream::connect(addr).unwrap();
        stalled.write_all(b"GET /health HTTP/1.1\r\n").unwrap();
        stalled.flush().unwrap();
        // Once a later connection is answered, the stalled one was accepted and is being read.
        let later = http()
            .get(format!("{}/health", host.base()))
            .send()
            .unwrap();
        assert_eq!(later.status(), 200);
        std::thread::sleep(Duration::from_millis(200));
        let server = host.server.take().unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let started = Instant::now();
        std::thread::spawn(move || {
            server.stop();
            let _ = done_tx.send(());
        });
        assert!(
            done_rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "stop() is still waiting for the stalled connection after 5 s"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        let config = HostConfig {
            bind: addr,
            ..host.config.clone()
        };
        let again = HostServer::start(
            config,
            testing::services(&host.db, &host.desk, &host.terminal_id),
        )
        .unwrap();
        assert_eq!(again.local_addr(), addr, "the port was released");
        again.stop();
        drop(stalled);
    }

    #[test]
    fn a_busy_port_is_reported_at_start() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        let taken = host.server.as_ref().unwrap().local_addr();
        let config = HostConfig {
            bind: taken,
            ..host.config.clone()
        };
        let err = HostServer::start(
            config,
            testing::services(&host.db, &host.desk, &host.terminal_id),
        )
        .unwrap_err();
        assert_eq!(err.code(), "lan_port_busy");
    }

    #[test]
    fn the_serve_loop_is_restarted_with_backoff() {
        assert_eq!(
            (0..7).map(backoff_ms).collect::<Vec<_>>(),
            [1_000, 2_000, 5_000, 10_000, 30_000, 30_000, 30_000]
        );
    }
}
