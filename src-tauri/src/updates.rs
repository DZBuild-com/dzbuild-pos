//! Updates (spec §13, PLT-010/011): the Tauri updater plugin behind `UpdateSource`, so the
//! policy — the channel, the kill switch and rollout, downloads only with every drawer closed,
//! no install during a shift, a verified backup and a journal row before the setup runs, the
//! database closed before Windows starts the setup — is tested without a network.

use crate::commands::blocking;
use crate::state::{APP_VERSION, AppPaths, AppState, SharedState};
use pos_core::audit::Actor;
use pos_core::db::{BackupKind, META_INSTALL_ID, meta_get};
use pos_core::error::AppError;
use pos_core::settings;
use pos_core::time::now_ms;
use pos_core::updates::{
    Channel, Decision, InstallBlocker, ManifestExtras, Notes, channel_endpoint, check_artifact,
    decide, decide_for_till, install_blockers,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// A release a check found; `handle` is the plugin's (absent in tests and from the mirror).
#[derive(Clone)]
pub struct RemoteUpdate {
    pub version: String,
    pub raw: serde_json::Value,
    pub handle: Option<Arc<tauri_plugin_updater::Update>>,
    /// Found on the main PC's mirror: downloaded and installed from there.
    pub mirror: bool,
}

/// Where the main PC keeps the update it verified, for its tills (`pos-server` serves
/// its `latest.json` and `setup.exe`; nothing else is read from it).
pub fn mirror_dir(paths: &AppPaths) -> PathBuf {
    Path::new(&paths.data_dir).join("updates").join("mirror")
}

/// A download that receives nothing for this long has stalled: it fails, and the next check,
/// download or install can run (finding 14).
pub const DOWNLOAD_STALL: Duration = Duration::from_secs(90);

/// How long the check before an install may take (finding 13).
pub const INSTALL_RECHECK: Duration = Duration::from_secs(15);

/// Offline at install time, the release is installed only when a check answered this recently.
pub const RECENT_CHECK_MS: i64 = 6 * 3_600_000;

pub trait UpdateSource: Send + Sync {
    fn check<'a>(&'a self, endpoint: &'a str) -> BoxFuture<'a, Option<RemoteUpdate>>;
    fn download<'a>(
        &'a self,
        update: &'a RemoteUpdate,
        progress: Box<dyn FnMut(u64, Option<u64>) + Send + 'a>,
    ) -> BoxFuture<'a, Vec<u8>>;
    /// Windows: runs the before-exit hook (the database closes to writes, the windows hide, the
    /// tray goes), starts the setup (`/P /UPDATE`) and exits the process. An `Err` after the
    /// hook ran (Windows did not start the setup: its administrator prompt was refused) leaves
    /// a process that can only restart: [`UpdateSource::has_shut_down`] says so.
    fn install(&self, update: &RemoteUpdate, bytes: Vec<u8>) -> Result<(), AppError>;
    /// The before-exit hook ran in this process.
    fn has_shut_down(&self) -> bool;
    /// Starts the app again: a new process, its database open to writes again.
    fn restart(&self);
}

/// A signature, a signed version or a base64 problem is a corrupt download; anything else is
/// a failed check (offline, the server, a bad manifest).
pub fn map_plugin_error(error: &tauri_plugin_updater::Error) -> AppError {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => {
            tracing::warn!(error = %error, "update refused: its signature does not check");
            AppError::conflict("update_corrupt")
        }
        other => {
            tracing::warn!(error = %other, "update check or download failed");
            AppError::Sync {
                code: "update_check_failed".into(),
            }
        }
    }
}

pub struct PluginSource {
    app: AppHandle,
    /// Set first thing in the before-exit hook, which the plugin runs before `ShellExecuteW`.
    shut_down: Arc<AtomicBool>,
}

