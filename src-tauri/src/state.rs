//! Process-wide state (spec §4.2) and the start/stop hooks.

use crate::approvals::Approvals;
use crate::lan::vault::{self, TokenVault};
use crate::lan::{Lan, LanOptions};
use crate::logging::{self, LogSlot};
use crate::printing::{Notifier, PrintFailed, PrintQueue};
use crate::staging::Staging;
use pos_core::audit::{self, Actor};
use pos_core::auth::{self, Permission, Session};
use pos_core::datapack::{self, Pack};
use pos_core::db::{self, Db, DbOptions};
use pos_core::error::{AppError, ErrorPayload};
use pos_core::sync::{capture, replica, role};
use pos_core::terminal::{self, Terminal, TerminalRole};
use pos_core::till::TillContext;
use pos_core::time::{business_date, now_ms};
use serde::Serialize;
use serde_json::json;
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Manager};
use tokio::sync::watch;

/// Stops the start instead of opening the database: an interrupted restore left its WAL set
/// aside, so opening `pos.db` now would read it without its WAL.
const RESTORE_NEEDS_ATTENTION: &str = "restore_needs_attention";

/// The running app's version (journaled `app.updated` when it changes, Task 4).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub data_dir: String,
    pub db_path: String,
    pub backups_dir: String,
    pub logs_dir: String,
    /// This PC's printers, receipt options and scanner layout (`pos_hw::HardwareConfig`).
    pub hardware_config: String,
    /// Product photos (`crate::media`), the only folder the asset protocol serves.
    pub media: String,
    /// This PC's choices (`local.json`, Task 2).
    pub local_config: String,
    /// The demo shop is open: the database and its backups are under `demo\`.
    pub demo: bool,
}

impl AppPaths {
    /// `%APPDATA%\dev.dzbuild.pos\{pos.db, backups, logs, hardware.json, media, local.json}`
    /// (spec §13), the real shop.
    pub fn new(data_dir: &Path) -> AppPaths {
        AppPaths::for_profile(data_dir, false)
    }

    /// The real shop, or the demo shop (ONB-002): its `pos.db` and `backups` under
    /// `<data>\demo\`; the logs, `hardware.json`, `local.json` and the photos (the asset
    /// protocol's one folder; their names carry the product's id) stay the PC's.
    pub fn for_profile(data_dir: &Path, demo: bool) -> AppPaths {
        let text = |p: PathBuf| p.to_string_lossy().into_owned();
        let shop = if demo {
            pos_core::demo::data_dir_of(data_dir)
        } else {
            data_dir.to_path_buf()
        };
        AppPaths {
            data_dir: text(data_dir.to_path_buf()),
            db_path: text(shop.join("pos.db")),
            backups_dir: text(shop.join("backups")),
            logs_dir: text(data_dir.join("logs")),
            hardware_config: text(data_dir.join("hardware.json")),
            media: text(data_dir.join("media")),
            local_config: text(data_dir.join(pos_core::local::FILE_NAME)),
            demo,
        }
    }

    /// The main PC's name when this PC is one of its tills (`local.json`, which the
    /// pairing writes because a damaged database cannot say). Never for the demo shop: a
    /// database of its own, not the till's copy.
    pub fn till_of(&self) -> Option<String> {
        if self.demo {
            return None;
        }
        pos_core::db::recovery::till_of(Path::new(&self.local_config))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartupFailure {
    pub error: ErrorPayload,
    pub data_dir: String,
    /// The main PC's name when this PC is one of its tills: the fatal screen then
    /// offers to join it again instead of restoring a backup.
    pub till_of: Option<String>,
    /// A till's operations its damaged copy had not sent to the main PC, when it can be read:
    /// the fatal screen says they stay in the damaged folder before the owner confirms.
    pub unsent: Option<i64>,
    /// This PC is the main PC of this many tills (`local.json`): a backup would take it back
    /// behind them, so the fatal screen sends the owner to making a till the main PC first.
    pub main_pc_tills: Option<i64>,
}

/// How the start went. `setup` opens the database on a blocking thread (a scheduled restore may
/// retry for about 10 s, an unclean shutdown runs the integrity check, a migration backs up
/// first) while the window shows the splash; whoever needs the outcome waits for it here.
/// Managed from `setup` on, even when the database fails to open, so the UI can explain why.
#[derive(Debug, Clone)]
pub struct StartupStatus(watch::Receiver<Outcome>);

/// `None` while the database opens, then `Some(failure)`: `Some(None)` when it opened.
type Outcome = Option<Option<StartupFailure>>;

/// The opening thread's end of [`StartupStatus`]: sends the outcome once.
#[derive(Debug)]
pub struct StartupReporter(watch::Sender<Outcome>);

impl StartupStatus {
    pub fn channel() -> (StartupReporter, StartupStatus) {
        let (sender, receiver) = watch::channel(None);
        (StartupReporter(sender), StartupStatus(receiver))
    }

    /// Waits until the database has opened or failed to: the failure to show, or `None`.
    pub async fn outcome(&self) -> Option<StartupFailure> {
        let mut receiver = self.0.clone();
        match receiver.wait_for(Option::is_some).await {
            Ok(outcome) => outcome.clone().flatten(),
            // The reporter went away without an outcome: the opening task died.
            Err(_) => Some(StartupFailure {
                error: AppError::Internal("the start was interrupted".into()).payload(),
                data_dir: String::new(),
                till_of: None,
                unsent: None,
                main_pc_tills: None,
            }),
        }
    }
}

impl StartupReporter {
    pub fn finish(self, failure: Option<StartupFailure>) {
        self.0.send_replace(Some(failure));
    }
}

/// Something the user must know after a start that otherwise succeeded; the UI shows it once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartupNotice {
    /// A scheduled restore could not replace the database; the current data was kept.
    /// `reason` is the error code (`backup_invalid`, `io_error`, …) that the UI translates.
    RestoreFailed { reason: String },
    /// The data folder's `logs` could not be used (`reason`, an error code): the technical logs
    /// go to `fallback_dir` this run, or nowhere when it is `None`.
    #[serde(rename_all = "camelCase")]
    LoggingUnavailable {
        reason: String,
        fallback_dir: Option<String>,
    },
    /// A restore brought back an older database: these numbers, already printed, will be
    /// issued again (`backup.restored` journals them with the replaced high-water marks).
    NumbersReissued { ranges: Vec<db::ReissuedRange> },
    /// The main PC's copy could not be installed: this PC kept its own database. `reason` is the
    /// error code.
    LanFailed { reason: String },
    /// Leaving the network could not archive the till's copy (`reason`, the error code): this PC
    /// is still a till, keeps its token and syncs; the archive is tried again at the next start.
    LeaveFailed { reason: String },
    /// WebView2 is older than `webview::MIN_WEBVIEW2` (PLT-023): the notice links its download.
    WebviewOutdated { version: String, minimum: String },
    /// AUD-006: this PC's clock is behind the newest journal row (journaled `clock.moved_back`).
    #[serde(rename_all = "camelCase")]
    ClockMovedBack { last_seen_at: i64, now: i64 },
    /// The first start after an update (journaled `app.updated`); the UI offers "What's new".
    AppUpdated { from: String, to: String },
    /// The last run closed for the update to `version`, but Windows did not start its setup (the
    /// administrator prompt was refused): the app restarted on the same version, data intact.
    UpdateNotInstalled { version: String },
    /// PLT-024: a regulatory data pack in `<data>\datapacks` was not used (`code`, the error code
    /// the UI translates: an unsigned, tampered or invalid file); the pack in force stays.
    DatapackSkipped { file: String, code: String },
}

impl StartupNotice {
    /// The notice for a logging start that could not use the data folder, if any.
    pub fn for_logging(logging: &logging::Logging) -> Option<StartupNotice> {
        let failure = logging.failure.as_ref()?;
        Some(StartupNotice::LoggingUnavailable {
            reason: failure.code(),
            fallback_dir: logging
                .dir
                .as_ref()
                .map(|dir| dir.to_string_lossy().into_owned()),
        })
    }
}

/// One automatic backup attempt of this run (`run_daily_backup`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AutoBackupOutcome {
    pub at: i64,
    pub ok: bool,
    /// The error code when it failed (`io_error`, `backup_invalid`, …).
    pub error: Option<String>,
}

/// What the app knows about its backups, for the banner every signed-in user sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    /// The last automatic backup attempt of this run; `None` before it ran or when today's
    /// backup already existed.
    pub last_auto: Option<AutoBackupOutcome>,
    /// When the newest backup in the backups folder was made.
    pub newest_at: Option<i64>,
    /// The newest backup is more than [`STALE_AFTER_DAYS`] business days old.
    pub stale: bool,
    /// A backup copy failed its integrity check and so did the live database, or the integrity
    /// check of the live database failed or could not run: stop selling and restore.
    pub database_damaged: bool,
    /// The code of this run's last failed second copy (`mirror_unavailable`…); `None` after a
    /// good one.
    pub mirror_error: Option<String>,
}

