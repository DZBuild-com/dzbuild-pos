//! WebView2 printing through `webview2-com` (spec §9.2): `ICoreWebView2_16::Print` with print
//! settings (printer, copies, page size and margins) and `ICoreWebView2_7::PrintToPdf`. A runtime
//! without the silent print shows the print dialog. The completion handlers run on the webview's
//! thread; the caller waits on a channel from its own blocking thread.
#![allow(unsafe_code)] // COM calls into WebView2; each unsafe block states its contract.

use crate::print_host::{PageSetup, PrintStatus};
use pos_core::error::AppError;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

const PRINT_WAIT: Duration = Duration::from_secs(120);

#[cfg(windows)]
mod com {
    use crate::print_host::PageSetup;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PRINT_MEDIA_SIZE_CUSTOM, COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE,
        COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT, ICoreWebView2PrintSettings,
        ICoreWebView2PrintSettings2,
    };
    use windows_core::Interface;

    fn inches(mm: u32) -> f64 {
        f64::from(mm) / 25.4
    }

    /// Page size, orientation and margins in inches (WebView2 names its margins left/right). The
    /// host page sets the same `@page` size and margin for the job (`HostJob`), so the page is the
    /// same whichever of the two Chromium honours.
    ///
    /// # Safety
    /// Call on the thread that owns the WebView2 controller, with settings it created.
    pub(super) unsafe fn apply(
        settings: &ICoreWebView2PrintSettings,
        setup: &PageSetup,
    ) -> windows_core::Result<()> {
        // SAFETY: the caller holds the controller's thread; each call returns an HRESULT.
        unsafe {
            settings.SetOrientation(if setup.landscape {
                COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE
            } else {
                COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT
            })?;
            settings.SetPageWidth(inches(setup.width_mm))?;
            settings.SetPageHeight(inches(setup.height_mm))?;
            settings.SetMarginTop(inches(setup.margin_mm))?;
            settings.SetMarginBottom(inches(setup.margin_mm))?;
            settings.SetMarginLeft(inches(setup.margin_mm))?;
            settings.SetMarginRight(inches(setup.margin_mm))?;
            settings.SetShouldPrintBackgrounds(true)?;
            settings.SetShouldPrintHeaderAndFooter(false)?;
            if let Ok(s2) = settings.cast::<ICoreWebView2PrintSettings2>() {
                s2.SetMediaSize(COREWEBVIEW2_PRINT_MEDIA_SIZE_CUSTOM)?;
            }
            Ok(())
        }
    }
}

