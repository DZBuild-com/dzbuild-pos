//! The DZBuild sync engine (spec §4.2 `sync`, §4.7). Two threads per process:
//! `pos-cloud-sync` owns the work — token refresh, derive, push, the events pages the other thread
//! brings, pulls by date while the events feed is not shipped or not answering, the daily store
//! profile, heartbeats, maintenance — and wakes every `cycle` or on
//! demand; `pos-cloud-events` long-polls DZBuild and hands the pages over (it never touches the
//! database). Offline, both back off quietly (no notice, no toast); a refused refresh or a
//! revocation suspends the link; every step commits its own progress, so a restart resumes where
//! it stopped. Local work (derive, the loi 18-07 purge) runs whatever the link and the network.
//! One answer the POS cannot read holds up only its own step. Nothing here runs inside a sale:
//! the till never waits for DZBuild.

use super::api::{self, Remote};
use super::derive::derive;
use super::http::{CallError, Endpoint, Page};
use super::link::{self, LinkContext};
use super::oauth::{self, Metadata};
use super::outbox::{self, EntityCount};
use super::pull::{self, EventItem, PullContext, Unreadable};
use super::push::{self, PushContext};
use super::store::{self, CloudLink, LinkStatus};
use crate::audit::Actor;
use crate::error::AppError;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded, unbounded};
use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub cycle: Duration,
    pub orders_poll: Duration,
    pub catalogue_poll: Duration,
    pub heartbeat: Duration,
    pub profile: Duration,
    pub maintenance: Duration,
    pub offline_backoff: [Duration; 4],
    pub events_wait_secs: u64,
}

impl Timing {
    pub const fn production() -> Timing {
        Timing {
            cycle: Duration::from_secs(30),
            orders_poll: Duration::from_secs(120),
            catalogue_poll: Duration::from_secs(600),
            heartbeat: Duration::from_secs(900),
            profile: Duration::from_secs(86_400),
            maintenance: Duration::from_secs(86_400),
            offline_backoff: [
                Duration::from_secs(30),
                Duration::from_secs(60),
                Duration::from_secs(120),
                Duration::from_secs(300),
            ],
            events_wait_secs: 25,
        }
    }

    /// Tests: everything in tens of milliseconds.
    pub const fn fast() -> Timing {
        Timing {
            cycle: Duration::from_millis(50),
            orders_poll: Duration::from_millis(100),
            catalogue_poll: Duration::from_millis(200),
            heartbeat: Duration::from_millis(200),
            profile: Duration::from_secs(3_600),
            maintenance: Duration::from_secs(3_600),
            offline_backoff: [Duration::from_millis(50); 4],
            events_wait_secs: 0,
        }
    }
}

/// Network or server failures of the events feed in a row before it counts as not answering
/// (orders are then pulled by date until it answers again).
const FEED_TRIES: u32 = 3;

/// Cycles in a row an events page whose detail cannot be read is tried: the
/// last try records that event `failed` with one notice, and the feed moves on past it.
pub const UNREADABLE_TRIES: u32 = 100;

/// A cheap handle on the engine's queue for callbacks created before the engine starts.
#[derive(Clone)]
pub struct SyncNotifier(Sender<Command>);

impl SyncNotifier {
    pub fn wake(&self) {
        let _ = self.0.send(Command::Wake);
    }

    pub fn link_changed(&self) {
        let _ = self.0.send(Command::LinkChanged);
    }
}

pub struct EngineDeps {
    pub link: Arc<LinkContext>,
    pub media_dir: Option<PathBuf>,
    pub timing: Timing,
    pub on_status: Arc<dyn Fn(&SyncStatus) + Send + Sync>,
    pub on_orders: Arc<dyn Fn(i64) + Send + Sync>,
}

/// What the top bar's dot and Settings › DZBuild show.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub linked: bool,
    pub link_status: Option<LinkStatus>,
    pub store_name: Option<String>,
    pub public_url: Option<String>,
    pub online: bool,
    pub paused: bool,
    pub running: bool,
    pub last_cycle_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub pending: i64,
    pub failed: i64,
    pub entities: Vec<EntityCount>,
    pub not_ready: Vec<String>,
    pub open_notices: i64,
    pub unseen_orders: i64,
    /// The events feed answers (orders arrive within seconds); else they are pulled every 2 minutes.
    pub events_live: bool,
    /// On a LAN till: the main PC that runs the link (its name), and Settings › DZBuild is
    /// read-only there (spec §4.6); `None` on the main PC or a single PC. The engine never runs on
    /// a till, so only the status command fills it.
    pub main_pc: Option<String>,
}

enum Command {
    Wake,
    LinkChanged,
    /// A page of the events feed from the events thread `generation`, fetched for the link `link`.
    Events {
        generation: u64,
        link: String,
        page: Page<EventItem>,
    },
    /// The events thread `generation` ended.
    EventsStopped {
        generation: u64,
        why: FeedStop,
    },
    Shutdown,
}

/// Why the events thread ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeedStop {
    /// DZBuild has no feed yet: orders are pulled by date until the daily retry.
    NotReady,
    /// The refresh token was refused: the link is suspended.
    Relink,
    /// DZBuild refused the feed even a fresh token: orders are pulled by date and the feed is
    /// asked again after a `heartbeat` period, never in a loop that rotates the refresh token.
    Unauthorized,
}

/// The linked store's client and its discovered metadata, shared with the owner's commands;
/// `None` until the first cycle online, and again after a link change.
type SharedRemote = Arc<RwLock<Option<(Remote, Arc<Metadata>)>>>;

pub struct SyncHandle {
    tx: Sender<Command>,
    status: Arc<RwLock<SyncStatus>>,
    remote: SharedRemote,
    done: Receiver<()>,
    worker: Option<JoinHandle<()>>,
}

impl SyncHandle {
    pub fn start(deps: EngineDeps) -> Result<SyncHandle, AppError> {
        let (tx, rx) = unbounded();
        let (done_tx, done) = bounded(1);
        let mut state = Engine::new(deps, tx.clone());
        let (status, remote) = (state.status.clone(), state.remote.clone());
        let worker = std::thread::Builder::new()
            .name("pos-cloud-sync".into())
            .spawn(move || {
                state.run(&rx);
                let _ = done_tx.send(());
            })
            .map_err(|e| AppError::Internal(format!("sync thread: {e}")))?;
        Ok(SyncHandle {
            tx,
            status,
            remote,
            done,
            worker: Some(worker),
        })
    }

    pub fn status(&self) -> SyncStatus {
        self.status
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// "Synchroniser maintenant", and after a cloud setting changed.
    pub fn wake(&self) {
        let _ = self.tx.send(Command::Wake);
    }

    pub fn link_changed(&self) {
        let _ = self.tx.send(Command::LinkChanged);
    }

    pub fn notifier(&self) -> SyncNotifier {
        SyncNotifier(self.tx.clone())
    }

    /// The linked store's client, for the owner's commands (claim, lookups).
    pub fn remote(&self) -> Option<Remote> {
        self.remote
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(r, _)| r.clone())
    }

    pub fn meta(&self) -> Option<Arc<Metadata>> {
        self.remote
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(_, m)| m.clone())
    }
}

impl Drop for SyncHandle {
    /// Stops the sync thread (it holds a database handle, which must go with the state) and waits
    /// at most 2 s: a request in flight is abandoned — its outbox item stays pending with its key.
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if self.done.recv_timeout(Duration::from_secs(2)).is_ok() {
            if let Some(worker) = self.worker.take()
                && worker.join().is_err()
            {
                tracing::error!(target: "cloud", "the sync thread panicked");
            }
        } else {
            tracing::warn!(target: "cloud", "the sync thread is still in a request; leaving it");
        }
    }
}

