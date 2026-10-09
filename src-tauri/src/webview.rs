//! WebView2 handles F3 (find), F5 (reload), F7 (caret browsing), F12 (devtools), Ctrl+P… before
//! the page sees the key, so `preventDefault` cannot stop them. The till's F-key map needs those
//! keys (spec §10.5): the browser accelerators are switched off; text editing keys stay. Also the
//! runtime's version against the supported minimum (PLT-023) and its memory target while the
//! window is minimised (NFR-004).
#![allow(unsafe_code)] // COM calls into WebView2; the unsafe blocks state their contract.

use std::sync::atomic::{AtomicBool, Ordering};

/// The oldest WebView2 runtime the app supports: `ICoreWebView2_19` (memory target) and the
/// `chrome120` build target of Vite. The installer carries the same minimum (Task 20).
pub const MIN_WEBVIEW2: &str = "120.0.2210.55";

/// This PC's WebView2 version (`tauri::webview_version`), read once in `setup`; `None` when
/// Windows did not tell.
pub struct WebviewInfo(pub Option<String>);

/// Dotted versions compared number by number; an unreadable one counts as outdated.
pub fn is_outdated(version: &str, minimum: &str) -> bool {
    let parse = |v: &str| -> Option<Vec<u64>> {
        v.split('.').map(|part| part.trim().parse().ok()).collect()
    };
    match (parse(version), parse(minimum)) {
        (Some(version), Some(minimum)) => version < minimum,
        _ => true,
    }
}

/// Whether the main webview's memory target is LOW now (it is NORMAL at start).
static LOW: AtomicBool = AtomicBool::new(false);

/// NFR-004: WebView2 trims its memory while the main window is minimised and returns to normal
/// when it shows again (only the changes reach WebView2).
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main"
        || !matches!(
            event,
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Focused(_)
        )
    {
        return;
    }
    let minimized = window.is_minimized().unwrap_or(false);
    if LOW.swap(minimized, Ordering::SeqCst) == minimized {
        return;
    }
    use tauri::Manager;
    if let Some(webview) = window.app_handle().get_webview_window("main") {
        set_memory_target(&webview, minimized);
    }
}

pub fn set_memory_target(window: &tauri::WebviewWindow, low: bool) {
    #[cfg(windows)]
    {
        let scheduled = window.with_webview(move |webview| {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_19,
            };
            use windows_core::Interface;
            let level = if low {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            };
            // SAFETY: as in `disable_browser_accelerators`: Tauri runs this closure on the thread
            // that owns the WebView2 controller; every call returns an HRESULT, checked below.
            let outcome = unsafe {
                webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.cast::<ICoreWebView2_19>())
                    .and_then(|core| core.SetMemoryUsageTargetLevel(level))
            };
            if let Err(error) = outcome {
                tracing::warn!(%error, "could not set the WebView2 memory target");
            }
        });
        if let Err(error) = scheduled {
            tracing::warn!(%error, "the main webview is not ready for its memory target");
        }
    }
    #[cfg(not(windows))]
    let _ = (window, low);
}

pub fn disable_browser_accelerators(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        let scheduled = window.with_webview(|webview| {
            use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
            use windows_core::Interface;
            // SAFETY: Tauri runs this closure on the thread that owns the WebView2 controller;
            // every call returns an HRESULT, checked below.
            let outcome = unsafe {
                webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.Settings())
                    .and_then(|settings| settings.cast::<ICoreWebView2Settings3>())
                    .and_then(|settings| settings.SetAreBrowserAcceleratorKeysEnabled(false))
            };
            if let Err(error) = outcome {
                tracing::warn!(%error, "could not switch off the WebView2 accelerator keys");
            }
        });
        if let Err(error) = scheduled {
            tracing::warn!(%error, "the main webview is not ready for its settings");
        }
    }
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_below_the_minimum_are_outdated() {
        assert!(is_outdated("119.0.2151.97", MIN_WEBVIEW2));
        assert!(is_outdated("120.0.2210.54", MIN_WEBVIEW2));
        assert!(!is_outdated("120.0.2210.55", MIN_WEBVIEW2));
        assert!(!is_outdated("141.0.3537.71", MIN_WEBVIEW2));
        assert!(is_outdated("garbage", MIN_WEBVIEW2));
    }
}
