//! The `customer-display` window (spec §4.1, §10.5, HW-042): its own HTML entry, full screen on
//! the chosen monitor, never on the till's own screen; a windowed preview from the settings when
//! there is no second monitor. The full-screen window never takes the keyboard from the till
//! (the scanner types into the focused window).

use pos_core::error::AppError;
use pos_hw::config::CustomerScreenConfig;
use serde::Serialize;
use specta::Type;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const LABEL: &str = "customer-display";
const ENTRY: &str = "customer-display.html";
/// How long `close` waits for the window to be gone.
const CLOSE_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    /// As Windows names it (`\\.\DISPLAY2`), what `CustomerScreenConfig.monitor` keeps.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub primary: bool,
    /// The till's main window is on this monitor.
    pub main_window: bool,
}

fn window_error(error: tauri::Error) -> AppError {
    AppError::Internal(format!("customer display window: {error}"))
}

/// The PC's monitors, in physical pixels.
pub fn monitors(app: &AppHandle) -> Result<Vec<MonitorInfo>, AppError> {
    let primary = app
        .primary_monitor()
        .map_err(window_error)?
        .and_then(|m| m.name().cloned());
    let main = app
        .get_webview_window("main")
        .and_then(|w| w.current_monitor().ok().flatten())
        .and_then(|m| m.name().cloned());
    Ok(app
        .available_monitors()
        .map_err(window_error)?
        .into_iter()
        .map(|m| {
            let name = m.name().cloned().unwrap_or_default();
            MonitorInfo {
                width: m.size().width,
                height: m.size().height,
                x: m.position().x,
                y: m.position().y,
                primary: primary.as_deref() == Some(name.as_str()),
                main_window: main.as_deref() == Some(name.as_str()),
                name,
            }
        })
        .collect())
}

/// The customer screen's monitor: the named one if it is not the till's, else the first other
/// one, else none (no second monitor). When no monitor is known to hold the till's window, none
/// either: a full-screen, always-on-top window on the till's own screen would lock the till.
pub fn pick<'a>(monitors: &'a [MonitorInfo], wanted: Option<&str>) -> Option<&'a MonitorInfo> {
    if !monitors.iter().any(|m| m.main_window) {
        return None;
    }
    wanted
        .and_then(|name| monitors.iter().find(|m| m.name == name && !m.main_window))
        .or_else(|| monitors.iter().find(|m| !m.main_window))
}

fn scale_of(app: &AppHandle, m: &MonitorInfo) -> f64 {
    app.available_monitors()
        .ok()
        .and_then(|all| {
            all.into_iter()
                .find(|x| x.name().map(String::as_str) == Some(m.name.as_str()))
        })
        .map_or(1.0, |x| x.scale_factor())
}

/// Logical position and size; hidden until placed.
fn build(
    app: &AppHandle,
    (x, y): (f64, f64),
    (w, h): (f64, f64),
    preview: bool,
) -> Result<WebviewWindow, AppError> {
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(ENTRY.into()))
        .title("DZBuild POS")
        .position(x, y)
        .inner_size(w, h)
        .decorations(preview)
        .always_on_top(!preview)
        .skip_taskbar(!preview)
        .resizable(preview)
        // The full-screen window can never become the active one, even when touched.
        .focusable(preview)
        .focused(false)
        .visible(false)
        .build()
        .map_err(window_error)
}

/// Shows `window` without leaving the till's keyboard on it.
fn show_behind_the_till(app: &AppHandle, window: &WebviewWindow) -> Result<(), AppError> {
    window.show().map_err(window_error)?;
    if window.is_focused().unwrap_or(false)
        && let Some(main) = app.get_webview_window("main")
        && let Err(error) = main.set_focus()
    {
        tracing::debug!(%error, "the till could not take the focus back");
    }
    Ok(())
}

/// The window is the full-screen customer screen on `m` already.
fn placed_on(window: &WebviewWindow, m: &MonitorInfo) -> bool {
    let full_screen = window.is_fullscreen().unwrap_or(false);
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .and_then(|current| current.name().cloned());
    full_screen && monitor.as_deref() == Some(m.name.as_str())
}

/// The window is the settings' windowed preview.
fn is_preview(window: &WebviewWindow) -> bool {
    window.is_decorated().unwrap_or(false)
}