impl PluginSource {
    pub fn new(app: AppHandle) -> PluginSource {
        PluginSource {
            app,
            shut_down: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Right before Windows runs the setup: the clean-shutdown mark (no write lands behind it) and
/// the last log lines. `cleanup_before_exit` hides the windows and removes the tray.
fn before_exit(app: &AppHandle) {
    if let Some(st) = app.try_state::<SharedState>()
        && let Err(error) = st.db.mark_clean_shutdown()
    {
        tracing::error!(%error, "could not mark the database closed before the update");
    }
    if let Some(slot) = app.try_state::<crate::logging::LogSlot>() {
        slot.flush();
    }
    app.cleanup_before_exit();
}

impl UpdateSource for PluginSource {
    fn check<'a>(&'a self, endpoint: &'a str) -> BoxFuture<'a, Option<RemoteUpdate>> {
        Box::pin(async move {
            use tauri_plugin_updater::UpdaterExt;
            let url = url::Url::parse(endpoint)
                .map_err(|_| AppError::validation("endpoint", "update_endpoint_invalid"))?;
            let (exit_app, shut_down) = (self.app.clone(), self.shut_down.clone());
            let updater = self
                .app
                .updater_builder()
                .endpoints(vec![url])
                .map_err(|e| map_plugin_error(&e))?
                // No overall timeout: a setup of several MB takes minutes on a slow line. A
                // connection that does not open, or a transfer that stops sending, fails instead
                // (`read_timeout` restarts at each read; the plugin applies it to the download).
                .configure_client(|client| {
                    client
                        .connect_timeout(Duration::from_secs(20))
                        .read_timeout(Duration::from_secs(60))
                })
                .on_before_exit(move || {
                    shut_down.store(true, Ordering::SeqCst);
                    before_exit(&exit_app);
                })
                .build()
                .map_err(|e| map_plugin_error(&e))?;
            let found = updater.check().await.map_err(|e| map_plugin_error(&e))?;
            Ok(found.map(|update| RemoteUpdate {
                version: update.version.clone(),
                raw: update.raw_json.clone(),
                handle: Some(Arc::new(update)),
                mirror: false,
            }))
        })
    }

    fn download<'a>(
        &'a self,
        update: &'a RemoteUpdate,
        mut progress: Box<dyn FnMut(u64, Option<u64>) + Send + 'a>,
    ) -> BoxFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let handle = update
                .handle
                .clone()
                .ok_or_else(|| AppError::conflict("update_not_available"))?;
            let mut received = 0_u64;
            handle
                .download(
                    |chunk, total| {
                        received += chunk as u64;
                        progress(received, total);
                    },
                    || {},
                )
                .await
                .map_err(|e| map_plugin_error(&e))
        })
    }

    fn install(&self, update: &RemoteUpdate, bytes: Vec<u8>) -> Result<(), AppError> {
        let handle = update
            .handle
            .as_ref()
            .ok_or_else(|| AppError::conflict("update_not_available"))?;
        handle
            .install(bytes)
            .map_err(|e| AppError::Internal(format!("update install: {e}")))
    }

    fn has_shut_down(&self) -> bool {
        self.shut_down.load(Ordering::SeqCst)
    }

    /// Through the event loop: `RunEvent::Exit` runs `state::on_exit`, then Tauri starts the
    /// new process (the single-instance lock is released on the way, as for a restore).
    fn restart(&self) {
        self.app.request_restart();
    }
}

/// The signature of the setup for this machine in a Tauri manifest (`platforms.<key>.signature`).
fn manifest_signature(raw: &serde_json::Value) -> Option<&str> {
    pos_core::updates::PLATFORM_KEYS
        .iter()
        .find_map(|key| raw.get("platforms").and_then(|p| p.get(*key)))
        .and_then(|platform| platform.get("signature"))
        .and_then(serde_json::Value::as_str)
}

/// The plugin's own check of a setup, for one that comes from the main PC: the
/// minisign signature against the updater public key of this build, then the version the
/// signature's trusted comment binds it to (the plugin's `requireSignedVersion`). Anything else
/// is `update_corrupt`: a main PC cannot push an unsigned, altered or older build.
pub fn verify_signed(
    bytes: &[u8],
    signature: &str,
    pubkey: &str,
    version: &str,
) -> Result<(), AppError> {
    use base64::Engine;
    let corrupt = || AppError::conflict("update_corrupt");
    let text = |b64: &str| -> Result<String, AppError> {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| corrupt())?;
        String::from_utf8(raw).map_err(|_| corrupt())
    };
    let key = minisign_verify::PublicKey::decode(&text(pubkey)?).map_err(|_| corrupt())?;
    let signature = minisign_verify::Signature::decode(&text(signature)?).map_err(|_| corrupt())?;
    key.verify(bytes, &signature, true).map_err(|_| corrupt())?;
    let signed = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
        .ok_or_else(corrupt)?;
    if same_version(signed, version) {
        Ok(())
    } else {
        Err(corrupt())
    }
}

fn parsed(version: &str) -> Option<semver::Version> {
    semver::Version::parse(version.trim_start_matches('v')).ok()
}