/// A backup older than this many business days raises the banner.
pub const STALE_AFTER_DAYS: i64 = 2;

/// The commands' clock ([`AppState::now`]): this PC's, unless a test pins it to an instant to
/// drive a shop through a month day by day, or shifts it (a till two hours ahead of its main PC).
#[derive(Debug, Default)]
pub struct Clock {
    /// Unix milliseconds a test pinned; 0 while the clock is this PC's.
    pinned: AtomicI64,
    /// Milliseconds a test added to it; 0 otherwise.
    offset_ms: AtomicI64,
}

impl Clock {
    pub fn now(&self) -> i64 {
        let base = match self.pinned.load(Ordering::SeqCst) {
            0 => now_ms(),
            pinned => pinned,
        };
        base + self.offset_ms.load(Ordering::SeqCst)
    }

    /// Every command reads `ms` as "now" from here on.
    #[cfg(test)]
    pub fn pin(&self, ms: i64) {
        self.pinned.store(ms, Ordering::SeqCst);
    }

    /// Every command reads "now" `ms` away from this PC's clock (or the pinned instant).
    #[cfg(test)]
    pub fn shift(&self, ms: i64) {
        self.offset_ms.store(ms, Ordering::SeqCst);
    }
}

/// The automatic backup's outcome, kept for `backup_status`.
#[derive(Debug, Default)]
pub struct BackupWatch {
    last_auto: RwLock<Option<AutoBackupOutcome>>,
    damaged: AtomicBool,
    mirror_error: RwLock<Option<String>>,
}

impl BackupWatch {
    fn record(&self, outcome: AutoBackupOutcome) {
        match self.last_auto.write() {
            Ok(mut slot) => *slot = Some(outcome),
            Err(poisoned) => *poisoned.into_inner() = Some(outcome),
        }
    }

