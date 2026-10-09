//! Health, paths, startup failure, folders, the cold-start mark, fixed links and regulatory data.

use super::blocking;
use crate::state::{AppPaths, AppState, SharedState, StartupFailure, StartupNotice, StartupStatus};
use pos_core::auth::Permission;
use pos_core::datapack::Pack;
use pos_core::error::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub paths: AppPaths,
    pub schema_version: i64,
    pub os: String,
    pub terminal_code: String,
    /// This PC's WebView2 runtime; `None` when Windows did not tell.
    pub webview_version: Option<String>,
    /// NFR-003: milliseconds from the start to the first usable screen, once it showed.
    pub cold_start_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FolderKind {
    Data,
    Backups,
    Logs,
}

pub(crate) fn folder_path(paths: &AppPaths, kind: FolderKind) -> String {
    match kind {
        FolderKind::Data => paths.data_dir.clone(),
        FolderKind::Backups => paths.backups_dir.clone(),
        FolderKind::Logs => paths.logs_dir.clone(),
    }
}

#[tauri::command]
#[specta::specta]
pub fn app_ping() -> String {
    "pong".to_owned()
}

#[tauri::command]
#[specta::specta]
pub fn app_data_dir(paths: State<'_, AppPaths>) -> String {
    paths.data_dir.clone()
}

/// Waits while the database opens in the background (the UI shows the splash meanwhile), then
/// answers the failure for the fatal screen, or `null`. Touches neither SQLite nor Argon2.
#[tauri::command]
#[specta::specta]
pub async fn app_startup_error(
    status: State<'_, StartupStatus>,
) -> Result<Option<StartupFailure>, AppError> {
    Ok(status.outcome().await)
}

/// Notices of this start (a failed scheduled restore), once the database has opened; empty when
/// it did not open.
#[tauri::command]
#[specta::specta]
pub async fn app_startup_notices(
    app: AppHandle,
    status: State<'_, StartupStatus>,
) -> Result<Vec<StartupNotice>, AppError> {
    if status.outcome().await.is_some() {
        return Ok(Vec::new());
    }
    Ok(app
        .try_state::<SharedState>()
        .map(|state| state.notices.clone())
        .unwrap_or_default())
}

#[tauri::command]
#[specta::specta]
pub async fn app_info(app: AppHandle, state: State<'_, SharedState>) -> Result<AppInfo, AppError> {
    let package = app.package_info();
    Ok(AppInfo {
        name: package.name.clone(),
        version: package.version.to_string(),
        identifier: app.config().identifier.clone(),
        paths: state.paths.clone(),
        schema_version: state.db.report().schema_version,
        os: format!(
            "{} {} ({})",
            tauri_plugin_os::platform(),
            tauri_plugin_os::version(),
            tauri_plugin_os::arch()
        ),
        terminal_code: state.terminal.code.clone(),
        webview_version: app
            .try_state::<crate::webview::WebviewInfo>()
            .and_then(|info| info.0.clone()),
        cold_start_ms: ready_ms_if_recorded(),
    })
}

