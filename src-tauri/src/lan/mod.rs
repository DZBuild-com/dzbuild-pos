//! The LAN in the app (spec §4.6): a main PC runs the server of `pos-server` and advertises
//! itself; a till runs the engine that sends its outbox and mirrors the main PC; a single PC
//! runs nothing. `AppState::open` sets the write hook of the role before any write.

#[cfg(test)]
mod checklist_tests;
#[cfg(test)]
mod close_tests;
pub mod datapack;
#[cfg(test)]
mod pack_cash_tests;
#[cfg(test)]
mod pack_tests;
pub mod registers;
#[cfg(test)]
mod registers_tests;
#[cfg(test)]
mod returns_tests;
pub mod route;
pub mod rpc;
#[cfg(test)]
mod smoke_tests;
#[cfg(test)]
pub(crate) mod two_pcs;
pub mod vault;

use crate::state::{AppState, SharedState};
use pos_core::db::WriteHook;
use pos_core::error::AppError;
use pos_core::local::{LocalLan, LocalLanRole};
use pos_core::sync::apply::ApplyReport;
use pos_core::sync::feed::HostHook;
use pos_core::sync::outbox::OutboxHook;
use pos_core::sync::pairing::PairingDesk;
use pos_core::sync::role::{self, LanRole};
use pos_core::sync::status::{
    ClientStatus, HostStatus, LanStatus, LinkState, client_status, host_counts,
};
use pos_core::terminal::TerminalRole;
use pos_server::client::{ClientConfig, ClientEvents, LanClient, RpcReply};
use pos_server::discovery::Advertiser;
use pos_server::host::{HostConfig, HostServer, HostServices, RpcHandler};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, Weak};
use std::time::Duration;
use tokio::sync::{Notify, watch};
use vault::TokenVault;

/// The LAN's state changed: the top bar's dot and the network page follow it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LanStatusChanged {
    pub status: LanStatus,
}

/// How much of the main PC's copy a joining till has received (`total` when announced).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LanPairProgress {
    pub received: u64,
    pub total: Option<u64>,
}

/// The main PC's window was asked to close while tills are online: the page asks first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct LanCloseRequested {
    pub tills_online: i64,
}

/// A second click on the window's close button within this delay always closes.
pub const CLOSE_AGAIN_MS: i64 = 60_000;

const VAULT_UNAVAILABLE: &str = "lan_vault_unavailable";

/// Waits before each new try of a till's start while the Credential Manager does not answer.
const VAULT_RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

pub type StatusSink = Arc<dyn Fn(&LanStatus) + Send + Sync>;

/// The window hears every status as `lan-status-changed`.
pub fn window_sink(handle: &tauri::AppHandle) -> StatusSink {
    let handle = handle.clone();
    Arc::new(move |status| {
        use tauri_specta::Event;
        let event = LanStatusChanged {
            status: status.clone(),
        };
        if let Err(error) = event.emit(&handle) {
            tracing::warn!(%error, "could not tell the window the LAN status");
        }
    })
}

#[derive(Debug, Clone, Copy)]
pub struct LanOptions {
    pub bind: SocketAddr,
    pub advertise: bool,
}

impl Default for LanOptions {
    fn default() -> Self {
        LanOptions {
            bind: SocketAddr::from(([0, 0, 0, 0], pos_server::wire::PORT)),
            advertise: true,
        }
    }
}

enum Running {
    Idle,
    Host {
        server: HostServer,
        advertiser: Option<Advertiser>,
    },
    /// Shared with a host service in flight ([`Lan::rpc`]), which never holds the lock.
    Client(Arc<LanClient>),
}

pub struct Lan {
    running: Mutex<Running>,
    role: RwLock<TerminalRole>,
    pub desk: Arc<Mutex<PairingDesk>>,
    pub wake: Arc<Notify>,
    head: Arc<RwLock<Option<watch::Sender<i64>>>>,
    pub vault: Arc<dyn TokenVault>,
    options: LanOptions,
    /// Why this role's server or engine is not running (an error code): the port was busy, the
    /// till's token is missing or could not be read.
    down: Mutex<Option<String>>,
    /// "Fermer" was confirmed on the main PC: its window closes without asking again.
    close_confirmed: AtomicBool,
    /// When the main PC last held a close request (0: never).
    close_asked_at: AtomicI64,
    /// The app and the status sink of the last start: "Synchroniser" on a till whose engine did
    /// not start (its token could not be read) starts it again.
    last_start: Mutex<Option<(Weak<AppState>, StatusSink)>>,
    /// A retry of a till's start is waiting for the Credential Manager (slow to answer at a
    /// Windows autostart).
    vault_retrying: AtomicBool,
    /// A test's link of a till (online, skew) without a main PC.
    #[cfg(test)]
    pub test_link: Mutex<Option<(bool, Option<i64>)>>,
    /// A test's lost answer: the next keyed call reaches the main PC and its answer is dropped.
    #[cfg(test)]
    pub test_lose_answer: AtomicBool,
}

