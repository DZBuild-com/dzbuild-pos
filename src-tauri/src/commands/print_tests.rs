//! Printing and PDF through a fake print host: originals and duplicates, per-PC printers, the
//! journal, logos, receipt pages, and the fiscal archive into a temp folder.

use super::print;
use crate::commands::testshop::shop;
use crate::print_host::{
    HostChannel, HostJob, HostPage, PageSetup, PrintHost, PrintStatus, PrintSurface, Rendered,
};
use pos_core::billing::print::PrintOptions;
use pos_core::error::AppError;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One job on paper: the page printed, the printer, the copies and the page width in mm.
type Job = (String, Option<String>, u32, u32);

#[derive(Default)]
struct Paper {
    jobs: Mutex<Vec<Job>>,
    last_page: Mutex<Option<String>>,
}

impl PrintSurface for Paper {
    fn render(&self, job: HostJob) -> Result<Rendered, AppError> {
        let template = match &job.page {
            HostPage::Document(m) => format!(
                "document:{}",
                m.duplicate.clone().unwrap_or_else(|| "original".into())
            ),
            HostPage::Receipt(_) => "receipt".into(),
            other => format!("{other:?}").chars().take(12).collect(),
        };
        *self.last_page.lock().unwrap() = Some(template);
        Ok(Rendered { height_px: 1_890 })
    }
    fn print(
        &self,
        setup: &PageSetup,
        printer: Option<&str>,
        copies: u32,
    ) -> Result<PrintStatus, AppError> {
        let page = self.last_page.lock().unwrap().clone().unwrap_or_default();
        self.jobs
            .lock()
            .unwrap()
            .push((page, printer.map(str::to_owned), copies, setup.width_mm));
        Ok(PrintStatus::Printed)
    }
    fn pdf(&self, _setup: &PageSetup, path: &Path) -> Result<(), AppError> {
        std::fs::write(path, b"%PDF-1.7 fake").map_err(|e| AppError::Io(e.to_string()))
    }
    fn close_if_idle(&self, _: Duration) {}
}

#[test]
fn the_first_output_is_the_original_and_every_next_one_a_journaled_duplicata() {
    let t = shop();
    let invoice = t.validated_invoice();
    let paper = Arc::new(Paper::default());
    let host = PrintHost::new(paper.clone(), Arc::new(HostChannel::new()));
    t.set_document_printer(pos_core::documents::DocType::Invoice, "HP LaserJet M110", 2);
    let first = print::print_document(
        &t.st,
        &host,
        print::PrintDocumentInput {
            id: invoice.clone(),
            options: PrintOptions::default(),
            printer: None,
            copies: None,
        },
    )
    .unwrap();
    assert!(!first.duplicate);
    let second = print::print_document(
        &t.st,
        &host,
        print::PrintDocumentInput {
            id: invoice.clone(),
            options: PrintOptions::default(),
            printer: None,
            copies: Some(1),
        },
    )
    .unwrap();
    assert!(second.duplicate);
    let jobs = paper.jobs.lock().unwrap().clone();
    assert_eq!(
        jobs[0],
        (
            "document:original".into(),
            Some("HP LaserJet M110".into()),
            2,
            210
        )
    );
    assert_eq!(jobs[1].0, "document:DUPLICATA");
    assert_eq!(jobs[1].2, 1, "the dialog's copies win");
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action = 'document.printed'"),
        1
    );
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action = 'document.reprinted'"),
        1
    );
    let dir = tempfile::tempdir().unwrap();
    let pdf = dir.path().join("facture.pdf");
    let saved = print::export_pdf_to(
        &t.st,
        &host,
        print::PdfDocumentInput {
            id: invoice,
            options: PrintOptions::default(),
        },
        &pdf,
    )
    .unwrap();
    assert!(saved.duplicate);
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action = 'document.exported'"),
        1
    );
}

#[test]
fn a_ticket_prints_as_an_80_mm_duplicate_page() {
    let t = shop();
    let ticket = t.cash_ticket();
    let paper = Arc::new(Paper::default());
    let host = PrintHost::new(paper.clone(), Arc::new(HostChannel::new()));
    let out = print::print_document(
        &t.st,
        &host,
        print::PrintDocumentInput {
            id: ticket,
            options: PrintOptions::default(),
            printer: None,
            copies: None,
        },
    )
    .unwrap();
    assert!(out.duplicate);
    let job = paper.jobs.lock().unwrap()[0].clone();
    assert_eq!((job.0.as_str(), job.3), ("receipt", 80));
}