    fn last_auto(&self) -> Option<AutoBackupOutcome> {
        match self.last_auto.read() {
            Ok(slot) => slot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The outcome of the last second copy: its error code, or `None` after a good one (or
    /// once no folder is chosen any more).
    pub fn record_mirror(&self, error: Option<String>) {
        match self.mirror_error.write() {
            Ok(mut slot) => *slot = error,
            Err(poisoned) => *poisoned.into_inner() = error,
        }
    }

    fn mirror_error(&self) -> Option<String> {
        match self.mirror_error.read() {
            Ok(slot) => slot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The live database failed its integrity check (or the check could not run): the "stop
    /// selling" alert stays up for the rest of this run.
    pub fn mark_damaged(&self) {
        self.damaged.store(true, Ordering::SeqCst);
    }
}

pub struct AppState {
    pub db: Db,
    pub session: RwLock<Option<Session>>,
    pub terminal: Terminal,
    pub hw: Arc<pos_hw::Registry>,
    pub sync: pos_core::sync::Handle,
    pub datapack: Arc<Pack>,
    pub paths: AppPaths,
    pub notices: Vec<StartupNotice>,
    /// Manager approvals waiting for the next till write of the sign-in they were given to.
    pub approvals: Approvals,
    pub backups: BackupWatch,
    /// Prints receipts after the commit; failures reach the window as `print-failed`.
    pub printer: PrintQueue,
    /// The optional DZBuild link (M7); `None` when the bundled configuration is invalid.
    pub cloud: Option<crate::cloud::CloudRuntime>,
    /// Files opened for an import, in memory for the user who opened them.
    pub staging: Staging,
    /// Where every command takes "now" from ([`AppState::now`]).
    pub clock: Clock,
    /// The scale, pole display and serial scanner threads of this PC (`crate::devices::start`).
    pub devices: crate::devices::Devices,
    /// The shop's network (M5): the main PC's server or the till's engine; nothing on a single
    /// PC. Started by `on_ready`, stopped by `on_exit`.
    pub lan: Lan,
    /// This PC's `local.json` (mirror folder, log level, light mode, text size, keyboard).
    pub local: crate::local::LocalStore,
    /// This PC's licence (M9): checked at the start, every 10 minutes and by the gate of every
    /// command (`license::handle`).
    pub license: crate::license::LicenseRuntime,
}

pub type SharedState = Arc<AppState>;

fn poisoned() -> AppError {
    AppError::Internal("session lock poisoned".into())
}

/// Who a host service runs for (M5 RPC): the till's user, the till whose drawer holds the
/// money of the call, and the manager approvals this PC confirmed for it (Task 20 review).
#[derive(Debug, Clone)]
pub struct Caller {
    pub session: Session,
    pub drawer_till: Option<String>,
    pub approvals: Vec<auth::OverrideGrant>,
}

thread_local! {
    /// The till user a host service runs as, on the thread running it (M5 RPC).
    static CALLER: std::cell::RefCell<Option<Caller>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with `session` as the current session of this thread only
/// ([`AppState::current_session`]); reset afterwards, even when `f` fails or panics. The main
/// PC's own sign-in is untouched.
pub fn with_caller<T>(session: Session, f: impl FnOnce() -> T) -> T {
    with_till_caller(
        Caller {
            session,
            drawer_till: None,
            approvals: Vec::new(),
        },
        f,
    )
}

/// [`with_caller`] for a till's call: [`AppState::till`] gives its operations the till's drawer
/// and the approvals confirmed for the call instead of this PC's own.
pub fn with_till_caller<T>(caller: Caller, f: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CALLER.with(|c| *c.borrow_mut() = None);
        }
    }
    CALLER.with(|c| *c.borrow_mut() = Some(caller));
    let _reset = Reset;
    f()
}

impl AppState {
    /// [`AppState::open_with`] this PC's Credential Manager and the LAN's port 7345.
    pub fn open(paths: &AppPaths, notify: Notifier) -> Result<AppState, AppError> {
        AppState::open_with(paths, notify, vault::default_vault(), LanOptions::default())
    }

    /// Archives a till's copy when it left the network and installs a main PC's staged copy
    /// (either failure keeps this PC's database and becomes a startup notice; neither, nor the
    /// adoption and `local.json`'s place, for the demo shop's database), applies a
    /// scheduled restore, opens the database, adopts an installed copy (this PC becomes that
    /// main PC's till), re-generates the LAN triggers of its role when the schema changed and
    /// sets its write hook (both before any business write), seeds roles and the terminal (not
    /// on a till: they come from the main PC), journals a restore or an unclean shutdown, then
    /// seeds the catalogue defaults off a till (the data pack's VAT rates,
    /// `catalog::ensure_defaults`, idempotent). The restore's `backup.restored` row comes
    /// from the restored database itself (`db::journal_pending_restore`), so it is written
    /// exactly once even when a previous start crashed after the swap. A failed restore (after
    /// the retries in `apply_pending_restore`) keeps the current database and becomes a
    /// startup notice; a restore that needs attention stops the start without opening the
    /// database. Nothing on the network starts here (`on_ready` does). Blocking (restore
    /// retries, integrity check, migrations): never call it on the main thread. `notify`
    /// receives the receipts the print queue could not print; `vault` holds a till's token.
    pub fn open_with(
        paths: &AppPaths,
        notify: Notifier,
        vault: Arc<dyn TokenVault>,
        options: LanOptions,
    ) -> Result<AppState, AppError> {
        let db_path = PathBuf::from(&paths.db_path);
        let data_dir = PathBuf::from(&paths.data_dir);
        let backups_dir = PathBuf::from(&paths.backups_dir);
        let mut notices = Vec::new();
        let started = now_ms();
        // The demo shop (M6) is a database of its own, never the shop network's copy: the LAN
        // files staged in the data folder and `local.json`'s place are the real shop's, applied
        // and kept at its next start.
        let real_shop = !paths.demo;
        let left = if real_shop {
            replica::apply_pending_reset(&data_dir, &db_path, &backups_dir, started)
        } else {
            Ok(false)
        };
        let left_network = matches!(left, Ok(true));
        match left {
            Ok(true) => {
                tracing::info!("this PC left the network; the till's copy is archived");
                // Only now: until the copy is archived this PC is still a till and needs it.
                if let Err(error) = vault.delete() {
                    tracing::warn!(code = %error.code(), "the till's token could not be removed");
                }
            }
            Ok(false) => {}
            Err(error) => {
                tracing::error!(code = %error.code(), "leaving the network could not archive the till's copy");
                notices.push(StartupNotice::LeaveFailed {
                    reason: error.code(),
                });
            }
        }
        let installed = if real_shop {
            replica::install_pending(&data_dir, &db_path, &backups_dir, started)
        } else {
            Ok(None)
        };
        match installed {
            Ok(Some(_)) => tracing::info!("the main PC's copy is installed"),
            Ok(None) => {}
            Err(error) => {
                tracing::error!(code = %error.code(), "the main PC's copy could not be installed; keeping this PC's database");
                notices.push(StartupNotice::LanFailed {
                    reason: error.code(),
                });
            }
        }
        let restored = match db::apply_pending_restore(&db_path) {
            Ok(source) => source,
            Err(error) if error.code() == RESTORE_NEEDS_ATTENTION => {
                tracing::error!(%error, "an interrupted restore needs attention; the database stays closed");
                return Err(error);
            }
            Err(error) => {
                tracing::error!(%error, "scheduled restore failed; keeping the current database");
                notices.push(StartupNotice::RestoreFailed {
                    reason: error.code(),
                });
                None
            }
        };
        // A guided restore that failed after moving the damaged files aside: never create an
        // empty shop in their place (the fatal screen offers the restore again).
        db::recovery::refuse_empty_start(&db_path)?;
        // Finding 10: a `pos.db` gone while the shop's backups are there (an antivirus, a
        // deletion) sends the start to the guided restore instead of making an empty shop. A
        // till that just left the network makes its new database on purpose.
        if real_shop && !left_network {
            let mirror = pos_core::local::LocalConfig::load(Path::new(&paths.local_config))
                .backup_mirror
                .map(|m| PathBuf::from(m.path));
            db::recovery::refuse_lost_database(&db_path, &backups_dir, mirror.as_deref())?;
        }
        let db = Db::open(
            &db_path,
            DbOptions {
                readers: 4,
                backup_dir: Some(PathBuf::from(&paths.backups_dir)),
            },
        )?;
        if let Some(source) = &restored {
            tracing::info!(source = %source.display(), "scheduled restore applied");
        }
        // Finding 39: the demo profile's database is the demo shop's, whatever built it.
        if paths.demo {
            db.write(|tx| pos_core::demo::mark(tx))?;
        }
        let lan = Lan::new(vault, options);
        if real_shop && let Some(pending) = replica::pending_adoption(&data_dir)? {
            replica::adopt(&db, &data_dir, &pending, now_ms())?;
        }
        let place = db.read(role::current)?;
        lan.set_role(place.role);
        if real_shop {
            let host = place.role == TerminalRole::Host;
            let (terminal, tills, set_up) = db.read(move |c| {
                let tills = if host { role::active_tills(c)? } else { 0 };
                Ok((
                    terminal::local(c)?,
                    tills,
                    pos_core::setup::status(c)?.completed,
                ))
            })?;
            crate::lan::record_place(
                &data_dir,
                &crate::lan::Place {
                    role: place.clone(),
                    terminal,
                    tills,
                    // A till that left the network is a single PC from here on.
                    set_up: set_up || left_network,
                },
            );
        }
        if let Some(set) = db.read(role::capture_set)? {
            db.write(move |tx| capture::ensure(tx, &set).map(drop))?;
        }
        let local = db.read(terminal::local)?.map(|t| t.id).unwrap_or_default();
        if let Some(hook) = lan.hook(&place, &local) {
            db.set_write_hook(Some(hook));
            // Feed rows an interrupted start left behind go into an outbox item (a till) or get
            // their commit mark (a main PC) now.
            db.write(|_| Ok(()))?;
        }
        let terminal = pos_core::setup::bootstrap(&db)?;
        let report = db.report().clone();
        let actor = Actor::system(Some(terminal.id.clone()));
        let notes = if real_shop {
            StartNotes::read(&data_dir, db.read(pos_core::setup::status)?.completed)
        } else {
            StartNotes::default()
        };
        let journaled = notes.events(&actor, place.role == TerminalRole::Client);
        let (reissued, clock, version) = db.write(move |tx| {
            for event in &journaled {
                audit::append(tx, event)?;
            }
            let reissued = db::journal_pending_restore(tx, actor.terminal_id.as_deref())?;
            if report.unclean_shutdown {
                let event = actor.event(
                    "db.unclean_shutdown",
                    "database",
                    None,
                    json!({ "integrityOk": report.integrity_ok }),
                );
                audit::append(tx, &event)?;
            }
            let clock = pos_core::health::check_clock(tx, &actor, now_ms())?;
            let version = pos_core::updates::record_app_version(tx, &actor, APP_VERSION)?;
            Ok((reissued, clock, version))
        })?;
        notes.forget(&data_dir);
        // M9: this PC's licence, right after the database, before the rest of the start.
        // Never fails the start: without a licence the shop is read-only, its books open.
        #[cfg(not(test))]
        let license_env = crate::license::LicenseEnv::system();
        #[cfg(test)]
        let license_env = crate::license::LicenseEnv::for_tests_in(&data_dir);
        let license = crate::license::LicenseRuntime::open(
            &db,
            license_env,
            paths.demo,
            &Actor::system(Some(terminal.id.clone())),
            now_ms(),
        );
        if let Some(ranges) = reissued.filter(|ranges| !ranges.is_empty()) {
            tracing::warn!(
                ?ranges,
                "the restored database will issue these numbers again"
            );
            notices.push(StartupNotice::NumbersReissued { ranges });
        }
        if let Some(jump) = clock {
            tracing::warn!(?jump, "the PC clock is behind the journal");
            notices.push(StartupNotice::ClockMovedBack {
                last_seen_at: jump.last_seen_at,
                now: jump.now,
            });
        }
        if let Some(pos_core::updates::VersionChange {
            from: Some(from),
            to,
        }) = version
        {
            tracing::info!(%from, %to, "first start of this version");
            notices.push(StartupNotice::AppUpdated { from, to });
        }
        if let Some(version) = crate::updates::take_not_installed(paths) {
            tracing::warn!(%version, "the last run could not start the update setup");
            notices.push(StartupNotice::UpdateNotInstalled { version });
        }
        // PLT-024: the newest signed pack in force from this PC's `datapacks` folder, else the
        // bundled one; a file that fails its signature or its checks is named, never used.
        let (best, skipped) = pos_core::datapack_update::load_best(
            &data_dir.join("datapacks"),
            datapack::bundled()?,
            &business_date(now_ms()).format("%Y-%m-%d").to_string(),
            pos_core::datapack_update::DATAPACK_PUBLIC_KEY,
        );
        for skip in skipped {
            tracing::warn!(file = %skip.file_name, code = %skip.code, "a data pack was not used");
            notices.push(StartupNotice::DatapackSkipped {
                file: skip.file_name,
                code: skip.code,
            });
        }
        let datapack = Arc::new(best);
        // final review: the version this PC runs with, which a till compares with its main PC's
        let active = datapack.version;
        if let Err(error) = db.write(move |tx| pos_core::datapack_update::note_active(tx, active)) {
            tracing::warn!(%error, "the data pack in force is not noted");
        }
        let pack = datapack.clone();
        // M1a Task 5: VAT rates come from the data pack, never from SQL; idempotent. A till's
        // rates are the main PC's.
        if place.role != TerminalRole::Client {
            db.write(move |tx| pos_core::catalog::ensure_defaults(tx, &pack))?;
        }
        // An import a crash or a power cut stopped: its applied batches stay (undoable), the job
        // is failed.
        db.write(|tx| pos_core::imports::engine::abandon_running(tx, now_ms()))?;
        // A photo saved later creates it too; a failure here only delays that.
        if let Err(error) = std::fs::create_dir_all(&paths.media) {
            tracing::warn!(%error, "could not create the media folder");
        }
        let hw = Arc::new(pos_hw::Registry::open(&paths.hardware_config));
        let printer = PrintQueue::start(db.clone(), hw.clone(), notify);
        let cloud = crate::cloud::CloudRuntime::open(&db, &terminal, paths);
        Ok(AppState {
            db,
            session: RwLock::new(None),
            terminal,
            hw,
            sync: pos_core::sync::Handle::default(),
            datapack,
            paths: paths.clone(),
            notices,
            approvals: Approvals::default(),
            backups: BackupWatch::default(),
            printer,
            cloud,
            staging: Staging::default(),
            clock: Clock::default(),
            devices: crate::devices::Devices::idle(),
            lan,
            local: crate::local::LocalStore::open(Path::new(&paths.local_config)),
            license,
        })
    }

    /// The backups banner's data (`backup_status`) at `now`. Reads the backups folder.
    pub fn backup_status(&self, now: i64) -> BackupStatus {
        let newest_at = match self.db.list_backups() {
            Ok(backups) => backups.iter().map(|b| b.at).max(),
            Err(error) => {
                tracing::warn!(%error, "could not list the backups for the status");
                None
            }
        };
        let stale = newest_at.is_some_and(|at| {
            (business_date(now) - business_date(at)).num_days() > STALE_AFTER_DAYS
        });
        BackupStatus {
            last_auto: self.backups.last_auto(),
            newest_at,
            stale,
            database_damaged: self.backups.damaged.load(Ordering::SeqCst),
            mirror_error: self.backups.mirror_error(),
        }
    }

    /// The signed-in session; on a thread running a host service for a till, that till's user
    /// ([`with_caller`]).
    pub fn current_session(&self) -> Result<Option<Session>, AppError> {
        if let Some(caller) = CALLER.with(|c| c.borrow().as_ref().map(|c| c.session.clone())) {
            return Ok(Some(caller));
        }
        self.session
            .read()
            .map(|s| s.clone())
            .map_err(|_| poisoned())
    }

    pub fn session(&self) -> Result<Session, AppError> {
        self.current_session()?
            .ok_or_else(|| AppError::auth("session_required"))
    }

    pub fn set_session(&self, session: Option<Session>) -> Result<(), AppError> {
        self.replace_session(session).map(drop)
    }

    /// Puts `session` in place and hands back the one it replaced, in one step.
    pub fn replace_session(&self, session: Option<Session>) -> Result<Option<Session>, AppError> {
        let mut guard = self.session.write().map_err(|_| poisoned())?;
        Ok(std::mem::replace(&mut *guard, session))
    }

    /// Rebuilds the logged-in session from the database (`pos_core::auth::refresh`); commands
    /// call it after any users or roles edit, so a changed role, permission, name or language
    /// applies at once. A user deactivated or removed meanwhile is signed out and the error is
    /// `session_invalid`; nobody signed in gives `None`. The lock is not held across the read
    /// (`auth_session` reads it on the main thread); a sign-in or sign-out that lands meanwhile
    /// wins and is returned as is. Reads SQLite: call it on a blocking thread.
    pub fn refresh_session(&self) -> Result<Option<Session>, AppError> {
        let Some(current) = self.current_session()? else {
            return Ok(None);
        };
        let (user_id, started_at) = (current.user_id.clone(), current.started_at);
        let refreshed = self.db.read(move |conn| auth::refresh(conn, &current));
        let mut guard = self.session.write().map_err(|_| poisoned())?;
        let same_sign_in = guard
            .as_ref()
            .is_some_and(|s| s.user_id == user_id && s.started_at == started_at);
        if !same_sign_in {
            return Ok(guard.clone());
        }
        match refreshed {
            Ok(fresh) => {
                *guard = Some(fresh.clone());
                Ok(Some(fresh))
            }
            Err(error) => {
                if error.code() == "session_invalid" {
                    *guard = None;
                }
                Err(error)
            }
        }
    }

    /// `demo_not_allowed` while the demo shop is open. Joining a network, becoming a
    /// main PC, linking DZBuild and the second copy would reach the real shop's files, vault or
    /// accounts: the guard runs first, before any of them is touched.
    pub fn demo_guard(&self) -> Result<(), AppError> {
        if self.paths.demo {
            Err(pos_core::demo::not_allowed())
        } else {
            Ok(())
        }
    }

    /// The logged-in session if it holds `permission` (spec §4.2 `state.require`).
    pub fn require(&self, permission: Permission) -> Result<Session, AppError> {
        let session = self.session()?;
        auth::require(&session, permission)?;
        Ok(session)
    }

    /// [`require`](Self::require), and the user must be an owner (`owner_only`): custom roles
    /// and restores belong to the owner (spec §4.5, §13).
    pub fn require_owner(&self, permission: Permission) -> Result<Session, AppError> {
        let session = self.require(permission)?;
        if session.is_owner {
            Ok(session)
        } else {
            Err(AppError::auth("owner_only"))
        }
    }

    /// The logged-in session if it holds any of `permissions` (reads shared by the back office
    /// and the till: `products.view` or `sales.create`, …).
    pub fn require_any(&self, permissions: &[Permission]) -> Result<Session, AppError> {
        let session = self.session()?;
        if permissions.iter().any(|p| session.has(*p)) {
            return Ok(session);
        }
        let first = permissions
            .first()
            .copied()
            .ok_or_else(|| AppError::Internal("require_any without a permission".into()))?;
        auth::require(&session, first)?;
        Ok(session)
    }

    /// The operator of a till command (M1a `TillContext`): the session must hold `permission`
    /// or a pending manager approval of it; every pending approval of this sign-in rides along
    /// so the service can accept an override, a discount or a void it would refuse otherwise.
    /// For a host service run here for a till ([`with_till_caller`]), the approvals are those
    /// this PC confirmed for the call and the money goes to the till's drawer
    /// ([`TillContext::drawer_till`]).
    pub fn till(&self, permission: Permission) -> Result<TillContext, AppError> {
        let session = self.session()?;
        let now = self.now();
        let for_till = CALLER.with(|c| {
            c.borrow()
                .as_ref()
                .map(|c| (c.approvals.clone(), c.drawer_till.clone()))
        });
        let (grants, drawer_till) = match for_till {
            Some(found) => found,
            None => (self.approvals.pending(&session, now)?, None),
        };
        if !session.has(permission) && !grants.iter().any(|g| g.permission == permission) {
            auth::require(&session, permission)?;
        }
        // CFG-008: the shop's business day may end after midnight (a restaurant, a night shop).
        let cutoff = self.db.read(pos_core::till::day_cutoff)?;
        let ctx = grants
            .into_iter()
            .fold(
                TillContext::new(session, self.datapack.clone(), now),
                TillContext::with_approval,
            )
            .with_day_cutoff(cutoff);
        Ok(match drawer_till {
            Some(terminal_id) => ctx.for_till(terminal_id),
            None => ctx,
        })
    }

    /// A till write succeeded: the approvals it carried are spent.
    pub fn spend_approvals(&self, ctx: &TillContext) -> Result<(), AppError> {
        if ctx.approvals.is_empty() {
            return Ok(());
        }
        self.approvals.spend(&ctx.session)
    }

    /// "Now" for every command and for the LAN server: this PC's clock, or the instant a test
    /// pinned, moved by the shift a test set ([`Clock`]).
    pub fn now(&self) -> i64 {
        self.clock.now()
    }

    /// This PC's clock reads `ms` away from here on (a till ahead of its main PC).
    #[cfg(test)]
    pub fn shift_clock(&self, ms: i64) {
        self.clock.shift(ms);
    }

    pub fn actor(&self, session: Option<&Session>) -> Actor {
        Actor {
            user_id: session.map(|s| s.user_id.clone()),
            terminal_id: Some(self.terminal.id.clone()),
        }
    }

    #[cfg(test)]
    pub fn for_tests() -> SharedState {
        AppState::for_tests_with(pos_hw::Registry::default(), crate::printing::silent())
    }

    /// An in-memory state with `hw` as this PC's printers and `notify` for print failures.
    #[cfg(test)]
    pub fn for_tests_with(hw: pos_hw::Registry, notify: Notifier) -> SharedState {
        let db = Db::open_memory().unwrap();
        let terminal = pos_core::setup::bootstrap(&db).unwrap();
        let datapack = Arc::new(datapack::bundled().unwrap());
        let pack = datapack.clone();
        db.write(move |tx| pos_core::catalog::ensure_defaults(tx, &pack))
            .unwrap();
        let hw = Arc::new(hw);
        let printer = PrintQueue::start(db.clone(), hw.clone(), notify);
        let license = crate::license::LicenseRuntime::open(
            &db,
            crate::license::LicenseEnv::for_tests(),
            false,
            &Actor::system(Some(terminal.id.clone())),
            now_ms(),
        );
        Arc::new(AppState {
            db,
            session: RwLock::new(None),
            terminal,
            hw,
            sync: pos_core::sync::Handle::default(),
            datapack,
            paths: AppPaths::new(Path::new("test-data")),
            notices: Vec::new(),
            approvals: Approvals::default(),
            backups: BackupWatch::default(),
            printer,
            cloud: None,
            staging: Staging::default(),
            clock: Clock::default(),
            devices: crate::devices::Devices::idle(),
            lan: Lan::new(
                Arc::new(vault::MemoryVault::default()),
                LanOptions {
                    bind: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                    advertise: false,
                },
            ),
            local: crate::local::LocalStore::in_memory(),
            license,
        })
    }

    /// The DZBuild link, or `cloud_config_invalid` when the bundled configuration is invalid.
    pub fn cloud(&self) -> Result<&crate::cloud::CloudRuntime, AppError> {
        self.cloud
            .as_ref()
            .ok_or_else(|| AppError::sync("cloud_config_invalid"))
    }

    /// A state on a file database in a temporary folder (backups work), with a DZBuild runtime
    /// on `config` (a local stand-in) and `secrets`.
    #[cfg(test)]
    pub fn for_tests_cloud(
        config: pos_core::cloud::config::CloudConfig,
        secrets: Arc<dyn pos_core::cloud::secrets::SecretStore>,
    ) -> SharedState {
        // lives as long as the test process
        let root = tempfile::tempdir().unwrap().keep();
        let paths = AppPaths::new(&root);
        let db = Db::open(
            Path::new(&paths.db_path),
            DbOptions {
                readers: 2,
                backup_dir: Some(PathBuf::from(&paths.backups_dir)),
            },
        )
        .unwrap();
        let terminal = pos_core::setup::bootstrap(&db).unwrap();
        let datapack = Arc::new(datapack::bundled().unwrap());
        let pack = datapack.clone();
        db.write(move |tx| pos_core::catalog::ensure_defaults(tx, &pack))
            .unwrap();
        let hw = Arc::new(pos_hw::Registry::default());
        let printer = PrintQueue::start(db.clone(), hw.clone(), crate::printing::silent());
        let media = PathBuf::from(&paths.media);
        let cloud =
            crate::cloud::CloudRuntime::new(db.clone(), &terminal, config, secrets, media).unwrap();
        let license = crate::license::LicenseRuntime::open(
            &db,
            crate::license::LicenseEnv::for_tests(),
            false,
            &Actor::system(Some(terminal.id.clone())),
            now_ms(),
        );
        Arc::new(AppState {
            db,
            session: RwLock::new(None),
            terminal,
            hw,
            sync: pos_core::sync::Handle::default(),
            datapack,
            paths,
            notices: Vec::new(),
            approvals: Approvals::default(),
            backups: BackupWatch::default(),
            printer,
            cloud: Some(cloud),
            staging: Staging::default(),
            clock: Clock::default(),
            devices: crate::devices::Devices::idle(),
            lan: Lan::new(
                Arc::new(vault::MemoryVault::default()),
                LanOptions {
                    bind: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                    advertise: false,
                },
            ),
            local: crate::local::LocalStore::in_memory(),
            license,
        })
    }
}

/// What earlier runs left in the data folder for the real shop's journal: the demo left
/// (`demo.left`), a till's damaged copy set aside to pair again (journaled once this PC is set up
/// again), the owner's "new shop anyway" (journaled in the new shop). Each note is removed only
/// once its row is committed.
#[derive(Debug, Clone, Default)]
struct StartNotes {
    demo_left: Option<serde_json::Value>,
    rejoin: Option<db::recovery::RejoinNote>,
    fresh: bool,
}

impl StartNotes {
    fn read(data_dir: &Path, set_up: bool) -> StartNotes {
        StartNotes {
            demo_left: pos_core::demo::left_note(data_dir),
            // Kept until this PC is set up again: a till pairs again from the setup.
            rejoin: set_up
                .then(|| db::recovery::rejoin_note(data_dir))
                .flatten(),
            fresh: data_dir.join(db::recovery::FRESH_MARKER).is_file(),
        }
    }

    /// The rows to journal, in order; `till`: this PC is a till (again).
    fn events(&self, actor: &Actor, till: bool) -> Vec<audit::AuditEvent> {
        let mut events = Vec::new();
        if let Some(note) = &self.demo_left {
            events.push(actor.event("demo.left", "demo", None, note.clone()));
        }
        if let Some(note) = &self.rejoin {
            let action = if till {
                "lan.till_rejoined"
            } else {
                "recovery.set_aside"
            };
            let payload = json!({
                "damagedFolder": note.damaged_folder,
                "moved": note.moved,
                "unsent": note.unsent,
                "at": note.at,
            });
            events.push(actor.event(action, "database", None, payload));
        }
        if self.fresh {
            events.push(actor.event("recovery.fresh_start", "database", None, json!({})));
        }
        events
    }

    fn forget(&self, data_dir: &Path) {
        if self.demo_left.is_some() {
            pos_core::demo::forget_left_note(data_dir);
        }
        if self.rejoin.is_some() {
            db::recovery::forget_marker(data_dir, db::recovery::REJOIN_MARKER);
        }
        if self.fresh {
            db::recovery::forget_marker(data_dir, db::recovery::FRESH_MARKER);
        }
    }
}

/// The app data folder as `AppPaths` (the demo shop's while `demo.active` exists), or the
/// `io_error` the fatal screen shows when Windows cannot tell where it is.
pub(crate) fn data_paths(resolved: Result<PathBuf, tauri::Error>) -> Result<AppPaths, AppError> {
    resolved
        .map(|dir| AppPaths::for_profile(&dir, pos_core::demo::is_active(&dir)))
        .map_err(|error| AppError::Io(format!("app data folder: {error}")))
}

/// Opens the state for `setup`'s background task, with the notices of the start so far
/// (logging) before those of the opening.
pub(crate) fn open_state(
    paths: &AppPaths,
    early: Vec<StartupNotice>,
    notify: Notifier,
) -> Result<AppState, AppError> {
    let mut state = AppState::open(paths, notify).inspect_err(|error| {
        tracing::error!(%error, "could not open the database");
    })?;
    state.notices.splice(0..0, early);
    Ok(state)
}

/// What the fatal screen shows for `error`; `data_dir` is empty when there is no data folder.
pub(crate) fn startup_failure(error: &AppError, data_dir: &str) -> StartupFailure {
    StartupFailure {
        error: error.payload(),
        data_dir: data_dir.to_owned(),
        till_of: None,
        unsent: None,
        main_pc_tills: None,
    }
}

/// The fatal screen's failure when the data folder is known: with this PC's place on the
/// shop's network (`local.json`), read here because the database did not open, and a till's
/// unsent operations read from its damaged copy when it still reads.
pub(crate) fn failure_at(error: &AppError, paths: &AppPaths) -> StartupFailure {
    let till_of = paths.till_of();
    let unsent = till_of
        .as_ref()
        .and_then(|_| db::recovery::unsent_in(Path::new(&paths.db_path)));
    let main_pc_tills = if paths.demo {
        None
    } else {
        pos_core::local::LocalConfig::load(Path::new(&paths.local_config))
            .lan
            .filter(|lan| lan.role == pos_core::local::LocalLanRole::Main && lan.tills > 0)
            .map(|lan| lan.tills)
    };
    StartupFailure {
        till_of,
        unsent,
        main_pc_tills,
        ..startup_failure(error, &paths.data_dir)
    }
}

/// Tauri `setup` hook. Never fails: the startup status is managed first, and every failure
/// (no data folder, the database) is reported to the fatal screen instead of aborting the app
/// without a word. Logs go to the data folder, else to `%TEMP%`, else nowhere, with a startup
/// notice. The database opens on a blocking thread, so the window paints the splash instead
/// of freezing. No backup starts here (see `on_ready`).
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let (reporter, status) = StartupStatus::channel();
    app.manage(status);
    let paths = match data_paths(app.path().app_data_dir()) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("DZBuild POS: {error}");
            reporter.finish(Some(startup_failure(&error, "")));
            return Ok(());
        }
    };
    let logging =
        logging::init_best_effort(Path::new(&paths.logs_dir), &logging::fallback_logs_dir());
    let mut early: Vec<StartupNotice> = StartupNotice::for_logging(&logging).into_iter().collect();
    app.manage(LogSlot::new(logging.guard));
    // This PC's log level (`local.json`); the Diagnostic screen changes it live (Task 14).
    let level = pos_core::local::LocalConfig::load(Path::new(&paths.local_config)).log_level;
    if logging.dir.is_some()
        && let Err(error) = logging::set_level(level)
    {
        tracing::warn!(%error, "log level not applied");
    }
    app.manage(paths.clone());
    // Finding 39: every A4 page printed from the demo shop says so.
    if let Some(host) = app.try_state::<Arc<crate::print_host::PrintHost>>() {
        host.set_demo(paths.demo);
    }
    tracing::info!(version = %app.package_info().version, data_dir = %paths.data_dir, demo = paths.demo, "DZBuild POS starting");
    let webview_version = tauri::webview_version().ok();
    if let Some(version) = &webview_version
        && crate::webview::is_outdated(version, crate::webview::MIN_WEBVIEW2)
    {
        tracing::warn!(version, "WebView2 is older than the supported minimum");
        early.push(StartupNotice::WebviewOutdated {
            version: version.clone(),
            minimum: crate::webview::MIN_WEBVIEW2.into(),
        });
    }
    app.manage(crate::webview::WebviewInfo(webview_version));
    if let Err(error) = crate::tray::install(app.handle()) {
        tracing::warn!(%error, "no tray icon this run");
    }
    let handle = app.handle().clone();
    let emitter = app.handle().clone();
    let notify: Notifier = Arc::new(move |event: PrintFailed| {
        use tauri_specta::Event;
        if let Err(error) = event.emit(&emitter) {
            tracing::warn!(%error, "could not tell the window a receipt failed");
        }
    });
    tauri::async_runtime::spawn(async move {
        let to_open = paths.clone();
        let opened =
            tauri::async_runtime::spawn_blocking(move || open_state(&to_open, early, notify))
                .await
                .unwrap_or_else(|e| Err(AppError::Internal(format!("database start failed: {e}"))));
        let failure = match opened {
            Ok(state) => {
                handle.manage::<SharedState>(Arc::new(state));
                // The serial ports and the customer screen's window: off the async threads, and
                // the start never waits for them.
                let devices = handle.clone();
                drop(tauri::async_runtime::spawn_blocking(move || {
                    crate::devices::start(&devices);
                }));
                None
            }
            Err(error) => Some(failure_at(&error, &paths)),
        };
        reporter.finish(failure);
    });
    Ok(())
}

/// `RunEvent::Ready`: the event loop runs and the main window (created from the config before
/// `setup`) is shown. Only now, and once the database has opened, start the daily auto-backup
/// and the shop's network on blocking threads, so the first paint and the login screen never
/// wait for the disk or the network. The window hears the network as `lan-status-changed`. Once
/// the backup is done, its outcome (and the second copy's) shows in the tray, the DZBuild link's
/// engine starts on the main PC only (never on a till), then the update checker.
pub fn on_ready(handle: &AppHandle) {
    let handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let Some(status) = handle
            .try_state::<StartupStatus>()
            .map(|s| s.inner().clone())
        else {
            return;
        };
        if status.outcome().await.is_some() {
            return;
        }
        let Some(state) = handle.try_state::<SharedState>().map(|s| s.inner().clone()) else {
            return;
        };
        // M9: the licence is checked again every 10 minutes (a trial ends while the app runs).
        crate::license::spawn_refresher(state.clone());
        let backup_state = state.clone();
        let backup = tauri::async_runtime::spawn_blocking(move || run_daily_backup(&backup_state));
        let sink = crate::lan::window_sink(&handle);
        let lan_state = state.clone();
        // Never awaited here: a till's engine does not wait for the backup, nor the backup for
        // a main PC's server.
        drop(tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = lan_state.lan.start(&lan_state, sink) {
                tracing::error!(code = %error.code(), "the LAN did not start");
            }
        }));
        if let Err(error) = backup.await {
            tracing::warn!(%error, "the daily backup task failed");
        }
        let expiry_state = state.clone();
        if let Err(error) =
            tauri::async_runtime::spawn_blocking(move || run_expiries(&expiry_state)).await
        {
            tracing::warn!(%error, "the voucher and points expiry task failed");
        }
        // DAT-007 / DAT-011: the hourly copies while a drawer is open and the background check.
        drop(tauri::async_runtime::spawn(maintenance(state.clone())));
        let status = state.backup_status(now_ms());
        let problem = status.database_damaged
            || status.stale
            || status.mirror_error.is_some()
            || status.last_auto.as_ref().is_some_and(|a| !a.ok);
        crate::tray::set_backup_problem(&handle, problem);
        // The DZBuild link after the backup, off the async threads too (it reads the LAN role).
        let app = handle.clone();
        if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
            crate::cloud::start_engine(&app, &state);
        })
        .await
        {
            tracing::warn!(%error, "the DZBuild link task failed");
        }
        crate::updates::spawn_checker(handle.clone());
    });
}