struct Events {
    sink: StatusSink,
    host_id: Option<String>,
    last: Mutex<Option<ClientStatus>>,
}

impl Events {
    fn send(&self, status: ClientStatus) {
        (self.sink)(&LanStatus {
            role: TerminalRole::Client,
            host_id: self.host_id.clone(),
            host: None,
            client: Some(status),
        });
    }
}

impl ClientEvents for Events {
    fn status(&self, status: &ClientStatus) {
        if let Ok(mut last) = self.last.lock() {
            *last = Some(status.clone());
        }
        self.send(status.clone());
    }

    fn applied(&self, report: &ApplyReport) {
        let status = match self.last.lock() {
            Ok(mut last) => last.as_mut().map(|status| {
                status.applied_seq = report.applied_seq;
                status.clone()
            }),
            Err(_) => None,
        };
        if let Some(status) = status {
            self.send(status);
        }
    }
}

/// The host services a till may call ([`rpc::SERVICES`]), run as the till's user.
fn services(st: &SharedState) -> Arc<dyn RpcHandler> {
    Arc::new(rpc::AppRpc::new(Arc::downgrade(st)))
}

impl Lan {
    pub fn new(vault: Arc<dyn TokenVault>, options: LanOptions) -> Lan {
        Lan {
            running: Mutex::new(Running::Idle),
            role: RwLock::new(TerminalRole::Standalone),
            desk: Arc::new(Mutex::new(PairingDesk::default())),
            wake: Arc::new(Notify::new()),
            head: Arc::new(RwLock::new(None)),
            vault,
            options,
            down: Mutex::new(None),
            close_confirmed: AtomicBool::new(false),
            close_asked_at: AtomicI64::new(0),
            last_start: Mutex::new(None),
            vault_retrying: AtomicBool::new(false),
            #[cfg(test)]
            test_link: Mutex::new(None),
            #[cfg(test)]
            test_lose_answer: AtomicBool::new(false),
        }
    }

    /// Start, stop and the readers take turns here; a host service never holds it.
    fn running(&self) -> MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn role(&self) -> TerminalRole {
        self.role.read().map(|r| *r).unwrap_or_default()
    }

    pub fn set_role(&self, role: TerminalRole) {
        if let Ok(mut slot) = self.role.write() {
            *slot = role;
        }
    }

    pub fn is_client(&self) -> bool {
        self.role() == TerminalRole::Client
    }

    fn set_down(&self, code: Option<String>) {
        *self.down.lock().unwrap_or_else(PoisonError::into_inner) = code;
    }