/// The same version, `v` prefix or not (semver when both parse).
fn same_version(a: &str, b: &str) -> bool {
    match (parsed(a), parsed(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

/// `offered` is a newer version than `current` (semver; anything unreadable is not).
fn newer(offered: &str, current: &str) -> bool {
    matches!((parsed(offered), parsed(current)), (Some(o), Some(c)) if o > c)
}

/// A till's updates from its main PC, which serves the update it downloaded and
/// verified (`pos-server` `/updates/*`, the till's token). The till checks it again itself:
/// minisign signature and signed version here, digest and size in the service.
pub struct MirrorSource {
    app: AppHandle,
    pubkey: String,
    shut_down: Arc<AtomicBool>,
}

impl MirrorSource {
    pub fn new(app: AppHandle, pubkey: String) -> MirrorSource {
        MirrorSource {
            app,
            pubkey,
            shut_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The main PC's API with this till's token, when this PC is a till.
    async fn api(&self) -> Result<pos_server::client::api::HostApi, AppError> {
        let st = self
            .app
            .try_state::<SharedState>()
            .map(|s| s.inner().clone())
            .ok_or_else(|| AppError::conflict("update_not_available"))?;
        blocking(move || {
            let place = st.db.read(pos_core::sync::role::current)?;
            let address = place
                .host_address
                .filter(|_| place.role == pos_core::terminal::TerminalRole::Client)
                .ok_or_else(|| AppError::conflict("lan_not_paired"))?;
            let token = st.lan.vault.get()?;
            let base = pos_server::client::api::normalize_address(&address)?;
            pos_server::client::api::HostApi::new(
                &base,
                token,
                APP_VERSION,
                pos_core::db::migrations::LATEST_VERSION,
            )
        })
        .await
    }
}

impl UpdateSource for MirrorSource {
    fn check<'a>(&'a self, _endpoint: &'a str) -> BoxFuture<'a, Option<RemoteUpdate>> {
        Box::pin(async move {
            let Some(raw) = self.api().await?.update_manifest().await? else {
                return Ok(None);
            };
            let version = raw
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(|v| v.trim_start_matches('v').to_owned())
                .ok_or_else(|| AppError::conflict("update_not_available"))?;
            if !newer(&version, APP_VERSION) {
                return Ok(None);
            }
            // The main PC mirrors only its own build's setup: a till of the other arch (a
            // 32-bit till next to a 64-bit main PC, or the reverse) takes its own from the
            // internet instead of being offered a setup its signature cannot match.
            if !pos_core::updates::mirror_serves(&raw, pos_core::updates::ARCH) {
                tracing::info!("the main PC mirrors a setup for another Windows architecture");
                return Ok(None);
            }
            Ok(Some(RemoteUpdate {
                version,
                raw,
                handle: None,
                mirror: true,
            }))
        })
    }

    fn download<'a>(
        &'a self,
        update: &'a RemoteUpdate,
        mut progress: Box<dyn FnMut(u64, Option<u64>) + Send + 'a>,
    ) -> BoxFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let signature = manifest_signature(&update.raw)
                .ok_or_else(|| AppError::conflict("update_corrupt"))?
                .to_owned();
            let bytes = self.api().await?.update_setup(&mut *progress).await?;
            verify_signed(&bytes, &signature, &self.pubkey, &update.version)?;
            Ok(bytes)
        })
    }

    /// As the plugin does on Windows: the before-exit hook, the setup started through the shell
    /// (its administrator prompt) with the plugin's passive-mode arguments, then the process
    /// ends.
    fn install(&self, update: &RemoteUpdate, bytes: Vec<u8>) -> Result<(), AppError> {
        let arch = if cfg!(target_arch = "x86") {
            "x86"
        } else {
            "x64"
        };
        let path =
            std::env::temp_dir().join(format!("DZBuildPOS_{}_{arch}-setup.exe", update.version));
        std::fs::write(&path, &bytes)?;
        self.shut_down.store(true, Ordering::SeqCst);
        before_exit(&self.app);
        launch_setup(&path)?;
        std::process::exit(0);
    }

    fn has_shut_down(&self) -> bool {
        self.shut_down.load(Ordering::SeqCst)
    }

    fn restart(&self) {
        self.app.request_restart();
    }
}

/// `cmd /C start` goes through the shell, which shows the setup's administrator prompt (a
/// per-machine install), with the arguments the plugin gives an NSIS setup in passive mode.
#[cfg(windows)]
fn launch_setup(path: &Path) -> Result<(), AppError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .args(["/P", "/UPDATE", "/R"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
        .map_err(|e| AppError::Internal(format!("update install: {e}")))
}

#[cfg(not(windows))]
fn launch_setup(_path: &Path) -> Result<(), AppError> {
    Err(AppError::conflict("update_not_available"))
}

/// The app's source: a till asks its main PC first (the update the main PC verified,
/// over the LAN, even without Internet), then the internet; every other PC asks the internet.
pub struct ShopSource {
    app: AppHandle,
    plugin: PluginSource,
    mirror: MirrorSource,
}

impl ShopSource {
    pub fn new(app: AppHandle, pubkey: String) -> ShopSource {
        ShopSource {
            plugin: PluginSource::new(app.clone()),
            mirror: MirrorSource::new(app.clone(), pubkey),
            app,
        }
    }

    fn on_a_till(&self) -> bool {
        self.app
            .try_state::<SharedState>()
            .is_some_and(|st| st.lan.role() == pos_core::terminal::TerminalRole::Client)
    }
}

impl UpdateSource for ShopSource {
    fn check<'a>(&'a self, endpoint: &'a str) -> BoxFuture<'a, Option<RemoteUpdate>> {
        Box::pin(async move {
            if self.on_a_till() {
                match self.mirror.check(endpoint).await {
                    Ok(Some(found)) => return Ok(Some(found)),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::info!(code = %error.code(), "the main PC's update mirror did not answer");
                    }
                }
            }
            self.plugin.check(endpoint).await
        })
    }

    fn download<'a>(
        &'a self,
        update: &'a RemoteUpdate,
        progress: Box<dyn FnMut(u64, Option<u64>) + Send + 'a>,
    ) -> BoxFuture<'a, Vec<u8>> {
        if update.mirror {
            self.mirror.download(update, progress)
        } else {
            self.plugin.download(update, progress)
        }
    }

    fn install(&self, update: &RemoteUpdate, bytes: Vec<u8>) -> Result<(), AppError> {
        if update.mirror {
            self.mirror.install(update, bytes)
        } else {
            self.plugin.install(update, bytes)
        }
    }

    fn has_shut_down(&self) -> bool {
        self.plugin.has_shut_down() || self.mirror.has_shut_down()
    }

    fn restart(&self) {
        self.plugin.restart();
    }
}