/// The daily automatic backup with the shop's retention, its outcome kept for `backup_status`,
/// its second copy, then the weekly integrity check of the live database. The copy and its
/// integrity check run on a dedicated read connection (`Db::backup`), so sales keep committing
/// on the writer meanwhile. A copy that fails its integrity check most likely means the live
/// database is damaged: it is checked too, and a failure raises the "stop selling" alert, as
/// does a weekly check that fails or cannot run. Blocking: call it on a blocking thread.
pub fn run_daily_backup(st: &AppState) {
    let actor = Actor::system(Some(st.terminal.id.clone()));
    let retention = st
        .db
        .read(pos_core::db::Retention::from_settings)
        .unwrap_or_default();
    let error = match st.db.daily_backup_if_due_with(retention) {
        Ok(Some(info)) => {
            tracing::info!(file = %info.file_name, "daily backup done");
            st.backups.record(AutoBackupOutcome {
                at: info.at,
                ok: true,
                error: None,
            });
            crate::backups::after_backup(st, &info, actor.clone());
            None
        }
        Ok(None) => None,
        Err(error) => Some(error),
    };
    if let Some(error) = error {
        tracing::error!(%error, "the daily backup failed");
        if copy_failed_its_check(&error) && !live_database_sound(&st.db) {
            tracing::error!(
                "the live database fails its integrity check: stop selling and restore"
            );
            st.backups.mark_damaged();
        }
        st.backups.record(AutoBackupOutcome {
            at: now_ms(),
            ok: false,
            error: Some(error.code()),
        });
    }
    let checked = st.db.integrity_check_if_due(actor.clone(), now_ms());
    crate::backups::watch_integrity(st, actor, checked);
}

