//! The DZBuild link in the shell (spec §4.7): the bundled public client values, the Windows
//! Credential Manager, the link attempt in progress, the sync engine and its two window events.
//! Nothing here runs a request inside a command that sells, and nothing here is needed to sell:
//! without a link the engine idles on a 30 s timer reading one row. The link runs on the main PC
//! only (spec §4.6): a till never starts the engine.

use crate::state::{AppPaths, SharedState};
use pos_core::cloud::api::Remote;
use pos_core::cloud::config::CloudConfig;
use pos_core::cloud::engine::{EngineDeps, SyncHandle, SyncNotifier, SyncStatus, Timing};
use pos_core::cloud::http::{Http, SystemClock};
use pos_core::cloud::link::{DeviceInfo, LinkAttempt, LinkContext};
use pos_core::cloud::loopback::Pages;
use pos_core::cloud::secrets::SecretStore;
use pos_core::cloud::store::{self, LinkStatus};
use pos_core::db::Db;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::terminal::Terminal;
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// The public client values (no secret: `CloudConfig` refuses unknown keys).
const BUNDLED: &str = include_str!("../dzbuild.toml");

pub fn bundled_config() -> Result<CloudConfig, AppError> {
    let config: CloudConfig = toml::from_str(BUNDLED).map_err(|e| {
        // the position only: a parser's text can quote a value
        tracing::error!(target: "cloud", span = ?e.span(), "dzbuild.toml does not parse");
        AppError::sync("cloud_config_invalid")
    })?;
    config.validate()?;
    Ok(config)
}

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// The page the browser shows after the redirect to 127.0.0.1, in the till's language: no
/// script, no link, no external resource.
pub fn loopback_pages(lang: Lang) -> Pages {
    const CATALOGUES: [(Lang, &str); 3] = [
        (Lang::Ar, include_str!("../../src/locales/ar/sync.json")),
        (Lang::Fr, include_str!("../../src/locales/fr/sync.json")),
        (Lang::En, include_str!("../../src/locales/en/sync.json")),
    ];
    let text = CATALOGUES
        .iter()
        .find(|(l, _)| *l == lang)
        .map_or(CATALOGUES[1].1, |(_, t)| *t);
    let catalogue: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let word = |key: &str| escape(catalogue["loopback"][key].as_str().unwrap_or_default());
    let dir = if lang.is_rtl() { "rtl" } else { "ltr" };
    let page = |message: String| {
        format!(
            "<!doctype html><html lang=\"{code}\" dir=\"{dir}\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{title}</title>\
             <style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#f7f7f5;\
             color:#1c1c1a;font:16px/1.5 'Segoe UI',Tahoma,system-ui,sans-serif}}main{{max-width:32rem;\
             padding:2rem;text-align:center}}h1{{font-size:1.25rem}}</style></head><body><main>\
             <h1>{title}</h1><p>{message}</p><p>{close}</p></main></body></html>",
            code = lang.as_str(),
            title = word("title"),
            close = word("close"),
        )
    };
    Pages {
        done: page(word("done")),
        failed: page(word("failed")),
    }
}

pub type UrlOpener = Arc<dyn Fn(&str) -> Result<(), AppError> + Send + Sync>;

type StatusHook = Arc<dyn Fn(&SyncStatus) + Send + Sync>;
type OrdersHook = Arc<dyn Fn(i64) + Send + Sync>;

/// The running engine's queue, for `LinkContext::on_change` (made before any engine).
type NotifierSlot = Arc<Mutex<Option<SyncNotifier>>>;

#[derive(Clone)]
pub struct EngineHooks {
    pub on_status: StatusHook,
    pub on_orders: OrdersHook,
}

impl EngineHooks {
    pub fn silent() -> EngineHooks {
        EngineHooks {
            on_status: Arc::new(|_| {}),
            on_orders: Arc::new(|_| {}),
        }
    }
}

/// The sync state changed (the top bar's dot, Settings › DZBuild).
#[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusChanged {
    pub status: SyncStatus,
}

/// The number of new online orders nobody opened yet changed (the nav badge, a soft sound).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct OnlineOrdersChanged {
    pub unseen: i64,
}

pub struct CloudRuntime {
    pub ctx: Arc<LinkContext>,
    engine: Mutex<Option<SyncHandle>>,
    hooks: Mutex<Option<(EngineHooks, Timing)>>,
    notifier: NotifierSlot,
    attempt: Mutex<Option<Arc<LinkAttempt>>>,
    opener: Mutex<Option<UrlOpener>>,
    media_dir: PathBuf,
}

