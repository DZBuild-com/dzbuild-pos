//! Closing the main window (or Quit in the tray): the backup on close when it is due (DAT-024),
//! with the window saying so, then the exit (`state::on_exit` closes the database cleanly).
//! `AppHandle::exit` and `restart` end the event loop without a close request, so a restore's
//! restart never comes through here.

use crate::state::SharedState;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tauri_specta::Event;

static CLOSING: AtomicBool = AtomicBool::new(false);

/// "app-closing": the window shows "Sauvegarde avant fermeture…" until the process ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct AppClosing {
    pub backup: bool,
}

/// Shows the main window again (restored, in front, focused); a failure is logged only.
pub fn bring_back(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    for (step, outcome) in [
        ("unminimize", window.unminimize()),
        ("show", window.show()),
        ("set_focus", window.set_focus()),
    ] {
        if let Err(error) = outcome {
            tracing::debug!(%error, step, "could not bring the window back before closing");
        }
    }
}

/// The main window's close request: a main PC with tills online asks first (LAN-003,
/// `state::hold_close_for_tills`; "Fermer" closes the window again through
/// `lan_confirm_close`). Otherwise the window stays open while the backup on close runs on its
/// own thread, then the app exits. A click once the close has started is ignored, so the backup
/// is never cut short by the window going away.
pub fn on_close_requested(window: &tauri::Window, api: &tauri::CloseRequestApi) {
    if window.label() != "main" {
        return;
    }
    api.prevent_close();
    // Quit from the tray while the window is minimised or hidden: the tills' confirmation and the
    // "backing up before closing" overlay must be seen (finding 27).
    bring_back(window.app_handle());
    if CLOSING.load(Ordering::SeqCst) || crate::state::hold_close_for_tills(window) {
        return;
    }
    if CLOSING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = window.app_handle().clone();
    let fallback = app.clone();
    let spawned = std::thread::Builder::new()
        .name("pos-close".into())
        .spawn(move || {
            if let Some(st) = app.try_state::<SharedState>().map(|s| s.inner().clone())
                && crate::backups::close_backup_due(&st)
            {
                if let Err(error) = (AppClosing { backup: true }).emit(&app) {
                    tracing::warn!(%error, "could not tell the window it is closing");
                }
                crate::backups::backup_on_close(&st);
            }
            app.exit(0);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "could not start the close backup; closing now");
        fallback.exit(0);
    }
}