/// How long after the start the background quick check runs (the start's own work first).
const QUICK_CHECK_AFTER: std::time::Duration = std::time::Duration::from_secs(30);
/// How often the hourly backup looks whether a copy is due.
const HOURLY_EVERY: std::time::Duration = std::time::Duration::from_secs(600);

/// The maintenance of a running PC (M6 rows, Task 21b): 30 seconds after the start the live
/// database gets a `PRAGMA quick_check` (DAT-011), then every ten minutes an hourly copy is made
/// when one is due while this PC's drawer is open (DAT-007), and a till sends the register entries
/// the main PC has not kept yet. Each runs on a blocking thread.
async fn maintenance(state: SharedState) {
    tokio::time::sleep(QUICK_CHECK_AFTER).await;
    let check = state.clone();
    if let Err(error) = tauri::async_runtime::spawn_blocking(move || run_quick_check(&check)).await
    {
        tracing::warn!(%error, "the quick check task failed");
    }
    // final review: a till compares its data pack with its main PC's soon after the start
    let follow = state.clone();
    if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = crate::lan::datapack::follow_host(&follow) {
            tracing::warn!(code = %error.code(), "the main PC's data pack could not be read");
        }
    })
    .await
    {
        tracing::warn!(%error, "the data pack task failed");
    }
    let mut every = tokio::time::interval(HOURLY_EVERY);
    every.tick().await;
    loop {
        every.tick().await;
        let hourly = state.clone();
        if let Err(error) =
            tauri::async_runtime::spawn_blocking(move || run_hourly_backup(&hourly)).await
        {
            tracing::warn!(%error, "the hourly backup task failed");
        }
        let registers = state.clone();
        if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
            // final review: a money call whose answer was lost goes again with its key
            crate::lan::route::replay_calls(&registers);
            // final review: the main PC's regulatory data pack, installed for the next start
            if let Err(error) = crate::lan::datapack::follow_host(&registers) {
                tracing::warn!(code = %error.code(), "the main PC's data pack could not be read");
            }
            crate::lan::registers::send_pending(&registers)
        })
        .await
        {
            tracing::warn!(%error, "the register entries task failed");
        }
    }
}