/// Opens a data folder in Explorer from Rust, so the webview needs no opener permission.
/// These folders hold `pos.db` and its copies: see [`may_open_folder`] for who may.
#[tauri::command]
#[specta::specta]
pub async fn app_open_folder(
    app: AppHandle,
    paths: State<'_, AppPaths>,
    status: State<'_, StartupStatus>,
    kind: FolderKind,
) -> Result<(), AppError> {
    let startup_failed = status.outcome().await.is_some();
    let state = app.try_state::<SharedState>();
    may_open_folder(state.as_ref().map(|s| s.inner().as_ref()), startup_failed)?;
    let folder = folder_path(&paths, kind);
    std::fs::create_dir_all(&folder)?;
    app.opener()
        .open_path(folder, None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}

/// Once the database has opened, only a `settings.data` holder may open the data, backups or
/// logs folder (spec §4.5: permission checks happen in Rust). Without an open database there
/// is no session to check: only the fatal screen of a failed start may open them, to help the
/// shop hand its data to support.
pub(crate) fn may_open_folder(
    state: Option<&AppState>,
    startup_failed: bool,
) -> Result<(), AppError> {
    match state {
        Some(st) => st.require(Permission::SettingsData).map(drop),
        None if startup_failed => Ok(()),
        None => Err(AppError::auth("session_required")),
    }
}

/// PLT-024: "Redémarrer pour l'appliquer" once a signed regulatory pack is installed (it applies
/// at the next start). The restart is M0's `restart_soon`: `RunEvent::Exit` closes the database
/// cleanly first (`state::on_exit`).
#[tauri::command]
#[specta::specta]
pub async fn app_restart(app: AppHandle, state: State<'_, SharedState>) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || may_restart(&st)).await?;
    super::backup::restart_soon(app);
    Ok(())
}

/// Whoever installs a regulatory pack (`settings.company`) may restart the app to apply it.
pub(crate) fn may_restart(st: &AppState) -> Result<(), AppError> {
    st.require(Permission::SettingsCompany).map(drop)
}

#[tauri::command]
#[specta::specta]
pub async fn datapack_get(state: State<'_, SharedState>) -> Result<Pack, AppError> {
    Ok(state.datapack.as_ref().clone())
}

static READY_MS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

/// Milliseconds from the process start to the first usable screen, fixed at the first call.
pub(crate) fn ready_ms() -> u64 {
    *READY_MS.get_or_init(|| {
        let ms = crate::STARTED
            .get()
            .map(|start| u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        tracing::info!(cold_start_ms = ms, "first screen ready");
        ms
    })
}

/// The cold start once the first screen recorded it (`app_info`, the Diagnostic screen); never
/// records it.
pub(crate) fn ready_ms_if_recorded() -> Option<u64> {
    READY_MS.get().copied()
}

/// NFR-003: the UI calls it once the login, setup or sale screen has painted. Reads a clock
/// only: no SQLite, no session.
#[tauri::command]
#[specta::specta]
pub async fn app_ready() -> u64 {
    ready_ms()
}

/// The only pages the app opens in the browser: the webview names one of these, never a URL.
/// (`LinkKind` already names the links between documents.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExternalLink {
    Webview2Download,
    DzbuildSite,
    DownloadPage,
    /// A DZBuild account (and the shop's website) for a shop without one: Settings › About, the
    /// setup wizard's last step.
    DzbuildRegister,
    /// DZBuild's tips and guides for shop owners (Settings › About).
    DzbuildBlog,
}

pub(crate) fn link_url(link: ExternalLink) -> &'static str {
    match link {
        ExternalLink::Webview2Download => "https://go.microsoft.com/fwlink/p/?LinkId=2124703",
        ExternalLink::DzbuildSite => "https://dzbuild.com/pos",
        ExternalLink::DownloadPage => "https://dzbuild.com/pos",
        ExternalLink::DzbuildRegister => "https://dzbuild.com/register",
        ExternalLink::DzbuildBlog => "https://blog.dzbuild.com",
    }
}

/// Opens one of the fixed links in the system browser; the webview never names a URL. Offline,
/// the browser says so; nothing here waits for the network.
#[tauri::command]
#[specta::specta]
pub async fn app_open_link(app: AppHandle, kind: ExternalLink) -> Result<(), AppError> {
    blocking(move || {
        app.opener()
            .open_url(link_url(kind), None::<&str>)
            .map_err(|e| AppError::Io(e.to_string()))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn ping_answers_pong() {
        assert_eq!(app_ping(), "pong");
    }

    #[test]
    fn folders_follow_the_data_layout() {
        let paths = AppPaths::new(Path::new("C:/data/dev.dzbuild.pos"));
        assert_eq!(folder_path(&paths, FolderKind::Data), paths.data_dir);
        assert!(folder_path(&paths, FolderKind::Backups).ends_with("backups"));
        assert!(folder_path(&paths, FolderKind::Logs).ends_with("logs"));
        assert!(paths.db_path.ends_with("pos.db"));
    }

    #[test]
    fn the_ready_mark_is_recorded_once() {
        let first = ready_ms();
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(ready_ms(), first, "only the first screen counts");
        assert_eq!(
            link_url(ExternalLink::Webview2Download),
            "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
        );
        assert_eq!(
            link_url(ExternalLink::DownloadPage),
            "https://dzbuild.com/pos"
        );
        assert_eq!(
            link_url(ExternalLink::DzbuildSite),
            "https://dzbuild.com/pos"
        );
        assert_eq!(
            link_url(ExternalLink::DzbuildRegister),
            "https://dzbuild.com/register"
        );
        assert_eq!(
            link_url(ExternalLink::DzbuildBlog),
            "https://blog.dzbuild.com"
        );
    }
}
