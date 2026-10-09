//! Labels (LBL-001…013), scale items and the PLU file (PRD-074), data-collector files (STK-012).
//! Printing goes to the label printer only after the DOC-034 guard; what was printed is recorded
//! only once the printer took the job. A lot or an expiry date the layout had to leave out
//! (décret 13-378) comes back with the preview and the print.

use super::{blocking, pick_dialog, save_dialog, today};
use crate::lan::route::forward;
use crate::printing::hw_error;
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::catalog::scale_items::{self, ScaleItem, ScaleItemInput};
use pos_core::error::AppError;
use pos_core::labels::{
    self, AssignedBarcode, LabelBatch, LabelData, LabelGroup, LabelLang, LabelRequest,
    LabelSheetFormat, LabelTemplate, LabelWarning, LabelWarningCode, StaleLabel,
    WeighedLabelRequest,
};
use pos_core::lang::Lang;
use pos_core::money::Qty;
use pos_core::time::business_date;
use pos_docs::sheet::{self, SheetFormat, SheetMeta};
use pos_hw::collector::{self, CollectorProblem};
use pos_hw::labels::LabelDpi;
use pos_hw::{HwError, PrintOutcome};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;
use tauri::{AppHandle, State, Window};

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SizePreset {
    pub width_mm: u16,
    pub height_mm: u16,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplatesView {
    pub templates: Vec<LabelTemplate>,
    pub category_templates: BTreeMap<String, String>,
    pub size_presets: Vec<SizePreset>,
    pub sheet_formats: Vec<LabelSheetFormat>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelPreviewInput {
    /// Saved or not: the designer's draft.
    pub template: LabelTemplate,
    /// `None`: the first product with a barcode.
    pub product_id: Option<String>,
    pub lang: Option<LabelLang>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelPreview {
    /// `data:image/png;base64,…`
    pub png: String,
    pub label: LabelData,
    /// The label printer's resolution, else 203 dpi.
    pub dpi: LabelDpi,
    /// `lot_or_expiry_dropped` when the layout left the lot or the expiry date out.
    pub warnings: Vec<LabelWarning>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelPrintInput {
    pub request: LabelRequest,
    /// `None`: the default label printer of this PC.
    pub printer_id: Option<String>,
    /// LBL-010: an internal EAN-13 for each product without a barcode (needs `products.edit`).
    pub assign_missing_barcodes: bool,
    /// DOC-034: the user confirmed labels without Arabic (journaled).
    pub arabic_override: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelPrintDone {
    pub outcome: PrintOutcome,
    /// Labels printed, copies included.
    pub labels: u32,
    pub assigned: Vec<AssignedBarcode>,
    /// `lot_or_expiry_dropped` for each label printed without the lot or the expiry date it
    /// carries (too small a label).
    pub warnings: Vec<LabelWarning>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WeighedLabelInput {
    pub request: WeighedLabelRequest,
    pub printer_id: Option<String>,
    pub arabic_override: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectorImport {
    pub matched: u32,
    /// Codes that name no product.
    pub unknown: Vec<String>,
    /// Lines that could not be read (`code_invalid`, `qty_invalid`).
    pub problems: Vec<CollectorProblem>,
}

/// The templates are read wherever products are (the till's weighed labels included).
const VIEW: &[Permission] = &[Permission::ProductsLabels, Permission::ProductsView];
/// A weighed label is printed at the counter too (VRT-010).
const WEIGHED: &[Permission] = &[Permission::ProductsLabels, Permission::SalesCreate];
const STALE_LIMIT: u32 = 500;
const STALE_MAX: u32 = 5_000;
const COLLECTOR_EXTENSIONS: [&str; 3] = ["csv", "txt", "dat"];

#[tauri::command]
#[specta::specta]
pub async fn labels_templates(
    state: State<'_, SharedState>,
) -> Result<LabelTemplatesView, AppError> {
    let st = state.inner().clone();
    blocking(move || templates(&st)).await
}

/// Both settings in one transaction.
#[tauri::command]
#[specta::specta]
pub async fn labels_save_templates(
    state: State<'_, SharedState>,
    templates: Vec<LabelTemplate>,
    category_templates: BTreeMap<String, String>,
) -> Result<LabelTemplatesView, AppError> {
    let st = state.inner().clone();
    blocking(move || save_templates(&st, templates, category_templates)).await
}

#[tauri::command]
#[specta::specta]
pub async fn labels_build(
    state: State<'_, SharedState>,
    request: LabelRequest,
) -> Result<LabelBatch, AppError> {
    let st = state.inner().clone();
    blocking(move || build(&st, request)).await
}

#[tauri::command]
#[specta::specta]
pub async fn labels_preview(
    state: State<'_, SharedState>,
    input: LabelPreviewInput,
) -> Result<LabelPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || preview(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn labels_print(
    state: State<'_, SharedState>,
    input: LabelPrintInput,
) -> Result<LabelPrintDone, AppError> {
    let st = state.inner().clone();
    blocking(move || print(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn labels_print_weighed(
    state: State<'_, SharedState>,
    input: WeighedLabelInput,
) -> Result<LabelPrintDone, AppError> {
    let st = state.inner().clone();
    blocking(move || print_weighed(&st, input)).await
}

/// `limit`: 500 by default, 5 000 at most.
#[tauri::command]
#[specta::specta]
pub async fn labels_stale(
    state: State<'_, SharedState>,
    limit: Option<u32>,
) -> Result<Vec<StaleLabel>, AppError> {
    let st = state.inner().clone();
    blocking(move || stale(&st, limit)).await
}

/// The labels of `product_ids` marked up to date without printing; how many were.
#[tauri::command]
#[specta::specta]
pub async fn labels_dismiss(
    state: State<'_, SharedState>,
    product_ids: Vec<String>,
) -> Result<u32, AppError> {
    let st = state.inner().clone();
    blocking(move || dismiss(&st, product_ids)).await
}

#[tauri::command]
#[specta::specta]
pub async fn scale_items_list(state: State<'_, SharedState>) -> Result<Vec<ScaleItem>, AppError> {
    let st = state.inner().clone();
    blocking(move || scale_items(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn scale_items_set(
    state: State<'_, SharedState>,
    product_id: String,
    input: ScaleItemInput,
) -> Result<ScaleItem, AppError> {
    let st = state.inner().clone();
    blocking(move || set_scale_item(&st, product_id, input)).await
}

/// The PLU list for the scales' PC tools after a Rust save dialog: the file name, `None` when the
/// user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn scale_plu_export(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    format: SheetFormat,
    lang: Lang,
) -> Result<Option<String>, AppError> {
    let st = state.inner().clone();
    blocking(move || {
        let bytes = plu_bytes(&st, format, lang)?;
        let name = sheet::file_name("plu", &today(), format);
        let Some(path) = save_dialog(&app, &window, &name, &[format.extension()])? else {
            return Ok(None);
        };
        // beside the chosen file first: a failed write never leaves it half replaced
        pos_docs::archive::write_file(&path, &bytes)?;
        Ok(path.file_name().map(|n| n.to_string_lossy().into_owned()))
    })
    .await
}

/// A data collector's `code;quantity` file (at most 5 MB) picked in a Rust open dialog, added to
/// the open count `count_id`; `None` when the user cancels the dialog.
#[tauri::command]
#[specta::specta]
pub async fn counts_import_collector(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    count_id: String,
) -> Result<Option<CollectorImport>, AppError> {
    let st = state.inner().clone();
    may_import_collector(&st)?;
    blocking(move || {
        let filter = COLLECTOR_EXTENSIONS.join(", ");
        let Some(path) = pick_dialog(&app, &window, &filter, &COLLECTOR_EXTENSIONS)? else {
            return Ok(None);
        };
        let size = std::fs::metadata(&path)
            .map_err(|e| AppError::Io(format!("collector: {e}")))?
            .len();
        if size > collector::MAX_FILE_BYTES as u64 {
            return Err(hw_error(HwError::Collector(
                "the file is larger than 5 MB".into(),
            )));
        }
        let bytes = std::fs::read(&path).map_err(|e| AppError::Io(format!("collector: {e}")))?;
        import_collector_bytes(&st, count_id, &bytes).map(Some)
    })
    .await
}

/// Checked before the dialog opens: a count is kept at the till (a manager's approval counts).
pub(crate) fn may_import_collector(st: &AppState) -> Result<(), AppError> {
    st.till(Permission::StockCount).map(drop)
}

/// The shop's business day, `YYYY-MM-DD` (promotions running, packing and expiry dates).
pub(crate) fn day(st: &AppState) -> String {
    business_date(st.now()).format("%Y-%m-%d").to_string()
}

pub(crate) fn templates(st: &AppState) -> Result<LabelTemplatesView, AppError> {
    st.require_any(VIEW)?;
    st.db.read(|c| {
        Ok(LabelTemplatesView {
            templates: labels::templates(c)?,
            category_templates: labels::category_templates(c)?,
            size_presets: labels::SIZE_PRESETS
                .iter()
                .map(|&(width_mm, height_mm)| SizePreset {
                    width_mm,
                    height_mm,
                })
                .collect(),
            sheet_formats: labels::sheet_formats(),
        })
    })
}

pub(crate) fn save_templates(
    st: &AppState,
    list: Vec<LabelTemplate>,
    category_templates: BTreeMap<String, String>,
) -> Result<LabelTemplatesView, AppError> {
    let actor = st.require(Permission::ProductsLabels)?.actor();
    let list = serde_json::to_value(&list).map_err(|e| AppError::Internal(e.to_string()))?;
    let by_category =
        serde_json::to_value(&category_templates).map_err(|e| AppError::Internal(e.to_string()))?;
    st.db.write(move |tx| {
        pos_core::settings::set_value(tx, &actor, "labels.templates", list)?;
        pos_core::settings::set_value(tx, &actor, "labels.category_templates", by_category)?;
        Ok(())
    })?;
    templates(st)
}

pub(crate) fn build(st: &AppState, request: LabelRequest) -> Result<LabelBatch, AppError> {
    st.require(Permission::ProductsLabels)?;
    let day = day(st);
    st.db.read(move |c| labels::build(c, &request, &day))
}

/// A layout the label cannot take (too small for its barcode) names the template's field.
fn template_error(error: HwError) -> AppError {
    hw_error(error.within("template"))
}

/// A refusal of the label printer: a template too small for what it prints names its field as
/// the designer does (`template.widthDmm`…), so every screen says which setting to make larger;
/// any other error as it comes.
fn print_error(error: HwError) -> AppError {
    match error.field() {
        Some("widthDmm" | "heightDmm") => template_error(error),
        _ => hw_error(error),
    }
}

pub(crate) fn preview(st: &AppState, input: LabelPreviewInput) -> Result<LabelPreview, AppError> {
    st.require(Permission::ProductsLabels)?;
    let day = day(st);
    let template = input.template.clone();
    let label = st.db.read(move |c| {
        labels::preview_label(
            c,
            &input.template,
            input.product_id.as_deref(),
            input.lang,
            &day,
        )
    })?;
    let dpi = st
        .hw
        .label_printer(None)
        .map_or(LabelDpi::D203, |p| p.profile.dpi);
    let png = st
        .hw
        .label_preview(&template, &label, dpi)
        .map_err(template_error)?;
    let group = LabelGroup {
        template,
        labels: vec![label.clone()],
    };
    let warnings = st
        .hw
        .labels_dropped(std::slice::from_ref(&group), dpi)
        .map_err(template_error)?;
    Ok(LabelPreview {
        png: pos_hw::labels::data_url(&png),
        label,
        dpi,
        warnings,
    })
}

/// Build → DOC-034 guard → internal EAN-13s (LBL-010) → the label printer → what was printed.
/// Nothing is written before the guard passes and the printer is known. On a till the labels
/// print here and what they write goes to the main PC ([`assign_barcodes`], [`record_printed`]):
/// an offline till prints none (`host_required`), rather than labels the main PC's
/// reprint queue never hears of.
pub(crate) fn print(st: &AppState, input: LabelPrintInput) -> Result<LabelPrintDone, AppError> {
    st.require(Permission::ProductsLabels)?;
    if input.assign_missing_barcodes {
        st.require(Permission::ProductsEdit)?;
    }
    host_reachable(st)?;
    let printer = st
        .hw
        .label_printer(input.printer_id.as_deref())
        .map_err(hw_error)?;
    let day = day(st);
    let load =
        |request: LabelRequest, day: String| st.db.read(move |c| labels::build(c, &request, &day));
    let mut batch = load(input.request.clone(), day.clone())?;
    labels::check_arabic(&batch, input.arabic_override)?;
    let mut assigned = Vec::new();
    if input.assign_missing_barcodes {
        let ids: Vec<String> = batch
            .warnings
            .iter()
            .filter(|w| w.code == LabelWarningCode::NoBarcode)
            .map(|w| w.product_id.clone())
            .collect();
        if !ids.is_empty() {
            // Reloaded from this PC's copy: on a till, only once the main PC's barcodes are in it.
            assigned = assign_barcodes_with(st, ids, true)?;
            batch = load(input.request, day)?;
        }
    }
    // Laid out once: the bytes and the labels that leave out their lot or expiry date.
    let (outcome, warnings) = st
        .hw
        .print_labels(&batch.groups, Some(printer.id.as_str()))
        .map_err(print_error)?;
    let total = batch.total;
    record_printed(st, batch, input.arabic_override)?;
    Ok(LabelPrintDone {
        outcome,
        labels: total,
        assigned,
        warnings,
    })
}

/// A till prints shelf labels (on its label printer or on an A4 sheet) only while the main PC
/// answers: what the print writes, the reprint queue and the barcodes it assigns, is the main
/// PC's. Checked before anything is printed.
pub(crate) fn host_reachable(st: &AppState) -> Result<(), AppError> {
    if st.lan.is_client() && !st.lan.is_online() {
        return Err(AppError::conflict("host_required"));
    }
    Ok(())
}

/// How long a weighed label waits for the scale to settle.
const WEIGHED_READ: std::time::Duration = std::time::Duration::from_secs(2);

/// The weight a weighed label carries. With a scale on this PC, its stable reading now: the
/// label's weight barcode sets what the till charges (VRT-010), so it is never a weight the
/// window sent (an older reading, a refused print sent again); an unstable or silent scale
/// refuses. Without a scale, the weight typed by hand.
pub(crate) fn weighed_net(st: &AppState, typed: Qty) -> Result<Qty, AppError> {
    match st.devices.with_scale(|s| s.read_stable(WEIGHED_READ)) {
        Ok(reading) => Ok(reading.net),
        Err(HwError::ScaleNotConfigured) => Ok(typed),
        Err(error) => Err(hw_error(error)),
    }
}

/// LBL-010: an internal EAN-13 for each of `product_ids` without a barcode. Barcodes are the
/// main PC's: on a till this runs there (host service `labels.assign_barcodes`), and the answer
/// comes back once the till's copy has them.
pub(crate) fn assign_barcodes(
    st: &AppState,
    product_ids: Vec<String>,
) -> Result<Vec<AssignedBarcode>, AppError> {
    assign_barcodes_with(st, product_ids, false)
}

/// `settled`: the caller reads the barcodes back from this PC's copy (on a till, the main PC's
/// answer must have reached it: `route::forward_settled`).
fn assign_barcodes_with(
    st: &AppState,
    product_ids: Vec<String>,
    settled: bool,
) -> Result<Vec<AssignedBarcode>, AppError> {
    let input = json!({ "productIds": product_ids });
    let forwarded = if settled {
        crate::lan::route::forward_settled(st, "labels.assign_barcodes", input)?
    } else {
        forward(st, "labels.assign_barcodes", input)?
    };
    if let Some(done) = forwarded {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsLabels)?.actor();
    st.require(Permission::ProductsEdit)?;
    st.db
        .write(move |tx| labels::assign_missing_barcodes(tx, &actor, &product_ids))
}

/// What each product's label now says (the reprint queue) and the batch's journal entry, once
/// the printer took the job. The reprint queue is the main PC's: on a till this runs there
/// (host service `labels.record_printed`), with the labels as the till printed them.
pub(crate) fn record_printed(
    st: &AppState,
    batch: LabelBatch,
    arabic_override: bool,
) -> Result<(), AppError> {
    if let Some(done) = forward(
        st,
        "labels.record_printed",
        json!({ "batch": batch, "arabicOverride": arabic_override }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsLabels)?.actor();
    let now = st.now();
    st.db
        .write(move |tx| labels::record_printed(tx, &actor, &batch, arabic_override, now))
}

/// A package weighed at the counter: its weight (the scale's, [`weighed_net`]), amount and
/// weight barcode (VRT-010). An expiry date the label has no room for is reported as
/// `labels_print` reports it.
pub(crate) fn print_weighed(
    st: &AppState,
    input: WeighedLabelInput,
) -> Result<LabelPrintDone, AppError> {
    let session = st.require_any(WEIGHED)?;
    let printer = st
        .hw
        .label_printer(input.printer_id.as_deref())
        .map_err(hw_error)?;
    let day = day(st);
    let mut request = input.request;
    request.net = weighed_net(st, request.net)?;
    let batch = st.db.read(move |c| labels::weighed(c, &request, &day))?;
    labels::check_arabic(&batch, input.arabic_override)?;
    // Laid out once: the bytes and the labels that leave out their lot or expiry date.
    let (outcome, warnings) = st
        .hw
        .print_labels(&batch.groups, Some(printer.id.as_str()))
        .map_err(print_error)?;
    let (actor, total, now) = (session.actor(), batch.total, st.now());
    let arabic_override = input.arabic_override;
    st.db
        .write(move |tx| labels::record_printed(tx, &actor, &batch, arabic_override, now))?;
    Ok(LabelPrintDone {
        outcome,
        labels: total,
        assigned: Vec::new(),
        warnings,
    })
}

pub(crate) fn stale(st: &AppState, limit: Option<u32>) -> Result<Vec<StaleLabel>, AppError> {
    st.require(Permission::ProductsLabels)?;
    let day = day(st);
    let limit = limit.unwrap_or(STALE_LIMIT).clamp(1, STALE_MAX);
    st.db.read(move |c| labels::stale(c, &day, limit))
}

pub(crate) fn dismiss(st: &AppState, product_ids: Vec<String>) -> Result<u32, AppError> {
    let actor = st.require(Permission::ProductsLabels)?.actor();
    let (day, now) = (day(st), st.now());
    st.db
        .write(move |tx| labels::dismiss(tx, &actor, &product_ids, &day, now))
}

pub(crate) fn scale_items(st: &AppState) -> Result<Vec<ScaleItem>, AppError> {
    st.require(Permission::ProductsView)?;
    st.db.read(scale_items::list)
}

pub(crate) fn set_scale_item(
    st: &AppState,
    product_id: String,
    input: ScaleItemInput,
) -> Result<ScaleItem, AppError> {
    let actor = st.require(Permission::ProductsEdit)?.actor();
    let now = st.now();
    st.db
        .write(move |tx| scale_items::set(tx, &actor, &product_id, &input, now))
}

/// The PLU table as xlsx or csv in `lang` (the scale makers' PC tools import either).
pub(crate) fn plu_bytes(
    st: &AppState,
    format: SheetFormat,
    lang: Lang,
) -> Result<Vec<u8>, AppError> {
    st.require(Permission::ProductsView)?;
    let table = st.db.read(scale_items::plu_table)?;
    let meta = SheetMeta {
        title: pos_docs::receipt::label(lang, "label.pluList"),
        subtitle: None,
        lang,
        extra_labels: BTreeMap::new(),
    };
    sheet::render(&table, &meta, format)
}

/// The rows of a collector file added to the open count `count_id`; lines that cannot be read
/// are reported, never guessed (`collector_file_invalid` for a file with nothing to read).
pub(crate) fn import_collector_bytes(
    st: &AppState,
    count_id: String,
    bytes: &[u8],
) -> Result<CollectorImport, AppError> {
    let ctx = st.till(Permission::StockCount)?;
    let file = collector::parse(bytes).map_err(hw_error)?;
    let rows: Vec<(String, Qty)> = file.rows.into_iter().map(|r| (r.code, r.qty)).collect();
    let write = ctx.clone();
    let imported = st
        .db
        .write(move |tx| pos_core::stock::counts::import_lines(tx, &write, &count_id, &rows))?;
    st.spend_approvals(&ctx)?;
    Ok(CollectorImport {
        matched: imported.matched,
        unknown: imported.unknown,
        problems: file.problems,
    })
}
