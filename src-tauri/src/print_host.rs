//! The print host (spec §4.1, §9.2): a hidden `print-host` window renders the React template of
//! a job (`print-host.html`), then WebView2 prints it silently or writes a PDF. The page never
//! chooses what to print: it pulls the one job Rust offers (`print_host_next`) and says when it
//! has rendered (`print_host_rendered`). One job at a time; a job that does not render in time
//! frees the host; the window closes after ten idle minutes (RAM budget, spec §2).
//!
//! The protocol is document-agnostic: a job is a page (a template and its model) plus a page
//! setup, copies and a target printer or PDF path. A new kind of page (the M4 A4 label sheets)
//! is one more [`HostPage`] variant and its template (`src/documents/page.tsx`).

use pos_core::billing::settings::Paper;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PRINT_HOST: &str = "print-host";
const RENDER_WAIT: Duration = Duration::from_secs(20);
const FIRST_RENDER_WAIT: Duration = Duration::from_secs(40);
const IDLE_CLOSE: Duration = Duration::from_secs(600);

/// What the page renders: the template (`template`) and its print model (`model`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "template", content = "model", rename_all = "snake_case")]
pub enum HostPage {
    Document(Box<pos_docs::a4::document::DocumentPrintModel>),
    Receipt(Box<pos_docs::a4::receipt::ReceiptPage>),
    Statement(Box<pos_docs::a4::statement::StatementA4>),
    Table(Box<pos_docs::a4::table::TablePrint>),
    Warranty(Box<pos_docs::a4::warranty::WarrantyPrint>),
    ArchiveSummary(Box<pos_docs::fiscal_archive::ArchiveSummary>),
    /// An A4 planche of labels (LBL-013), cells placed from the paper's edges.
    LabelSheet(Box<crate::commands::label_sheets::LabelSheet>),
    /// The instalment contract (TMP-001, TMP-009).
    Contract(Box<pos_docs::a4::packs::ContractPrint>),
    /// The bon de route of a delivery round (VRT-100, DOC-030).
    RouteSheet(Box<pos_docs::a4::packs::RouteSheetPrint>),
    /// The indicative zakat statement (FIN-007).
    ZakatStatement(Box<pos_docs::a4::packs::ZakatStatementPrint>),
    /// The record of processing activities with the DPO (PDP-005, loi 18-07).
    ProcessingRegister(Box<pos_docs::a4::packs::ProcessingRegisterPrint>),
    /// HW-002: a receipt on the roll of a printer that only takes pages through its Windows
    /// driver ([`PageSetup::roll`]).
    ReceiptRoll(Box<ReceiptRollPage>),
}