/// Prints the host's page on `printer` (the Windows default when `None`).
pub fn print(
    window: &tauri::WebviewWindow,
    setup: &PageSetup,
    printer: Option<&str>,
    copies: u32,
) -> Result<PrintStatus, AppError> {
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_PRINT_STATUS_PRINTER_UNAVAILABLE, COREWEBVIEW2_PRINT_STATUS_SUCCEEDED,
            ICoreWebView2_16, ICoreWebView2Environment6, ICoreWebView2PrintSettings2,
        };
        use webview2_com::{CoTaskMemPWSTR, PrintCompletedHandler};
        use windows_core::Interface;
        let (tx, rx) = mpsc::channel::<Result<PrintStatus, AppError>>();
        let (setup, printer) = (*setup, printer.map(str::to_owned));
        let copies = i32::try_from(copies.clamp(1, 5)).unwrap_or(1);
        window
            .with_webview(move |webview| {
                // SAFETY: Tauri runs this closure on the thread that owns the WebView2 controller;
                // every call returns an HRESULT, checked by `?`.
                let started = unsafe {
                    (|| -> windows_core::Result<bool> {
                        let core = webview.controller().CoreWebView2()?;
                        let Ok(core16) = core.cast::<ICoreWebView2_16>() else {
                            return Ok(false);
                        };
                        let settings = webview
                            .environment()
                            .cast::<ICoreWebView2Environment6>()?
                            .CreatePrintSettings()?;
                        com::apply(&settings, &setup)?;
                        let settings2 = settings.cast::<ICoreWebView2PrintSettings2>()?;
                        if let Some(name) = &printer {
                            let name = CoTaskMemPWSTR::from(name.as_str());
                            settings2.SetPrinterName(*name.as_ref().as_pcwstr())?;
                        }
                        settings2.SetCopies(copies)?;
                        let done = tx.clone();
                        let handler =
                            PrintCompletedHandler::create(Box::new(move |result, status| {
                                let outcome = match (result, status) {
                                    (Ok(()), COREWEBVIEW2_PRINT_STATUS_SUCCEEDED) => {
                                        Ok(PrintStatus::Printed)
                                    }
                                    (Ok(()), COREWEBVIEW2_PRINT_STATUS_PRINTER_UNAVAILABLE) => {
                                        Err(AppError::conflict("printer_unavailable"))
                                    }
                                    _ => Err(AppError::conflict("print_failed")),
                                };
                                let _ = done.send(outcome);
                                Ok(())
                            }));
                        core16.Print(&settings, &handler)?;
                        Ok(true)
                    })()
                };
                match started {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = tx.send(Ok(PrintStatus::Dialog));
                    }
                    Err(error) => {
                        tracing::error!(%error, "WebView2 could not start printing");
                        let _ = tx.send(Err(AppError::conflict("print_failed")));
                    }
                }
            })
            .map_err(|_| AppError::conflict("print_host_unavailable"))?;
        match rx.recv_timeout(PRINT_WAIT) {
            Ok(Ok(PrintStatus::Dialog)) => {
                // The dialog belongs to the page: a hidden window would hide it too.
                let _ = window.show();
                let _ = window.set_focus();
                window
                    .print()
                    .map_err(|_| AppError::conflict("print_failed"))?;
                Ok(PrintStatus::Dialog)
            }
            Ok(outcome) => outcome,
            // Still in flight, or lost: its outcome is unknown (the surface closes the window).
            Err(_) => Err(AppError::conflict(crate::print_host::PRINT_TIMEOUT)),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, setup, printer, copies);
        Err(AppError::conflict("print_host_unavailable"))
    }
}

/// Writes the host's page as a PDF at `path`.
pub fn print_to_pdf(
    window: &tauri::WebviewWindow,
    setup: &PageSetup,
    path: &Path,
) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_7, ICoreWebView2Environment6,
        };
        use webview2_com::{CoTaskMemPWSTR, PrintToPdfCompletedHandler};
        use windows_core::Interface;
        let (tx, rx) = mpsc::channel::<bool>();
        let (setup, target) = (*setup, path.to_string_lossy().into_owned());
        window
            .with_webview(move |webview| {
                // SAFETY: Tauri runs this closure on the thread that owns the WebView2 controller;
                // every call returns an HRESULT, checked by `?`.
                let started = unsafe {
                    (|| -> windows_core::Result<()> {
                        let core7 = webview
                            .controller()
                            .CoreWebView2()?
                            .cast::<ICoreWebView2_7>()?;
                        let settings = webview
                            .environment()
                            .cast::<ICoreWebView2Environment6>()?
                            .CreatePrintSettings()?;
                        com::apply(&settings, &setup)?;
                        let file = CoTaskMemPWSTR::from(target.as_str());
                        let done = tx.clone();
                        let handler =
                            PrintToPdfCompletedHandler::create(Box::new(move |result, ok| {
                                let _ = done.send(result.is_ok() && ok);
                                Ok(())
                            }));
                        core7.PrintToPdf(*file.as_ref().as_pcwstr(), &settings, &handler)
                    })()
                };
                if let Err(error) = started {
                    tracing::error!(%error, "WebView2 could not start PrintToPdf");
                    let _ = tx.send(false);
                }
            })
            .map_err(|_| AppError::conflict("print_host_unavailable"))?;
        match rx.recv_timeout(PRINT_WAIT) {
            Ok(true) => Ok(()),
            Ok(false) => Err(AppError::conflict("pdf_failed")),
            Err(_) => Err(AppError::conflict(crate::print_host::PRINT_TIMEOUT)),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, setup, path);
        Err(AppError::conflict("print_host_unavailable"))
    }
}