/// Opens, moves or closes the customer screen as `config` says (at the start and after each
/// hardware save). No second monitor, or the screen switched off: the full-screen window closes
/// (a preview the settings opened stays until they close it). Already in place: left alone.
pub fn sync(app: &AppHandle, config: &CustomerScreenConfig) -> Result<(), AppError> {
    let all = monitors(app)?;
    let target = if config.enabled {
        pick(&all, config.monitor.as_deref()).cloned()
    } else {
        None
    };
    let existing = app.get_webview_window(LABEL);
    let Some(m) = target else {
        return match existing {
            Some(window) if !is_preview(&window) => close(app),
            _ => Ok(()),
        };
    };
    if existing.as_ref().is_some_and(|w| placed_on(w, &m)) {
        return Ok(());
    }
    close(app)?;
    let scale = scale_of(app, &m);
    let window = build(
        app,
        (f64::from(m.x) / scale, f64::from(m.y) / scale),
        (f64::from(m.width) / scale, f64::from(m.height) / scale),
        false,
    )?;
    window
        .set_fullscreen_on_monitor(PhysicalPosition::new(f64::from(m.x), f64::from(m.y)))
        .map_err(window_error)?;
    show_behind_the_till(app, &window)
}

/// How often [`watch`] looks at the monitors again.
const WATCH_EVERY: Duration = Duration::from_secs(3);

/// Runs [`sync`] again whenever a monitor comes, goes or moves, or the till's window changes
/// screen, with the settings `config` reads then. When the customer's monitor is unplugged,
/// Windows would move the full-screen, always-on-top window onto the till's screen; the next
/// look closes it instead (or moves it to the monitor that is left for it).
pub fn watch(app: AppHandle, config: impl Fn() -> Option<CustomerScreenConfig> + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("customer-display-watch".into())
        .spawn(move || {
            let mut seen = monitors(&app).ok();
            loop {
                std::thread::sleep(WATCH_EVERY);
                let now = monitors(&app).ok();
                if now.is_none() || now == seen {
                    continue;
                }
                if let Some(config) = config()
                    && let Err(error) = sync(&app, &config)
                {
                    tracing::warn!(%error, "the customer screen could not follow the monitors");
                }
                seen = now;
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "the customer screen will not follow the monitors");
    }
}

/// A 800 × 480 window with borders on the till's screen: the settings' "Aperçu".
pub fn open_preview(app: &AppHandle) -> Result<(), AppError> {
    close(app)?;
    let window = build(app, (80.0, 80.0), (800.0, 480.0), true)?;
    window.show().map_err(window_error)
}

/// Closes the customer screen and waits (at most [`CLOSE_WAIT`]) until the event loop has
/// destroyed it: its label is free again for the next one. Blocking: never on the main thread.
pub fn close(app: &AppHandle) -> Result<(), AppError> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    window.destroy().map_err(window_error)?;
    let started = Instant::now();
    while app.get_webview_window(LABEL).is_some() {
        if started.elapsed() > CLOSE_WAIT {
            return Err(AppError::Internal(
                "customer display window: still open".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, x: i32, main_window: bool) -> MonitorInfo {
        MonitorInfo {
            name: name.into(),
            width: 1024,
            height: 768,
            x,
            y: 0,
            primary: x == 0,
            main_window,
        }
    }

    #[test]
    fn no_second_monitor_opens_no_window() {
        assert_eq!(pick(&[monitor(r"\\.\DISPLAY1", 0, true)], None), None);
        assert_eq!(pick(&[], None), None);
    }

    #[test]
    fn a_till_window_on_an_unknown_monitor_opens_no_window() {
        let screens = [
            monitor(r"\\.\DISPLAY1", 0, false),
            monitor(r"\\.\DISPLAY2", 1024, false),
        ];
        assert_eq!(
            pick(&screens, Some(r"\\.\DISPLAY2")),
            None,
            "it could be the till's own screen"
        );
    }

    #[test]
    fn the_named_monitor_wins_then_the_first_other_one() {
        let screens = [
            monitor(r"\\.\DISPLAY1", 0, true),
            monitor(r"\\.\DISPLAY2", 1024, false),
            monitor(r"\\.\DISPLAY3", 2048, false),
        ];
        assert_eq!(
            pick(&screens, Some(r"\\.\DISPLAY3")).map(|m| m.x),
            Some(2048)
        );
        assert_eq!(
            pick(&screens, Some(r"\\.\DISPLAY9")).map(|m| m.x),
            Some(1024),
            "a missing monitor falls back"
        );
        assert_eq!(
            pick(&screens, Some(r"\\.\DISPLAY1")).map(|m| m.x),
            Some(1024),
            "never on the till's own screen"
        );
    }
}
