//! A4 label sheets (LBL-013): where each label goes on the planche and what it says, printed or
//! saved as a PDF through the print host (spec §9.2) as its `label_sheet` page. The sheet is A4
//! without margins: the cells are placed from the paper's edges. The labels are journaled
//! (`labels.printed`, which also clears them from the reprint queue) only once the print host has
//! printed the sheet or written its PDF. On a till the sheet prints there and its journal goes to
//! the main PC, as for the label printer (`labels::record_printed`).

use super::labels::{day, host_reachable, record_printed};
use super::print::{A4Printed, PdfSaved};
use super::{blocking, save_dialog};
use crate::print_host::{HostPage, PageSetup, PrintHost, PrintStatus};
use crate::printing::hw_error;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::labels::{
    self, LabelBatch, LabelRequest, LabelSheetCell, LabelSheetFormat, LabelSymbology,
};
use pos_core::lang::Lang;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State, Window};

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SheetInput {
    pub request: LabelRequest,
    /// One of `labels_templates().sheetFormats`.
    pub format_id: String,
    /// The first free position of the sheet, 1-based, row by row from its start corner.
    pub start: u16,
    /// DOC-034: the user confirmed labels without Arabic (journaled).
    pub arabic_override: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SheetBarcode {
    pub symbology: LabelSymbology,
    pub data: String,
    /// `"1010…"` per module row (one row for a linear symbol, one per row of a QR code).
    pub rows: Vec<String>,
    /// The digits printed under a linear symbol.
    pub hri: Option<String>,
}

/// The words of one label as a label printer prints them (`pos_hw::labels::texts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SheetLabel {
    pub rtl: bool,
    pub lang: Lang,
    pub shop: Option<String>,
    pub name: Option<String>,
    pub name_second: Option<String>,
    pub price: Option<String>,
    pub old_price: Option<String>,
    pub weight: Option<String>,
    pub details: Vec<String>,
    pub barcode: Option<SheetBarcode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelSheet {
    pub format: LabelSheetFormat,
    pub pages: u16,
    /// Each copy's place; `label` indexes `labels`.
    pub cells: Vec<LabelSheetCell>,
    pub labels: Vec<SheetLabel>,
}

#[tauri::command]
#[specta::specta]
pub async fn labels_sheet(
    state: State<'_, SharedState>,
    input: SheetInput,
) -> Result<LabelSheet, AppError> {
    let st = state.inner().clone();
    blocking(move || sheet(&st, input)).await
}

/// Prints the sheet on this PC's A4 printer (Réglages › Documents), else Windows' default one.
#[tauri::command]
#[specta::specta]
pub async fn labels_sheet_print(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: SheetInput,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print(&st, &host, input)).await
}

/// The sheet as a PDF where the Rust save dialog says; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn labels_sheet_export_pdf(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: SheetInput,
) -> Result<Option<PdfSaved>, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || {
        export_pdf(&st, &host, input, |name| {
            save_dialog(&app, &window, name, &["pdf"])
        })
    })
    .await
}

pub(crate) fn sheet(st: &AppState, input: SheetInput) -> Result<LabelSheet, AppError> {
    st.require(Permission::ProductsLabels)?;
    make(st, &input).map(|(_, sheet)| sheet)
}

pub(crate) fn print(
    st: &AppState,
    host: &PrintHost,
    input: SheetInput,
) -> Result<A4Printed, AppError> {
    st.require(Permission::ProductsLabels)?;
    host_reachable(st)?;
    let (batch, sheet) = make(st, &input)?;
    let printer = st.hw.config().map_err(hw_error)?.documents.default_printer;
    let status = host.print(
        HostPage::LabelSheet(Box::new(sheet)),
        |_| PageSetup::a4(0),
        printer.as_deref(),
        1,
    )?;
    record_printed(st, batch, input.arabic_override)?;
    Ok(A4Printed {
        printer,
        copies: 1,
        duplicate: false,
        dialog: status == PrintStatus::Dialog,
    })
}

/// The sheet of `input` as a PDF where `pick` says, offered `etiquettes-<day>.pdf` once the
/// sheet is made (the permission, the main PC on a till and the Arabic guard answer before the
/// save dialog opens); `None` when the user cancels it, and nothing is journaled then.
pub(crate) fn export_pdf(
    st: &AppState,
    host: &PrintHost,
    input: SheetInput,
    pick: impl FnOnce(&str) -> Result<Option<PathBuf>, AppError>,
) -> Result<Option<PdfSaved>, AppError> {
    st.require(Permission::ProductsLabels)?;
    host_reachable(st)?;
    let (batch, sheet) = make(st, &input)?;
    let Some(path) = pick(&format!("etiquettes-{}.pdf", day(st)))? else {
        return Ok(None);
    };
    host.pdf(
        HostPage::LabelSheet(Box::new(sheet)),
        |_| PageSetup::a4(0),
        &path,
    )?;
    record_printed(st, batch, input.arabic_override)?;
    Ok(Some(PdfSaved {
        path: path.to_string_lossy().into_owned(),
        duplicate: false,
    }))
}

/// Format → labels (prices, names and barcodes from `labels::build`) → DOC-034 guard → the words
/// and bars of each label → the place of each copy. The batch comes back for the journal.
fn make(st: &AppState, input: &SheetInput) -> Result<(LabelBatch, LabelSheet), AppError> {
    let format = labels::sheet_formats()
        .into_iter()
        .find(|f| f.id == input.format_id)
        .ok_or_else(|| AppError::validation("formatId", "value_invalid"))?;
    let (request, day) = (input.request.clone(), day(st));
    let batch = st.db.read(move |c| labels::build(c, &request, &day))?;
    labels::check_arabic(&batch, input.arabic_override)?;
    let mut out = Vec::new();
    let mut copies = Vec::new();
    for group in &batch.groups {
        for label in &group.labels {
            let t = pos_hw::labels::texts(label, &group.template.fields);
            let barcode = match &label.barcode {
                Some(b) => {
                    let symbol = pos_hw::barcode::encode(b.symbology, &b.data).map_err(hw_error)?;
                    Some(SheetBarcode {
                        symbology: b.symbology,
                        data: b.data.clone(),
                        rows: pos_hw::barcode::rows(&symbol),
                        hri: t.hri,
                    })
                }
                None => None,
            };
            out.push(SheetLabel {
                rtl: t.rtl,
                lang: label.lang.primary(),
                shop: t.shop,
                name: t.name,
                name_second: t.name_second,
                price: t.price,
                old_price: t.old_price,
                weight: t.weight,
                details: t.details,
                barcode,
            });
            copies.push(label.copies);
        }
    }
    let cells = labels::plan(&format, input.start, &copies)?;
    let pages = cells.last().map_or(0, |c| c.page + 1);
    let sheet = LabelSheet {
        format,
        pages,
        cells,
        labels: out,
    };
    Ok((batch, sheet))
}