    fn down(&self) -> Option<String> {
        self.down
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The write hook of this role: commit marks and pokes on a main PC, the outbox on a till.
    pub fn hook(&self, role: &LanRole, terminal_id: &str) -> Option<Arc<dyn WriteHook>> {
        match role.role {
            TerminalRole::Standalone => None,
            TerminalRole::Host => {
                let head = self.head.clone();
                Some(Arc::new(HostHook::new(Arc::new(move |h| {
                    if let Ok(slot) = head.read()
                        && let Some(sender) = slot.as_ref()
                    {
                        sender.send_replace(h);
                    }
                }))))
            }
            TerminalRole::Client => {
                let wake = self.wake.clone();
                Some(Arc::new(OutboxHook::new(
                    terminal_id.to_owned(),
                    Arc::new(move || wake.notify_one()),
                )))
            }
        }
    }

    /// Starts what the database's role runs, after stopping what ran. A main PC whose port is
    /// taken and a till without its token start nothing and say why in [`Lan::status`].
    /// Blocking (the database, a bind, the Credential Manager).
    pub fn start(&self, st: &SharedState, sink: StatusSink) -> Result<(), AppError> {
        *self
            .last_start
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some((Arc::downgrade(st), sink.clone()));
        let mut running = self.running();
        self.shut(std::mem::replace(&mut *running, Running::Idle));
        self.set_down(None);
        let role = st.db.read(role::current)?;
        self.set_role(role.role);
        *running = match role.role {
            TerminalRole::Standalone => Running::Idle,
            TerminalRole::Host => self.start_host(st, &role)?,
            TerminalRole::Client => match self.start_client(st, &role, sink.clone()) {
                Ok(started) => started,
                Err(error) => {
                    self.set_down(Some(error.code()));
                    if error.code() == VAULT_UNAVAILABLE {
                        self.retry_vault_later(st, sink);
                    }
                    return Err(error);
                }
            },
        };
        Ok(())
    }

    /// A till's engine is not running because its token could not be read or is missing.
    fn waiting_for_token(&self) -> bool {
        self.is_client()
            && matches!(&*self.running(), Running::Idle)
            && self
                .down()
                .is_some_and(|code| matches!(code.as_str(), VAULT_UNAVAILABLE | "lan_not_paired"))
    }

    /// The Credential Manager did not answer (it can be slow at a Windows autostart): the till's
    /// start is tried again a few times, further apart each time, until it reads the token.
    fn retry_vault_later(&self, st: &SharedState, sink: StatusSink) {
        if self.vault_retrying.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak = Arc::downgrade(st);
        let spawned = std::thread::Builder::new()
            .name("pos-lan-vault-retry".into())
            .spawn(move || {
                for delay in VAULT_RETRY_DELAYS {
                    std::thread::sleep(delay);
                    let Some(st) = weak.upgrade() else {
                        return;
                    };
                    if !st.lan.is_client() || st.lan.down().as_deref() != Some(VAULT_UNAVAILABLE) {
                        break;
                    }
                    st.lan.restart(&st, sink.clone());
                    if st.lan.down().as_deref() != Some(VAULT_UNAVAILABLE) {
                        break;
                    }
                }
                if let Some(st) = weak.upgrade() {
                    st.lan.vault_retrying.store(false, Ordering::SeqCst);
                }
            });
        if spawned.is_err() {
            self.vault_retrying.store(false, Ordering::SeqCst);
        }
    }

    fn start_host(&self, st: &SharedState, role: &LanRole) -> Result<Running, AppError> {
        let shop_name = st
            .db
            .read(pos_core::company::get)?
            .map(|c| c.name_fr)
            .unwrap_or_default();
        let config = HostConfig {
            bind: self.options.bind,
            host_id: role.host_id.clone().unwrap_or_default(),
            shop_name: shop_name.clone(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: pos_core::db::migrations::LATEST_VERSION,
            snapshot_dir: Path::new(&st.paths.data_dir)
                .join(pos_core::sync::replica::LAN_DIR)
                .join("snapshots"),
            update_dir: crate::updates::mirror_dir(&st.paths),
        };
        let services = HostServices {
            db: st.db.clone(),
            desk: self.desk.clone(),
            rpc: services(st),
            host_terminal_id: st.terminal.id.clone(),
            // The main PC's clock is `AppState::now` (a test shifts it to put a till 2 h ahead).
            clock: {
                let weak = Arc::downgrade(st);
                Arc::new(move || {
                    weak.upgrade()
                        .map_or_else(pos_core::time::now_ms, |state| state.now())
                })
            },
        };
        match HostServer::start(config.clone(), services) {
            Ok(server) => {
                if let Ok(mut slot) = self.head.write() {
                    *slot = Some(server.head_watch());
                }
                let advertiser = if self.options.advertise {
                    Advertiser::start(
                        &config.host_id,
                        &shop_name,
                        server.local_addr().port(),
                        &config.app_version,
                        config.schema_version,
                    )
                    .inspect_err(|e| tracing::warn!(code = %e.code(), "mDNS advertisement failed"))
                    .ok()
                } else {
                    None
                };
                Ok(Running::Host { server, advertiser })
            }
            Err(error) => {
                tracing::error!(code = %error.code(), "the LAN server could not start");
                self.set_down(Some(error.code()));
                Ok(Running::Idle)
            }
        }
    }

    fn start_client(
        &self,
        st: &SharedState,
        role: &LanRole,
        sink: StatusSink,
    ) -> Result<Running, AppError> {
        let Some(token) = self.vault.get()? else {
            self.set_down(Some("lan_not_paired".into()));
            return Ok(Running::Idle);
        };
        let config = ClientConfig {
            base: format!("http://{}", role.host_address.clone().unwrap_or_default()),
            host_id: role.host_id.clone().unwrap_or_default(),
            terminal_id: st.terminal.id.clone(),
            token,
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: pos_core::db::migrations::LATEST_VERSION,
        };
        let events = Arc::new(Events {
            sink,
            host_id: role.host_id.clone(),
            last: Mutex::new(None),
        });
        let client = LanClient::start(st.db.clone(), config, self.wake.clone(), events)?;
        Ok(Running::Client(Arc::new(client)))
    }

    fn shut(&self, running: Running) {
        match running {
            Running::Host { server, advertiser } => {
                if let Some(advertiser) = advertiser {
                    advertiser.stop();
                }
                server.stop();
                if let Ok(mut slot) = self.head.write() {
                    *slot = None;
                }
            }
            Running::Client(client) => match Arc::try_unwrap(client) {
                Ok(client) => client.stop(),
                // A host service still holds it: dropping the last handle stops the engine.
                Err(shared) => drop(shared),
            },
            Running::Idle => {}
        }
    }

    /// Stops the server (in-flight requests finish, at most 5 s) or the engine (between two
    /// cycles). Blocking.
    pub fn stop(&self) {
        let mut running = self.running();
        self.shut(std::mem::replace(&mut *running, Running::Idle));
    }

    pub fn restart(&self, st: &SharedState, sink: StatusSink) {
        if let Err(error) = self.start(st, sink) {
            tracing::error!(code = %error.code(), "the LAN could not start");
        }
    }

    pub fn is_online(&self) -> bool {
        #[cfg(test)]
        if let Ok(slot) = self.test_link.lock()
            && let Some((online, _)) = *slot
        {
            return online;
        }
        match &*self.running() {
            Running::Client(client) => client.is_online(),
            _ => false,
        }
    }

    pub fn skew_ms(&self) -> Option<i64> {
        #[cfg(test)]
        if let Ok(slot) = self.test_link.lock()
            && let Some((_, skew)) = *slot
        {
            return skew;
        }
        match &*self.running() {
            Running::Client(client) => client.skew_ms(),
            _ => None,
        }
    }

    /// On a till, a whole cycle now: the main PC is checked again (its clock, its version) before
    /// the outbox and the feed. Elsewhere a poke.
    pub fn sync_now(&self) {
        if self.waiting_for_token() {
            // The engine never started (the token could not be read, or was missing): the vault
            // is read again, and the status says what is still wrong.
            let last = self
                .last_start
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let Some((weak, sink)) = last
                && let Some(st) = weak.upgrade()
            {
                self.restart(&st, sink);
            }
            return;
        }
        match &*self.running() {
            Running::Client(client) => client.sync_now(),
            _ => self.wake.notify_one(),
        }
    }

    /// The main PC is asked to close (at `now`) while tills are online: hold the first request and
    /// ask (`Some(tills online)`); a second request within [`CLOSE_AGAIN_MS`] or a confirmed close
    /// goes through, so the window always closes on a double click even without the page. The
    /// tills online are counted as the status counts them, on this PC's clock.
    pub fn should_hold_close(&self, st: &AppState, now: i64) -> Result<Option<i64>, AppError> {
        if self.role() != TerminalRole::Host || self.close_confirmed.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let at = st.now();
        let (_, online, _) = st.db.read(move |c| host_counts(c, at))?;
        if online == 0 {
            return Ok(None);
        }
        let asked = self.close_asked_at.swap(now, Ordering::SeqCst);
        if asked != 0 && now - asked < CLOSE_AGAIN_MS {
            return Ok(None);
        }
        Ok(Some(online))
    }

    /// "Fermer" in the main PC's close confirmation: the next close is never held.
    pub fn confirm_close(&self) {
        self.close_confirmed.store(true, Ordering::SeqCst);
    }

    /// A host service from this till; `host_required` when not connected. Blocking, and never
    /// under the lock: the till's status and its sales do not wait for the answer.
    pub fn rpc(
        &self,
        service: &str,
        user_id: &str,
        approvals: Vec<pos_server::wire::RpcGrant>,
        input: Value,
        timeout: Duration,
    ) -> Result<RpcReply, AppError> {
        self.rpc_keyed(service, user_id, approvals, input, timeout, None)
    }

    /// [`Self::rpc`] with the till's idempotency key of a write that moves money (final review):
    /// the main PC answers a key it already answered from its record.
    pub fn rpc_keyed(
        &self,
        service: &str,
        user_id: &str,
        approvals: Vec<pos_server::wire::RpcGrant>,
        input: Value,
        timeout: Duration,
        idempotency_key: Option<String>,
    ) -> Result<RpcReply, AppError> {
        let client = match &*self.running() {
            Running::Client(client) => client.clone(),
            _ => return Err(AppError::conflict("host_required")),
        };
        let reply =
            client.rpc_blocking(service, user_id, approvals, input, timeout, idempotency_key)?;
        #[cfg(test)]
        if self.test_lose_answer.swap(false, Ordering::SeqCst) {
            return Err(AppError::sync("lan_offline"));
        }
        Ok(reply)
    }

    pub fn status(&self, st: &AppState) -> Result<LanStatus, AppError> {
        let role = st.db.read(role::current)?;
        let running = self.running();
        Ok(match (role.role, &*running) {
            (TerminalRole::Standalone, _) => LanStatus {
                role: TerminalRole::Standalone,
                host_id: None,
                host: None,
                client: None,
            },
            (TerminalRole::Host, running) => {
                let now = st.now();
                let (tills_total, tills_online, notices_open) =
                    st.db.read(move |c| host_counts(c, now))?;
                let port = match running {
                    Running::Host { server, .. } => server.local_addr().port(),
                    _ => self.options.bind.port(),
                };
                LanStatus {
                    role: TerminalRole::Host,
                    host_id: role.host_id.clone(),
                    host: Some(HostStatus {
                        listening: matches!(running, Running::Host { .. }),
                        port,
                        addresses: host_addresses(),
                        tills_total,
                        tills_online,
                        notices_open,
                        pairing_open_until: self.desk.lock().ok().and_then(|d| d.open_until(now)),
                        error: self.down(),
                    }),
                    client: None,
                }
            }
            (TerminalRole::Client, Running::Client(client)) => LanStatus {
                role: TerminalRole::Client,
                host_id: role.host_id.clone(),
                host: None,
                client: Some(client.status()?),
            },
            (TerminalRole::Client, _) => {
                let why = self.down().unwrap_or_else(|| "lan_not_paired".into());
                LanStatus {
                    role: TerminalRole::Client,
                    host_id: role.host_id.clone(),
                    host: None,
                    client: Some(
                        st.db.read(move |c| {
                            client_status(c, LinkState::Attention, None, Some(why))
                        })?,
                    ),
                }
            }
        })
    }
}

/// This PC's LAN address as the tills would type it (the interface of the default route; none
/// on an isolated LAN without a gateway, where the owner reads it in Windows' settings). No
/// packet leaves: connecting a UDP socket only picks the route.
pub fn host_addresses() -> Vec<String> {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.0.2.1:9")?;
            socket.local_addr()
        })
        .map(|addr| vec![addr.ip().to_string()])
        .unwrap_or_default()
}