/// Left in the data folder when Windows did not start the setup after the database closed; the
/// next start turns it into `StartupNotice::UpdateNotInstalled` and deletes it.
pub const NOT_INSTALLED_MARKER: &str = "update-not-installed.json";

fn not_installed_marker(paths: &AppPaths) -> PathBuf {
    Path::new(&paths.data_dir).join(NOT_INSTALLED_MARKER)
}

/// The version the last start could not install, once: the marker is deleted when read.
pub fn take_not_installed(paths: &AppPaths) -> Option<String> {
    let marker = not_installed_marker(paths);
    let text = match std::fs::read_to_string(&marker) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(%error, "could not read the update-not-installed marker");
            return None;
        }
    };
    if let Err(error) = std::fs::remove_file(&marker) {
        tracing::warn!(%error, "could not delete the update-not-installed marker");
    }
    let version = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("version").and_then(|v| v.as_str()).map(str::to_owned));
    if version.is_none() {
        tracing::warn!("the update-not-installed marker is unreadable");
    }
    version
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DownloadState {
    Idle,
    Downloading { received: u64, total: Option<u64> },
    Ready,
    Failed { code: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOffer {
    pub version: String,
    pub notes: Option<Notes>,
    pub pub_date: Option<String>,
    pub decision: Decision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub channel: Channel,
    pub auto_check: bool,
    pub auto_download: bool,
    pub last_check_at: Option<i64>,
    pub last_check_error: Option<String>,
    pub offer: Option<UpdateOffer>,
    pub download: DownloadState,
    pub blockers: Vec<InstallBlocker>,
    pub installing: bool,
}

/// "update-progress": bytes received so far, and the total when the server gave one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub received: u64,
    pub total: Option<u64>,
}

#[derive(Default)]
struct Inner {
    last_check_at: Option<i64>,
    /// The last check that answered (offline at install time, a recent one lets it go on).
    last_answer_at: Option<i64>,
    last_check_error: Option<String>,
    remote: Option<RemoteUpdate>,
    extras: Option<ManifestExtras>,
    offer: Option<UpdateOffer>,
    download: Option<DownloadState>,
    bytes: Option<Arc<Vec<u8>>>,
    installing: bool,
}

/// This PC's place in the shop for the update decision (M6 review): every PC of a shop network
/// shares the main PC's rollout place; a till takes only the version its main PC runs.
struct ShopPlace {
    rollout_id: String,
    /// A till's main PC (its address).
    host_address: Option<String>,
}

impl ShopPlace {
    fn of(st: &AppState) -> Result<ShopPlace, AppError> {
        let (place, install_id) = st.db.read(|c| {
            Ok((
                pos_core::sync::role::current(c)?,
                meta_get(c, META_INSTALL_ID)?,
            ))
        })?;
        let till = place.role == pos_core::terminal::TerminalRole::Client;
        Ok(ShopPlace {
            rollout_id: place
                .host_id
                .filter(|_| place.role != pos_core::terminal::TerminalRole::Standalone)
                .or(install_id)
                .unwrap_or_default(),
            host_address: place.host_address.filter(|_| till),
        })
    }

    fn decide(&self, extras: &ManifestExtras, offered: &str, main_pc: Option<&str>) -> Decision {
        if self.host_address.is_some() {
            decide_for_till(extras, APP_VERSION, offered, main_pc)
        } else {
            decide(extras, APP_VERSION, offered, &self.rollout_id)
        }
    }
}

/// Writes what the main PC verified for its tills: the old manifest goes first, then the setup,
/// then the manifest that names it (each through a temporary file renamed into place), so a till
/// never reads a manifest whose setup is not complete. The manifest is marked with this build's
/// arch (`mirror_arch`): the setup next to it is this PC's own, so a till of the other arch
/// skips the mirror.
fn write_mirror(dir: &Path, raw: &serde_json::Value, bytes: &[u8]) -> Result<(), AppError> {
    let raw = pos_core::updates::mark_mirror(raw, pos_core::updates::ARCH);
    std::fs::create_dir_all(dir)?;
    let put = |name: &str, content: &[u8]| -> Result<(), AppError> {
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, dir.join(name))?;
        Ok(())
    };
    if let Err(error) = std::fs::remove_file(dir.join(pos_server::host::UPDATE_MANIFEST))
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    put(pos_server::host::UPDATE_SETUP, bytes)?;
    put(
        pos_server::host::UPDATE_MANIFEST,
        &serde_json::to_vec(&raw)?,
    )
}