/// DAT-011: `PRAGMA quick_check` of the live database; a failure (or a check that cannot run)
/// raises M0's "stop selling and restore" alert, which M6's guided restore handles. Blocking.
pub fn run_quick_check(st: &AppState) {
    match st.db.quick_check() {
        Ok(true) => {}
        Ok(false) => {
            tracing::error!("the live database fails its quick check: stop selling and restore");
            st.backups.mark_damaged();
        }
        Err(error) => {
            tracing::error!(%error, "the quick check could not run: stop selling and restore");
            st.backups.mark_damaged();
        }
    }
}

/// DAT-007: an hourly automatic copy (kept `backup.keep_hourly` per day, the past days by the
/// shop's retention) while this PC's drawer is open: a shop that is selling never loses more than
/// an hour. Its outcome is kept for `backup_status` like the daily copy's. Blocking.
pub fn run_hourly_backup(st: &AppState) {
    let terminal = st.terminal.id.clone();
    let open = st
        .db
        .read(move |c| pos_core::cash::current(c, &terminal).map(|s| s.is_some()));
    let keep: u32 = match st
        .db
        .read(|c| pos_core::settings::get(c, "backup.keep_hourly"))
    {
        Ok(keep) => keep,
        Err(error) => {
            tracing::warn!(%error, "hourly backup skipped: its setting could not be read");
            return;
        }
    };
    match open {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            tracing::warn!(%error, "hourly backup skipped: the drawer could not be read");
            return;
        }
    }
    match st.db.hourly_backup_if_due(now_ms(), keep) {
        Ok(Some(info)) => {
            tracing::info!(file = %info.file_name, "hourly backup done");
            st.backups.record(AutoBackupOutcome {
                at: info.at,
                ok: true,
                error: None,
            });
        }
        Ok(None) => {}
        Err(error) => {
            tracing::error!(%error, "the hourly backup failed");
            st.backups.record(AutoBackupOutcome {
                at: now_ms(),
                ok: false,
                error: Some(error.code()),
            });
        }
    }
}

