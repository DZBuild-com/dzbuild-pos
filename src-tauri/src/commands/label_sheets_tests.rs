//! A4 label sheets (LBL-013): the plan of a partly used planche with the barcode modules the sheet
//! draws, the DOC-034 guard, and printing through the print host, journaled only once the host
//! printed the sheet or wrote its PDF.

use super::label_sheets::{self, SheetInput};
use super::testshop::{TestShop, shop};
use crate::print_host::{
    HostChannel, HostJob, HostPage, PageSetup, PrintHost, PrintStatus, PrintSurface, Rendered,
};
use pos_core::catalog::products::{Barcode, Product, ProductInput};
use pos_core::error::AppError;
use pos_core::labels::{LabelItem, LabelLang, LabelRequest, LabelSource};
use pos_core::money::{Money, Qty};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Candia milk with its Arabic name: the first-run shop (activity `general`) guards every
/// category, so its labels keep Arabic (DOC-034).
fn milk(shop: &TestShop) -> Product {
    let mut input = ProductInput::simple(
        "Lait Candia 1 L",
        pos_core::catalog::UNIT_PIECE_ID,
        &shop.rate_id(900),
        Money(15_000),
    );
    input.name_ar = "حليب كانديا 1 ل".into();
    input.barcodes = vec![Barcode {
        barcode: "6130123456782".into(),
        pack_qty: Qty(1_000),
        label: None,
    }];
    shop.product_with(input)
}

fn sheet_of(product_id: &str, copies: u16, start: u16, arabic_override: bool) -> SheetInput {
    SheetInput {
        request: LabelRequest {
            source: LabelSource::Products {
                items: vec![LabelItem {
                    product_id: product_id.into(),
                    copies,
                    lot_id: None,
                }],
            },
            template_id: None,
            lang: Some(LabelLang::ArFr),
        },
        format_id: "a4_24_70x37".into(),
        start,
        arabic_override,
    }
}

fn printed(shop: &TestShop) -> (i64, i64) {
    (
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'labels.printed'"),
        shop.count("SELECT COUNT(*) FROM label_prints"),
    )
}

#[test]
fn a_sheet_places_copies_from_the_start_position_with_barcode_rows() {
    let shop = shop();
    shop.owner();
    let milk = milk(&shop);
    let input = sheet_of(&milk.id, 3, 23, false);
    let sheet = label_sheets::sheet(&shop.st, input.clone()).unwrap();
    assert_eq!(
        (sheet.pages, sheet.cells.len(), sheet.labels.len()),
        (2, 3, 1)
    );
    assert_eq!(
        (
            sheet.cells[0].page,
            sheet.cells[0].x_dmm,
            sheet.cells[0].y_dmm
        ),
        (0, 700, 5 + 7 * 370)
    );
    assert_eq!(
        (
            sheet.cells[2].page,
            sheet.cells[2].x_dmm,
            sheet.cells[2].y_dmm
        ),
        (1, 0, 5)
    );
    let label = &sheet.labels[0];
    assert!(label.rtl);
    assert_eq!(label.name.as_deref(), Some("حليب كانديا 1 ل"));
    assert_eq!(label.name_second.as_deref(), Some("Lait Candia 1 L"));
    let barcode = label.barcode.as_ref().unwrap();
    assert_eq!((barcode.rows.len(), barcode.rows[0].len()), (1, 95));
    assert!(barcode.rows[0].starts_with("101"), "the EAN-13 start guard");
    assert_eq!(barcode.hri.as_deref(), Some("6130123456782"));
    let unknown = SheetInput {
        format_id: "a4_99".into(),
        ..input.clone()
    };
    assert_eq!(
        label_sheets::sheet(&shop.st, unknown)
            .unwrap_err()
            .payload()
            .field
            .as_deref(),
        Some("formatId")
    );
    let past_the_sheet = SheetInput { start: 25, ..input };
    assert_eq!(
        label_sheets::sheet(&shop.st, past_the_sheet)
            .unwrap_err()
            .payload()
            .field
            .as_deref(),
        Some("start")
    );
}

#[test]
fn a_food_product_without_its_arabic_name_needs_the_override_on_a_sheet_too() {
    let shop = shop();
    shop.owner();
    let bare = shop.product("Lait Candia 1 L", 15_000, 900, Some("6130123456782"));
    let refused = label_sheets::sheet(&shop.st, sheet_of(&bare.id, 1, 1, false)).unwrap_err();
    assert_eq!(refused.code(), "label_arabic_name_missing");
    let sheet = label_sheets::sheet(&shop.st, sheet_of(&bare.id, 1, 1, true)).unwrap();
    assert_eq!(sheet.cells.len(), 1);
    assert_eq!(
        sheet.labels[0].name.as_deref(),
        Some("Lait Candia 1 L"),
        "the French name stands in"
    );
}

/// The print host's WebView2, as a stand-in: it prints (or fails to) and keeps what it was given.
#[derive(Default)]
struct Paper {
    broken: AtomicBool,
    /// Each page rendered: a label sheet, and the language of the job.
    rendered: Mutex<Vec<(bool, pos_core::lang::Lang)>>,
    printed: Mutex<Vec<(Option<String>, u32, PageSetup)>>,
    pdfs: Mutex<Vec<PageSetup>>,
}

impl Paper {
    fn check(&self) -> Result<(), AppError> {
        if self.broken.load(Ordering::SeqCst) {
            Err(AppError::conflict("print_host_unavailable"))
        } else {
            Ok(())
        }
    }
}

