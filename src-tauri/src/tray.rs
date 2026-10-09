//! The system tray (spec §4.1, PLT-020): Open and Quit, and a tooltip with the version and what
//! needs attention (an update ready, a backup problem). Labels come from the UI catalogues in
//! the signed-in user's language (`retitle` after a login or a language change). M5 adds
//! "close to the tray" and autostart for the host role.

use crate::texts::{catalogue_text, fill};
use pos_core::lang::Lang;
use std::sync::RwLock;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "main";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayStatus {
    pub update_ready: Option<String>,
    pub backup_problem: bool,
}

/// The last language and status, so either can change alone.
static CURRENT: RwLock<(Lang, TrayStatus)> = RwLock::new((
    Lang::Fr,
    TrayStatus {
        update_ready: None,
        backup_problem: false,
    },
));

pub fn menu_label(lang: Lang, key: &str) -> String {
    catalogue_text(lang, &["tray", key]).unwrap_or_else(|| key.to_owned())
}

/// At most 127 characters (Windows' limit for a tray tooltip).
pub fn tooltip_text(lang: Lang, version: &str, status: &TrayStatus) -> String {
    let mut lines = vec![fill(&menu_label(lang, "tooltip"), &[("version", version)])];
    if let Some(next) = &status.update_ready {
        lines.push(fill(&menu_label(lang, "updateReady"), &[("version", next)]));
    }
    if status.backup_problem {
        lines.push(menu_label(lang, "backupProblem"));
    }
    lines.join("\n").chars().take(127).collect()
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_menu(app: &AppHandle, lang: Lang) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let open = MenuItemBuilder::with_id("open", menu_label(lang, "open")).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", menu_label(lang, "quit")).build(app)?;
    MenuBuilder::new(app).items(&[&open, &quit]).build()
}

/// Creates the tray icon with the app icon, in French until a user signs in.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let version = app.package_info().version.to_string();
    let menu = build_menu(app, Lang::Fr)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip_text(Lang::Fr, &version, &TrayStatus::default()))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "quit" => {
                // The window's close request runs the close backup (Task 9), then exits; the
                // window comes back first so its confirmation or overlay is seen (finding 27).
                crate::closing::bring_back(app);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.close();
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let (lang, status) = match CURRENT.read() {
        Ok(current) => current.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    let version = app.package_info().version.to_string();
    if let Err(error) = tray.set_tooltip(Some(tooltip_text(lang, &version, &status))) {
        tracing::warn!(%error, "could not update the tray tooltip");
    }
    match build_menu(app, lang) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                tracing::warn!(%error, "could not update the tray menu");
            }
        }
        Err(error) => tracing::warn!(%error, "could not build the tray menu"),
    }
}

/// The tray in `lang` (a sign-in, a language change).
pub fn retitle(app: &AppHandle, lang: Lang) {
    match CURRENT.write() {
        Ok(mut current) => current.0 = lang,
        Err(poisoned) => poisoned.into_inner().0 = lang,
    }
    refresh(app);
}

fn update_status(app: &AppHandle, change: impl FnOnce(&mut TrayStatus)) {
    match CURRENT.write() {
        Ok(mut current) => change(&mut current.1),
        Err(poisoned) => change(&mut poisoned.into_inner().1),
    }
    refresh(app);
}

/// The updater (Task 12): `Some(version)` once an update is downloaded and waits for install.
pub fn set_update_ready(app: &AppHandle, version: Option<String>) {
    update_status(app, |status| status.update_ready = version);
}

/// The backup watch (Task 9): a failed, stale, unmirrored or damaged backup.
pub fn set_backup_problem(app: &AppHandle, problem: bool) {
    update_status(app, |status| status.backup_problem = problem);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_speaks_the_ui_language_and_carries_the_status() {
        let quiet = TrayStatus::default();
        assert_eq!(tooltip_text(Lang::Fr, "0.6.0", &quiet), "DZBuild POS 0.6.0");
        let busy = TrayStatus {
            update_ready: Some("0.6.1".into()),
            backup_problem: true,
        };
        let fr = tooltip_text(Lang::Fr, "0.6.0", &busy);
        assert!(fr.contains("0.6.1") && fr.contains("sauvegarde"), "{fr}");
        let ar = tooltip_text(Lang::Ar, "0.6.0", &busy);
        assert!(ar.contains("0.6.1") && ar.contains("النسخ"), "{ar}");
        assert!(
            tooltip_text(Lang::En, "0.6.0", &busy).chars().count() <= 127,
            "Windows cuts tooltips at 127"
        );
        assert_eq!(menu_label(Lang::Ar, "open"), "فتح");
    }
}
