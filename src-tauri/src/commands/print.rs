//! Print, PDF and preview of A4/A5 documents through the print host (spec §9.2; DOC-017/018):
//! the first output of a validated document is the original, every later one a DUPLICATA; an
//! output is journaled once it succeeded; the PDF path comes from the Rust save dialog. The A4
//! statements, the warranty certificates (DOC-012) and the PDF of any table (IMX-013) go through
//! the same host.

use super::exports::{self, ExportDone, ExportTarget};
use super::{blocking, save_dialog, today};
use crate::print_host::{HostPage, PRINT_TIMEOUT, PageSetup, PrintHost, PrintStatus};
use crate::printing::{hw_error, load_receipt};
use crate::state::{AppState, SharedState};
use base64::Engine;
use pos_core::auth::{self, Permission, Session};
use pos_core::billing::kind_permission;
use pos_core::billing::print::{PrintOptions, print_context};
use pos_core::billing::settings::Paper;
use pos_core::billing::warranty::WarrantySource;
use pos_core::documents::outputs::{self, Output, OutputKind};
use pos_core::documents::{self, DocType, Document};
use pos_core::documents::{DocumentParty, PartyRole};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_docs::a4::document::{PartyBlock, party_block};
use pos_docs::a4::statement::{StatementParty, build_statement};
use pos_docs::a4::warranty::build_warranty;
use pos_docs::sheet::{self, SheetFormat};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, State, Window};

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintDocumentInput {
    pub id: String,
    #[serde(default)]
    pub options: PrintOptions,
    /// A Windows printer chosen in the print dialog; this PC's printer of the kind otherwise.
    #[serde(default)]
    #[specta(optional)]
    pub printer: Option<String>,
    /// 1 to 5; this PC's copies of the kind otherwise.
    #[serde(default)]
    #[specta(optional)]
    pub copies: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PdfDocumentInput {
    pub id: String,
    #[serde(default)]
    pub options: PrintOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct A4Printed {
    pub printer: Option<String>,
    pub copies: u32,
    pub duplicate: bool,
    /// The print dialog was shown (old WebView2 runtime).
    pub dialog: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PdfSaved {
    pub path: String,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatementSide {
    Customer,
    Supplier,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementPrintInput {
    pub side: StatementSide,
    pub party_id: String,
    pub from: String,
    pub to: String,
    pub lang: Lang,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyInput {
    pub document_id: String,
    /// One warranted line; the first one otherwise.
    #[serde(default)]
    #[specta(optional)]
    pub line_id: Option<String>,
    /// The document's language otherwise.
    #[serde(default)]
    #[specta(optional)]
    pub lang: Option<Lang>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PdfExportRequest {
    pub target: ExportTarget,
    pub lang: Lang,
}

/// Any of these opens printing; the document's kind then needs its own.
const OUTPUT_PERMISSIONS: [Permission; 8] = [
    Permission::SalesReprint,
    Permission::SalesViewAll,
    Permission::DocumentsInvoice,
    Permission::DocumentsQuote,
    Permission::DocumentsDelivery,
    Permission::DocumentsCreditNote,
    Permission::PurchasesView,
    Permission::StockTransfer,
];

/// The session first (a call without one fails on it, never on the lookup), then the document,
/// then the permission of its kind (tickets and return notes: `sales.reprint`, on this terminal
/// only without `sales.view_all`, like M1b's reprint). A transfer note carries its prices at the
/// CMUP, so it also needs `reports.profit` (M1b's reprint rule).
pub(crate) fn require_output(st: &AppState, id: &str) -> Result<(Session, Document), AppError> {
    let session = st.require_any(&OUTPUT_PERMISSIONS)?;
    let doc = {
        let id = id.to_owned();
        st.db.read(move |c| documents::get(c, &id))?
    };
    // A return to a supplier is a return note of the purchase side (its costs).
    let needed = if super::documents::purchase_side(&doc) {
        Permission::PurchasesView
    } else {
        kind_permission(doc.doc_type).unwrap_or(match doc.doc_type {
            DocType::Ticket | DocType::ReturnNote => Permission::SalesReprint,
            _ => Permission::StockTransfer,
        })
    };
    auth::require(&session, needed)?;
    if doc.doc_type == DocType::Transfer {
        auth::require(&session, Permission::ReportsProfit)?;
    }
    if matches!(doc.doc_type, DocType::Ticket | DocType::ReturnNote)
        && !super::documents::purchase_side(&doc)
        && !super::documents::shown(&doc, &session)
    {
        return Err(AppError::not_found("document"));
    }
    Ok((session, doc))
}

/// One document output at a time, from choosing original or DUPLICATA to its journal row: two
/// windows printing the same invoice never both get the original. The print host's own lock is
/// only ever taken inside this one.
static OUTPUTS: Mutex<()> = Mutex::new(());

fn one_output() -> Result<MutexGuard<'static, ()>, AppError> {
    OUTPUTS
        .lock()
        .map_err(|_| AppError::Internal("document output lock poisoned".into()))
}

fn next_output(st: &AppState, id: &str) -> Result<Output, AppError> {
    let id = id.to_owned();
    st.db.read(move |c| outputs::next_output(c, &id))
}

/// The shop's logo as a `data:` URL: the print host never loads a file or the network.
fn logo_url(st: &AppState) -> Result<Option<String>, AppError> {
    let bytes = st.db.read(pos_core::company::logo)?;
    Ok(bytes.map(|b| {
        let mime = if b.starts_with(b"\x89PNG") {
            "image/png"
        } else {
            "image/jpeg"
        };
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        )
    }))
}

/// The page of `doc` and its paper; `duplicate` = the copy number when it is one. A ticket or a
/// return note prints its receipt, always marked DUPLICATA (its original was the receipt).
pub(crate) fn page_of(
    st: &AppState,
    doc: &Document,
    options: &PrintOptions,
    duplicate: Option<i64>,
) -> Result<(HostPage, Paper), AppError> {
    if matches!(doc.doc_type, DocType::Ticket | DocType::ReturnNote) {
        let (receipt, _) = load_receipt(&st.db, &doc.id, true, options.lang)?;
        let second = if options.monolingual {
            None
        } else {
            options.second_lang
        };
        let page = pos_docs::a4::receipt::receipt_page(receipt, second);
        return Ok((HostPage::Receipt(Box::new(page)), Paper::A4));
    }
    let (now, day, id, options) = (
        pos_core::time::now_ms(),
        today(),
        doc.id.clone(),
        options.clone(),
    );
    let ctx = st
        .db
        .read(move |c| print_context(c, &id, &options, &day, now))?;
    let mut model = pos_docs::a4::document::build_document(&ctx, &st.datapack, duplicate)?;
    if model.layout.show_logo && ctx.has_logo {
        model.logo = logo_url(st)?;
    }
    // CFG-003: while the shop's "BL valant facture" decision is set, a delivery note says so.
    if model.doc_type == DocType::DeliveryNote
        && let Some(auth) = st.db.read(pos_core::wholesale::bl_invoice_authorisation)?
    {
        pos_docs::a4::packs::stand_as_invoice(&mut model, &auth);
    }
    let paper = model.layout.paper;
    Ok((HostPage::Document(Box::new(model)), paper))
}

/// The page setup of `page` once rendered: receipts are 80 mm and as long as their content, a
/// table wider than seven columns turns landscape. It borrows nothing (`use<>`): the page then
/// moves into the host.
fn setup_for(page: &HostPage, paper: Paper, margin: u8) -> impl Fn(u32) -> PageSetup + use<> {
    let receipt = matches!(page, HostPage::Receipt(_));
    let wide = matches!(page, HostPage::Table(t) if t.columns.len() > 7);
    move |height| {
        if receipt {
            PageSetup::receipt(height)
        } else {
            PageSetup {
                landscape: wide,
                ..PageSetup::for_paper(paper, u32::from(margin))
            }
        }
    }
}

pub(crate) fn print_document(
    st: &AppState,
    host: &PrintHost,
    input: PrintDocumentInput,
) -> Result<A4Printed, AppError> {
    let (session, doc) = require_output(st, &input.id)?;
    let _one = one_output()?;
    let output = next_output(st, &doc.id)?;
    let (page, layout_paper) = page_of(
        st,
        &doc,
        &input.options,
        output.duplicate.then_some(output.copy_no),
    )?;
    let hw = st.hw.config().map_err(hw_error)?;
    let (printer, paper, copies) = hw.documents.for_kind(doc.doc_type, layout_paper);
    let printer = input.printer.filter(|p| !p.trim().is_empty()).or(printer);
    let copies = input.copies.unwrap_or(u32::from(copies)).clamp(1, 5);
    let setup = setup_for(&page, paper, hw.documents.margins_mm);
    let status = match host.print(page, setup, printer.as_deref(), copies) {
        Ok(status) => status,
        Err(error) if error.code() == PRINT_TIMEOUT => {
            // The copy may come out yet: it counts as made, so the next one is a DUPLICATA. The
            // user checks the printer before printing again (the error says so).
            let (actor, id) = (session.actor(), doc.id.clone());
            let detail = json!({ "printer": printer, "copies": copies, "timeout": true });
            st.db.write(move |tx| {
                outputs::record_output(tx, &actor, &id, OutputKind::Print, &output, detail)
            })?;
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let dialog = status == PrintStatus::Dialog;
    let (actor, id) = (session.actor(), doc.id.clone());
    let detail = json!({
        "printer": printer,
        "copies": copies,
        "lang": input.options.lang.unwrap_or(doc.lang),
        "dialog": dialog,
    });
    st.db.write(move |tx| {
        outputs::record_output(tx, &actor, &id, OutputKind::Print, &output, detail)
    })?;
    Ok(A4Printed {
        printer,
        copies,
        duplicate: output.duplicate,
        dialog,
    })
}

pub(crate) fn export_pdf_to(
    st: &AppState,
    host: &PrintHost,
    input: PdfDocumentInput,
    path: &Path,
) -> Result<PdfSaved, AppError> {
    let (session, doc) = require_output(st, &input.id)?;
    let _one = one_output()?;
    let output = next_output(st, &doc.id)?;
    let (page, paper) = page_of(
        st,
        &doc,
        &input.options,
        output.duplicate.then_some(output.copy_no),
    )?;
    let margins = st.hw.config().map_err(hw_error)?.documents.margins_mm;
    let setup = setup_for(&page, paper, margins);
    if let Err(error) = host.pdf(page, setup, path) {
        if error.code() == PRINT_TIMEOUT {
            // The file may be written yet: it counts as made, so the next copy is a DUPLICATA.
            let (actor, id) = (session.actor(), doc.id.clone());
            let detail = json!({ "timeout": true });
            st.db.write(move |tx| {
                outputs::record_output(tx, &actor, &id, OutputKind::Pdf, &output, detail)
            })?;
        }
        return Err(error);
    }
    let (actor, id) = (session.actor(), doc.id.clone());
    let detail = json!({ "lang": input.options.lang.unwrap_or(doc.lang) });
    st.db.write(move |tx| {
        outputs::record_output(tx, &actor, &id, OutputKind::Pdf, &output, detail)
    })?;
    Ok(PdfSaved {
        path: path.to_string_lossy().into_owned(),
        duplicate: output.duplicate,
    })
}

/// HW-002: the original receipt of a ticket (or a return note) whose thermal print failed on a
/// printer that only takes pages (`printer_driver_only`), printed as a page of its roll through
/// the Windows driver of `printer_name`, then journaled as printed. Only an original still
/// pending is printed this way (`print_not_pending` otherwise: a DUPLICATA is a reprint).
pub(crate) fn print_receipt_roll(
    st: &AppState,
    host: &PrintHost,
    document_id: String,
    printer_name: String,
    width_mm: u16,
) -> Result<A4Printed, AppError> {
    let session = st.require(Permission::SalesCreate)?;
    if !matches!(width_mm, 58 | 80) {
        return Err(AppError::validation("widthMm", "value_invalid"));
    }
    let printer = printer_name.trim().to_owned();
    if printer.is_empty() {
        return Err(AppError::validation("printerName", "required"));
    }
    let doc = {
        let id = document_id.clone();
        st.db.read(move |c| documents::get(c, &id))?
    };
    if !matches!(doc.doc_type, DocType::Ticket | DocType::ReturnNote)
        || !super::documents::shown(&doc, &session)
    {
        return Err(AppError::not_found("document"));
    }
    let _one = one_output()?;
    if !st.printer.is_pending(&doc.id)? {
        return Err(AppError::conflict("print_not_pending"));
    }
    let (receipt, _) = load_receipt(&st.db, &doc.id, false, None)?;
    let lang = receipt.lang;
    let page = HostPage::ReceiptRoll(Box::new(crate::print_host::ReceiptRollPage {
        page: pos_docs::a4::receipt::receipt_page(receipt, None),
        width_mm,
    }));
    let status = host.print(
        page,
        move |height| PageSetup::roll(width_mm, height),
        Some(&printer),
        1,
    )?;
    st.printer.forget(&doc.id)?;
    let output = next_output(st, &doc.id)?;
    let original = Output {
        copy_no: output.copy_no,
        duplicate: false,
    };
    let (actor, id) = (session.actor(), doc.id.clone());
    let detail = json!({ "printer": printer, "widthMm": width_mm, "lang": lang, "driver": true });
    st.db.write(move |tx| {
        outputs::record_output(tx, &actor, &id, OutputKind::Print, &original, detail)
    })?;
    Ok(A4Printed {
        printer: Some(printer_name.trim().to_owned()),
        copies: 1,
        duplicate: false,
        dialog: status == PrintStatus::Dialog,
    })
}

/// The in-app preview (drafts included, marked BROUILLON); nothing is journaled.
pub(crate) fn print_model(st: &AppState, input: PdfDocumentInput) -> Result<HostPage, AppError> {
    let (_, doc) = require_output(st, &input.id)?;
    Ok(page_of(st, &doc, &input.options, None)?.0)
}

/// `text` as part of a file name the save dialog proposes (letters, digits, `-` and `_`).
fn file_safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// The seller and the buyer of a document as its A4 page prints them (M3's print context: the
/// snapshots the document was validated with), in `lang` alone: the pack pages (an instalment
/// contract) set them out like the document's own page.
pub(crate) fn party_blocks(
    st: &AppState,
    document_id: &str,
    lang: Lang,
) -> Result<(PartyBlock, PartyBlock), AppError> {
    let options = PrintOptions {
        lang: Some(lang),
        second_lang: None,
        monolingual: true,
    };
    let (id, day, now) = (document_id.to_owned(), today(), pos_core::time::now_ms());
    let ctx = st
        .db
        .read(move |c| print_context(c, &id, &options, &day, now))?;
    let buyer = ctx
        .counterpart
        .as_ref()
        .map_or_else(|| DocumentParty::named(PartyRole::Buyer, ""), Clone::clone);
    Ok((
        party_block(&ctx.seller, &st.datapack, lang),
        party_block(&buyer, &st.datapack, lang),
    ))
}

/// The shop as the seller of a page that is no document (a route sheet), in `lang`.
pub(crate) fn seller_block(st: &AppState, lang: Lang) -> Result<PartyBlock, AppError> {
    let seller = st
        .db
        .read(move |c| pos_core::billing::validate::seller_party(c, lang, None))?;
    Ok(party_block(&seller, &st.datapack, lang))
}

/// Prints `page` on this PC's default A4 printer, once.
pub(crate) fn print_page(
    st: &AppState,
    host: &PrintHost,
    page: HostPage,
) -> Result<A4Printed, AppError> {
    let hw = st.hw.config().map_err(hw_error)?;
    let printer = hw.documents.default_printer.clone();
    let setup = setup_for(&page, Paper::A4, hw.documents.margins_mm);
    let status = host.print(page, setup, printer.as_deref(), 1)?;
    Ok(A4Printed {
        printer,
        copies: 1,
        duplicate: false,
        dialog: status == PrintStatus::Dialog,
    })
}

/// Writes `page` as an A4 PDF at `path`.
fn pdf_page(
    st: &AppState,
    host: &PrintHost,
    page: HostPage,
    path: &Path,
) -> Result<PdfSaved, AppError> {
    let margins = st.hw.config().map_err(hw_error)?.documents.margins_mm;
    let setup = setup_for(&page, Paper::A4, margins);
    host.pdf(page, setup, path)?;
    Ok(PdfSaved {
        path: path.to_string_lossy().into_owned(),
        duplicate: false,
    })
}

/// A customer's statement needs `credit.view`, a supplier's `suppliers.manage` or
/// `purchases.view`.
fn may_see_statement(st: &AppState, side: StatementSide) -> Result<Session, AppError> {
    match side {
        StatementSide::Customer => st.require(Permission::CreditView),
        StatementSide::Supplier => {
            st.require_any(&[Permission::SuppliersManage, Permission::PurchasesView])
        }
    }
}

/// The A4 statement of a customer or a supplier over the period.
pub(crate) fn statement_page(
    st: &AppState,
    input: &StatementPrintInput,
) -> Result<HostPage, AppError> {
    may_see_statement(st, input.side)?;
    let (side, id, from, to, lang) = (
        input.side,
        input.party_id.clone(),
        input.from.clone(),
        input.to.clone(),
        input.lang,
    );
    let (pack, now) = (st.datapack.clone(), pos_core::time::now_ms());
    let page = st.db.read(move |c| {
        let seller = pos_core::billing::validate::seller_party(c, lang, None)?;
        let (party, statement) = match side {
            StatementSide::Customer => {
                let p = pos_core::customers::get(c, &id)?;
                let party = StatementParty {
                    name: p.name,
                    code: p.code,
                    address: p.address,
                    nif: p.nif,
                    rc: p.rc,
                    is_supplier: false,
                };
                let statement = pos_core::customers::credit::statement(c, &id, &from, &to)?;
                (party, statement)
            }
            StatementSide::Supplier => {
                let p = pos_core::suppliers::get(c, &id)?;
                let party = StatementParty {
                    name: p.name,
                    code: p.code,
                    address: p.address,
                    nif: p.nif,
                    rc: p.rc,
                    is_supplier: true,
                };
                (party, pos_core::suppliers::statement(c, &id, &from, &to)?)
            }
        };
        Ok(build_statement(
            &seller, &pack, party, &statement, lang, now,
        ))
    })?;
    Ok(HostPage::Statement(Box::new(page)))
}

/// `releve-<code>-<from>-<to>.pdf`, asked before the save dialog opens (the permission first).
fn statement_file_name(st: &AppState, input: &StatementPrintInput) -> Result<String, AppError> {
    may_see_statement(st, input.side)?;
    let (side, id) = (input.side, input.party_id.clone());
    let code = st.db.read(move |c| {
        Ok(match side {
            StatementSide::Customer => pos_core::customers::get(c, &id)?.code,
            StatementSide::Supplier => pos_core::suppliers::get(c, &id)?.code,
        })
    })?;
    Ok(format!(
        "releve-{}-{}-{}.pdf",
        file_safe(&code),
        file_safe(&input.from),
        file_safe(&input.to)
    ))
}

pub(crate) fn print_statement(
    st: &AppState,
    host: &PrintHost,
    input: StatementPrintInput,
) -> Result<A4Printed, AppError> {
    let page = statement_page(st, &input)?;
    let printed = print_page(st, host, page)?;
    journal_statement(st, &input)?;
    Ok(printed)
}

pub(crate) fn statement_pdf_to(
    st: &AppState,
    host: &PrintHost,
    input: StatementPrintInput,
    path: &Path,
) -> Result<PdfSaved, AppError> {
    let page = statement_page(st, &input)?;
    let saved = pdf_page(st, host, page, path)?;
    journal_statement(st, &input)?;
    Ok(saved)
}

/// PDP-003: a customer's statement printed or saved is journaled (`privacy.accessed`, `print`).
fn journal_statement(st: &AppState, input: &StatementPrintInput) -> Result<(), AppError> {
    if input.side != StatementSide::Customer {
        return Ok(());
    }
    let (actor, id) = (st.session()?.actor(), input.party_id.clone());
    st.db.write(move |tx| {
        pos_core::privacy::log_access(tx, &actor, &id, pos_core::privacy::AccessPurpose::Print)
    })
}

/// The warranted line of a validated ticket (`sales.reprint`) or invoice (`documents.invoice`)
/// the user may see (`input.line_id`, else the first one) and the certificate's language.
fn warranty_source(
    st: &AppState,
    input: &WarrantyInput,
) -> Result<(WarrantySource, Lang), AppError> {
    let session = st.require_any(&[Permission::SalesReprint, Permission::DocumentsInvoice])?;
    let (id, line, wanted) = (input.document_id.clone(), input.line_id.clone(), input.lang);
    st.db.read(move |c| {
        let doc = documents::get(c, &id)?;
        if !super::documents::shown(&doc, &session) {
            return Err(AppError::not_found("document"));
        }
        let source = pos_core::billing::warranty::sources(c, &id, line.as_deref())?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::conflict("warranty_not_set"))?;
        Ok((source, wanted.unwrap_or(doc.lang)))
    })
}

/// The warranty certificate (DOC-012, décret 13-327 art. 6).
pub(crate) fn warranty_page(st: &AppState, input: &WarrantyInput) -> Result<HostPage, AppError> {
    let (source, lang) = warranty_source(st, input)?;
    let settings = st.db.read(pos_core::billing::settings::print_settings)?;
    let page = build_warranty(
        &source,
        &settings,
        &st.datapack,
        lang,
        pos_core::time::now_ms(),
    );
    Ok(HostPage::Warranty(Box::new(page)))
}

/// `garantie-<number>.pdf`, asked before the save dialog opens (the checks first).
fn warranty_file_name(st: &AppState, input: &WarrantyInput) -> Result<String, AppError> {
    let (source, _) = warranty_source(st, input)?;
    Ok(format!("garantie-{}.pdf", file_safe(&source.number)))
}

pub(crate) fn print_warranty(
    st: &AppState,
    host: &PrintHost,
    input: WarrantyInput,
) -> Result<A4Printed, AppError> {
    let page = warranty_page(st, &input)?;
    print_page(st, host, page)
}

pub(crate) fn warranty_pdf_to(
    st: &AppState,
    host: &PrintHost,
    input: WarrantyInput,
    path: &Path,
) -> Result<PdfSaved, AppError> {
    let page = warranty_page(st, &input)?;
    pdf_page(st, host, page, path)
}

/// `<stem>-<day>.pdf`, the stem cleaned like the xlsx and csv exports'.
fn table_file_name(target: &ExportTarget) -> String {
    let csv = sheet::file_name(exports::stem(target), &today(), SheetFormat::Csv);
    let stem = csv.strip_suffix(".csv").unwrap_or(&csv);
    format!("{stem}.pdf")
}

/// IMX-013: the PDF of any table M2 exports, with M2's permission checks and its
/// `data.exported` journal for the customer and supplier directories.
pub(crate) fn export_table_pdf_to(
    st: &AppState,
    host: &PrintHost,
    request: PdfExportRequest,
    path: &Path,
) -> Result<ExportDone, AppError> {
    let exports::Built {
        table,
        meta,
        session,
        journaled,
    } = exports::table_of(st, request.target, request.lang)?;
    let (shop, nif) =
        super::data_files::company_of(st.db.read(pos_core::company::get)?, request.lang);
    let page = pos_docs::a4::table::build_table(
        &table,
        &meta,
        &shop,
        nif.as_deref(),
        pos_core::time::now_ms(),
    );
    pdf_page(st, host, HostPage::Table(Box::new(page)), path)?;
    let rows = u32::try_from(table.rows.len()).unwrap_or(u32::MAX);
    exports::journal(st, &session, journaled, rows, path)?;
    Ok(ExportDone {
        path: path.display().to_string(),
        rows,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn documents_print(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: PrintDocumentInput,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_document(&st, &host, input)).await
}

/// Opens the Rust save dialog on `<number>.pdf`; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn documents_export_pdf(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: PdfDocumentInput,
) -> Result<Option<PdfSaved>, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || {
        let (_, doc) = require_output(&st, &input.id)?;
        // A draft is refused before the dialog asks where to save it.
        next_output(&st, &doc.id)?;
        let name = format!(
            "{}.pdf",
            file_safe(doc.number.as_deref().unwrap_or("document"))
        );
        let Some(path) = save_dialog(&app, &window, &name, &["pdf"])? else {
            return Ok(None);
        };
        export_pdf_to(&st, &host, input, &path).map(Some)
    })
    .await
}

/// HW-002: the original receipt through the Windows driver of `printerName` (a `widthMm` roll)
/// after its thermal print answered `printer_driver_only`.
#[tauri::command]
#[specta::specta]
pub async fn documents_print_receipt_roll(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    document_id: String,
    printer_name: String,
    width_mm: u16,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_receipt_roll(&st, &host, document_id, printer_name, width_mm)).await
}

/// The print model of a document for the in-app preview (drafts included).
#[tauri::command]
#[specta::specta]
pub async fn documents_print_model(
    state: State<'_, SharedState>,
    input: PdfDocumentInput,
) -> Result<HostPage, AppError> {
    let st = state.inner().clone();
    blocking(move || print_model(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn statements_print(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: StatementPrintInput,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_statement(&st, &host, input)).await
}

/// Opens the Rust save dialog on `releve-<code>-<from>-<to>.pdf`; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn statements_export_pdf(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: StatementPrintInput,
) -> Result<Option<PdfSaved>, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || {
        let name = statement_file_name(&st, &input)?;
        let Some(path) = save_dialog(&app, &window, &name, &["pdf"])? else {
            return Ok(None);
        };
        statement_pdf_to(&st, &host, input, &path).map(Some)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn warranty_print(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: WarrantyInput,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_warranty(&st, &host, input)).await
}

/// Opens the Rust save dialog on `garantie-<number>.pdf`; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn warranty_export_pdf(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    input: WarrantyInput,
) -> Result<Option<PdfSaved>, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || {
        let name = warranty_file_name(&st, &input)?;
        let Some(path) = save_dialog(&app, &window, &name, &["pdf"])? else {
            return Ok(None);
        };
        warranty_pdf_to(&st, &host, input, &path).map(Some)
    })
    .await
}

/// The PDF of a table (IMX-013): the target's permission is checked before the Rust save dialog
/// opens on `<target>-<day>.pdf`; `None` when the user cancels it.
#[tauri::command]
#[specta::specta]
pub async fn exports_pdf(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    request: PdfExportRequest,
) -> Result<Option<ExportDone>, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    exports::may_export(&st, &request.target)?;
    blocking(move || {
        let name = table_file_name(&request.target);
        let Some(path) = save_dialog(&app, &window, &name, &["pdf"])? else {
            return Ok(None);
        };
        export_table_pdf_to(&st, &host, request, &path).map(Some)
    })
    .await
}