impl PrintSurface for Paper {
    fn render(&self, job: HostJob) -> Result<Rendered, AppError> {
        self.check()?;
        let sheet = matches!(job.page, HostPage::LabelSheet(_));
        self.rendered.lock().unwrap().push((sheet, job.lang));
        Ok(Rendered { height_px: 1_123 })
    }
    fn print(
        &self,
        setup: &PageSetup,
        printer: Option<&str>,
        copies: u32,
    ) -> Result<PrintStatus, AppError> {
        self.check()?;
        self.printed
            .lock()
            .unwrap()
            .push((printer.map(str::to_owned), copies, *setup));
        Ok(PrintStatus::Printed)
    }
    fn pdf(&self, setup: &PageSetup, path: &Path) -> Result<(), AppError> {
        self.check()?;
        self.pdfs.lock().unwrap().push(*setup);
        std::fs::write(path, b"%PDF-1.7 fake").map_err(|e| AppError::Io(e.to_string()))
    }
    fn close_if_idle(&self, _: Duration) {}
}

#[test]
fn a_sheet_is_journaled_once_the_print_host_printed_it_and_never_before() {
    let shop = shop();
    shop.owner();
    let milk = milk(&shop);
    let mut config = shop.st.hw.config().unwrap();
    config.documents.default_printer = Some("HP LaserJet M110".into());
    shop.st.hw.save(config).unwrap();
    let paper = Arc::new(Paper::default());
    let host = PrintHost::new(paper.clone(), Arc::new(HostChannel::new()));

    let bare = shop.product("Yaourt Soummam", 3_000, 900, None);
    let guarded = label_sheets::print(&shop.st, &host, sheet_of(&bare.id, 1, 1, false));
    assert_eq!(guarded.unwrap_err().code(), "label_arabic_name_missing");
    assert!(
        paper.rendered.lock().unwrap().is_empty(),
        "the guard answers before the host renders"
    );

    paper.broken.store(true, Ordering::SeqCst);
    let failed = label_sheets::print(&shop.st, &host, sheet_of(&milk.id, 3, 23, false));
    assert_eq!(failed.unwrap_err().code(), "print_host_unavailable");
    assert_eq!(printed(&shop), (0, 0), "a failed print is not journaled");

    paper.broken.store(false, Ordering::SeqCst);
    let done = label_sheets::print(&shop.st, &host, sheet_of(&milk.id, 3, 23, false)).unwrap();
    assert_eq!(
        (
            done.printer.as_deref(),
            done.copies,
            done.duplicate,
            done.dialog
        ),
        (Some("HP LaserJet M110"), 1, false, false)
    );
    assert_eq!(
        paper.rendered.lock().unwrap().as_slice(),
        [(true, pos_core::lang::Lang::Ar)]
    );
    assert_eq!(
        paper.printed.lock().unwrap().as_slice(),
        [(Some("HP LaserJet M110".into()), 1, PageSetup::a4(0))],
        "the cells are placed from the paper's edges: no margin"
    );
    assert_eq!(printed(&shop), (1, 1));

    let cancelled =
        label_sheets::export_pdf(&shop.st, &host, sheet_of(&milk.id, 3, 1, false), |_| {
            Ok(None)
        });
    assert_eq!(cancelled.unwrap(), None);
    assert_eq!(
        printed(&shop),
        (1, 1),
        "a cancelled save dialog journals nothing"
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("etiquettes.pdf");
    let mut offered = String::new();
    let saved =
        label_sheets::export_pdf(&shop.st, &host, sheet_of(&milk.id, 3, 1, false), |name| {
            offered = name.to_owned();
            Ok(Some(path.clone()))
        })
        .unwrap()
        .unwrap();
    assert!(
        offered.starts_with("etiquettes-") && offered.ends_with(".pdf"),
        "{offered}"
    );
    assert_eq!(saved.path, path.to_string_lossy());
    assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF"));
    assert_eq!(paper.pdfs.lock().unwrap().as_slice(), [PageSetup::a4(0)]);
    assert_eq!(
        printed(&shop),
        (2, 1),
        "one more journal row, the same product"
    );
    let journal = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'labels.printed' ORDER BY seq DESC",
                [],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap();
    let journal: serde_json::Value = serde_json::from_str(&journal).unwrap();
    assert_eq!(
        (&journal["copies"], &journal["arabicWarningOverridden"]),
        (&serde_json::json!(3), &serde_json::json!(false))
    );
}

/// As for the label printer, a sheet's journal row and reprint queue are the main PC's.
/// An offline till renders nothing and opens no save dialog (`host_required`), rather than a
/// planche the main PC's reprint queue never hears of.
#[test]
fn a_sheet_on_an_offline_till_needs_the_main_pc_and_prints_nothing() {
    let shop = shop();
    shop.owner();
    let milk = milk(&shop);
    let paper = Arc::new(Paper::default());
    let host = PrintHost::new(paper.clone(), Arc::new(HostChannel::new()));
    super::lan_rules_tests::as_till(&shop);
    super::lan_rules_tests::link(&shop, false, None);

    let printing = label_sheets::print(&shop.st, &host, sheet_of(&milk.id, 1, 1, false));
    assert_eq!(printing.unwrap_err().code(), "host_required");
    let mut asked = false;
    let saving = label_sheets::export_pdf(&shop.st, &host, sheet_of(&milk.id, 1, 1, false), |_| {
        asked = true;
        Ok(None)
    });
    assert_eq!(saving.unwrap_err().code(), "host_required");
    assert!(!asked, "no save dialog opens");
    assert!(
        paper.rendered.lock().unwrap().is_empty(),
        "nothing is rendered, printed or saved"
    );
    assert_eq!(printed(&shop), (0, 0));
    // the planche itself is only read: a till still sees what it would print
    assert_eq!(
        label_sheets::sheet(&shop.st, sheet_of(&milk.id, 1, 1, false))
            .unwrap()
            .cells
            .len(),
        1
    );
}
