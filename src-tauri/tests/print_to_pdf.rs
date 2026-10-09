//! One real WebView2 `PrintToPdf` (Windows with the WebView2 runtime, a desktop session):
//! `cargo test -p dzbuild-pos --test print_to_pdf -- --ignored`. Ignored in CI, run by Task 25.
#![cfg(windows)]

use dzbuild_pos_lib::print_host::PageSetup;
use dzbuild_pos_lib::webview_print::print_to_pdf;
use std::sync::mpsc;
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindowBuilder};

#[test]
#[ignore = "needs a desktop session and the WebView2 runtime"]
fn print_to_pdf_writes_a_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("FA2026-000045.pdf");
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let (tx, rx) = mpsc::channel();
    let target = path.clone();
    let app = tauri::Builder::default()
        .any_thread()
        .setup(move |app| {
            let window = WebviewWindowBuilder::new(
                app,
                "print-host",
                WebviewUrl::External("about:blank".parse()?),
            )
            .visible(false)
            .build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                let _ = window.eval("document.documentElement.dir = 'rtl'; document.body.innerHTML = '<h1>فاتورة FA2026-000045</h1><p>Net à payer : 23 449,00 DA</p>';");
                std::thread::sleep(Duration::from_secs(1));
                let result = print_to_pdf(&window, &PageSetup::a4(12), &target).map_err(|e| e.code());
                let _ = tx.send(result);
                handle.exit(0);
            });
            Ok(())
        })
        .build(context)
        .unwrap();
    app.run_return(|_, _| {});
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Ok(()));
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF-"), "a PDF file");
    assert!(bytes.len() > 1_000);
}