/// This PC's place as the start finds it, for [`record_place`].
#[derive(Debug, Clone)]
pub struct Place {
    pub role: LanRole,
    /// This PC's own terminal.
    pub terminal: Option<pos_core::terminal::Terminal>,
    /// A main PC's paired tills not revoked.
    pub tills: i64,
    /// The first run is done (a new PC, or one set aside to pair again, is not).
    pub set_up: bool,
}

/// `local.json` (in `data_dir`) says whether this PC is a till, or a main PC with tills
/// (M6 review), for the guided restore that runs when `pos.db` cannot be opened. Kept in step
/// with the role at every start (after a pairing, a promotion, leaving the network); a failure
/// is logged and the start goes on. A till's terminal is kept with its place, and a PC whose
/// till copy was set aside keeps that place until it is a till again or is set up as a new shop
/// (its empty database says nothing yet).
pub fn record_place(data_dir: &Path, place: &Place) {
    let terminal_id = place.terminal.as_ref().map(|t| t.id.clone());
    let terminal_code = place.terminal.as_ref().map(|t| t.code.clone());
    let lan = match place.role.role {
        TerminalRole::Client => Some(LocalLan {
            role: LocalLanRole::Till,
            host: place.role.host_name.clone().unwrap_or_default(),
            terminal_id,
            terminal_code,
            tills: 0,
        }),
        TerminalRole::Host => Some(LocalLan {
            role: LocalLanRole::Main,
            host: String::new(),
            terminal_id,
            terminal_code,
            tills: place.tills,
        }),
        TerminalRole::Standalone => None,
    };
    let path = data_dir.join(pos_core::local::FILE_NAME);
    let kept_till = || {
        pos_core::local::LocalConfig::load(&path)
            .lan
            .is_some_and(|lan| lan.role == LocalLanRole::Till)
    };
    if lan.is_none() && !place.set_up && kept_till() {
        return;
    }
    if let Err(error) = pos_core::local::record_lan(&path, lan) {
        tracing::warn!(code = %error.code(), "local.json could not record this PC's role");
    }
}