/// What the status reads from the shop's database (settings and open drawers).
struct ShopView {
    channel: Channel,
    auto_check: bool,
    auto_download: bool,
    blockers: Vec<InstallBlocker>,
}

pub struct UpdateService {
    source: Arc<dyn UpdateSource>,
    configured: String,
    inner: Mutex<Inner>,
    busy: AtomicBool,
}

/// Check, download or install at a time.
struct Busy<'a>(&'a AtomicBool);

impl<'a> Busy<'a> {
    fn take(flag: &'a AtomicBool) -> Result<Busy<'a>, AppError> {
        if flag.swap(true, Ordering::SeqCst) {
            return Err(AppError::conflict("update_busy"));
        }
        Ok(Busy(flag))
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// SQLite work of the async steps runs on a blocking thread, never on the runtime's.
async fn off_thread<T, F>(st: &SharedState, f: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> Result<T, AppError> + Send + 'static,
{
    let st = st.clone();
    blocking(move || f(&st)).await
}

impl UpdateService {
    pub fn new(source: Arc<dyn UpdateSource>, configured: String) -> UpdateService {
        UpdateService {
            source,
            configured,
            inner: Mutex::new(Inner::default()),
            busy: AtomicBool::new(false),
        }
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn channel(st: &AppState) -> Channel {
        st.db
            .read(|c| settings::get::<String>(c, "updates.channel"))
            .ok()
            .and_then(|s| Channel::parse(&s))
            .unwrap_or(Channel::Stable)
    }

    fn flag(st: &AppState, key: &'static str) -> bool {
        st.db
            .read(move |c| settings::get::<bool>(c, key))
            .unwrap_or(true)
    }

    fn blockers(st: &AppState) -> Result<Vec<InstallBlocker>, AppError> {
        let terminal = st.terminal.clone();
        st.db.read(move |c| install_blockers(c, &terminal))
    }

    fn shop_view(st: &AppState) -> Result<ShopView, AppError> {
        Ok(ShopView {
            blockers: Self::blockers(st)?,
            channel: Self::channel(st),
            auto_check: Self::flag(st, "updates.auto_check"),
            auto_download: Self::flag(st, "updates.auto_download"),
        })
    }

    fn compose(&self, view: ShopView) -> UpdateStatus {
        let inner = self.inner();
        UpdateStatus {
            current_version: APP_VERSION.into(),
            channel: view.channel,
            auto_check: view.auto_check,
            auto_download: view.auto_download,
            last_check_at: inner.last_check_at,
            last_check_error: inner.last_check_error.clone(),
            offer: inner.offer.clone(),
            download: inner.download.clone().unwrap_or(DownloadState::Idle),
            blockers: view.blockers,
            installing: inner.installing,
        }
    }

    /// Reads SQLite: call it on a blocking thread (the async steps use `status_of`).
    pub fn status(&self, st: &AppState) -> Result<UpdateStatus, AppError> {
        Ok(self.compose(Self::shop_view(st)?))
    }

    async fn status_of(&self, st: &SharedState) -> Result<UpdateStatus, AppError> {
        let view = off_thread(st, Self::shop_view).await?;
        Ok(self.compose(view))
    }

    /// Asks the channel's manifest. A manual check returns the error; the daily one only
    /// records it. A release the policy does not offer is shown (manual) but never kept for
    /// download, and a copy of it already downloaded is dropped (a release paused after the
    /// download is never installed).
    pub async fn check_now(
        &self,
        st: &SharedState,
        manual: bool,
    ) -> Result<UpdateStatus, AppError> {
        let _busy = Busy::take(&self.busy)?;
        let (channel, shop) =
            off_thread(st, |st| Ok((Self::channel(st), ShopPlace::of(st)?))).await?;
        let endpoint = channel_endpoint(&self.configured, channel)?;
        let found = self.source.check(&endpoint).await;
        // A till follows its main PC (M6 review): the version the main PC runs, from its health.
        let main_pc = match (&found, &shop.host_address) {
            (Ok(Some(_)), Some(address)) => {
                let address = address.clone();
                blocking(move || Ok(pos_server::client::join::probe(&address).ok()))
                    .await?
                    .map(|health| health.app_version)
            }
            _ => None,
        };
        {
            let mut inner = self.inner();
            inner.last_check_at = Some(now_ms());
            match found {
                Err(error) => {
                    inner.last_check_error = Some(error.code());
                    if manual {
                        return Err(error);
                    }
                }
                Ok(None) => {
                    inner.last_answer_at = inner.last_check_at;
                    inner.last_check_error = None;
                    inner.offer = None;
                    inner.remote = None;
                    inner.extras = None;
                    inner.download = None;
                    inner.bytes = None;
                }
                Ok(Some(remote)) => {
                    inner.last_answer_at = inner.last_check_at;
                    inner.last_check_error = None;
                    let extras = ManifestExtras::from_raw(&remote.raw);
                    let decision = shop.decide(&extras, &remote.version, main_pc.as_deref());
                    let same = inner
                        .remote
                        .as_ref()
                        .is_some_and(|r| r.version == remote.version);
                    if !same || decision != Decision::Offer {
                        inner.download = None;
                        inner.bytes = None;
                    }
                    inner.offer = Some(UpdateOffer {
                        version: remote.version.clone(),
                        notes: extras.notes.clone(),
                        pub_date: remote
                            .raw
                            .get("pub_date")
                            .and_then(|d| d.as_str())
                            .map(str::to_owned),
                        decision: decision.clone(),
                    });
                    if decision == Decision::Offer {
                        inner.remote = Some(remote);
                        inner.extras = Some(extras);
                    } else {
                        inner.remote = None;
                        inner.extras = None;
                    }
                }
            }
        }
        self.status_of(st).await
    }

    /// Downloads the offered release, checks its digest and keeps it in memory (a setup of a
    /// few MB); journals `update.downloaded`. A failure keeps nothing.
    pub async fn download(
        &self,
        st: &SharedState,
        mut progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<UpdateStatus, AppError> {
        let pending = {
            let inner = self.inner();
            match (&inner.download, inner.remote.clone(), inner.extras.clone()) {
                (Some(DownloadState::Ready), ..) => None,
                (_, Some(remote), Some(extras)) => Some((remote, extras)),
                _ => return Err(AppError::conflict("update_not_available")),
            }
        };
        let Some((remote, extras)) = pending else {
            return self.status_of(st).await;
        };
        let _busy = Busy::take(&self.busy)?;
        self.inner().download = Some(DownloadState::Downloading {
            received: 0,
            total: None,
        });
        let fetched = self
            .watched_download(&remote, &mut progress)
            .await
            .and_then(|bytes| check_artifact(&bytes, &extras).map(|()| bytes));
        match fetched {
            Ok(bytes) => {
                let size = bytes.len();
                // A main PC keeps what it verified for its tills.
                if st.lan.role() == pos_core::terminal::TerminalRole::Host {
                    let (dir, raw, copy) =
                        (mirror_dir(&st.paths), remote.raw.clone(), bytes.clone());
                    if let Err(error) = blocking(move || write_mirror(&dir, &raw, &copy)).await {
                        tracing::warn!(code = %error.code(), "the update was not mirrored for the tills");
                    }
                }
                {
                    let mut inner = self.inner();
                    inner.bytes = Some(Arc::new(bytes));
                    inner.download = Some(DownloadState::Ready);
                }
                let version = remote.version.clone();
                off_thread(st, move |st| {
                    let actor = Actor::system(Some(st.terminal.id.clone()));
                    st.db.write(move |tx| {
                        pos_core::audit::append(
                            tx,
                            &actor.event(
                                "update.downloaded",
                                "update",
                                Some(&version),
                                json!({ "size": size }),
                            ),
                        )
                        .map(drop)
                    })
                })
                .await?;
                self.status_of(st).await
            }
            Err(error) => {
                let mut inner = self.inner();
                inner.bytes = None;
                inner.download = Some(DownloadState::Failed { code: error.code() });
                Err(error)
            }
        }
    }

    /// Installs the downloaded release: refused during a shift (`update_blocked`) and before a
    /// verified download (`update_not_ready`); otherwise a `pre_migration` backup (mirrored),
    /// `update.install_started { from, to, backup }`, then the source runs the setup — the
    /// database is marked closed in the before-exit hook, just before the process ends. When
    /// Windows then does not start the setup, the app restarts (`UpdateNotInstalled` notice).
    /// Blocking (the backup): call it on a blocking thread.
    pub fn install(&self, st: &AppState, actor: Actor) -> Result<(), AppError> {
        let (remote, bytes) = {
            let inner = self.inner();
            match (&inner.download, inner.remote.clone(), inner.bytes.clone()) {
                (Some(DownloadState::Ready), Some(remote), Some(bytes)) => (remote, bytes),
                _ => return Err(AppError::conflict("update_not_ready")),
            }
        };
        if !Self::blockers(st)?.is_empty() {
            return Err(AppError::conflict("update_blocked"));
        }
        let _busy = Busy::take(&self.busy)?;
        // Finding 13: the kill switch holds until the setup runs.
        self.still_offered(st, &remote)?;
        self.inner().installing = true;
        let outcome = (|| {
            let backup = st.db.backup(BackupKind::PreMigration)?;
            crate::backups::after_backup(st, &backup, actor.clone());
            let (to, file) = (remote.version.clone(), backup.file_name.clone());
            let journal = actor.clone();
            st.db.write(move |tx| {
                pos_core::audit::append(
                    tx,
                    &journal.event(
                        "update.install_started",
                        "update",
                        Some(&to),
                        json!({ "from": APP_VERSION, "to": to, "backup": file }),
                    ),
                )
                .map(drop)
            })?;
            tracing::warn!(to = %remote.version, "starting the update setup");
            self.source.install(&remote, bytes.as_ref().clone())
        })();
        if let Err(error) = &outcome {
            self.inner().installing = false;
            if self.source.has_shut_down() {
                // The database is closed to writes and the windows and the tray are gone: only
                // a new process can sell again. The next start says why the update is missing.
                tracing::error!(%error, to = %remote.version, "Windows did not start the update setup; restarting");
                let marker = json!({ "version": remote.version }).to_string();
                if let Err(error) = std::fs::write(not_installed_marker(&st.paths), marker) {
                    tracing::warn!(%error, "could not leave the update-not-installed marker");
                }
                self.source.restart();
            }
        }
        outcome
    }

    /// The download with a watchdog: one that receives nothing for [`DOWNLOAD_STALL`] fails with
    /// `update_check_failed` (the source's own read timeout should come first).
    async fn watched_download(
        &self,
        remote: &RemoteUpdate,
        progress: &mut (impl FnMut(u64, Option<u64>) + Send),
    ) -> Result<Vec<u8>, AppError> {
        let last = Arc::new(Mutex::new(tokio::time::Instant::now()));
        let seen = last.clone();
        let download = self.source.download(
            remote,
            Box::new(move |received, total| {
                if let Ok(mut at) = seen.lock() {
                    *at = tokio::time::Instant::now();
                }
                progress(received, total);
            }),
        );
        tokio::pin!(download);
        loop {
            let since = last
                .lock()
                .map(|at| *at)
                .unwrap_or_else(|_| tokio::time::Instant::now());
            tokio::select! {
                done = &mut download => return done,
                () = tokio::time::sleep_until(since + DOWNLOAD_STALL) => {
                    let quiet = last.lock().map(|at| at.elapsed()).unwrap_or_default();
                    if quiet >= DOWNLOAD_STALL {
                        tracing::warn!("the update download stalled");
                        return Err(AppError::Sync { code: "update_check_failed".into() });
                    }
                }
            }
        }
    }

    /// Finding 13: right before the backup and the setup, the channel is asked again. A release
    /// paused, pulled, replaced or no longer offered to this PC since its download is dropped
    /// (`update_not_available`); offline, it installs only when a check answered within
    /// [`RECENT_CHECK_MS`] (`update_check_failed` otherwise).
    fn still_offered(&self, st: &AppState, remote: &RemoteUpdate) -> Result<(), AppError> {
        let shop = ShopPlace::of(st)?;
        let endpoint = channel_endpoint(&self.configured, Self::channel(st))?;
        let answer = tauri::async_runtime::block_on(async {
            tokio::time::timeout(INSTALL_RECHECK, self.source.check(&endpoint)).await
        });
        let now = now_ms();
        let fresh = match answer {
            Ok(Ok(fresh)) => fresh,
            Ok(Err(_)) | Err(_) => {
                let recent = self
                    .inner()
                    .last_answer_at
                    .is_some_and(|at| at <= now && now - at < RECENT_CHECK_MS);
                return if recent {
                    Ok(())
                } else {
                    Err(AppError::Sync {
                        code: "update_check_failed".into(),
                    })
                };
            }
        };
        let main_pc = match (&fresh, &shop.host_address) {
            (Some(_), Some(address)) => pos_server::client::join::probe(address)
                .ok()
                .map(|health| health.app_version),
            _ => None,
        };
        let offered = fresh.as_ref().is_some_and(|fresh| {
            fresh.version == remote.version
                && shop.decide(
                    &ManifestExtras::from_raw(&fresh.raw),
                    &fresh.version,
                    main_pc.as_deref(),
                ) == Decision::Offer
        });
        let mut inner = self.inner();
        inner.last_check_at = Some(now);
        inner.last_answer_at = Some(now);
        if offered {
            return Ok(());
        }
        tracing::warn!(version = %remote.version, "the downloaded release is no longer offered: dropped");
        inner.remote = None;
        inner.extras = None;
        inner.bytes = None;
        inner.download = None;
        inner.offer = None;
        Err(AppError::conflict("update_not_available"))
    }

    /// The background step: a check when due (automatic checks on), then a download when
    /// automatic downloads are on and no drawer is open.
    pub async fn auto_step(&self, st: &SharedState) -> Result<(), AppError> {
        let view = off_thread(st, Self::shop_view).await?;
        if !view.auto_check {
            return Ok(());
        }
        let due = self
            .inner()
            .last_check_at
            .is_none_or(|at| now_ms() - at >= 20 * 3_600_000);
        if due {
            self.check_now(st, false).await?;
        }
        let offered = self.inner().remote.is_some();
        let idle = !matches!(
            self.inner().download,
            Some(DownloadState::Ready | DownloadState::Downloading { .. })
        );
        if offered && idle && view.auto_download {
            let blockers = off_thread(st, Self::blockers).await?;
            if blockers.is_empty() {
                self.download(st, |_, _| {}).await?;
            }
        }
        Ok(())
    }
}

/// Started once the database opened: first step after 2 minutes (the start stays fast), then
/// every 6 hours; a step that fails is logged and retried at the next one.
pub fn spawn_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(120)).await;
        loop {
            if let (Some(service), Some(st)) = (
                app.try_state::<Arc<UpdateService>>()
                    .map(|s| s.inner().clone()),
                app.try_state::<SharedState>().map(|s| s.inner().clone()),
            ) {
                if let Err(error) = service.auto_step(&st).await {
                    tracing::info!(%error, "automatic update step");
                }
                let ready =
                    service
                        .status_of(&st)
                        .await
                        .ok()
                        .and_then(|s| match (s.download, s.offer) {
                            (DownloadState::Ready, Some(offer)) => Some(offer.version),
                            _ => None,
                        });
                crate::tray::set_update_ready(&app, ready);
            }
            tokio::time::sleep(Duration::from_secs(6 * 3_600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::updates::{ARCH, MIRROR_ARCH, arch_name, mirror_serves};

    #[test]
    fn the_main_pc_marks_its_mirror_with_its_own_arch() {
        let dir = tempfile::tempdir().unwrap();
        let raw = serde_json::json!({
            "version": "1.0.1",
            "platforms": {
                "windows-x86_64": { "url": "https://dzbuild.com/pos/download/1.0.1/a.exe", "signature": "s" },
                "windows-i686": { "url": "https://dzbuild.com/pos/download/1.0.1/b.exe", "signature": "t" }
            }
        });
        write_mirror(dir.path(), &raw, b"setup").unwrap();
        let stored: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.path().join(pos_server::host::UPDATE_MANIFEST)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            stored.get(MIRROR_ARCH).and_then(serde_json::Value::as_str),
            Some(ARCH)
        );
        assert_eq!(stored.get("platforms"), raw.get("platforms"));
        assert!(
            mirror_serves(&stored, ARCH),
            "a till of the same arch takes it"
        );
        let other = arch_name(!cfg!(target_arch = "x86"));
        assert!(
            !mirror_serves(&stored, other),
            "a till of the other arch asks the internet"
        );
        assert_eq!(
            std::fs::read(dir.path().join(pos_server::host::UPDATE_SETUP)).unwrap(),
            b"setup"
        );
    }
}