struct EventsThread {
    /// Its number: a page or an end sent by any other thread is stale.
    generation: u64,
    stop: Arc<AtomicBool>,
    /// The feed answers. Set by every page; cleared at once when DZBuild refuses the request or
    /// sends a page the POS cannot read, and after [`FEED_TRIES`] network or server failures in
    /// a row. A new thread is trusted until its first answer.
    healthy: Arc<AtomicBool>,
}

impl EventsThread {
    fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }
}

impl Drop for EventsThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct Last {
    orders: i64,
    catalogue: i64,
    heartbeat: i64,
    profile: i64,
    maintenance: i64,
}

/// What the run loop does after a command.
enum Next {
    Stop,
    /// A cycle now.
    Cycle,
    /// A cycle as soon as the queue is empty (events pages came).
    Soon,
    /// The cycle keeps its time.
    Keep,
}

struct Engine {
    deps: EngineDeps,
    tx: Sender<Command>,
    status: Arc<RwLock<SyncStatus>>,
    remote: SharedRemote,
    online: bool,
    offline_streak: usize,
    /// The link the client, the events thread and the kept pages belong to.
    bound: Option<String>,
    events: Option<EventsThread>,
    /// The number of the last events thread started.
    generation: u64,
    /// No events thread before this time (DZBuild refused the feed a fresh token).
    events_retry_at: i64,
    /// Events pages to apply, in order, each with the link it was fetched for.
    pages: Vec<(String, Page<EventItem>)>,
    /// Tries in a row of the first page in `pages` that did not apply ([`UNREADABLE_TRIES`]).
    page_tries: u32,
    last: Last,
    last_unseen: i64,
}