/// A receipt as a page of a 58 or 80 mm roll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptRollPage {
    pub page: pos_docs::a4::receipt::ReceiptPage,
    pub width_mm: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HostJob {
    pub job_id: String,
    pub lang: Lang,
    pub page: HostPage,
    /// The job's `@page` margin, the same as WebView2's print settings: whichever of the two
    /// Chromium honours, the page is the same, and the `@page` margin boxes (page numbers) always
    /// have their room.
    pub margin_mm: u32,
    /// The job's `@page` size (`[width, height]`, landscape already turned), the same as the
    /// print settings; `None` for a receipt, whose length is known once rendered.
    pub size_mm: Option<[u32; 2]>,
    /// Finding 39: "DÉMO – sans valeur fiscale" in the page's language when the demo shop is
    /// open; the print host repeats it on every page of every A4/A5 document, receipt page,
    /// statement, warranty, table and label sheet.
    pub demo_mark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub ok: bool,
    #[serde(default)]
    #[specta(optional)]
    pub error: Option<String>,
    /// The rendered height, for receipt pages sized to their content.
    pub height_px: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rendered {
    pub height_px: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSetup {
    pub width_mm: u32,
    pub height_mm: u32,
    pub margin_mm: u32,
    pub landscape: bool,
}

impl PageSetup {
    pub fn a4(margin_mm: u32) -> PageSetup {
        PageSetup {
            width_mm: 210,
            height_mm: 297,
            margin_mm,
            landscape: false,
        }
    }

    pub fn a5(margin_mm: u32) -> PageSetup {
        PageSetup {
            width_mm: 148,
            height_mm: 210,
            margin_mm,
            landscape: false,
        }
    }

    pub fn for_paper(paper: Paper, margin_mm: u32) -> PageSetup {
        match paper {
            Paper::A4 => PageSetup::a4(margin_mm),
            Paper::A5 => PageSetup::a5(margin_mm),
        }
    }

    /// An 80 mm page as long as the rendered receipt (96 px per inch, rounded up to the mm; at
    /// least 100 mm), plus 2 mm of slack and its 3 mm margins top and bottom: the receipt never
    /// runs onto a second page.
    pub fn receipt(height_px: u32) -> PageSetup {
        let content = (u64::from(height_px) * 254).div_ceil(960);
        PageSetup {
            width_mm: 80,
            height_mm: u32::try_from(content).unwrap_or(2_000).max(100) + 2 + 6,
            margin_mm: 3,
            landscape: false,
        }
    }

    /// HW-002: a page as wide as the `width_mm` roll (58 or 80 mm) and as long as the rendered
    /// receipt (96 px per inch, rounded up to the mm), plus 2 mm of slack and its 2 mm margins
    /// top and bottom: the printer's driver feeds and cuts it like one ticket.
    pub fn roll(width_mm: u16, height_px: u32) -> PageSetup {
        let content = (u64::from(height_px) * 254).div_ceil(960);
        PageSetup {
            width_mm: u32::from(width_mm),
            height_mm: u32::try_from(content).unwrap_or(2_000).max(30) + 2 + 4,
            margin_mm: 2,
            landscape: false,
        }
    }

    /// The `@page` size of the job: `None` for a receipt (sized once rendered), else the sheet
    /// as printed (landscape turned).
    fn css_size(&self, receipt: bool) -> Option<[u32; 2]> {
        match (receipt, self.landscape) {
            (true, _) => None,
            (false, false) => Some([self.width_mm, self.height_mm]),
            (false, true) => Some([self.height_mm, self.width_mm]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PrintStatus {
    Printed,
    /// The WebView2 runtime has no silent print: the print dialog was shown instead.
    Dialog,
}

/// Wakes the print-host page: a job is waiting (`print_host_next`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct PrintHostWake {
    pub job_id: String,
}

/// The job slot the page pulls from and the waiters of rendered jobs.
#[derive(Clone, Default)]
pub struct HostChannel {
    pending: Arc<Mutex<Option<HostJob>>>,
    waiters: Arc<Mutex<HashMap<String, SyncSender<RenderResult>>>>,
}

impl HostChannel {
    pub fn new() -> HostChannel {
        HostChannel::default()
    }

    pub fn offer(&self, job: HostJob) -> Receiver<RenderResult> {
        let (tx, rx) = mpsc::sync_channel(1);
        if let Ok(mut waiters) = self.waiters.lock() {
            waiters.insert(job.job_id.clone(), tx);
        }
        if let Ok(mut pending) = self.pending.lock() {
            *pending = Some(job);
        }
        rx
    }

    pub fn take(&self) -> Option<HostJob> {
        self.pending.lock().ok().and_then(|mut p| p.take())
    }

    /// `false` for a job nobody waits for any more (it timed out).
    pub fn rendered(&self, job_id: &str, result: RenderResult) -> bool {
        let waiter = self.waiters.lock().ok().and_then(|mut w| w.remove(job_id));
        waiter.is_some_and(|tx| tx.send(result).is_ok())
    }

    pub fn abandon(&self, job_id: &str) {
        if let Ok(mut w) = self.waiters.lock() {
            w.remove(job_id);
        }
        if let Ok(mut p) = self.pending.lock()
            && p.as_ref().is_some_and(|j| j.job_id == job_id)
        {
            *p = None;
        }
    }
}

/// Offers `job`, wakes the page, waits for its rendering.
pub fn await_render(
    channel: &HostChannel,
    job: HostJob,
    wake: impl FnOnce(&str) -> Result<(), AppError>,
    timeout: Duration,
) -> Result<Rendered, AppError> {
    let job_id = job.job_id.clone();
    let rx = channel.offer(job);
    if let Err(e) = wake(&job_id) {
        channel.abandon(&job_id);
        return Err(e);
    }
    match rx.recv_timeout(timeout) {
        Ok(r) if r.ok => Ok(Rendered {
            height_px: r.height_px,
        }),
        Ok(r) => {
            tracing::error!(error = ?r.error, "a document template failed to render");
            Err(AppError::conflict("print_render_failed"))
        }
        Err(_) => {
            channel.abandon(&job_id);
            Err(AppError::conflict("print_render_timeout"))
        }
    }
}

/// Where a job renders and prints: the hidden WebView2 window, or a stand-in in tests.
pub trait PrintSurface: Send + Sync {
    fn render(&self, job: HostJob) -> Result<Rendered, AppError>;
    fn print(
        &self,
        setup: &PageSetup,
        printer: Option<&str>,
        copies: u32,
    ) -> Result<PrintStatus, AppError>;
    fn pdf(&self, setup: &PageSetup, path: &Path) -> Result<(), AppError>;
    fn close_if_idle(&self, idle: Duration);
}

pub struct PrintHost {
    surface: Arc<dyn PrintSurface>,
    channel: Arc<HostChannel>,
    lock: Mutex<()>,
    /// The demo shop is open (`AppPaths::demo`, set once the data folder is known).
    demo: AtomicBool,
}

impl PrintHost {
    pub fn new(surface: Arc<dyn PrintSurface>, channel: Arc<HostChannel>) -> PrintHost {
        PrintHost {
            surface,
            channel,
            lock: Mutex::new(()),
            demo: AtomicBool::new(false),
        }
    }

    /// Every page printed from here on carries the demo's mark (or no longer does).
    pub fn set_demo(&self, demo: bool) {
        self.demo.store(demo, Ordering::SeqCst);
    }

    pub fn channel(&self) -> &HostChannel {
        &self.channel
    }

    /// The job of `page`, with the `@page` rule of its setup: the margin (known before the
    /// rendering, a receipt's too) and the size of a sheet.
    fn job(&self, page: HostPage, lang: Lang, setup: &impl Fn(u32) -> PageSetup) -> HostJob {
        let before = setup(0);
        let receipt = matches!(page, HostPage::Receipt(_) | HostPage::ReceiptRoll(_));
        HostJob {
            job_id: pos_core::ids::new_id(),
            lang,
            margin_mm: before.margin_mm,
            size_mm: before.css_size(receipt),
            page,
            demo_mark: self
                .demo
                .load(Ordering::SeqCst)
                .then(|| pos_docs::receipt::label(lang, "receipt.demo")),
        }
    }

    fn lang_of(page: &HostPage) -> Lang {
        match page {
            HostPage::Document(m) => m.lang,
            HostPage::Receipt(r) => r.receipt.lang,
            HostPage::Statement(s) => s.lang,
            HostPage::Table(t) => t.lang,
            HostPage::Warranty(w) => w.lang,
            HostPage::ArchiveSummary(a) => a.lang,
            HostPage::LabelSheet(s) => s.labels.first().map_or(Lang::Fr, |l| l.lang),
            HostPage::Contract(c) => c.lang,
            HostPage::RouteSheet(r) => r.lang,
            HostPage::ZakatStatement(z) => z.lang,
            HostPage::ProcessingRegister(r) => r.lang,
            HostPage::ReceiptRoll(r) => r.page.receipt.lang,
        }
    }

    /// Renders `page` then prints it; `setup` receives the rendered height (receipts).
    pub fn print(
        &self,
        page: HostPage,
        setup: impl Fn(u32) -> PageSetup,
        printer: Option<&str>,
        copies: u32,
    ) -> Result<PrintStatus, AppError> {
        let _one = self
            .lock
            .lock()
            .map_err(|_| AppError::Internal("print host lock poisoned".into()))?;
        let lang = Self::lang_of(&page);
        let rendered = self.surface.render(self.job(page, lang, &setup))?;
        self.surface
            .print(&setup(rendered.height_px), printer, copies)
    }

    /// Renders `page` then writes it as a PDF at `path`.
    pub fn pdf(
        &self,
        page: HostPage,
        setup: impl Fn(u32) -> PageSetup,
        path: &Path,
    ) -> Result<(), AppError> {
        let _one = self
            .lock
            .lock()
            .map_err(|_| AppError::Internal("print host lock poisoned".into()))?;
        let lang = Self::lang_of(&page);
        let rendered = self.surface.render(self.job(page, lang, &setup))?;
        self.surface.pdf(&setup(rendered.height_px), path)
    }

    /// Closes the hidden window after ten idle minutes, when no job runs.
    pub fn start_idle_closer(self: &Arc<Self>) {
        let host = Arc::clone(self);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(60));
                if let Ok(_free) = host.lock.try_lock() {
                    host.surface.close_if_idle(IDLE_CLOSE);
                }
            }
        });
    }
}

/// The real surface: the hidden WebView2 window.
pub struct WebviewSurface {
    app: tauri::AppHandle,
    channel: Arc<HostChannel>,
    last_used: Mutex<Instant>,
}

impl WebviewSurface {
    pub fn new(app: tauri::AppHandle, channel: Arc<HostChannel>) -> WebviewSurface {
        WebviewSurface {
            app,
            channel,
            last_used: Mutex::new(Instant::now()),
        }
    }

    fn touch(&self) {
        if let Ok(mut t) = self.last_used.lock() {
            *t = Instant::now();
        }
    }

    /// The hidden window, opened when it is not (`true` when it was just created).
    fn window(&self) -> Result<(tauri::WebviewWindow, bool), AppError> {
        use tauri::Manager;
        if let Some(w) = self.app.get_webview_window(PRINT_HOST) {
            return Ok((w, false));
        }
        tauri::WebviewWindowBuilder::new(
            &self.app,
            PRINT_HOST,
            tauri::WebviewUrl::App("print-host.html".into()),
        )
        .title("DZBuild POS")
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .inner_size(794.0, 1_123.0)
        .build()
        .map(|w| (w, true))
        .map_err(|error| {
            tracing::error!(%error, "could not open the print host window");
            AppError::conflict("print_host_unavailable")
        })
    }

    fn existing(&self) -> Result<tauri::WebviewWindow, AppError> {
        use tauri::Manager;
        self.app
            .get_webview_window(PRINT_HOST)
            .ok_or_else(|| AppError::conflict("print_host_unavailable"))
    }

    /// A print or a PDF that never answered (`print_timeout`): the window goes with it, still under
    /// the host's lock, which cancels what WebView2 had in flight; the next job gets a fresh page
    /// and never prints this job's DOM on this job's printer.
    fn drop_window_on_timeout<T>(&self, outcome: Result<T, AppError>) -> Result<T, AppError> {
        if outcome.as_ref().is_err_and(|e| e.code() == PRINT_TIMEOUT)
            && let Ok(w) = self.existing()
            && let Err(error) = w.destroy()
        {
            tracing::error!(%error, "could not close the print host after a timeout");
        }
        outcome
    }
}

/// The code of a print or PDF that WebView2 never answered: its outcome is unknown.
pub const PRINT_TIMEOUT: &str = "print_timeout";

impl PrintSurface for WebviewSurface {
    fn render(&self, job: HostJob) -> Result<Rendered, AppError> {
        self.touch();
        let (_, created) = self.window()?;
        let app = self.app.clone();
        let wait = if created {
            FIRST_RENDER_WAIT
        } else {
            RENDER_WAIT
        };
        await_render(
            &self.channel,
            job,
            move |job_id| {
                use tauri_specta::Event;
                PrintHostWake {
                    job_id: job_id.to_owned(),
                }
                .emit_to(&app, PRINT_HOST)
                .map_err(|_| AppError::conflict("print_host_unavailable"))
            },
            wait,
        )
    }

    fn print(
        &self,
        setup: &PageSetup,
        printer: Option<&str>,
        copies: u32,
    ) -> Result<PrintStatus, AppError> {
        self.touch();
        let outcome = crate::webview_print::print(&self.existing()?, setup, printer, copies);
        self.drop_window_on_timeout(outcome)
    }

    fn pdf(&self, setup: &PageSetup, path: &Path) -> Result<(), AppError> {
        self.touch();
        let outcome = crate::webview_print::print_to_pdf(&self.existing()?, setup, path);
        self.drop_window_on_timeout(outcome)
    }

    fn close_if_idle(&self, idle: Duration) {
        let stale = self
            .last_used
            .lock()
            .map(|t| t.elapsed() >= idle)
            .unwrap_or(false);
        if stale && let Ok(w) = self.existing() {
            let _ = w.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    fn job(id: &str) -> HostJob {
        HostJob {
            job_id: id.into(),
            lang: pos_core::lang::Lang::Fr,
            page: HostPage::ArchiveSummary(Box::new(sample_summary())),
            margin_mm: 12,
            size_mm: Some([210, 297]),
            demo_mark: None,
        }
    }

    #[test]
    fn the_page_gets_the_pending_job_once() {
        let channel = HostChannel::new();
        let _rx = channel.offer(job("j1"));
        assert_eq!(channel.take().map(|j| j.job_id), Some("j1".into()));
        assert!(channel.take().is_none());
    }

    #[test]
    fn a_render_timeout_frees_the_host_and_reports_print_render_timeout() {
        let channel = HostChannel::new();
        let err =
            await_render(&channel, job("slow"), |_| Ok(()), Duration::from_millis(50)).unwrap_err();
        assert_eq!(err.code(), "print_render_timeout");
        assert!(
            !channel.rendered(
                "slow",
                RenderResult {
                    ok: true,
                    error: None,
                    height_px: 10
                }
            ),
            "an abandoned job is forgotten"
        );
        let helper = {
            let channel = channel.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    if let Some(j) = channel.take() {
                        channel.rendered(
                            &j.job_id,
                            RenderResult {
                                ok: true,
                                error: None,
                                height_px: 1_123,
                            },
                        );
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
        };
        let done = await_render(&channel, job("next"), |_| Ok(()), Duration::from_secs(5)).unwrap();
        helper.join().unwrap();
        assert_eq!(done.height_px, 1_123);
        let err = {
            let channel2 = channel.clone();
            let failing = std::thread::spawn(move || {
                for _ in 0..100 {
                    if let Some(j) = channel2.take() {
                        channel2.rendered(
                            &j.job_id,
                            RenderResult {
                                ok: false,
                                error: Some("TypeError".into()),
                                height_px: 0,
                            },
                        );
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            let e = await_render(&channel, job("broken"), |_| Ok(()), Duration::from_secs(5))
                .unwrap_err();
            failing.join().unwrap();
            e
        };
        assert_eq!(err.code(), "print_render_failed");
    }

    struct Slow {
        in_flight: AtomicUsize,
        max: AtomicUsize,
    }

    impl PrintSurface for Slow {
        fn render(&self, _job: HostJob) -> Result<Rendered, AppError> {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(30));
            Ok(Rendered { height_px: 100 })
        }
        fn print(
            &self,
            _setup: &PageSetup,
            _printer: Option<&str>,
            _copies: u32,
        ) -> Result<PrintStatus, AppError> {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(PrintStatus::Printed)
        }
        fn pdf(&self, _setup: &PageSetup, _path: &std::path::Path) -> Result<(), AppError> {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }
        fn close_if_idle(&self, _idle: Duration) {}
    }

    #[test]
    fn jobs_run_one_at_a_time() {
        let surface = Arc::new(Slow {
            in_flight: AtomicUsize::new(0),
            max: AtomicUsize::new(0),
        });
        let host = Arc::new(PrintHost::new(
            surface.clone(),
            Arc::new(HostChannel::new()),
        ));
        let workers: Vec<_> = (0..4)
            .map(|i| {
                let host = host.clone();
                std::thread::spawn(move || {
                    host.print(job(&format!("j{i}")).page, |_| PageSetup::a4(12), None, 1)
                        .unwrap()
                })
            })
            .collect();
        for w in workers {
            assert_eq!(w.join().unwrap(), PrintStatus::Printed);
        }
        assert_eq!(surface.max.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn page_setups_follow_the_paper_and_the_receipt_height() {
        assert_eq!(
            PageSetup::for_paper(Paper::A4, 12),
            PageSetup {
                width_mm: 210,
                height_mm: 297,
                margin_mm: 12,
                landscape: false
            }
        );
        assert_eq!(PageSetup::for_paper(Paper::A5, 8).width_mm, 148);
        let r = PageSetup::receipt(1_890);
        assert_eq!(
            (r.width_mm, r.height_mm, r.margin_mm),
            (80, 509, 3),
            "1 890 px at 96 dpi is 500.06 mm, rounded up, plus 2 mm of slack and 6 mm of margins"
        );
        assert_eq!(
            PageSetup::receipt(3_780).height_mm,
            1_000 + 1 + 2 + 6,
            "never a fraction of a mm short of the content"
        );
    }

    /// HW-002: a receipt through a Windows driver is a page as wide as the roll and as long as
    /// the ticket, with 2 mm margins.
    #[test]
    fn a_roll_page_is_as_wide_as_the_roll_and_as_long_as_the_ticket() {
        let r = PageSetup::roll(58, 1_890);
        assert_eq!(
            (r.width_mm, r.height_mm, r.margin_mm, r.landscape),
            (58, 501 + 2 + 4, 2, false),
            "1 890 px at 96 dpi is 500.06 mm, rounded up, plus 2 mm of slack and 4 mm of margins"
        );
        assert_eq!(PageSetup::roll(80, 0).height_mm, 30 + 2 + 4);
        assert_eq!(PageSetup::roll(80, 0).css_size(true), None);
    }

    fn idle_host() -> PrintHost {
        PrintHost::new(
            Arc::new(Slow {
                in_flight: AtomicUsize::new(0),
                max: AtomicUsize::new(0),
            }),
            Arc::new(HostChannel::new()),
        )
    }

    /// Finding 39: once the demo shop is open, every A4 job carries its mark in the page's
    /// language (the print host repeats it on every page).
    #[test]
    fn a_job_of_the_demo_shop_carries_its_mark() {
        let host = idle_host();
        let setup = |_: u32| PageSetup::a4(12);
        assert_eq!(host.job(job("a").page, Lang::Fr, &setup).demo_mark, None);
        host.set_demo(true);
        assert_eq!(
            host.job(job("b").page, Lang::Fr, &setup)
                .demo_mark
                .as_deref(),
            Some("DÉMO – sans valeur fiscale")
        );
        assert_eq!(
            host.job(job("c").page, Lang::Ar, &setup)
                .demo_mark
                .as_deref(),
            Some("تجريبي – بدون قيمة جبائية")
        );
    }

    #[test]
    fn every_job_carries_the_page_rule_of_its_setup() {
        let host = idle_host();
        let a4 = host.job(job("a").page, pos_core::lang::Lang::Fr, &|_| {
            PageSetup::a4(12)
        });
        assert_eq!((a4.margin_mm, a4.size_mm), (12, Some([210, 297])));
        let landscape = host.job(job("b").page, pos_core::lang::Lang::Fr, &|_| PageSetup {
            landscape: true,
            ..PageSetup::a5(8)
        });
        assert_eq!(
            (landscape.margin_mm, landscape.size_mm),
            (8, Some([210, 148]))
        );
        let receipt = PageSetup::receipt(0);
        assert_eq!(
            (receipt.margin_mm, receipt.css_size(true)),
            (3, None),
            "a receipt's length is known once rendered"
        );
    }

    fn sample_summary() -> pos_docs::fiscal_archive::ArchiveSummary {
        pos_docs::fiscal_archive::build_summary(
            &pos_core::fiscal::archive::ChainSummary {
                documents: Vec::new(),
                audit: pos_core::audit::ChainReport {
                    rows: 0,
                    first_broken_seq: None,
                    last_seq: 0,
                    last_hash: pos_core::audit::GENESIS_HASH.into(),
                },
                closures: Vec::new(),
                receipts: Default::default(),
                till_journals: Vec::new(),
                till_merge_first_broken: None,
            },
            &[],
            pos_docs::fiscal_archive::ArchiveCompany {
                name: "Supérette El Amel".into(),
                nif: None,
                nis: None,
                rc: None,
            },
            "2026-01-01",
            "2026-12-31",
            pos_core::lang::Lang::Fr,
            0,
        )
    }
}