impl std::fmt::Debug for CloudRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudRuntime")
            .field("running", &self.running())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl CloudRuntime {
    /// `media_dir` is the folder product photos are named from (`products/<id>-<n>.jpg`).
    pub fn new(
        db: Db,
        terminal: &Terminal,
        config: CloudConfig,
        secrets: Arc<dyn SecretStore>,
        media_dir: PathBuf,
    ) -> Result<CloudRuntime, AppError> {
        config.validate()?;
        let version = env!("CARGO_PKG_VERSION");
        let http = Http::new(
            &config,
            Arc::new(SystemClock),
            format!("DZBuildPOS/{version} (Windows)"),
        )?;
        let notifier: NotifierSlot = Arc::new(Mutex::new(None));
        let slot = notifier.clone();
        let computer = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".to_owned());
        let ctx = Arc::new(LinkContext {
            db,
            http: Arc::new(http),
            config: Arc::new(config),
            secrets,
            device: DeviceInfo {
                name: format!("{computer} · {}", terminal.code),
                os: "Windows".to_owned(),
                app_version: version.to_owned(),
                terminal_code: terminal.code.clone(),
                terminal_id: terminal.id.clone(),
            },
            on_change: Arc::new(move || {
                if let Some(n) = lock(&slot).as_ref() {
                    n.link_changed();
                }
            }),
        });
        Ok(CloudRuntime {
            ctx,
            engine: Mutex::new(None),
            hooks: Mutex::new(None),
            notifier,
            attempt: Mutex::new(None),
            opener: Mutex::new(None),
            media_dir,
        })
    }

    /// The runtime of the app: the bundled configuration and the Windows Credential Manager.
    /// `None` (the link is off, logged) when the bundled configuration is invalid.
    pub fn open(db: &Db, terminal: &Terminal, paths: &AppPaths) -> Option<CloudRuntime> {
        let config = match bundled_config() {
            Ok(config) => config,
            Err(error) => {
                tracing::error!(target: "cloud", code = %error.code(), "the DZBuild link is off");
                return None;
            }
        };
        let secrets = secret_store();
        // photos are named `products/<id>-<n>.jpg` under the media folder (`crate::media`)
        let media = PathBuf::from(&paths.media);
        CloudRuntime::new(db.clone(), terminal, config, secrets, media)
            .inspect_err(
                |e| tracing::error!(target: "cloud", code = %e.code(), "the DZBuild link is off"),
            )
            .ok()
    }

    pub fn start(&self, hooks: EngineHooks, timing: Timing) -> Result<(), AppError> {
        *lock(&self.hooks) = Some((hooks, timing));
        self.resume()
    }

    /// Starts the engine again with the hooks given to `start` (after an unlink). Nothing on a
    /// till: the link runs on the main PC only.
    pub fn resume(&self) -> Result<(), AppError> {
        let Some((hooks, timing)) = lock(&self.hooks).clone() else {
            return Ok(());
        };
        if self.ctx.db.read(pos_core::sync::role::is_client)? {
            tracing::info!(target: "cloud", "a till: the DZBuild link runs on the main PC");
            return Ok(());
        }
        let mut engine = lock(&self.engine);
        if engine.is_some() {
            return Ok(());
        }
        let handle = SyncHandle::start(EngineDeps {
            link: self.ctx.clone(),
            media_dir: Some(self.media_dir.clone()),
            timing,
            on_status: hooks.on_status,
            on_orders: hooks.on_orders,
        })?;
        *lock(&self.notifier) = Some(handle.notifier());
        *engine = Some(handle);
        Ok(())
    }

    /// Stops the engine (its drop waits at most 2 s; a request in flight keeps its key).
    pub fn stop(&self) {
        let handle = lock(&self.engine).take();
        *lock(&self.notifier) = None;
        drop(handle);
    }

    pub fn running(&self) -> bool {
        lock(&self.engine).is_some()
    }

    pub fn wake(&self) {
        if let Some(engine) = lock(&self.engine).as_ref() {
            engine.wake();
        }
    }

    pub fn link_changed(&self) {
        if let Some(engine) = lock(&self.engine).as_ref() {
            engine.link_changed();
        }
    }

    pub fn status(&self) -> SyncStatus {
        if let Some(engine) = lock(&self.engine).as_ref() {
            return engine.status();
        }
        match self.ctx.db.read(store::active_link) {
            Ok(Some(link)) => SyncStatus {
                linked: true,
                link_status: Some(link.status),
                store_name: Some(link.store_name.clone()),
                public_url: link.public_url.clone(),
                ..SyncStatus::default()
            },
            _ => SyncStatus::default(),
        }
    }

    /// The engine's client for the linked store, when it has one now.
    pub fn remote_now(&self) -> Option<Remote> {
        lock(&self.engine).as_ref().and_then(SyncHandle::remote)
    }

    /// The engine's client, waiting up to 5 s right after a link. One `TokenManager` per link
    /// (the engine's): two would both spend the rotating refresh token.
    pub fn remote(&self) -> Result<Remote, AppError> {
        match self.ctx.db.read(store::active_link)? {
            None => return Err(AppError::sync("cloud_not_linked")),
            Some(link) if link.status == LinkStatus::RelinkRequired => {
                return Err(AppError::sync("cloud_relink_required"));
            }
            Some(link) if link.status != LinkStatus::Linked => {
                return Err(AppError::sync("cloud_not_linked"));
            }
            Some(_) => {}
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(remote) = self.remote_now() {
                return Ok(remote);
            }
            if !self.running() || Instant::now() >= deadline {
                return Err(AppError::sync("cloud_unavailable"));
            }
            self.wake();
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Only one link attempt at a time: a new one cancels the previous.
    pub fn set_attempt(&self, attempt: Arc<LinkAttempt>) {
        if let Some(previous) = lock(&self.attempt).replace(attempt) {
            previous.cancel();
        }
    }

    pub fn attempt(&self) -> Option<Arc<LinkAttempt>> {
        lock(&self.attempt).clone()
    }

    pub fn clear_attempt(&self) {
        if let Some(previous) = lock(&self.attempt).take() {
            previous.cancel();
        }
    }

    pub fn set_opener(&self, opener: UrlOpener) {
        *lock(&self.opener) = Some(opener);
    }

    /// Opens `url` in the system browser (never a webview: the owner types their DZBuild password
    /// only on dzbuild.com, RFC 8252).
    pub fn open_url(&self, url: &str) -> Result<(), AppError> {
        let opener = lock(&self.opener)
            .clone()
            .ok_or_else(|| AppError::sync("cloud_unavailable"))?;
        opener(url)
    }

    pub fn media_dir(&self) -> &Path {
        &self.media_dir
    }
}

#[cfg(windows)]
fn secret_store() -> Arc<dyn SecretStore> {
    match pos_core::cloud::secrets::CredentialManager::open() {
        Ok(store) => Arc::new(store),
        Err(error) => {
            tracing::warn!(target: "cloud", code = %error.code(), "no Credential Manager: linking will be refused");
            Arc::new(pos_core::cloud::secrets::NoSecretStore)
        }
    }
}

#[cfg(not(windows))]
fn secret_store() -> Arc<dyn SecretStore> {
    Arc::new(pos_core::cloud::secrets::NoSecretStore)
}

/// What failed when the system browser did not open, never the error's text: it can quote the
/// URL, and an authorization URL carries `state`.
fn opener_failure(error: &tauri_plugin_opener::Error) -> &'static str {
    match error {
        tauri_plugin_opener::Error::Io(_) => "io",
        tauri_plugin_opener::Error::ForbiddenUrl { .. } => "forbidden_url",
        _ => "other",
    }
}

/// `RunEvent::Ready`, after the daily backup, on a blocking thread (it reads this PC's LAN
/// role): the engine starts with the window's events and the system browser.
pub fn start_engine(handle: &AppHandle, state: &SharedState) {
    use tauri_plugin_opener::OpenerExt;
    use tauri_specta::Event;
    let Some(cloud) = &state.cloud else { return };
    // The demo shop never talks to DZBuild (its catalogue would reach the real store).
    if state.paths.demo {
        tracing::info!("the demo shop does not start the DZBuild link");
        return;
    }
    let browser = handle.clone();
    cloud.set_opener(Arc::new(move |url: &str| {
        browser
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| {
                tracing::warn!(target: "cloud", kind = opener_failure(&e), "the system browser did not open");
                AppError::sync("cloud_unavailable")
            })
    }));
    let (status_app, orders_app) = (handle.clone(), handle.clone());
    let hooks = EngineHooks {
        on_status: Arc::new(move |status: &SyncStatus| {
            if let Err(error) = (SyncStatusChanged {
                status: status.clone(),
            })
            .emit(&status_app)
            {
                tracing::warn!(%error, "could not tell the window the sync status");
            }
        }),
        on_orders: Arc::new(move |unseen| {
            if let Err(error) = (OnlineOrdersChanged { unseen }).emit(&orders_app) {
                tracing::warn!(%error, "could not tell the window about online orders");
            }
        }),
    };
    if let Err(error) = cloud.start(hooks, Timing::production()) {
        tracing::error!(target: "cloud", code = %error.code(), "the sync engine did not start");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use pos_core::cloud::secrets::MemorySecrets;
    use std::time::{Duration, Instant};

    fn until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn the_loopback_pages_speak_the_language_and_carry_no_script() {
        let ar = loopback_pages(Lang::Ar);
        assert!(ar.done.contains("dir=\"rtl\"") && ar.done.contains("lang=\"ar\""));
        assert!(ar.done.contains("عُد إلى DZBuild POS"));
        let fr = loopback_pages(Lang::Fr);
        assert!(fr.failed.contains("dir=\"ltr\"") && fr.failed.contains("Revenez à DZBuild POS"));
        for page in [&ar.done, &ar.failed, &fr.done, &fr.failed] {
            assert!(
                !page.contains("<script") && !page.contains("http"),
                "no script, no link: {page}"
            );
        }
    }

    #[test]
    fn html_is_escaped() {
        assert_eq!(
            escape("<b>\"x\" & 'y'</b>"),
            "&lt;b&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/b&gt;"
        );
    }

    #[test]
    fn the_engine_runs_after_start_and_stops_before_the_database_closes() {
        let st = AppState::for_tests_cloud(
            CloudConfig::loopback("http://127.0.0.1:9"),
            Arc::new(MemorySecrets::default()),
        );
        let cloud = st.cloud().unwrap();
        assert!(!cloud.running());
        assert!(!cloud.status().linked, "status works with the engine off");
        cloud.start(EngineHooks::silent(), Timing::fast()).unwrap();
        until("a cycle", || cloud.status().last_cycle_at.is_some());
        assert!(!cloud.status().linked);
        cloud.stop();
        assert!(!cloud.running());
        cloud.resume().unwrap();
        assert!(cloud.running(), "resume reuses the hooks given to start");
        cloud.stop();
        st.db.write(|_| Ok(())).unwrap();
    }

    #[test]
    fn a_link_change_before_the_engine_is_harmless_and_after_it_wakes_the_engine() {
        let st = AppState::for_tests_cloud(
            CloudConfig::loopback("http://127.0.0.1:9"),
            Arc::new(MemorySecrets::default()),
        );
        let cloud = st.cloud().unwrap();
        (cloud.ctx.on_change)();
        let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = seen.clone();
        let hooks = EngineHooks {
            on_status: Arc::new(move |_| {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
            ..EngineHooks::silent()
        };
        cloud.start(hooks, Timing::production()).unwrap();
        until("the first status", || {
            seen.load(std::sync::atomic::Ordering::SeqCst) >= 1
        });
        assert!(cloud.remote_now().is_none(), "no link, no client");
        assert_eq!(cloud.remote().unwrap_err().code(), "cloud_not_linked");
        cloud.stop();
    }

    #[test]
    fn opening_a_page_goes_through_the_injected_browser() {
        let st = AppState::for_tests_cloud(
            CloudConfig::loopback("http://127.0.0.1:9"),
            Arc::new(MemorySecrets::default()),
        );
        let cloud = st.cloud().unwrap();
        assert_eq!(
            cloud.open_url("https://dzbuild.com").unwrap_err().code(),
            "cloud_unavailable",
            "no browser yet"
        );
        let opened = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = opened.clone();
        cloud.set_opener(Arc::new(move |url: &str| {
            log.lock().unwrap().push(url.to_owned());
            Ok(())
        }));
        cloud.open_url("https://dzbuild.com/register").unwrap();
        assert_eq!(
            opened.lock().unwrap().as_slice(),
            ["https://dzbuild.com/register"]
        );
    }

    /// The DZBuild link runs on the main PC only (spec §4.6): a till's cloud tables are its own
    /// and stay empty, so its engine never starts, whatever asks.
    #[test]
    fn the_engine_never_starts_on_a_till() {
        let st = AppState::for_tests_cloud(
            CloudConfig::loopback("http://127.0.0.1:9"),
            Arc::new(MemorySecrets::default()),
        );
        st.db
            .write(|tx| {
                tx.execute(
                    "UPDATE lan_local SET role = 'client', host_id = '00000000-0000-7000-8000-00000000a001',
                         host_address = '127.0.0.1:7345', paired_at = 1 WHERE id = 1",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        let cloud = st.cloud().unwrap();
        cloud.start(EngineHooks::silent(), Timing::fast()).unwrap();
        assert!(!cloud.running(), "a till never runs the DZBuild link");
        cloud.resume().unwrap();
        assert!(!cloud.running());
        assert!(!cloud.status().linked, "the status still answers");
    }
}