/// LOY-001/002: the vouchers and the earned points past their validity are written off at each
/// start (one `expire` ledger row each, journaled once), on the PC that owns those ledgers — a
/// single PC or the main PC; a till's copy only mirrors them. A failure is logged and
/// tried again at the next start. Blocking: call it on a blocking thread.
pub fn run_expiries(st: &AppState) {
    match st.db.read(role::current) {
        Ok(current) if current.role == TerminalRole::Client => return,
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, "expiries skipped: the LAN role could not be read");
            return;
        }
    }
    let today = business_date(st.now()).format("%Y-%m-%d").to_string();
    let actor = Actor::system(Some(st.terminal.id.clone()));
    let done = st.db.write(move |tx| {
        let vouchers = pos_core::gift_vouchers::expire_due(tx, &actor, &today)?;
        let customers = pos_core::loyalty::expire_due(tx, &actor, &today)?;
        Ok((vouchers, customers))
    });
    match done {
        Ok((0, 0)) => {}
        Ok((vouchers, customers)) => {
            tracing::info!(
                vouchers,
                customers,
                "vouchers and points past their validity expired"
            );
        }
        Err(error) => tracing::warn!(%error, "vouchers and points not expired"),
    }
}

/// `Db::backup` refuses a copy that fails `integrity_check` with `backup_invalid`.
fn copy_failed_its_check(error: &AppError) -> bool {
    error.code() == "backup_invalid"
}

/// `PRAGMA integrity_check` of the live database; a check that cannot run counts as failed.
fn live_database_sound(db: &Db) -> bool {
    match db.integrity_check() {
        Ok(sound) => sound,
        Err(error) => {
            tracing::error!(%error, "the integrity check of the live database could not run");
            false
        }
    }
}

/// The main window's close button on a main PC with tills online (Task 24): hold once and ask
/// (`lan-close-requested`); `true` when the close is held. Any other role closes at once, as
/// does a second request within `lan::CLOSE_AGAIN_MS` or one after "Fermer"
/// (`lan_confirm_close`); a database that cannot be read never keeps the window open. Called by
/// `closing::on_close_requested` before the backup on close, so a held close neither backs up
/// nor exits.
pub fn hold_close_for_tills(window: &tauri::Window) -> bool {
    let Some(state) = window.app_handle().try_state::<SharedState>() else {
        return false;
    };
    match state.lan.should_hold_close(&state, now_ms()) {
        Ok(Some(tills_online)) => {
            use tauri_specta::Event;
            if let Err(error) =
                (crate::lan::LanCloseRequested { tills_online }).emit(window.app_handle())
            {
                tracing::warn!(%error, "could not ask to confirm closing the main PC");
            }
            true
        }
        Ok(None) => false,
        Err(error) => {
            tracing::warn!(code = %error.code(), "closing without checking the tills");
            false
        }
    }
}