#[test]
fn a_failed_print_journals_nothing() {
    struct Jammed;
    impl PrintSurface for Jammed {
        fn render(&self, _: HostJob) -> Result<Rendered, AppError> {
            Ok(Rendered { height_px: 1 })
        }
        fn print(&self, _: &PageSetup, _: Option<&str>, _: u32) -> Result<PrintStatus, AppError> {
            Err(AppError::conflict("printer_unavailable"))
        }
        fn pdf(&self, _: &PageSetup, _: &Path) -> Result<(), AppError> {
            Err(AppError::conflict("pdf_failed"))
        }
        fn close_if_idle(&self, _: Duration) {}
    }
    let t = shop();
    let invoice = t.validated_invoice();
    let host = PrintHost::new(Arc::new(Jammed), Arc::new(HostChannel::new()));
    let err = print::print_document(
        &t.st,
        &host,
        print::PrintDocumentInput {
            id: invoice.clone(),
            options: PrintOptions::default(),
            printer: None,
            copies: None,
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "printer_unavailable");
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action LIKE 'document.%print%'"),
        0,
        "the next print is still the original"
    );
}

#[test]
fn a_print_that_never_answers_counts_and_the_next_copy_is_a_duplicata() {
    struct Lost;
    impl PrintSurface for Lost {
        fn render(&self, _: HostJob) -> Result<Rendered, AppError> {
            Ok(Rendered { height_px: 1 })
        }
        fn print(&self, _: &PageSetup, _: Option<&str>, _: u32) -> Result<PrintStatus, AppError> {
            Err(AppError::conflict(crate::print_host::PRINT_TIMEOUT))
        }
        fn pdf(&self, _: &PageSetup, _: &Path) -> Result<(), AppError> {
            Err(AppError::conflict(crate::print_host::PRINT_TIMEOUT))
        }
        fn close_if_idle(&self, _: Duration) {}
    }
    let t = shop();
    let invoice = t.validated_invoice();
    let lost = PrintHost::new(Arc::new(Lost), Arc::new(HostChannel::new()));
    let input = || print::PrintDocumentInput {
        id: invoice.clone(),
        options: PrintOptions::default(),
        printer: None,
        copies: None,
    };
    let err = print::print_document(&t.st, &lost, input()).unwrap_err();
    assert_eq!(err.code(), "print_timeout");
    assert_eq!(
        t.count(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'document.printed'
             AND payload LIKE '%\"timeout\":true%'"
        ),
        1,
        "the original may come out yet: it is on record"
    );
    let paper = Arc::new(Paper::default());
    let host = PrintHost::new(paper.clone(), Arc::new(HostChannel::new()));
    let next = print::print_document(&t.st, &host, input()).unwrap();
    assert!(next.duplicate, "never a second original");
    assert_eq!(paper.jobs.lock().unwrap()[0].0, "document:DUPLICATA");
    let dir = tempfile::tempdir().unwrap();
    let err = print::export_pdf_to(
        &t.st,
        &lost,
        print::PdfDocumentInput {
            id: invoice.clone(),
            options: PrintOptions::default(),
        },
        &dir.path().join("facture.pdf"),
    )
    .unwrap_err();
    assert_eq!(err.code(), "print_timeout");
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action = 'document.exported'"),
        1
    );
}

#[test]
fn the_fiscal_archive_is_written_verified_and_anchored() {
    let t = shop();
    t.validated_invoice();
    let host = PrintHost::new(Arc::new(Paper::default()), Arc::new(HostChannel::new()));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("archive.zip");
    let stages = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&stages);
    let done = super::fiscal::export_archive_to(
        &t.st,
        &host,
        {
            let year = pos_core::time::business_date(pos_core::time::now_ms())
                .format("%Y")
                .to_string();
            super::fiscal::ArchiveInput {
                from: format!("{year}-01-01"),
                to: format!("{year}-12-31"),
                lang: pos_core::lang::Lang::Fr,
            }
        },
        &path,
        move |p| seen.lock().unwrap().push(p.stage),
    )
    .unwrap();
    assert!(done.files.iter().any(|f| f.name == "summary.pdf"));
    assert!(
        done.files
            .iter()
            .any(|f| f.name == "documents.csv" && f.rows == Some(1))
    );
    assert_eq!(
        pos_docs::fiscal_archive::verify(&path)
            .unwrap()
            .bad_files
            .len(),
        0
    );
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action = 'fiscal.archive_exported'"),
        1
    );
    assert_eq!(
        stages.lock().unwrap().last(),
        Some(&super::fiscal::ArchiveStage::Manifest)
    );
    assert!(
        !dir.path().join("archive.zip.partial").exists(),
        "the archive was put in place"
    );
}