/// The terminal this PC was when it was a till (`local.json`), for a pairing from a new,
/// empty database after its damaged copy was set aside.
pub fn former_terminal(data_dir: &Path) -> Option<String> {
    pos_core::local::LocalConfig::load(&data_dir.join(pos_core::local::FILE_NAME))
        .lan
        .filter(|lan| lan.role == LocalLanRole::Till)
        .and_then(|lan| lan.terminal_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppPaths, AppState, StartupNotice};
    use pos_core::local::{FILE_NAME as LOCAL_JSON, LocalConfig, LocalLan, LocalLanRole};
    use pos_core::sync::replica::{self, PendingReplica};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn options() -> LanOptions {
        LanOptions {
            bind: "127.0.0.1:0".parse().unwrap(),
            advertise: false,
        }
    }

    fn open(dir: &std::path::Path, vault: Arc<dyn vault::TokenVault>) -> crate::state::SharedState {
        Arc::new(
            AppState::open_with(
                &AppPaths::new(dir),
                crate::printing::silent(),
                vault,
                options(),
            )
            .unwrap(),
        )
    }

    fn quiet() -> StatusSink {
        Arc::new(|_| {})
    }

    fn eventually(what: &str, check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if check() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("{what}");
    }

    fn synced_rates(st: &AppState) -> i64 {
        st.db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'tax_rate.synced'",
                    [],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap()
    }

    #[test]
    fn a_single_pc_runs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let st = open(dir.path(), Arc::new(vault::MemoryVault::default()));
        st.lan.start(&st, quiet()).unwrap();
        let status = st.lan.status(&st).unwrap();
        assert_eq!(status.role, pos_core::terminal::TerminalRole::Standalone);
        assert!(status.host.is_none() && status.client.is_none());
        assert!(
            !st.db.has_write_hook(),
            "no hook, no trigger on a single PC"
        );
        assert!(
            st.db
                .read(pos_core::sync::capture::installed)
                .unwrap()
                .is_empty()
        );
        assert!(
            !dir.path().join(LOCAL_JSON).exists(),
            "nothing written for a single PC"
        );
    }

    #[test]
    fn a_main_pc_serves_and_a_new_till_starts_from_its_copy() {
        let dir = tempfile::tempdir().unwrap();
        let host_dir = dir.path().join("host");
        let host = open(&host_dir, Arc::new(vault::MemoryVault::default()));
        crate::commands::tests::first_run(&host);
        host.db
            .write(|tx| {
                pos_core::sync::role::become_host(tx, &pos_core::audit::Actor::default(), 0)
                    .map(drop)
            })
            .unwrap();
        host.lan.set_role(pos_core::terminal::TerminalRole::Host);
        host.lan.restart(&host, quiet());
        let status = host.lan.status(&host).unwrap().host.unwrap();
        assert!(status.listening);
        let address = format!("127.0.0.1:{}", status.port);
        let code = host
            .lan
            .desk
            .lock()
            .unwrap()
            .issue(pos_core::time::now_ms())
            .unwrap()
            .code;

        let till_dir = dir.path().join("till");
        let till_vault = Arc::new(vault::MemoryVault::default());
        let joined = pos_server::client::join::join(
            None,
            &till_dir,
            pos_server::client::join::JoinRequest {
                address: address.clone(),
                code,
                terminal_code: "C2".into(),
                terminal_name: "Caisse 2".into(),
                app_version: env!("CARGO_PKG_VERSION").into(),
                schema_version: pos_core::db::migrations::LATEST_VERSION,
                install_id: pos_core::ids::new_id(),
                former_terminal: None,
            },
            Arc::new(|_, _| {}),
        )
        .unwrap();
        till_vault.set(&joined.token).unwrap();
        let till = open(&till_dir, till_vault);
        assert_eq!(till.terminal.code, "C2");
        assert_eq!(till.lan.role(), pos_core::terminal::TerminalRole::Client);
        assert!(till.db.has_write_hook());
        assert_eq!(
            synced_rates(&till),
            synced_rates(&host),
            "the till seeded nothing of its own"
        );
        // The guided restore reads this PC's place when `pos.db` cannot be opened.
        assert_eq!(
            LocalConfig::load(&till_dir.join(LOCAL_JSON)).lan,
            Some(LocalLan {
                role: LocalLanRole::Till,
                host: "Supérette El Amel".into(),
                terminal_id: Some(till.terminal.id.clone()),
                terminal_code: Some("C2".into()),
                tills: 0,
            })
        );
        assert_eq!(LocalConfig::load(&host_dir.join(LOCAL_JSON)).lan, None);
        till.lan.start(&till, quiet()).unwrap();
        // Online once the first cycle has sent the outbox and pulled the feed (`Replaying`
        // already counts as online, with the adoption's journal row still on its way).
        eventually("the till is online", || {
            till.lan.is_online()
                && till.lan.status(&till).is_ok_and(|s| {
                    s.client
                        .is_some_and(|c| c.link == pos_core::sync::status::LinkState::Online)
                })
        });
        let client = till.lan.status(&till).unwrap().client.unwrap();
        assert_eq!(
            (client.host_name.as_str(), client.pending),
            ("Supérette El Amel", 0)
        );
        till.lan.stop();
        host.lan.stop();
    }

    /// A vault that reads but cannot keep a token (the Credential Manager refuses the write).
    struct ReadOnlyVault;

    impl vault::TokenVault for ReadOnlyVault {
        fn get(&self) -> Result<Option<String>, AppError> {
            Ok(None)
        }
        fn set(&self, _: &str) -> Result<(), AppError> {
            Err(AppError::conflict("lan_vault_unavailable"))
        }
        fn delete(&self) -> Result<(), AppError> {
            Ok(())
        }
    }

    /// A vault that does not answer at all.
    struct DeadVault;

    impl vault::TokenVault for DeadVault {
        fn get(&self) -> Result<Option<String>, AppError> {
            Err(AppError::conflict("lan_vault_unavailable"))
        }
        fn set(&self, _: &str) -> Result<(), AppError> {
            Err(AppError::conflict("lan_vault_unavailable"))
        }
        fn delete(&self) -> Result<(), AppError> {
            Err(AppError::conflict("lan_vault_unavailable"))
        }
    }

    /// The till's token must be kept: a vault that does not answer stops the join before the
    /// pairing code is spent, and one that refuses the token leaves no copy staged for the next
    /// start (a till without its token never syncs).
    #[test]
    fn a_join_without_a_working_vault_stages_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let host = open(
            &dir.path().join("host"),
            Arc::new(vault::MemoryVault::default()),
        );
        crate::commands::tests::first_run(&host);
        crate::commands::lan::become_host(&host, &quiet()).unwrap();
        let address = format!(
            "127.0.0.1:{}",
            host.lan.status(&host).unwrap().host.unwrap().port
        );
        let issue = || crate::commands::lan::pairing_code(&host).unwrap().code;
        let input = |code: String| crate::commands::lan::JoinInput {
            address: address.clone(),
            code,
            terminal_code: "C2".into(),
            terminal_name: "Caisse 2".into(),
            owner_pin: None,
        };

        let dead_dir = dir.path().join("dead");
        let dead = open(&dead_dir, Arc::new(DeadVault));
        let code = issue();
        assert_eq!(
            crate::commands::lan::join(&dead, input(code.clone()), Arc::new(|_, _| {}))
                .unwrap_err()
                .code(),
            "lan_vault_unavailable"
        );
        assert_eq!(
            host.db
                .read(
                    |c| Ok(c.query_row("SELECT COUNT(*) FROM lan_devices", [], |r| r
                        .get::<_, i64>(0))?)
                )
                .unwrap(),
            0,
            "nothing registered: the code is not spent"
        );

        let till_dir = dir.path().join("till");
        let till = open(&till_dir, Arc::new(ReadOnlyVault));
        assert_eq!(
            crate::commands::lan::join(&till, input(code), Arc::new(|_, _| {}))
                .unwrap_err()
                .code(),
            "lan_vault_unavailable"
        );
        assert!(
            !replica::has_pending(&till_dir),
            "no copy installed without its token"
        );
        assert!(
            !till_dir
                .join(replica::LAN_DIR)
                .join(replica::DOWNLOAD_PART)
                .exists()
        );
        host.lan.stop();
    }

    #[test]
    fn a_till_without_its_token_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let st = open(dir.path(), Arc::new(vault::MemoryVault::default()));
        crate::commands::tests::first_run(&st);
        st.db
            .write(|tx| {
                tx.execute(
                    "UPDATE lan_local SET role = 'client', host_id = ?1, host_address = '127.0.0.1:9', paired_at = 0",
                    [pos_core::ids::new_id()],
                )?;
                Ok(())
            })
            .unwrap();
        st.lan.set_role(pos_core::terminal::TerminalRole::Client);
        st.lan.start(&st, quiet()).unwrap();
        let client = st.lan.status(&st).unwrap().client.unwrap();
        assert_eq!(
            (client.link, client.last_error.as_deref()),
            (
                pos_core::sync::status::LinkState::Attention,
                Some("lan_not_paired")
            )
        );
    }

    /// "Synchroniser" on a till whose engine never started (its token was missing or could not
    /// be read) reads the vault again and starts it.
    #[test]
    fn sync_now_starts_a_till_whose_token_was_missing() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(vault::MemoryVault::default());
        let st = open(dir.path(), vault.clone());
        crate::commands::tests::first_run(&st);
        st.db
            .write(|tx| {
                tx.execute(
                    "UPDATE lan_local SET role = 'client', host_id = ?1, host_address = '127.0.0.1:9', paired_at = 0",
                    [pos_core::ids::new_id()],
                )?;
                Ok(())
            })
            .unwrap();
        st.lan.set_role(pos_core::terminal::TerminalRole::Client);
        st.lan.start(&st, quiet()).unwrap();
        let error = |st: &AppState| st.lan.status(st).unwrap().client.unwrap().last_error;
        assert_eq!(error(&st).as_deref(), Some("lan_not_paired"));
        vault::TokenVault::set(vault.as_ref(), &"e".repeat(64)).unwrap();
        st.lan.sync_now();
        assert!(
            matches!(&*st.lan.running(), Running::Client(_)),
            "the engine runs with the token read again"
        );
        assert_ne!(error(&st).as_deref(), Some("lan_not_paired"));
        st.lan.stop();
    }

    /// `local.json` follows the role of the last start; leaving the network archives
    /// the till's copy (a `pre_pairing` backup) and the PC starts again as a single PC.
    #[test]
    fn a_till_is_written_down_in_local_json_until_it_leaves() {
        let dir = tempfile::tempdir().unwrap();
        let st = open(dir.path(), Arc::new(vault::MemoryVault::default()));
        crate::commands::tests::first_run(&st);
        st.db
            .write(|tx| {
                tx.execute(
                    "UPDATE lan_local SET role = 'client', host_id = ?1, host_name = 'Principal',
                            host_address = '127.0.0.1:9', paired_at = 0",
                    [pos_core::ids::new_id()],
                )?;
                Ok(())
            })
            .unwrap();
        st.db.mark_clean_shutdown().unwrap();
        drop(st);

        let st = open(dir.path(), Arc::new(vault::MemoryVault::default()));
        assert_eq!(st.lan.role(), pos_core::terminal::TerminalRole::Client);
        assert!(st.db.has_write_hook(), "the outbox hook of a till");
        assert!(
            !st.db
                .read(pos_core::sync::capture::installed)
                .unwrap()
                .is_empty(),
            "the till's capture and guard triggers"
        );
        let local = dir.path().join(LOCAL_JSON);
        assert_eq!(
            LocalConfig::load(&local)
                .lan
                .map(|lan| (lan.role, lan.host)),
            Some((LocalLanRole::Till, "Principal".into()))
        );
        st.db.mark_clean_shutdown().unwrap();
        drop(st);

        replica::schedule_reset(dir.path()).unwrap();
        let st = open(dir.path(), Arc::new(vault::MemoryVault::default()));
        assert!(st.notices.is_empty(), "{:?}", st.notices);
        assert_eq!(st.lan.role(), pos_core::terminal::TerminalRole::Standalone);
        assert!(!st.db.has_write_hook());
        assert_eq!(LocalConfig::load(&local).lan, None);
        let archived = std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("pre_pairing"))
            .count();
        assert_eq!(archived, 1, "the till's copy is kept, never deleted");
    }

    /// Leaving the network keeps the till's token until its copy is archived at the next start:
    /// a failed archive is said as such (not as a failed install) and the PC stays a till with
    /// its token; once archived, the token goes.
    #[test]
    fn a_failed_leave_keeps_the_till_and_its_token() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(vault::MemoryVault::default());
        let st = open(dir.path(), vault.clone());
        crate::commands::tests::first_run(&st);
        st.db
            .write(|tx| {
                tx.execute(
                    "UPDATE lan_local SET role = 'client', host_id = ?1, host_name = 'Principal',
                            host_address = '127.0.0.1:9', paired_at = 0",
                    [pos_core::ids::new_id()],
                )?;
                Ok(())
            })
            .unwrap();
        vault::TokenVault::set(vault.as_ref(), &"c".repeat(64)).unwrap();
        st.db.mark_clean_shutdown().unwrap();
        drop(st);
        replica::schedule_reset(dir.path()).unwrap();
        // The backups folder cannot be made: the archive fails.
        std::fs::write(dir.path().join("backups"), b"in the way").unwrap();
        let st = open(dir.path(), vault.clone());
        assert_eq!(
            st.notices,
            [StartupNotice::LeaveFailed {
                reason: "io_error".into()
            }]
        );
        assert_eq!(st.lan.role(), pos_core::terminal::TerminalRole::Client);
        assert!(
            vault::TokenVault::get(vault.as_ref()).unwrap().is_some(),
            "still a till: its token stays"
        );
        st.db.mark_clean_shutdown().unwrap();
        drop(st);
        std::fs::remove_file(dir.path().join("backups")).unwrap();
        let st = open(dir.path(), vault.clone());
        assert!(st.notices.is_empty(), "{:?}", st.notices);
        assert_eq!(st.lan.role(), pos_core::terminal::TerminalRole::Standalone);
        assert_eq!(
            vault::TokenVault::get(vault.as_ref()).unwrap(),
            None,
            "the token goes once the copy is archived"
        );
    }

    /// A staged copy that cannot be installed leaves this PC on its own database, with a notice.
    #[test]
    fn a_copy_that_cannot_be_installed_keeps_this_pcs_database() {
        let dir = tempfile::tempdir().unwrap();
        let st = open(dir.path(), Arc::new(vault::MemoryVault::default()));
        crate::commands::tests::first_run(&st);
        st.db.mark_clean_shutdown().unwrap();
        drop(st);
        let downloaded = dir.path().join("download.part");
        std::fs::write(&downloaded, b"a copy that will go missing").unwrap();
        let pending = PendingReplica {
            seq: 1,
            host_id: pos_core::ids::new_id(),
            host_name: "Principal".into(),
            host_address: "127.0.0.1:9".into(),
            terminal_id: pos_core::ids::new_id(),
            terminal_code: "C2".into(),
            terminal_name: "Caisse 2".into(),
            paired_at: 0,
        };
        replica::stage(dir.path(), &downloaded, &pending).unwrap();
        std::fs::remove_file(dir.path().join(replica::LAN_DIR).join("replica.db")).unwrap();

        let st = open(dir.path(), Arc::new(vault::MemoryVault::default()));
        assert_eq!(
            st.notices,
            [StartupNotice::LanFailed {
                reason: "snapshot_invalid".into()
            }]
        );
        assert_eq!(st.terminal.code, "C1");
        assert_eq!(st.lan.role(), pos_core::terminal::TerminalRole::Standalone);
        assert!(st.db.read(pos_core::company::get).unwrap().is_some());
    }

    /// Task 25 puts a till two hours ahead by shifting the main PC's clock.
    #[test]
    fn the_pc_clock_can_be_shifted() {
        let st = AppState::for_tests();
        let two_hours = 2 * 3_600_000;
        st.shift_clock(-two_hours);
        let before = pos_core::time::now_ms() - two_hours;
        let now = st.now();
        assert!((before..=pos_core::time::now_ms() - two_hours).contains(&now));
        let nine_am = 1_788_246_000_000;
        st.clock.pin(nine_am);
        assert_eq!(st.now(), nine_am - two_hours);
    }

    #[test]
    fn the_firewall_lines_open_the_port_the_main_pc_listens_on() {
        let exe = std::path::Path::new("C:\\Program Files\\DZBuild POS\\DZBuild POS.exe");
        let [tcp, mdns] = pos_server::firewall::install_commands(exe);
        assert!(tcp.contains(&format!("localport={}", pos_server::wire::PORT)));
        assert!(tcp.contains("profile=private") && mdns.contains("profile=private"));
        assert_eq!(LanOptions::default().bind.port(), pos_server::wire::PORT);
    }
}