/// `RunEvent::Exit` (also on `restart`): `Db::close` writes the clean-shutdown mark, checkpoints
/// the WAL, and from then on every write is refused, so nothing lands behind the mark. A start
/// still opening the database is left to the next start's crash recovery. The log writer is
/// flushed last, so the final lines reach the file before the process ends.
pub fn on_exit(handle: &AppHandle) {
    if let Some(state) = handle.try_state::<SharedState>() {
        // The sync engine stops before `Db::close` writes the clean-shutdown mark.
        if let Some(cloud) = &state.cloud {
            cloud.stop();
        }
        // The device threads end and close their serial ports before the database closes.
        state.devices.stop_all();
        // The server finishes its in-flight requests (5 s at most); the engine stops between
        // two cycles. Both before the database closes.
        state.lan.stop();
        match state.db.clone().close() {
            Ok(()) => tracing::info!("database closed cleanly"),
            Err(error) => tracing::error!(%error, "could not mark a clean shutdown"),
        }
    }
    if let Some(slot) = handle.try_state::<LogSlot>() {
        slot.flush();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_the_database_and_the_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        assert_eq!(state.terminal.code, "C1");
        assert!(Path::new(&paths.db_path).exists());
        assert!(Path::new(&paths.media).is_dir(), "the photos folder");
        assert_eq!(state.datapack.wilayas.len(), 69);
        assert_eq!(state.session().unwrap_err().code(), "session_required");
    }

    /// The print queue's thread holds a database handle: dropping the state must still close
    /// the database before `drop` returns (the restore and crash tests reopen it at once).
    #[test]
    fn dropping_the_state_closes_the_database_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        drop(AppState::open(&paths, crate::printing::silent()).unwrap());
        assert!(
            !dir.path().join("pos.db-wal").exists(),
            "the last handle checkpointed and removed the WAL"
        );
    }

    #[test]
    fn second_start_after_a_crash_is_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        drop(AppState::open(&paths, crate::printing::silent()).unwrap()); // no clean shutdown: a power cut
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        assert!(state.db.report().unclean_shutdown);
        let actions: Vec<String> = state
            .db
            .read(|c| {
                let mut stmt = c.prepare("SELECT action FROM audit_log ORDER BY seq")?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(
            actions,
            ["app.updated", "tax_rate.synced", "db.unclean_shutdown"],
            "the first start records the running version, then the VAT rates are seeded once"
        );
    }

    /// An import a power cut stopped is failed at the next start (its batches stay, undoable).
    #[test]
    fn an_import_left_running_by_a_crash_is_failed_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        state
            .db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO import_jobs (id, kind, file_name, policy, status, mapping, created_at)
                     VALUES ('00000000-0000-7000-8000-0000000000aa', 'products', 'articles.csv',
                             'create_only', 'running', '{}', 1)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        drop(state);
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        let (status, finished): (String, Option<i64>) = state
            .db
            .read(|c| {
                Ok(
                    c.query_row("SELECT status, finished_at FROM import_jobs", [], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })?,
                )
            })
            .unwrap();
        assert_eq!(status, "failed");
        assert!(finished.is_some());
    }

    #[test]
    fn failed_restore_keeps_the_data_and_leaves_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        assert!(state.notices.is_empty());
        state.db.mark_clean_shutdown().unwrap();
        drop(state);
        let damaged = dir.path().join("damaged.db");
        std::fs::write(&damaged, vec![7_u8; 4_096]).unwrap();
        std::fs::write(
            dir.path().join(db::RESTORE_MARKER),
            damaged.to_string_lossy().as_bytes(),
        )
        .unwrap();
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        assert_eq!(
            state.notices,
            [StartupNotice::RestoreFailed {
                reason: "backup_invalid".into()
            }]
        );
        assert!(dir.path().join(db::RESTORE_FAILED_MARKER).exists());
        assert!(
            !state.db.report().unclean_shutdown,
            "the current database was kept as it was"
        );
    }

    #[test]
    fn a_restore_needing_attention_stops_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        AppState::open(&paths, crate::printing::silent())
            .unwrap()
            .db
            .mark_clean_shutdown()
            .unwrap();
        // A crash mid-swap: the staged copy is still there, and the WAL set aside for the swap
        // cannot go back because a live one has taken its place.
        std::fs::write(dir.path().join("pos.db.restoring"), b"staged copy").unwrap();
        std::fs::write(dir.path().join("pos.db-wal"), b"live wal").unwrap();
        std::fs::write(dir.path().join("pos.db-wal.replaced"), b"set-aside wal").unwrap();
        let error = AppState::open(&paths, crate::printing::silent())
            .err()
            .unwrap();
        assert_eq!(error.code(), "restore_needs_attention");
        assert_eq!(
            std::fs::read(dir.path().join("pos.db-wal")).unwrap(),
            b"live wal",
            "never opened"
        );
        assert!(
            dir.path().join("pos.db-wal.replaced").exists(),
            "the evidence is kept"
        );
    }

    /// Review finding: without a data folder the start reports a failure for the fatal screen
    /// instead of returning an error (which aborted the release app without a word).
    #[test]
    fn no_data_folder_is_reported_not_returned() {
        let error = data_paths(Err(tauri::Error::UnknownPath)).unwrap_err();
        let failure = startup_failure(&error, "");
        assert_eq!(
            (failure.error.code.as_str(), failure.data_dir.as_str()),
            ("io_error", "")
        );
        let (reporter, status) = StartupStatus::channel();
        reporter.finish(Some(failure.clone()));
        assert_eq!(
            tauri::async_runtime::block_on(status.outcome()),
            Some(failure)
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            data_paths(Ok(dir.path().to_path_buf())).unwrap(),
            AppPaths::new(dir.path())
        );
    }

    /// Review finding: a logs path that is a file (and a fallback that fails too) still opens
    /// the database, with a notice instead of an abort.
    #[test]
    fn a_logs_path_that_is_a_file_still_starts_with_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        std::fs::write(&paths.logs_dir, b"a file where the logs folder should be").unwrap();
        let fallback = dir.path().join("no-fallback");
        std::fs::write(&fallback, b"a file too").unwrap();
        let logging = logging::init_best_effort(Path::new(&paths.logs_dir), &fallback);
        let notice = StartupNotice::for_logging(&logging).unwrap();
        assert_eq!(
            notice,
            StartupNotice::LoggingUnavailable {
                reason: "io_error".into(),
                fallback_dir: None
            }
        );
        assert_eq!(
            serde_json::to_value(&notice).unwrap(),
            json!({ "kind": "loggingUnavailable", "reason": "io_error", "fallbackDir": null })
        );
        let (reporter, status) = StartupStatus::channel();
        let state = open_state(&paths, vec![notice.clone()], crate::printing::silent()).unwrap();
        reporter.finish(None);
        assert_eq!(tauri::async_runtime::block_on(status.outcome()), None);
        assert_eq!(state.notices, [notice]);
        assert_eq!(state.terminal.code, "C1");
    }

    /// The commands take "now" from the state's clock, which a test may pin to a day (M2 Task
    /// 29 drives a whole month through it); unpinned, it is this PC's.
    #[test]
    fn the_commands_read_the_clock_a_test_pinned() {
        let st = AppState::for_tests();
        let before = now_ms();
        let now = st.now();
        assert!((before..=now_ms()).contains(&now), "this PC's clock");
        crate::commands::tests::first_run(&st);
        let nine_am = 1_788_246_000_000; // 2026-09-01 09:00 in Algiers
        st.clock.pin(nine_am);
        assert_eq!(st.now(), nine_am);
        let ctx = st.till(Permission::SalesCreate).unwrap();
        assert_eq!(
            (ctx.now, ctx.business_day().as_str()),
            (nine_am, "2026-09-01")
        );
    }

    #[test]
    fn startup_status_waits_for_the_outcome() {
        let (reporter, status) = StartupStatus::channel();
        let waiting = status.clone();
        let opener = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            reporter.finish(None);
        });
        assert_eq!(tauri::async_runtime::block_on(waiting.outcome()), None);
        opener.join().unwrap();

        let (reporter, status) = StartupStatus::channel();
        let failure = StartupFailure {
            error: AppError::conflict("database_corrupt").payload(),
            data_dir: "D:/pos".into(),
            till_of: None,
            unsent: None,
            main_pc_tills: None,
        };
        reporter.finish(Some(failure.clone()));
        assert_eq!(
            tauri::async_runtime::block_on(status.outcome()),
            Some(failure)
        );

        let (reporter, status) = StartupStatus::channel();
        drop(reporter); // the opening task died without an outcome
        let interrupted = tauri::async_runtime::block_on(status.outcome()).unwrap();
        assert_eq!(interrupted.error.code, "internal_error");
    }

    #[test]
    fn a_start_after_a_failed_guided_restore_stops_with_database_missing() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        std::fs::write(
            dir.path().join(db::RESTORE_FAILED_MARKER),
            "E:\\gone.db\nio_error\nr1\n",
        )
        .unwrap();
        let error = AppState::open(&paths, crate::printing::silent())
            .err()
            .unwrap();
        assert_eq!(error.code(), "database_missing");
        assert!(
            !Path::new(&paths.db_path).exists(),
            "no empty shop was created"
        );
    }

    #[test]
    fn a_start_journals_the_new_version_and_a_clock_set_back() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        assert!(
            state.notices.is_empty(),
            "a first install shows no \"updated\" notice"
        );
        // An older version ran last, and the journal holds a row dated tomorrow (the journal is
        // append-only, so the row is appended, never edited).
        let tomorrow = now_ms() + 86_400_000;
        state
            .db
            .write(move |tx| {
                db::meta_set(tx, pos_core::updates::META_APP_VERSION, "0.0.9")?;
                let actor = Actor::system(None);
                pos_core::audit::append_for_tests(
                    tx,
                    &actor.event("settings.updated", "setting", Some("x"), json!({})),
                    tomorrow,
                )?;
                Ok(())
            })
            .unwrap();
        state.db.mark_clean_shutdown().unwrap();
        drop(state);
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        let kinds: Vec<&str> = state
            .notices
            .iter()
            .map(|n| match n {
                StartupNotice::ClockMovedBack { .. } => "clock",
                StartupNotice::AppUpdated { .. } => "updated",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["clock", "updated"]);
        assert!(matches!(
            &state.notices[1],
            StartupNotice::AppUpdated { from, to } if from == "0.0.9" && to == APP_VERSION
        ));
    }

    #[test]
    fn the_demo_profile_opens_its_own_database_and_shares_the_pc_files() {
        let dir = tempfile::tempdir().unwrap();
        let real = AppPaths::for_profile(dir.path(), false);
        let demo = AppPaths::for_profile(dir.path(), true);
        assert!(demo.db_path.ends_with("demo\\pos.db") || demo.db_path.ends_with("demo/pos.db"));
        assert_ne!(demo.backups_dir, real.backups_dir);
        assert_eq!(
            (demo.logs_dir.as_str(), demo.hardware_config.as_str()),
            (real.logs_dir.as_str(), real.hardware_config.as_str())
        );
        assert_eq!(demo.local_config, real.local_config);
        assert!(demo.demo && !real.demo);
        let state = AppState::open(&demo, crate::printing::silent()).unwrap();
        assert!(Path::new(&demo.db_path).exists() && !Path::new(&real.db_path).exists());
        drop(state);
        std::fs::write(dir.path().join(pos_core::demo::MARKER), b"demo").unwrap();
        assert_eq!(
            data_paths(Ok(dir.path().to_path_buf())).unwrap(),
            demo,
            "the demo marker picks the profile"
        );
    }

    /// The M6 merge: the demo shop's database is never the shop's network copy. Its start leaves
    /// the real shop's place on the network (`local.json`), a till's token and the LAN
    /// files staged in the data folder alone; the real shop's next start applies them.
    #[test]
    fn the_demo_shop_leaves_the_real_shops_network_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join(pos_core::local::FILE_NAME);
        std::fs::write(
            &local,
            r#"{ "lan": { "role": "till", "host": "Supérette El Amel" }, "lightMode": true }"#,
        )
        .unwrap();
        let reset = dir
            .path()
            .join(pos_core::sync::replica::LAN_DIR)
            .join("reset.pending");
        std::fs::create_dir_all(reset.parent().unwrap()).unwrap();
        std::fs::write(&reset, b"leave").unwrap();
        let vault = Arc::new(vault::MemoryVault::default());
        vault.set("till-token").unwrap();
        let demo = AppPaths::for_profile(dir.path(), true);
        let options = LanOptions {
            bind: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            advertise: false,
        };
        let state =
            AppState::open_with(&demo, crate::printing::silent(), vault.clone(), options).unwrap();
        assert!(Path::new(&demo.db_path).exists());
        drop(state);
        let real = AppPaths::for_profile(dir.path(), false);
        assert_eq!(
            real.till_of().as_deref(),
            Some("Supérette El Amel"),
            "the till's place is kept for the real shop"
        );
        assert!(
            reset.exists(),
            "leaving the network waits for the real shop"
        );
        assert_eq!(vault.get().unwrap().as_deref(), Some("till-token"));
    }

    #[test]
    fn local_changes_are_saved_and_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        let state = AppState::open(&paths, crate::printing::silent()).unwrap();
        let saved = state
            .local
            .update(&state.db, Actor::system(None), |c| c.light_mode = true)
            .unwrap();
        assert!(saved.light_mode);
        assert!(pos_core::local::LocalConfig::load(Path::new(&paths.local_config)).light_mode);
        let rows: i64 = state
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'local.updated'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(rows, 1);
    }
}