fn ms(d: Duration) -> i64 {
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

/// Ends the cycle: the network or DZBuild is down, or the link must be suspended. An answer the
/// POS cannot read, or a refusal, is its own step's problem: the rest of the cycle goes on.
fn fatal(e: &CallError) -> bool {
    matches!(
        e,
        CallError::Offline(_)
            | CallError::Server { .. }
            | CallError::Relink
            | CallError::Unauthorized
    )
}

/// A database error in the middle of the network work: stop the cycle (logged by code only).
fn db_stop(e: AppError) -> CallError {
    tracing::error!(target: "cloud", code = %e.code(), "database error while syncing");
    CallError::Invalid(format!("database: {}", e.code()))
}

impl Engine {
    fn new(deps: EngineDeps, tx: Sender<Command>) -> Engine {
        Engine {
            deps,
            tx,
            status: Arc::new(RwLock::new(SyncStatus::default())),
            remote: Arc::new(RwLock::new(None)),
            online: true,
            offline_streak: 0,
            bound: None,
            events: None,
            generation: 0,
            events_retry_at: 0,
            pages: Vec::new(),
            page_tries: 0,
            last: Last::default(),
            last_unseen: -1,
        }
    }

    fn run(&mut self, rx: &Receiver<Command>) {
        let mut next_cycle = Instant::now();
        loop {
            let command =
                match rx.recv_timeout(next_cycle.saturating_duration_since(Instant::now())) {
                    Ok(command) => command,
                    Err(RecvTimeoutError::Timeout) => Command::Wake,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
            match self.handle(command) {
                Next::Stop => break,
                Next::Keep => continue,
                Next::Soon => {
                    next_cycle = Instant::now();
                    continue;
                }
                Next::Cycle => {}
            }
            if let Err(e) = self.cycle() {
                tracing::warn!(target: "cloud", code = %e.code(), "sync cycle failed");
            }
            let wait = if self.online {
                self.deps.timing.cycle
            } else {
                self.deps.timing.offline_backoff[self.offline_streak.min(3)]
            };
            next_cycle = Instant::now() + wait;
        }
        self.events = None;
    }

    fn handle(&mut self, command: Command) -> Next {
        match command {
            Command::Shutdown => Next::Stop,
            Command::Wake => Next::Cycle,
            Command::LinkChanged => {
                self.let_go();
                Next::Cycle
            }
            Command::Events {
                generation,
                link,
                page,
            } => {
                // a thread let go (a link change, a page waiting) hands nothing over: its cursor
                // was not kept, the next thread fetches again from the stored one
                if self.current(generation) {
                    self.pages.push((link, page));
                    Next::Soon
                } else {
                    Next::Keep
                }
            }
            Command::EventsStopped { generation, why } => {
                self.events_stopped(generation, why);
                Next::Keep
            }
        }
    }

    fn current(&self, generation: u64) -> bool {
        self.events
            .as_ref()
            .is_some_and(|e| e.generation == generation)
    }

    /// Forgets the client, the events thread and its pages (a link change, a suspended link).
    fn let_go(&mut self) {
        *self.remote.write().unwrap_or_else(PoisonError::into_inner) = None;
        self.events = None;
        self.pages.clear();
        self.page_tries = 0;
        self.events_retry_at = 0;
    }

    fn feed_live(&self) -> bool {
        self.events.as_ref().is_some_and(EventsThread::healthy)
    }

    fn now(&self) -> i64 {
        self.deps.link.http.clock().now_ms()
    }

    fn cycle(&mut self) -> Result<(), AppError> {
        let db = self.deps.link.db.clone();
        let link = db.read(store::active_link)?;
        let settings = db.read(store::cloud_settings)?;
        let now = self.now();
        // Local work runs whatever the link and the network: the loi 18-07 purge too.
        self.maintain(&settings, now);
        let Some(link) = link else {
            self.let_go();
            self.bound = None;
            self.publish_status(None, &settings, now, false);
            return Ok(());
        };
        if self.bound.as_deref() != Some(link.id.as_str()) {
            self.let_go();
            self.bound = Some(link.id.clone());
        }
        // The outbox is ready when DZBuild is. A long change feed or a catalogue walk is read to
        // its end, a chunk per transaction, so a sale commits between two chunks.
        loop {
            let (l, s) = (link.clone(), settings.clone());
            let derived = db.write(move |tx| derive(tx, &l, &s, now))?;
            if !derived.more {
                break;
            }
        }
        if link.status != LinkStatus::Linked || !settings.sync_enabled {
            if link.status == LinkStatus::Linked {
                // paused: the feed stops, the owner's commands keep the client
                self.events = None;
                self.pages.clear();
            } else {
                self.let_go();
            }
            self.publish_status(Some(&link), &settings, now, false);
            return Ok(());
        }
        let before = self.deps.link.http.counts();
        let outcome = self.network(&link, &settings, now);
        let after = self.deps.link.http.counts();
        match outcome {
            // Calls went out and none succeeded (each refused or failed in its own way): the
            // cycle synced nothing, whatever each step made of its failure.
            Ok(()) if after.0 > before.0 && after.1 == before.1 => {
                tracing::warn!(target: "cloud", calls = after.0 - before.0, "no DZBuild call succeeded this cycle");
                self.online = false;
                self.publish_status(Some(&link), &settings, now, false);
            }
            Ok(()) => {
                self.online = true;
                self.offline_streak = 0;
                let id = link.id.clone();
                db.write(move |tx| store::touch_sync(tx, &id, now))?;
                self.publish_status(Some(&link), &settings, now, true);
            }
            Err(CallError::Relink | CallError::Unauthorized) => {
                self.let_go();
                link::mark_relink_required(&self.deps.link, &link)?;
                self.publish_status(Some(&link), &settings, now, false);
            }
            Err(e) if e.is_offline() || e.is_transient() => {
                self.online = false;
                self.offline_streak += 1;
                self.publish_status(Some(&link), &settings, now, false);
            }
            Err(e) => {
                tracing::warn!(target: "cloud", code = e.code(), "sync stopped early");
                self.publish_status(Some(&link), &settings, now, false);
            }
        }
        Ok(())
    }

    /// Local housekeeping once a `maintenance` period, linked or not: sent and skipped items
    /// after 30 days, the old event inbox, and (loi 18-07) the buyer details of the online orders
    /// closed `cloud.orders.retention_days` ago. A database error waits for the next cycle.
    fn maintain(&mut self, settings: &store::CloudSettings, now: i64) {
        if now - self.last.maintenance < ms(self.deps.timing.maintenance) {
            return;
        }
        let days = settings.orders_retention_days;
        let done = self.deps.link.db.write(move |tx| {
            outbox::prune_sent(tx, now)?;
            tx.execute(
                "DELETE FROM cloud_inbox WHERE received_at < ?1",
                [now - outbox::PRUNE_AFTER_MS],
            )?;
            // orders of a store no longer linked (an unlink that predates this rule) close too
            if store::active_link(tx)?.is_none() {
                let stores: Vec<String> = {
                    let mut stmt = tx.prepare(
                        "SELECT DISTINCT store_id FROM online_orders WHERE closed_at IS NULL",
                    )?;
                    stmt.query_map([], |r| r.get(0))?
                        .collect::<Result<Vec<_>, _>>()?
                };
                for store in stores {
                    super::orders::close_unlinked_store(tx, &store, now)?;
                }
            }
            super::orders::purge_personal_data(tx, days, now)?;
            Ok(())
        });
        match done {
            Ok(()) => self.last.maintenance = now,
            Err(e) => tracing::error!(target: "cloud", code = %e.code(), "cloud maintenance"),
        }
    }

    fn remote_for(&mut self, link: &CloudLink) -> Result<Remote, CallError> {
        if let Some((remote, _)) = self
            .remote
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            return Ok(remote.clone());
        }
        let meta = oauth::discover(&self.deps.link.http, &self.deps.link.config).map_err(|e| {
            match e.code().as_str() {
                "cloud_unavailable" | "cloud_rate_limited" => {
                    CallError::Offline("discovery".into())
                }
                _ => CallError::Invalid("discovery".into()),
            }
        })?;
        let meta = Arc::new(meta);
        let remote = link::remote_for(&self.deps.link, link, meta.clone());
        *self.remote.write().unwrap_or_else(PoisonError::into_inner) = Some((remote.clone(), meta));
        Ok(remote)
    }

    fn network(
        &mut self,
        link: &CloudLink,
        settings: &store::CloudSettings,
        now: i64,
    ) -> Result<(), CallError> {
        let remote = self.remote_for(link)?;
        let db = self.deps.link.db.clone();
        let actor = Actor::system(Some(self.deps.link.device.terminal_id.clone()));
        // Events brought by the other thread, in order. A page fetched for another link never
        // applies here: its stock moves and its cursor are that link's.
        let mut pages = std::mem::take(&mut self.pages);
        pages.retain(|(fetched_for, _)| *fetched_for == link.id);
        for (index, (_, page)) in pages.iter().enumerate() {
            let ctx = PullContext {
                remote: &remote,
                db: &db,
                link,
                settings,
                actor: actor.clone(),
                now,
            };
            // The last try records what it still cannot read `failed`
            let unreadable = if self.page_tries.saturating_add(1) >= UNREADABLE_TRIES {
                Unreadable::Record
            } else {
                Unreadable::Wait
            };
            match pull::apply_events_with(&ctx, page, unreadable) {
                Ok(report) if report.revoked => {
                    self.let_go();
                    link::mark_revoked(&self.deps.link, link).map_err(db_stop)?;
                    return Ok(());
                }
                Ok(_) => self.page_tries = 0,
                Err(e) => {
                    self.pages = pages[index..].to_vec();
                    if fatal(&e) {
                        return Err(e);
                    }
                    // A page that cannot be applied here (a detail the POS cannot read) waits for
                    // the next cycle and holds up nothing else, up to `UNREADABLE_TRIES` tries.
                    // The feed stops meanwhile, so no page piles up behind it, and orders are
                    // pulled by date.
                    self.page_tries = self.page_tries.saturating_add(1);
                    tracing::warn!(target: "cloud", code = e.code(), tries = self.page_tries, "an events page waits for the next cycle");
                    self.events = None;
                    break;
                }
            }
        }
        let media = self.deps.media_dir.clone();
        push::push_due(&PushContext {
            remote: &remote,
            db: &db,
            link,
            settings,
            media_dir: media.as_deref(),
            now,
        })?;
        if self.events.is_none()
            && self.pages.is_empty()
            && now >= self.events_retry_at
            && self.due(link, Endpoint::Events, now)?
        {
            let thread = self.spawn_events(remote.clone(), &link.id);
            self.events = Some(thread);
        }
        let live = self.feed_live();
        let ctx = PullContext {
            remote: &remote,
            db: &db,
            link,
            settings,
            actor,
            now,
        };
        if !live && now - self.last.orders >= ms(self.deps.timing.orders_poll) {
            self.pull_by_date(link, Endpoint::Orders, now, || pull::pull_orders(&ctx))?;
            self.last.orders = now;
        }
        if !live && now - self.last.catalogue >= ms(self.deps.timing.catalogue_poll) {
            self.pull_by_date(link, Endpoint::Products, now, || pull::pull_products(&ctx))?;
            self.last.catalogue = now;
        }
        if now - self.last.profile >= ms(self.deps.timing.profile) {
            match api::store_profile(&remote) {
                Ok(profile) => {
                    let id = link.id.clone();
                    db.write(move |tx| {
                        store::update_profile(
                            tx,
                            &id,
                            &store::ProfileUpdate {
                                store_name: profile.name.clone(),
                                store_plan: profile.plan.clone(),
                                public_url: profile.public_url.clone(),
                                stock_deduction: profile.stock_deduction.clone(),
                                active_products_cap: profile
                                    .plan_limits
                                    .as_ref()
                                    .and_then(|l| l.active_products),
                                ..store::ProfileUpdate::default()
                            },
                        )
                    })
                    .map_err(db_stop)?;
                }
                Err(CallError::NotReady { .. }) => {}
                Err(e) if fatal(&e) => return Err(e),
                Err(e) => tracing::warn!(target: "cloud", code = e.code(), "store profile"),
            }
            self.last.profile = now;
        }
        if let Some(device) = link.device_id.as_deref()
            && now - self.last.heartbeat >= ms(self.deps.timing.heartbeat)
        {
            let store = link.store_id.clone();
            let counts = db
                .read(move |c| outbox::counts(c, &store))
                .map_err(db_stop)?;
            let beat = api::Heartbeat {
                app_version: self.deps.link.device.app_version.clone(),
                pending: counts.iter().map(|c| c.pending).sum(),
                failed: counts.iter().map(|c| c.failed).sum(),
                last_sync_at: link.last_sync_at.map(super::wire::ApiTime),
            };
            match api::heartbeat(&remote, device, &beat) {
                Ok(()) | Err(CallError::NotReady { .. }) => {}
                // DZBuild no longer knows this device: the owner removed it
                Err(CallError::Rejected {
                    status: 404 | 410, ..
                }) => {
                    self.let_go();
                    link::mark_revoked(&self.deps.link, link).map_err(db_stop)?;
                    return Ok(());
                }
                Err(e) if fatal(&e) => return Err(e),
                Err(e) => tracing::warn!(target: "cloud", code = e.code(), "heartbeat"),
            }
            self.last.heartbeat = now;
        }
        Ok(())
    }

    fn due(&self, link: &CloudLink, endpoint: Endpoint, now: i64) -> Result<bool, CallError> {
        let store = link.store_id.clone();
        self.deps
            .link
            .db
            .read(move |c| store::endpoint_due(c, &store, endpoint, now))
            .map_err(db_stop)
    }

    /// Orders and products by `updated_since` while the events feed is not shipped or not
    /// answering; an endpoint DZBuild has not shipped either is asked again the next day (one
    /// notice). A list the POS cannot read waits for the next poll and stops nothing else.
    fn pull_by_date(
        &self,
        link: &CloudLink,
        endpoint: Endpoint,
        now: i64,
        pull: impl FnOnce() -> Result<pull::PullReport, CallError>,
    ) -> Result<(), CallError> {
        if !self.due(link, endpoint, now)? {
            return Ok(());
        }
        let store = link.store_id.clone();
        match pull() {
            Ok(_) => self
                .deps
                .link
                .db
                .write(move |tx| store::set_ready(tx, &store, endpoint, now))
                .map_err(db_stop),
            Err(CallError::NotReady { .. }) => self
                .deps
                .link
                .db
                .write(move |tx| store::set_not_ready(tx, &store, endpoint, now).map(drop))
                .map_err(db_stop),
            Err(e) if fatal(&e) => Err(e),
            Err(e) => {
                tracing::warn!(target: "cloud", endpoint = endpoint.name(), code = e.code(), "pull by date");
                Ok(())
            }
        }
    }

    /// The events thread `generation` ended. Not shipped: pull by date until tomorrow's retry. A
    /// refused refresh token: the link is suspended now (in an idle shop no other call may meet
    /// it for a long time). A fresh token refused on the feed alone: pull by date, and ask the
    /// feed again after a `heartbeat` period (the next cycle's pulls meet a token refused
    /// everywhere and suspend the link). The next cycle keeps its time.
    fn events_stopped(&mut self, generation: u64, why: FeedStop) {
        if !self.current(generation) {
            return; // a thread already let go: its end says nothing about the current link
        }
        self.events = None;
        let db = self.deps.link.db.clone();
        let now = self.now();
        let link = match db.read(store::active_link) {
            Ok(Some(link))
                if self.bound.as_deref() == Some(link.id.as_str())
                    && link.status == LinkStatus::Linked =>
            {
                link
            }
            Ok(_) => return,
            Err(e) => {
                tracing::error!(target: "cloud", code = %e.code(), "could not read the link after the events feed ended");
                return;
            }
        };
        match why {
            FeedStop::NotReady => {
                let store_id = link.store_id.clone();
                let outcome = db.write(move |tx| {
                    store::set_not_ready(tx, &store_id, Endpoint::Events, now).map(drop)
                });
                if let Err(e) = outcome {
                    tracing::error!(target: "cloud", code = %e.code(), "could not record the events feed as not shipped");
                }
            }
            FeedStop::Unauthorized => {
                self.events_retry_at = now.saturating_add(ms(self.deps.timing.heartbeat));
            }
            FeedStop::Relink => {
                self.let_go();
                if let Err(e) = link::mark_relink_required(&self.deps.link, &link) {
                    tracing::error!(target: "cloud", code = %e.code(), "could not suspend the link");
                }
            }
        }
    }

    fn spawn_events(&mut self, remote: Remote, link_id: &str) -> EventsThread {
        self.generation += 1;
        let generation = self.generation;
        let stop = Arc::new(AtomicBool::new(false));
        let healthy = Arc::new(AtomicBool::new(true));
        let (flag, answers, tx, wait, link) = (
            stop.clone(),
            healthy.clone(),
            self.tx.clone(),
            self.deps.timing.events_wait_secs,
            link_id.to_owned(),
        );
        let cursor = self
            .deps
            .link
            .db
            .read(|c| store::remote_cursor(c, "cloud:events"))
            .unwrap_or(None);
        let spawned = std::thread::Builder::new()
            .name("pos-cloud-events".into())
            .spawn(move || {
                let mut cursor = cursor;
                let mut pause = Duration::from_millis(500);
                let mut failures = 0_u32;
                let stopped = |why| {
                    let _ = tx.send(Command::EventsStopped { generation, why });
                };
                while !flag.load(Ordering::SeqCst) {
                    match pull::long_poll(&remote, cursor.as_deref(), wait) {
                        Ok(page) => {
                            pause = Duration::from_millis(500);
                            failures = 0;
                            answers.store(true, Ordering::SeqCst);
                            if let Some(next) = page.next_cursor.clone() {
                                cursor = Some(next);
                            }
                            // let go while the request was in flight: hand nothing over
                            if flag.load(Ordering::SeqCst) {
                                break;
                            }
                            let empty = page.items.is_empty();
                            if !empty
                                && tx
                                    .send(Command::Events {
                                        generation,
                                        link: link.clone(),
                                        page,
                                    })
                                    .is_err()
                            {
                                break;
                            }
                            if empty && wait == 0 {
                                std::thread::sleep(Duration::from_millis(100));
                            }
                        }
                        Err(CallError::NotReady { .. }) => {
                            stopped(FeedStop::NotReady);
                            break;
                        }
                        Err(CallError::Relink) => {
                            stopped(FeedStop::Relink);
                            break;
                        }
                        Err(CallError::Unauthorized) => {
                            stopped(FeedStop::Unauthorized);
                            break;
                        }
                        Err(e) => {
                            failures = failures.saturating_add(1);
                            // a refusal or a page the POS cannot read does not mend by itself;
                            // the network or DZBuild down gets a few tries first
                            let passing = e.is_offline() || matches!(e, CallError::Server { .. });
                            if !passing || failures >= FEED_TRIES {
                                answers.store(false, Ordering::SeqCst);
                            }
                            std::thread::sleep(pause);
                            pause = (pause * 2).min(Duration::from_secs(60));
                        }
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::error!(target: "cloud", reason = %e, "events thread");
            healthy.store(false, Ordering::SeqCst);
        }
        EventsThread {
            generation,
            stop,
            healthy,
        }
    }

    fn publish_status(
        &mut self,
        link: Option<&CloudLink>,
        settings: &store::CloudSettings,
        now: i64,
        success: bool,
    ) {
        let db = self.deps.link.db.clone();
        let store = link.map(|l| l.store_id.clone()).unwrap_or_default();
        let (entities, not_ready, open, unseen) = db
            .read(move |c| {
                Ok((
                    outbox::counts(c, &store)?,
                    store::not_ready_endpoints(c, &store)?,
                    store::notices(c, false)?.len() as i64,
                    super::orders::unseen_count(c)?,
                ))
            })
            .unwrap_or_default();
        let previous = self
            .status
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let status = SyncStatus {
            linked: link.is_some(),
            link_status: link.map(|l| l.status),
            store_name: link.map(|l| l.store_name.clone()),
            public_url: link.and_then(|l| l.public_url.clone()),
            online: self.online && link.is_some(),
            paused: !settings.sync_enabled,
            running: link.is_some_and(|l| l.status == LinkStatus::Linked) && settings.sync_enabled,
            last_cycle_at: Some(now),
            last_success_at: if success {
                Some(now)
            } else {
                previous.last_success_at
            },
            pending: entities.iter().map(|e| e.pending).sum(),
            failed: entities.iter().map(|e| e.failed).sum(),
            entities,
            not_ready,
            open_notices: open,
            unseen_orders: unseen,
            events_live: self.feed_live(),
            main_pc: None,
        };
        let changed = status_differs(&previous, &status);
        *self.status.write().unwrap_or_else(PoisonError::into_inner) = status.clone();
        if changed {
            (self.deps.on_status)(&status);
        }
        if unseen != self.last_unseen {
            self.last_unseen = unseen;
            (self.deps.on_orders)(unseen);
        }
    }
}

/// Everything but the clock fields: the window is told only when something it shows changed.
fn status_differs(a: &SyncStatus, b: &SyncStatus) -> bool {
    let strip = |s: &SyncStatus| SyncStatus {
        last_cycle_at: None,
        last_success_at: None,
        ..s.clone()
    };
    strip(a) != strip(b) || a.last_success_at.is_none() != b.last_success_at.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::api::DeviceInfo;
    use crate::cloud::http::{Clock, Http, ManualClock, SystemClock};
    use crate::cloud::link::LinkContext;
    use crate::cloud::secrets::{MemorySecrets, SecretStore, refresh_account};
    use crate::cloud::store::CloudNoticeKind;
    use crate::cloud::testkit::{MockDz, created, linked_shop, ok};
    use crate::money::{Money, Qty};
    use crate::payments::PaymentInput;
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::testkit::Shop;
    use serde_json::json;
    use std::time::Instant;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    const LINK_ID: &str = "00000000-0000-7000-8000-00000000c001";

    /// How a test engine runs: its client's timeout and clock, its timing, its callbacks.
    struct Opts {
        request_timeout: Duration,
        clock: Arc<dyn Clock>,
        timing: Timing,
        on_status: Arc<dyn Fn(&SyncStatus) + Send + Sync>,
    }

    impl Opts {
        fn new(request_timeout: Duration) -> Opts {
            Opts {
                request_timeout,
                clock: Arc::new(SystemClock),
                timing: Timing::fast(),
                on_status: Arc::new(|_| {}),
            }
        }
    }

    fn deps(shop: &Shop, dz_uri: &str, opts: Opts) -> (EngineDeps, Arc<MemorySecrets>) {
        let secrets = Arc::new(MemorySecrets::default());
        secrets.set(&refresh_account(LINK_ID), "rt_1").unwrap();
        let config = crate::cloud::config::CloudConfig::loopback(dz_uri);
        let http = Http::with_timeouts(
            &config,
            opts.clock,
            "t".into(),
            Duration::from_secs(2),
            opts.request_timeout,
        )
        .unwrap();
        let link = Arc::new(LinkContext {
            db: shop.db.clone(),
            http: Arc::new(http),
            config: Arc::new(config),
            secrets: secrets.clone(),
            device: DeviceInfo {
                name: "C1".into(),
                os: "Windows".into(),
                app_version: "0.7.0".into(),
                terminal_code: "C1".into(),
                terminal_id: shop.terminal.id.clone(),
            },
            on_change: Arc::new(|| {}),
        });
        let deps = EngineDeps {
            link,
            media_dir: None,
            timing: opts.timing,
            on_status: opts.on_status,
            on_orders: Arc::new(|_| {}),
        };
        (deps, secrets)
    }

    fn engine(
        shop: &Shop,
        dz_uri: &str,
        request_timeout: Duration,
    ) -> (SyncHandle, Arc<MemorySecrets>) {
        engine_opts(shop, dz_uri, Opts::new(request_timeout))
    }

    fn engine_opts(shop: &Shop, dz_uri: &str, opts: Opts) -> (SyncHandle, Arc<MemorySecrets>) {
        let (deps, secrets) = deps(shop, dz_uri, opts);
        (SyncHandle::start(deps).unwrap(), secrets)
    }

    fn requests_to(dz: &MockDz, path: &str) -> usize {
        dz.requests()
            .iter()
            .filter(|r| r.url.path() == path)
            .count()
    }

    fn link_status(shop: &Shop) -> Option<LinkStatus> {
        shop.db.read(store::active_link).unwrap().map(|l| l.status)
    }

    fn empty_page() -> ResponseTemplate {
        ok(json!({ "items": [], "next_cursor": null, "has_more": false }))
    }

    /// Order 9001 as DZBuild sends it, with its buyer.
    fn order_9001() -> serde_json::Value {
        json!({
            "id": 9001, "order_number": 1042, "status": "confirmed", "subtotal": 100, "total": 100,
            "customer_name": "Samira B.", "customer_phone": "0550 12 34 56", "address": "Cité 5 juillet",
            "commune": "Bab Ezzouar", "customer_notes": "après 17h",
            "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00", "items": []
        })
    }

    fn until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn sell(shop: &Shop) -> String {
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let till = shop.till();
        let input = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(&milk.id, Qty(1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(10_000))],
        };
        shop.db
            .write(move |tx| create_sale(tx, &till, input))
            .unwrap()
            .document
            .id
    }

    #[test]
    fn offline_keeps_the_outbox_and_raises_no_notice() {
        let (shop, _) = linked_shop();
        sell(&shop);
        let (handle, _) = engine(&shop, "http://127.0.0.1:9", Duration::from_secs(2));
        until("an offline cycle", || {
            handle.status().last_cycle_at.is_some()
        });
        let status = handle.status();
        assert!(status.linked && !status.online);
        assert!(status.pending >= 1, "the sale waits: {status:?}");
        assert!(
            shop.db
                .read(|c| crate::cloud::store::notices(c, false))
                .unwrap()
                .is_empty(),
            "no notice for a dead network"
        );
    }

    #[test]
    fn not_ready_raises_one_notice_and_retries_daily() {
        let dz = MockDz::start();
        dz.mount_link(crate::cloud::testkit::one_store());
        dz.mount(Mock::given(path("/v1/pos/sales")).respond_with(ResponseTemplate::new(501)));
        dz.mount(Mock::given(path("/v1/events")).respond_with(ResponseTemplate::new(404)));
        dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(
            json!({ "items": [], "next_cursor": null, "has_more": false }),
        )));
        dz.mount(Mock::given(path("/v1/products")).respond_with(ok(
            json!({ "items": [], "next_cursor": null, "has_more": false }),
        )));
        dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
        let (shop, _) = linked_shop();
        sell(&shop);
        // the engine's day is the clock's: moved by hand below
        let clock = ManualClock::at(crate::time::now_ms());
        let (handle, _) = engine_opts(
            &shop,
            &dz.uri(),
            Opts {
                clock: clock.clone(),
                ..Opts::new(Duration::from_secs(5))
            },
        );
        until("three cycles", || handle.status().last_success_at.is_some());
        until("the feed found not shipped", || {
            shop.db
                .read(|c| store::not_ready_endpoints(c, "141"))
                .unwrap()
                .contains(&"events".to_owned())
        });
        std::thread::sleep(Duration::from_millis(500)); // several more fast cycles
        let sales = requests_to(&dz, "/v1/pos/sales");
        assert_eq!(sales, 1, "asked once, not every cycle");
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        let subjects: Vec<&str> = notices
            .iter()
            .filter(|n| n.code == "cloud_not_ready")
            .map(|n| n.subject.as_str())
            .collect();
        assert!(
            subjects.contains(&"pos.sales") && subjects.contains(&"events"),
            "{subjects:?}"
        );
        assert!(handle.status().not_ready.contains(&"pos.sales".to_owned()));
        assert!(
            !handle.status().events_live,
            "orders are pulled by date instead"
        );
        // a day later, the endpoint is asked again (and still not shipped: one notice)
        clock.advance(store::NOT_READY_RETRY_MS + 1_000);
        handle.wake();
        until("the daily retry", || requests_to(&dz, "/v1/pos/sales") == 2);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(requests_to(&dz, "/v1/pos/sales"), 2, "once a day");
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        let sales_notices: Vec<_> = notices
            .iter()
            .filter(|n| n.code == "cloud_not_ready" && n.subject == "pos.sales")
            .collect();
        assert_eq!(sales_notices.len(), 1, "{notices:?}");
    }

    #[test]
    fn a_restart_mid_push_resends_the_same_key_and_keeps_one_row() {
        let dz = MockDz::start();
        dz.mount_link(crate::cloud::testkit::one_store());
        dz.mount(Mock::given(path("/v1/events")).respond_with(ResponseTemplate::new(404)));
        dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(
            json!({ "items": [], "next_cursor": null, "has_more": false }),
        )));
        dz.mount(Mock::given(path("/v1/products")).respond_with(ok(
            json!({ "items": [], "next_cursor": null, "has_more": false }),
        )));
        dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
        // DZBuild records the sale, but its answer never reaches the till (timeout, crash)
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/sales"))
                .respond_with(created(json!({ "id": 99120 })).set_delay(Duration::from_secs(3)))
                .up_to_n_times(1),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/sales"))
                .respond_with(created(json!({ "id": 99120 }))),
        );
        let (shop, _) = linked_shop();
        let ticket = sell(&shop);
        let (handle, _) = engine(&shop, &dz.uri(), Duration::from_secs(1));
        until("the sale sent", || {
            shop.db
                .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM sync_outbox WHERE kind = 'pos_sale' AND status = 'sent'", [], |r| r.get::<_, i64>(0))?))
                .unwrap()
                == 1
        });
        drop(handle);
        let keys: Vec<String> = dz
            .requests()
            .iter()
            .filter(|r| r.url.path() == "/v1/pos/sales")
            .map(|r| crate::cloud::testkit::header(r, "idempotency-key").unwrap())
            .collect();
        assert!(keys.len() >= 2, "the lost answer was asked again");
        assert!(keys.iter().all(|k| *k == format!("sale:{ticket}")));
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM sync_outbox WHERE kind = 'pos_sale'"),
            1
        );
    }

    #[test]
    fn a_refused_refresh_suspends_the_link() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/.well-known/oauth-authorization-server")).respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(crate::cloud::oauth::tests::metadata_json(&dz.uri())),
            ),
        );
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
        ));
        let (shop, _) = linked_shop();
        sell(&shop);
        let (handle, secrets) = engine(&shop, &dz.uri(), Duration::from_secs(2));
        until("the relink state", || {
            handle.status().link_status == Some(crate::cloud::store::LinkStatus::RelinkRequired)
        });
        assert_eq!(
            secrets
                .get(&refresh_account("00000000-0000-7000-8000-00000000c001"))
                .unwrap(),
            None
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM sync_outbox WHERE status = 'pending'"),
            1,
            "the sale still waits"
        );
    }

    #[test]
    fn events_are_applied_once_and_the_cursor_survives_a_restart() {
        let dz = MockDz::start();
        dz.mount_link(crate::cloud::testkit::one_store());
        dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
        dz.mount(
            Mock::given(path("/v1/events"))
                .and(query_param("cursor", "c1"))
                .respond_with(
                    ok(json!({ "items": [], "next_cursor": "c1", "has_more": false }))
                        .set_delay(Duration::from_millis(200)),
                ),
        );
        dz.mount(Mock::given(path("/v1/events")).respond_with(ok(json!({
            "items": [{ "id": "e1", "type": "order.created", "data": { "id": 9001 } }], "next_cursor": "c1", "has_more": false
        }))));
        dz.mount(Mock::given(path("/v1/orders/9001")).respond_with(ok(json!({
            "id": 9001, "order_number": 1042, "status": "confirmed", "subtotal": 100, "total": 100,
            "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00", "items": []
        }))));
        let (shop, _) = linked_shop();
        let (handle, _) = engine(&shop, &dz.uri(), Duration::from_secs(5));
        until("the order", || {
            shop.count("SELECT COUNT(*) FROM online_orders") == 1
        });
        assert!(handle.status().events_live);
        drop(handle);
        let (handle, _) = engine(&shop, &dz.uri(), Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(600));
        drop(handle);
        let first_pages = dz
            .requests()
            .iter()
            .filter(|r| {
                r.url.path() == "/v1/events" && !r.url.query().unwrap_or("").contains("cursor=c1")
            })
            .count();
        assert_eq!(first_pages, 1, "after the restart the feed resumes at c1");
        assert_eq!(shop.count("SELECT COUNT(*) FROM cloud_inbox"), 1);
    }

    #[test]
    fn a_catalogue_walk_is_derived_to_its_end_in_one_cycle() {
        use crate::catalog::products::{self, ProductInput};
        use crate::sync::changelog;
        let (shop, _) = linked_shop();
        // a catalogue from before the link: only the walk of "publish all" reads it
        shop.db
            .write(|tx| changelog::unregister(tx, changelog::CLOUD_CONSUMER))
            .unwrap();
        let rate = shop.tva19.clone();
        shop.db
            .write(move |tx| {
                for n in 0..crate::cloud::derive::EPOCH_BATCH + 3 {
                    let input = ProductInput::simple(
                        &format!("P{n}"),
                        crate::catalog::UNIT_PIECE_ID,
                        &rate,
                        Money(1_000),
                    );
                    products::create(tx, &Actor::system(None), input)?;
                }
                changelog::register(tx, changelog::CLOUD_CONSUMER, crate::testkit::NOON).map(drop)
            })
            .unwrap();
        shop.set("cloud.publish.mode", json!("all"));
        let first: Arc<std::sync::Mutex<Option<i64>>> = Arc::default();
        let (seen, db) = (first.clone(), shop.db.clone());
        let on_status: Arc<dyn Fn(&SyncStatus) + Send + Sync> = Arc::new(move |_| {
            let mut seen = seen.lock().unwrap();
            if seen.is_none() {
                *seen = Some(
                    db.read(|c| {
                        Ok(c.query_row(
                            "SELECT COUNT(*) FROM sync_outbox WHERE kind = 'product_upsert'",
                            [],
                            |r| r.get::<_, i64>(0),
                        )?)
                    })
                    .unwrap(),
                );
            }
        });
        let (handle, _) = engine_opts(
            &shop,
            crate::cloud::testkit::UNREACHABLE,
            Opts {
                on_status,
                ..Opts::new(Duration::from_secs(2))
            },
        );
        until("the first cycle", || first.lock().unwrap().is_some());
        drop(handle);
        let expected = i64::try_from(crate::cloud::derive::EPOCH_BATCH).unwrap() + 3;
        assert_eq!(
            *first.lock().unwrap(),
            Some(expected),
            "the whole catalogue in the first cycle, not a chunk per cycle"
        );
    }

    fn quiet_store(dz: &MockDz) {
        dz.mount_link(crate::cloud::testkit::one_store());
        dz.mount(Mock::given(path("/v1/events")).respond_with(ResponseTemplate::new(404)));
        dz.mount(Mock::given(path("/v1/orders")).respond_with(empty_page()));
        dz.mount(Mock::given(path("/v1/products")).respond_with(empty_page()));
        dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
        dz.mount(Mock::given(path("/v1/pos/closures")).respond_with(ResponseTemplate::new(501)));
    }

    fn slow_heartbeat() -> Timing {
        Timing {
            heartbeat: Duration::from_secs(3_600),
            profile: Duration::from_secs(3_600),
            ..Timing::fast()
        }
    }

    /// In an idle shop the events thread is the one refreshing the token: when DZBuild refuses
    /// the refresh token there, the link is suspended at once, and no thread is started again in
    /// a loop meanwhile.
    #[test]
    fn a_refresh_refused_on_the_events_feed_suspends_the_link() {
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/.well-known/oauth-authorization-server")).respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(crate::cloud::oauth::tests::metadata_json(&dz.uri())),
            ),
        );
        // the first refresh works; the next one is refused (the grant was revoked meanwhile)
        dz.mount(
            Mock::given(path("/oauth/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "at_2", "token_type": "Bearer", "expires_in": 900, "refresh_token": "rt_2"
                })))
                .up_to_n_times(1),
        );
        dz.mount(Mock::given(path("/oauth/token")).respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
        ));
        dz.mount(Mock::given(path("/v1/store")).respond_with(ok(json!({}))));
        dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
        dz.mount(Mock::given(path("/v1/orders")).respond_with(empty_page()));
        dz.mount(Mock::given(path("/v1/products")).respond_with(empty_page()));
        // the feed refuses the token once the first cycle is over
        dz.mount(
            Mock::given(path("/v1/events"))
                .respond_with(ResponseTemplate::new(401).set_delay(Duration::from_millis(300))),
        );
        let (shop, _) = linked_shop(); // nothing to push: the outbox is empty
        // no heartbeat nor profile after the first cycle: only the feed meets the refusal
        let (handle, secrets) = engine_opts(
            &shop,
            &dz.uri(),
            Opts {
                timing: slow_heartbeat(),
                ..Opts::new(Duration::from_secs(5))
            },
        );
        until("the relink state", || {
            link_status(&shop) == Some(LinkStatus::RelinkRequired)
        });
        std::thread::sleep(Duration::from_millis(300));
        drop(handle);
        assert_eq!(secrets.get(&refresh_account(LINK_ID)).unwrap(), None);
        let tokens = requests_to(&dz, "/oauth/token");
        assert!(tokens <= 2, "one refresh, one refusal: {tokens}");
        let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
        assert_eq!(
            notices
                .iter()
                .filter(|n| n.kind == CloudNoticeKind::Relink)
                .count(),
            1,
            "{notices:?}"
        );
    }

    /// DZBuild refuses the feed a fresh token (a scope it lacks): the feed is asked again later,
    /// not in a loop that rotates the refresh token each time; orders come by date meanwhile and
    /// the link stays.
    #[test]
    fn a_feed_refusing_a_fresh_token_is_asked_again_later_not_in_a_loop() {
        let dz = MockDz::start();
        dz.mount_link(crate::cloud::testkit::one_store());
        dz.mount(Mock::given(path("/v1/events")).respond_with(ResponseTemplate::new(401)));
        dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(
            json!({ "items": [order_9001()], "next_cursor": null, "has_more": false }),
        )));
        dz.mount(Mock::given(path("/v1/products")).respond_with(empty_page()));
        dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
        let (shop, _) = linked_shop();
        let (handle, _) = engine_opts(
            &shop,
            &dz.uri(),
            Opts {
                timing: slow_heartbeat(),
                ..Opts::new(Duration::from_secs(5))
            },
        );
        until("the order, by date", || {
            shop.count("SELECT COUNT(*) FROM online_orders") == 1
        });
        std::thread::sleep(Duration::from_millis(1_000));
        let status = handle.status();
        drop(handle);
        assert!(!status.events_live, "{status:?}");
        assert_eq!(link_status(&shop), Some(LinkStatus::Linked));
        let (events, tokens) = (
            requests_to(&dz, "/v1/events"),
            requests_to(&dz, "/oauth/token"),
        );
        assert!(
            events <= 2,
            "the refused feed is not asked again at once: {events}"
        );
        assert!(tokens <= 2, "{tokens}");
    }

    /// A feed DZBuild keeps refusing (an expired cursor) or failing (5xx) is not live: orders
    /// still arrive, pulled by date.
    #[test]
    fn a_failing_events_feed_falls_back_to_orders_by_date() {
        for answer in [
            ResponseTemplate::new(400)
                .set_body_json(json!({ "error": { "code": "cursor_invalid" } })),
            ResponseTemplate::new(500),
        ] {
            let dz = MockDz::start();
            dz.mount_link(crate::cloud::testkit::one_store());
            dz.mount(Mock::given(path("/v1/events")).respond_with(answer));
            dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(
                json!({ "items": [order_9001()], "next_cursor": null, "has_more": false }),
            )));
            dz.mount(Mock::given(path("/v1/products")).respond_with(empty_page()));
            dz.mount(
                Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))),
            );
            let (shop, _) = linked_shop();
            // the client's retries of a 5xx wait on this clock, not on the test
            let clock = ManualClock::at(crate::time::now_ms());
            let (handle, _) = engine_opts(
                &shop,
                &dz.uri(),
                Opts {
                    clock,
                    ..Opts::new(Duration::from_secs(5))
                },
            );
            until("the order, by date", || {
                shop.count("SELECT COUNT(*) FROM online_orders") == 1
            });
            // the cycle that pulled it publishes its status at its end
            until("the feed shown as not live", || {
                !handle.status().events_live
            });
            drop(handle);
        }
    }

    /// A page whose order detail the POS cannot read waits for the next cycle, but holds up
    /// nothing else: sales still go, the status stays online, orders come by date.
    #[test]
    fn an_events_page_that_cannot_be_read_holds_up_nothing_else() {
        let dz = MockDz::start();
        dz.mount_link(crate::cloud::testkit::one_store());
        dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(created(json!({}))));
        dz.mount(Mock::given(path("/v1/orders")).respond_with(empty_page()));
        dz.mount(Mock::given(path("/v1/products")).respond_with(empty_page()));
        dz.mount(
            Mock::given(path("/v1/events"))
                .and(query_param("cursor", "c1"))
                .respond_with(
                    ok(json!({ "items": [], "next_cursor": "c1", "has_more": false }))
                        .set_delay(Duration::from_millis(200)),
                ),
        );
        dz.mount(Mock::given(path("/v1/events")).respond_with(ok(json!({
            "items": [{ "id": "e1", "type": "order.created", "data": { "id": 9001 } }], "next_cursor": "c1", "has_more": false
        }))));
        // a detail in a shape this version cannot read
        dz.mount(
            Mock::given(path("/v1/orders/9001"))
                .respond_with(ok(json!({ "id": 9001, "status": 7 }))),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/sales"))
                .respond_with(created(json!({ "id": 99120 }))),
        );
        let (shop, _) = linked_shop();
        let (handle, _) = engine(&shop, &dz.uri(), Duration::from_secs(5));
        until("the page tried", || {
            requests_to(&dz, "/v1/orders/9001") >= 1
        });
        sell(&shop);
        until("the sale sent", || {
            shop.db
                .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM sync_outbox WHERE kind = 'pos_sale' AND status = 'sent'", [], |r| r.get::<_, i64>(0))?))
                .unwrap()
                == 1
        });
        until("the page tried again", || {
            requests_to(&dz, "/v1/orders/9001") >= 2
        });
        let status = handle.status();
        drop(handle);
        assert!(status.online, "{status:?}");
        assert!(
            !status.events_live,
            "orders are pulled by date while the page waits"
        );
        assert!(requests_to(&dz, "/v1/orders") >= 1, "pulled by date");
        let cursor = shop
            .db
            .read(|c| store::remote_cursor(c, "cloud:events"))
            .unwrap();
        assert_eq!(cursor, None, "nothing skipped: the page is still to apply");
        assert_eq!(shop.count("SELECT COUNT(*) FROM cloud_inbox"), 0);
    }

    /// loi 18-07: an online order's buyer details go `retention_days` after it closed, also
    /// while the link waits for a relink or after an unlink.
    #[test]
    fn buyer_details_are_purged_whatever_the_link() {
        for status in [LinkStatus::RelinkRequired, LinkStatus::Unlinked] {
            let (shop, link) = linked_shop();
            let order: crate::cloud::orders::RemoteOrder =
                serde_json::from_str(&order_9001().to_string()).unwrap();
            let closed = crate::time::now_ms() - 91 * 86_400_000;
            shop.db
                .write(move |tx| {
                    crate::cloud::orders::upsert_remote(tx, &link, &order, crate::testkit::NOON)?;
                    tx.execute("UPDATE online_orders SET closed_at = ?1", [closed])?;
                    store::set_status(tx, &link.id, status, crate::testkit::NOON)
                })
                .unwrap();
            let (handle, _) = engine(
                &shop,
                crate::cloud::testkit::UNREACHABLE,
                Duration::from_secs(2),
            );
            until("the purge", || {
                shop.count(
                    "SELECT COUNT(*) FROM online_orders WHERE personal_data_purged_at IS NOT NULL",
                ) == 1
            });
            drop(handle);
            assert_eq!(
                shop.count(
                    "SELECT COUNT(*) FROM online_orders WHERE customer_name IS NULL AND customer_phone IS NULL
                     AND address IS NULL AND customer_notes IS NULL"
                ),
                1,
                "{status:?}"
            );
        }
    }

    /// The heartbeat hears that DZBuild no longer knows this device (a 404 with an API error
    /// code): the link is revoked and its credential deleted. A bare 404 is an endpoint not
    /// shipped yet: the link stays.
    #[test]
    fn a_heartbeat_answered_404_with_a_code_revokes_the_link_a_bare_404_does_not() {
        for (answer, revoked) in [
            (
                ResponseTemplate::new(404).set_body_json(
                    json!({ "error": { "code": "device_not_found", "message": "unknown device" } }),
                ),
                true,
            ),
            (ResponseTemplate::new(404), false),
        ] {
            let dz = MockDz::start();
            dz.mount(Mock::given(path("/v1/devices/dev_1/heartbeat")).respond_with(answer));
            quiet_store(&dz);
            let (shop, _) = linked_shop();
            let (handle, secrets) = engine(&shop, &dz.uri(), Duration::from_secs(5));
            until("the heartbeat", || {
                requests_to(&dz, "/v1/devices/dev_1/heartbeat") >= 1
            });
            if revoked {
                until("the revocation", || {
                    link_status(&shop) == Some(LinkStatus::Revoked)
                });
            } else {
                std::thread::sleep(Duration::from_millis(500)); // a few heartbeats more
            }
            drop(handle);
            let expected = if revoked {
                LinkStatus::Revoked
            } else {
                LinkStatus::Linked
            };
            assert_eq!(link_status(&shop), Some(expected));
            assert_eq!(
                secrets.get(&refresh_account(LINK_ID)).unwrap().is_none(),
                revoked
            );
            let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
            assert_eq!(
                notices.iter().any(|n| n.kind == CloudNoticeKind::Revoked),
                revoked,
                "{notices:?}"
            );
        }
    }

    /// The heartbeat carries the outbox counts under its own Idempotency-Key, and the daily
    /// store profile updates the link.
    #[test]
    fn the_heartbeat_tells_the_outbox_counts_and_the_profile_updates_the_link() {
        let dz = MockDz::start();
        // mounted first: this profile wins over the one `mount_link` serves
        dz.mount(Mock::given(path("/v1/store")).respond_with(ok(json!({
            "name": "Boutique Amel Centre", "public_url": "https://centre.dzbuild.app", "plan": "business",
            "plan_limits": { "active_products": 1000 }
        }))));
        dz.mount(Mock::given(path("/v1/pos/sales")).respond_with(ResponseTemplate::new(501)));
        quiet_store(&dz);
        let (shop, _) = linked_shop();
        sell(&shop);
        // an item DZBuild refused earlier
        shop.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, status,
                         attempts, last_error, created_at)
                     VALUES (?1, 'cloud', '141', 'pos_refund', 'r1', 'refund:r1', 'failed', 1, 'cloud_request_invalid', ?2)",
                    rusqlite::params![crate::ids::new_id(), crate::testkit::NOON],
                )?;
                Ok(())
            })
            .unwrap();
        let (handle, _) = engine(&shop, &dz.uri(), Duration::from_secs(5));
        until("the heartbeat", || {
            requests_to(&dz, "/v1/devices/dev_1/heartbeat") >= 1
        });
        drop(handle);
        let beats: Vec<wiremock::Request> = dz
            .requests()
            .into_iter()
            .filter(|r| r.url.path() == "/v1/devices/dev_1/heartbeat")
            .collect();
        let body: serde_json::Value = serde_json::from_slice(&beats[beats.len() - 1].body).unwrap();
        let pending = shop.count(
            "SELECT COUNT(*) FROM sync_outbox WHERE target = 'cloud' AND status = 'pending'",
        );
        assert!(pending >= 1, "the sale waits");
        assert_eq!(
            (
                body["app_version"].as_str(),
                body["pending"].as_i64(),
                body["failed"].as_i64()
            ),
            (Some("0.7.0"), Some(pending), Some(1)),
            "{body}"
        );
        for beat in &beats {
            let key = crate::cloud::testkit::header(beat, "idempotency-key").unwrap();
            assert!(key.starts_with("beat:") && key.len() <= 64, "{key}");
        }
        let link = shop.db.read(store::active_link).unwrap().unwrap();
        assert_eq!(
            (
                link.store_name.as_str(),
                link.store_plan.as_deref(),
                link.public_url.as_deref(),
                link.active_products_cap
            ),
            (
                "Boutique Amel Centre",
                Some("business"),
                Some("https://centre.dzbuild.app"),
                Some(1000)
            )
        );
    }

    /// A page sent by a thread the engine let go, or kept from another link, is never applied:
    /// its stock move and its cursor belong elsewhere. A link change drops what is kept.
    #[test]
    fn pages_of_another_link_or_a_stale_thread_are_never_applied() {
        let dz = MockDz::start();
        quiet_store(&dz);
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let page: Page<EventItem> = serde_json::from_str(
            &json!({
                "items": [{ "id": "e1", "type": "inventory.level_changed",
                            "data": { "external_id": milk.id, "delta": 5, "source": "dashboard" } }],
                "next_cursor": "c9", "has_more": false
            })
            .to_string(),
        )
        .unwrap();
        let (deps, _) = deps(&shop, &dz.uri(), Opts::new(Duration::from_secs(5)));
        let (tx, _rx) = unbounded();
        let mut engine = Engine::new(deps, tx);
        let applied = || {
            (
                shop.count("SELECT COUNT(*) FROM stock_movements WHERE ref_type = 'dzbuild'"),
                shop.db
                    .read(|c| store::remote_cursor(c, "cloud:events"))
                    .unwrap(),
            )
        };
        // from a thread that is not (or no longer) the engine's
        let stale = Command::Events {
            generation: 7,
            link: link.id.clone(),
            page: page.clone(),
        };
        assert!(matches!(engine.handle(stale), Next::Keep));
        assert!(engine.pages.is_empty());
        // kept from another link
        engine
            .pages
            .push(("00000000-0000-7000-8000-00000000c002".into(), page.clone()));
        engine.cycle().unwrap();
        assert!(engine.pages.is_empty());
        assert_eq!(applied(), (0, None), "another link's page is dropped");
        assert_eq!(shop.count("SELECT COUNT(*) FROM cloud_inbox"), 0);
        // the same page fetched for this link applies
        engine.pages.push((link.id.clone(), page.clone()));
        engine.cycle().unwrap();
        assert_eq!(applied(), (1, Some("c9".to_owned())));
        // what is kept for a link goes with a link change
        engine.pages.push((link.id.clone(), page));
        assert!(matches!(engine.handle(Command::LinkChanged), Next::Cycle));
        assert!(engine.pages.is_empty());
    }
}
