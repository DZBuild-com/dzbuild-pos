//! Commercial documents (spec §5.1 `documents`, §5.3; AUD-003/004, DOC-001/016/018): a draft
//! becomes a validated document with a gap-free number and a SHA-256 hash chained per
//! (type, series). Validated and cancelled documents are immutable (triggers of 0002 and the
//! checks here); corrections are new documents (bon de retour, avoir). One model serves every
//! kind: tickets and return notes (M1), purchase documents (M2), quotes, proformas, delivery
//! notes, invoices and credit notes (M3, with their parties, links and outputs).

use crate::audit::{self, Actor, GENESIS_HASH, canonical_json};
use crate::company::{PriceMode, TaxRegime};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::numbering;
use crate::payments::PaymentMethod;
use crate::sqlenum::sql_text_enum;
use crate::tax::timbre::StampMode;
use crate::tax::tva::{self, LineAmounts};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use specta::Type;

pub mod links;
pub mod outputs;
pub mod parties;

pub use crate::numbering::DocType;
pub use parties::{DocumentParty, PartyRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DocStatus {
    Draft,
    Validated,
    /// Only a draft can be cancelled; a validated document is corrected by another document.
    Cancelled,
}

impl DocStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            DocStatus::Draft => "draft",
            DocStatus::Validated => "validated",
            DocStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<DocStatus> {
        [DocStatus::Draft, DocStatus::Validated, DocStatus::Cancelled]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(DocStatus);

/// Avoir marchandise (goods come back into stock) or avoir financier (a price reduction),
/// DOC-028; both cite the invoice they correct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CreditKind {
    Goods,
    Financial,
}

impl CreditKind {
    pub const ALL: [CreditKind; 2] = [CreditKind::Goods, CreditKind::Financial];

    pub const fn as_str(self) -> &'static str {
        match self {
            CreditKind::Goods => "goods",
            CreditKind::Financial => "financial",
        }
    }

    pub fn parse(s: &str) -> Option<CreditKind> {
        CreditKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(CreditKind);

/// Where an M3 document came from. `Ticket`: the invoice of a till ticket, which keeps the
/// turnover, the stock and the cash; `Delivery`: the invoice of delivery notes, which moved the
/// stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DocOrigin {
    Direct,
    Quote,
    Proforma,
    Delivery,
    Ticket,
}

impl DocOrigin {
    pub const ALL: [DocOrigin; 5] = [
        DocOrigin::Direct,
        DocOrigin::Quote,
        DocOrigin::Proforma,
        DocOrigin::Delivery,
        DocOrigin::Ticket,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            DocOrigin::Direct => "direct",
            DocOrigin::Quote => "quote",
            DocOrigin::Proforma => "proforma",
            DocOrigin::Delivery => "delivery",
            DocOrigin::Ticket => "ticket",
        }
    }

    pub fn parse(s: &str) -> Option<DocOrigin> {
        DocOrigin::ALL.into_iter().find(|o| o.as_str() == s)
    }
}

sql_text_enum!(DocOrigin);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLine {
    pub id: String,
    pub line_no: i64,
    pub product_id: Option<String>,
    /// Name in the document language at the time of the sale.
    pub description: String,
    /// The name in the document's second language (bilingual documents, UX-042).
    pub description_alt: Option<String>,
    /// Negative on return notes.
    pub qty: Qty,
    pub unit_code: String,
    pub unit_price: Money,
    pub price_overridden: bool,
    pub discount_bp: Rate,
    pub discount_amount: Money,
    pub tax_rate_bp: Rate,
    pub total_ht: Money,
    pub tax_amount: Money,
    pub total_ttc: Money,
    /// CMUP at the time of the sale (margin reports).
    pub cost_price: Money,
    pub lot_id: Option<String>,
    pub serial_no: Option<String>,
    /// Set on the component lines of a composite product.
    pub parent_line_id: Option<String>,
    /// On a return line: the sold line it returns.
    pub origin_line_id: Option<String>,
    pub restock_location_id: Option<String>,
    pub reason_code: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTax {
    pub rate_bp: Rate,
    pub base_ht: Money,
    pub tax_amount: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Payment {
    pub id: String,
    pub method: PaymentMethod,
    /// Positive for a tender, negative for change and refunds.
    pub amount: Money,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub cash_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub doc_type: DocType,
    pub number: Option<String>,
    pub series: String,
    pub fiscal_year: Option<i32>,
    pub status: DocStatus,
    /// `YYYY-MM-DD`, Algiers.
    pub business_date: String,
    pub customer_id: Option<String>,
    pub supplier_id: Option<String>,
    pub user_id: String,
    pub terminal_id: String,
    pub cash_session_id: Option<String>,
    pub price_level_id: Option<String>,
    pub lang: Lang,
    /// Regime and price mode when the document was made (an IFU document never shows TVA).
    pub tax_regime: TaxRegime,
    pub price_mode: PriceMode,
    pub subtotal_ht: Money,
    pub discount_total: Money,
    pub tax_total: Money,
    pub total_ttc: Money,
    pub stamp_duty: Money,
    pub stamp_mode: StampMode,
    pub rounding: Money,
    pub total_due: Money,
    pub paid_total: Money,
    pub change_given: Money,
    pub credit_amount: Money,
    pub note: Option<String>,
    pub parent_id: Option<String>,
    pub source_ref: Option<String>,
    pub validated_at: Option<i64>,
    pub chain_seq: Option<i64>,
    pub prev_hash: Option<String>,
    pub hash: Option<String>,
    pub created_at: i64,
    /// Quotes and proformas: the last day the offer holds.
    pub valid_until: Option<String>,
    /// Invoices settled later: the date printed as "date de règlement" (décret 05-468 art. 3).
    pub due_date: Option<String>,
    /// Bilingual print (UX-042), stored with the document (UX-004).
    pub second_lang: Option<Lang>,
    pub credit_kind: Option<CreditKind>,
    pub origin: Option<DocOrigin>,
    /// Seller then buyer as they were at validation (M3 documents only).
    pub parties: Vec<DocumentParty>,
    pub lines: Vec<DocumentLine>,
    pub taxes: Vec<DocumentTax>,
    pub payments: Vec<Payment>,
}

/// A line to write into a draft. `id` is chosen by the caller so that component lines can name
/// their parent line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftLine {
    pub id: String,
    pub product_id: Option<String>,
    pub description: String,
    pub qty: Qty,
    pub unit_code: String,
    pub unit_price: Money,
    pub price_overridden: bool,
    pub tax_rate_bp: Rate,
    pub amounts: LineAmounts,
    pub cost_price: Money,
    pub parent_line_id: Option<String>,
    pub origin_line_id: Option<String>,
    pub restock_location_id: Option<String>,
    pub reason_code: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftPayment {
    pub method: PaymentMethod,
    pub amount: Money,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
}

/// Everything a draft needs; the line totals and per-rate taxes are computed from `lines`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftDocument {
    pub doc_type: DocType,
    pub terminal_code: String,
    pub business_date: NaiveDate,
    pub customer_id: Option<String>,
    pub supplier_id: Option<String>,
    pub user_id: String,
    pub terminal_id: String,
    pub cash_session_id: Option<String>,
    pub price_level_id: Option<String>,
    pub lang: Lang,
    pub tax_regime: TaxRegime,
    pub price_mode: PriceMode,
    pub stamp_mode: StampMode,
    pub stamp_duty: Money,
    /// The duty added to the amount due (0 when absorbed or off).
    pub stamp_charged: Money,
    pub rounding: Money,
    pub paid_total: Money,
    pub change_given: Money,
    pub credit_amount: Money,
    pub note: Option<String>,
    pub parent_id: Option<String>,
    pub source_ref: Option<String>,
    pub lines: Vec<DraftLine>,
    pub payments: Vec<DraftPayment>,
}

/// One row of the document history (SAL-056, RET-002).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub id: String,
    pub doc_type: DocType,
    pub number: Option<String>,
    pub status: DocStatus,
    pub business_date: String,
    pub validated_at: Option<i64>,
    pub total_due: Money,
    pub user_id: String,
    pub terminal_id: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentQuery {
    pub doc_type: Option<DocType>,
    pub terminal_id: Option<String>,
    pub cash_session_id: Option<String>,
    /// `YYYY-MM-DD`.
    pub business_date: Option<String>,
    /// Start of a number (`TK2026-C1-0001`), any case.
    pub number_prefix: Option<String>,
    /// Keyset pagination: only documents validated before this instant (pass the last row's
    /// `validated_at` together with its id in `before_id`; the order is `validated_at` then `id`,
    /// both descending).
    pub before_validated_at: Option<i64>,
    /// Keyset pagination tie-break: the id of the last row of the previous page, so documents
    /// validated at the same instant (an exchange's return note and ticket) are not skipped.
    /// Without it, the page starts strictly before `before_validated_at`.
    pub before_id: Option<String>,
    /// 1..=200, default 50.
    pub limit: Option<u32>,
}

/// What [`verify_chain`] found for one (type, series) chain; `last_seq`/`last_hash` anchor it
/// (Z report, backup).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocChainReport {
    pub doc_type: DocType,
    pub series: String,
    /// Documents checked.
    pub rows: i64,
    pub first_broken_seq: Option<i64>,
    pub first_broken_number: Option<String>,
    pub last_seq: i64,
    pub last_hash: String,
}

const DOC_SELECT: &str = "SELECT id, doc_type, number, series, fiscal_year, status, business_date,
        customer_id, supplier_id, user_id, terminal_id, cash_session_id, price_level_id, lang,
        tax_regime, price_mode, subtotal_ht, discount_total, tax_total, total_ttc, stamp_duty,
        stamp_mode, rounding, total_due, paid_total, change_given, credit_amount, note, parent_id,
        source_ref, validated_at, chain_seq, prev_hash, hash, created_at, valid_until, due_date,
        second_lang, credit_kind, origin
     FROM documents";

fn map_doc(r: &Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: r.get(0)?,
        doc_type: r.get(1)?,
        number: r.get(2)?,
        series: r.get(3)?,
        fiscal_year: r.get(4)?,
        status: r.get(5)?,
        business_date: r.get(6)?,
        customer_id: r.get(7)?,
        supplier_id: r.get(8)?,
        user_id: r.get(9)?,
        terminal_id: r.get(10)?,
        cash_session_id: r.get(11)?,
        price_level_id: r.get(12)?,
        lang: r.get(13)?,
        tax_regime: r.get(14)?,
        price_mode: r.get(15)?,
        subtotal_ht: r.get(16)?,
        discount_total: r.get(17)?,
        tax_total: r.get(18)?,
        total_ttc: r.get(19)?,
        stamp_duty: r.get(20)?,
        stamp_mode: r.get(21)?,
        rounding: r.get(22)?,
        total_due: r.get(23)?,
        paid_total: r.get(24)?,
        change_given: r.get(25)?,
        credit_amount: r.get(26)?,
        note: r.get(27)?,
        parent_id: r.get(28)?,
        source_ref: r.get(29)?,
        validated_at: r.get(30)?,
        chain_seq: r.get(31)?,
        prev_hash: r.get(32)?,
        hash: r.get(33)?,
        created_at: r.get(34)?,
        valid_until: r.get(35)?,
        due_date: r.get(36)?,
        second_lang: r.get(37)?,
        credit_kind: r.get(38)?,
        origin: r.get(39)?,
        parties: Vec::new(),
        lines: Vec::new(),
        taxes: Vec::new(),
        payments: Vec::new(),
    })
}

fn load_children(conn: &Connection, doc: &mut Document) -> Result<(), AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, line_no, product_id, description, qty, unit_code, unit_price, price_overridden,
                discount_bp, discount_amount, tax_rate_bp, total_ht, tax_amount, total_ttc, cost_price,
                lot_id, serial_no, parent_line_id, origin_line_id, restock_location_id, reason_code, note,
                description_alt
         FROM document_lines WHERE document_id = ?1 ORDER BY line_no",
    )?;
    doc.lines = stmt
        .query_map([&doc.id], |r| {
            Ok(DocumentLine {
                id: r.get(0)?,
                line_no: r.get(1)?,
                product_id: r.get(2)?,
                description: r.get(3)?,
                description_alt: r.get(22)?,
                qty: r.get(4)?,
                unit_code: r.get(5)?,
                unit_price: r.get(6)?,
                price_overridden: r.get(7)?,
                discount_bp: r.get(8)?,
                discount_amount: r.get(9)?,
                tax_rate_bp: r.get(10)?,
                total_ht: r.get(11)?,
                tax_amount: r.get(12)?,
                total_ttc: r.get(13)?,
                cost_price: r.get(14)?,
                lot_id: r.get(15)?,
                serial_no: r.get(16)?,
                parent_line_id: r.get(17)?,
                origin_line_id: r.get(18)?,
                restock_location_id: r.get(19)?,
                reason_code: r.get(20)?,
                note: r.get(21)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT rate_bp, base_ht, tax_amount FROM document_taxes WHERE document_id = ?1 ORDER BY rate_bp DESC",
    )?;
    doc.taxes = stmt
        .query_map([&doc.id], |r| {
            Ok(DocumentTax {
                rate_bp: r.get(0)?,
                base_ht: r.get(1)?,
                tax_amount: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT id, method, amount, reference, cheque_bank, cheque_date, cash_session_id
         FROM payments WHERE document_id = ?1 ORDER BY rowid",
    )?;
    doc.payments = stmt
        .query_map([&doc.id], |r| {
            Ok(Payment {
                id: r.get(0)?,
                method: r.get(1)?,
                amount: r.get(2)?,
                reference: r.get(3)?,
                cheque_bank: r.get(4)?,
                cheque_date: r.get(5)?,
                cash_session_id: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    doc.parties = parties::of(conn, &doc.id)?;
    Ok(())
}

/// A document with its lines, taxes, payments and parties.
pub fn get(conn: &Connection, id: &str) -> Result<Document, AppError> {
    let mut doc = conn
        .prepare_cached(&format!("{DOC_SELECT} WHERE id = ?1"))?
        .query_row([id], map_doc)
        .optional()?
        .ok_or_else(|| AppError::not_found("document"))?;
    load_children(conn, &mut doc)?;
    Ok(doc)
}

/// A validated document by its printed number (the receipt barcode, RET-002).
pub fn find_by_number(conn: &Connection, number: &str) -> Result<Option<Document>, AppError> {
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM documents WHERE number = ?1 AND status = 'validated'",
            [number.trim()],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| get(conn, &id)).transpose()
}

fn too_large() -> AppError {
    AppError::validation("amount", "amount_invalid")
}

/// Writes a draft: the document row, its lines (numbered from 1), per-rate taxes computed from
/// the lines, and its payments. Checks that the amount due is `TTC + charged duty + rounding`.
pub fn insert_draft(
    tx: &Transaction<'_>,
    draft: &DraftDocument,
    now: i64,
) -> Result<String, AppError> {
    let id = new_id();
    insert_draft_as(tx, &id, draft, now)?;
    Ok(id)
}

/// Replaces a draft in place, keeping its id (M2 purchase orders edited before they are sent,
/// M3 drafts saved again or finished). The document row is updated, never deleted: the rows
/// that point at the draft (its stored cart, its parties, the links of the documents it was
/// made from) stay; only its lines, taxes and payments are written again. Its creation time and
/// its M3 fields are kept. A validated or cancelled document is never touched
/// (`document_already_validated`).
pub fn replace_draft(
    tx: &Transaction<'_>,
    id: &str,
    draft: &DraftDocument,
    now: i64,
) -> Result<(), AppError> {
    let current = get(tx, id)?;
    if current.status != DocStatus::Draft {
        return Err(AppError::conflict("document_already_validated"));
    }
    if current.doc_type != draft.doc_type {
        return Err(AppError::validation("docType", "value_invalid"));
    }
    let (totals, total_due) = draft_totals(draft)?;
    for sql in [
        "DELETE FROM payments WHERE document_id = ?1",
        "DELETE FROM document_taxes WHERE document_id = ?1",
        "DELETE FROM document_lines WHERE document_id = ?1",
    ] {
        tx.execute(sql, [id])?;
    }
    let changed = tx
        .prepare_cached(
            "UPDATE documents SET series = ?2, business_date = ?3, customer_id = ?4, supplier_id = ?5,
                 user_id = ?6, terminal_id = ?7, cash_session_id = ?8, price_level_id = ?9, lang = ?10,
                 tax_regime = ?11, price_mode = ?12, subtotal_ht = ?13, discount_total = ?14,
                 tax_total = ?15, total_ttc = ?16, stamp_duty = ?17, stamp_mode = ?18, rounding = ?19,
                 total_due = ?20, paid_total = ?21, change_given = ?22, credit_amount = ?23, note = ?24,
                 parent_id = ?25, source_ref = ?26, updated_at = ?27
             WHERE id = ?1 AND status = 'draft'",
        )?
        .execute(params![
            id,
            draft.doc_type.series(&draft.terminal_code),
            draft.business_date.format("%Y-%m-%d").to_string(),
            draft.customer_id,
            draft.supplier_id,
            draft.user_id,
            draft.terminal_id,
            draft.cash_session_id,
            draft.price_level_id,
            draft.lang,
            draft.tax_regime,
            draft.price_mode,
            totals.subtotal_ht,
            totals.discount_total,
            totals.tax_total,
            totals.total_ttc,
            draft.stamp_duty,
            draft.stamp_mode,
            draft.rounding,
            total_due,
            draft.paid_total,
            draft.change_given,
            draft.credit_amount,
            draft.note,
            draft.parent_id,
            draft.source_ref,
            now
        ])?;
    if changed != 1 {
        return Err(AppError::conflict("document_already_validated"));
    }
    insert_children(tx, id, draft, &totals, now)
}

/// A draft's totals from its lines and its amount due (`TTC + charged duty + rounding`); a
/// draft has at least one line.
fn draft_totals(draft: &DraftDocument) -> Result<(tva::Totals, Money), AppError> {
    if draft.lines.is_empty() {
        return Err(AppError::validation("lines", "document_empty"));
    }
    let totals = tva::totals(draft.lines.iter().map(|l| (l.tax_rate_bp, &l.amounts)))?;
    let total_due = totals
        .total_ttc
        .checked_add(draft.stamp_charged)
        .and_then(|m| m.checked_add(draft.rounding))
        .ok_or_else(too_large)?;
    Ok((totals, total_due))
}

/// [`insert_draft`] under a chosen id.
pub(crate) fn insert_draft_as(
    tx: &Transaction<'_>,
    id: &str,
    draft: &DraftDocument,
    now: i64,
) -> Result<(), AppError> {
    let (totals, total_due) = draft_totals(draft)?;
    tx.prepare_cached(
        "INSERT INTO documents (id, doc_type, series, business_date, customer_id, supplier_id, user_id,
             terminal_id, cash_session_id, price_level_id, lang, tax_regime, price_mode, subtotal_ht,
             discount_total, tax_total, total_ttc, stamp_duty, stamp_mode, rounding, total_due, paid_total,
             change_given, credit_amount, note, parent_id, source_ref, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                 ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?28)",
    )?
    .execute(params![
        id,
        draft.doc_type,
        draft.doc_type.series(&draft.terminal_code),
        draft.business_date.format("%Y-%m-%d").to_string(),
        draft.customer_id,
        draft.supplier_id,
        draft.user_id,
        draft.terminal_id,
        draft.cash_session_id,
        draft.price_level_id,
        draft.lang,
        draft.tax_regime,
        draft.price_mode,
        totals.subtotal_ht,
        totals.discount_total,
        totals.tax_total,
        totals.total_ttc,
        draft.stamp_duty,
        draft.stamp_mode,
        draft.rounding,
        total_due,
        draft.paid_total,
        draft.change_given,
        draft.credit_amount,
        draft.note,
        draft.parent_id,
        draft.source_ref,
        now
    ])?;
    insert_children(tx, id, draft, &totals, now)
}

/// The lines (numbered from 1), per-rate taxes and payments of the draft `id`.
fn insert_children(
    tx: &Transaction<'_>,
    id: &str,
    draft: &DraftDocument,
    totals: &tva::Totals,
    now: i64,
) -> Result<(), AppError> {
    let mut insert_line = tx.prepare_cached(
        "INSERT INTO document_lines (id, document_id, line_no, product_id, description, qty, unit_code,
             unit_price, price_overridden, discount_bp, discount_amount, tax_rate_bp, total_ht, tax_amount,
             total_ttc, cost_price, parent_line_id, origin_line_id, restock_location_id, reason_code, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
    )?;
    for (index, l) in draft.lines.iter().enumerate() {
        let line_no = i64::try_from(index + 1).map_err(|_| too_large())?;
        insert_line.execute(params![
            l.id,
            id,
            line_no,
            l.product_id,
            l.description,
            l.qty,
            l.unit_code,
            l.unit_price,
            l.price_overridden,
            l.amounts.discount_bp,
            l.amounts.discount_amount,
            l.tax_rate_bp,
            l.amounts.total_ht,
            l.amounts.tax,
            l.amounts.total_ttc,
            l.cost_price,
            l.parent_line_id,
            l.origin_line_id,
            l.restock_location_id,
            l.reason_code,
            l.note
        ])?;
    }
    let mut insert_tax = tx.prepare_cached(
        "INSERT INTO document_taxes (document_id, rate_bp, base_ht, tax_amount) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for t in &totals.per_rate {
        insert_tax.execute(params![id, t.rate_bp, t.base_ht, t.tax])?;
    }
    let mut insert_payment = tx.prepare_cached(
        "INSERT INTO payments (id, document_id, cash_session_id, method, amount, reference, cheque_bank,
             cheque_date, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for p in &draft.payments {
        insert_payment.execute(params![
            new_id(),
            id,
            draft.cash_session_id,
            p.method,
            p.amount,
            p.reference,
            p.cheque_bank,
            p.cheque_date,
            now,
            draft.user_id,
            draft.terminal_id
        ])?;
    }
    Ok(())
}

/// The M3 fields a draft carries until it is validated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftExtras {
    pub valid_until: Option<String>,
    pub due_date: Option<String>,
    pub second_lang: Option<Lang>,
    pub credit_kind: Option<CreditKind>,
    pub origin: Option<DocOrigin>,
}

pub fn extras_of(doc: &Document) -> DraftExtras {
    DraftExtras {
        valid_until: doc.valid_until.clone(),
        due_date: doc.due_date.clone(),
        second_lang: doc.second_lang,
        credit_kind: doc.credit_kind,
        origin: doc.origin,
    }
}

/// The kind of the draft `id`; a validated or cancelled document is `document_already_validated`.
fn draft_type(tx: &Transaction<'_>, id: &str) -> Result<DocType, AppError> {
    let row: Option<(DocType, DocStatus)> = tx
        .query_row(
            "SELECT doc_type, status FROM documents WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        None => Err(AppError::not_found("document")),
        Some((_, status)) if status != DocStatus::Draft => {
            Err(AppError::conflict("document_already_validated"))
        }
        Some((doc_type, _)) => Ok(doc_type),
    }
}

/// Writes the M3 fields of a draft (the trigger of 0004 guards the same rules).
pub fn set_extras(tx: &Transaction<'_>, id: &str, extras: &DraftExtras) -> Result<(), AppError> {
    let doc_type = draft_type(tx, id)?;
    if extras.valid_until.is_some() && !matches!(doc_type, DocType::Quote | DocType::Proforma) {
        return Err(AppError::validation("validUntil", "value_invalid"));
    }
    if extras.due_date.is_some() && doc_type != DocType::Invoice {
        return Err(AppError::validation("dueDate", "value_invalid"));
    }
    if extras.credit_kind.is_some() && doc_type != DocType::CreditNote {
        return Err(AppError::validation("creditKind", "value_invalid"));
    }
    for (field, day) in [
        ("validUntil", &extras.valid_until),
        ("dueDate", &extras.due_date),
    ] {
        if day
            .as_deref()
            .is_some_and(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err())
        {
            return Err(AppError::validation(field, "date_invalid"));
        }
    }
    tx.execute(
        "UPDATE documents SET valid_until = ?2, due_date = ?3, second_lang = ?4, credit_kind = ?5,
             origin = ?6, updated_at = ?7
         WHERE id = ?1 AND status = 'draft'",
        params![
            id,
            extras.valid_until,
            extras.due_date,
            extras.second_lang,
            extras.credit_kind,
            extras.origin,
            crate::time::now_ms()
        ],
    )?;
    Ok(())
}

/// A draft takes the day it is validated (décret 05-468 art. 3: "date d'établissement").
pub fn restamp(
    tx: &Transaction<'_>,
    id: &str,
    business_date: NaiveDate,
    now: i64,
) -> Result<(), AppError> {
    draft_type(tx, id)?;
    tx.execute(
        "UPDATE documents SET business_date = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'draft'",
        params![id, business_date.format("%Y-%m-%d").to_string(), now],
    )?;
    Ok(())
}

/// The names of the lines in the second language, taken just before validation.
pub fn set_alt_descriptions(
    tx: &Transaction<'_>,
    id: &str,
    names: &[(String, String)],
) -> Result<(), AppError> {
    draft_type(tx, id)?;
    let mut stmt = tx.prepare_cached(
        "UPDATE document_lines SET description_alt = ?3 WHERE document_id = ?1 AND id = ?2",
    )?;
    for (line_id, name) in names {
        stmt.execute(params![id, line_id, name])?;
    }
    Ok(())
}

/// The CMUP of the lines on the day they leave the stock (margin reports).
pub fn set_line_costs(
    tx: &Transaction<'_>,
    id: &str,
    costs: &[(String, Money)],
) -> Result<(), AppError> {
    draft_type(tx, id)?;
    let mut stmt = tx.prepare_cached(
        "UPDATE document_lines SET cost_price = ?3 WHERE document_id = ?1 AND id = ?2",
    )?;
    for (line_id, cost) in costs {
        stmt.execute(params![id, line_id, cost])?;
    }
    Ok(())
}

/// Keeps the cart the draft was typed from, so the editor reopens it exactly.
pub fn store_cart(tx: &Transaction<'_>, id: &str, cart: &Value, now: i64) -> Result<(), AppError> {
    draft_type(tx, id)?;
    tx.execute(
        "INSERT INTO document_drafts (document_id, cart, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (document_id) DO UPDATE SET cart = excluded.cart, updated_at = excluded.updated_at",
        params![id, serde_json::to_string(cart)?, now],
    )?;
    Ok(())
}

/// The cart a draft was typed from, when one was stored.
pub fn stored_cart(conn: &Connection, id: &str) -> Result<Option<Value>, AppError> {
    let text: Option<String> = conn
        .query_row(
            "SELECT cart FROM document_drafts WHERE document_id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(text.map(|t| serde_json::from_str(&t)).transpose()?)
}

/// The version of [`canonical`]: a later extension of the hashed form bumps it, and a verifier
/// picks the canonical form of each document by the `v` it was hashed with.
pub const CANONICAL_VERSION: i64 = 1;

/// The JSON that is hashed (spec §5.3): id, type, number, business date, customer, lines,
/// payments and totals, plus the series, fiscal year, parent and validation time, and every
/// field that decides how the document prints and where it counts fiscally: tax regime, price
/// and stamp modes, terminal, user, cash session, language and note; per payment the cheque's
/// bank and date; per line the price override, reason and restock location. `v` names this form
/// ([`CANONICAL_VERSION`]); it is frozen from the first real ticket on. The M3 fields (validity,
/// due date, second language, credit kind, origin, the seller and buyer snapshots) are added
/// under `ext`, and a line's `descriptionAlt`, only when present, so the documents of M0-M2
/// hash exactly as before.
pub fn canonical(doc: &Document) -> Value {
    let lines: Vec<Value> = doc
        .lines
        .iter()
        .map(|l| {
            let mut line = json!({
                "lineNo": l.line_no,
                "productId": l.product_id,
                "description": l.description,
                "qty": l.qty,
                "unitCode": l.unit_code,
                "unitPrice": l.unit_price,
                "discountBp": l.discount_bp,
                "discountAmount": l.discount_amount,
                "taxRateBp": l.tax_rate_bp,
                "totalHt": l.total_ht,
                "taxAmount": l.tax_amount,
                "totalTtc": l.total_ttc,
                "parentLineId": l.parent_line_id,
                "originLineId": l.origin_line_id,
                "priceOverridden": l.price_overridden,
                "reasonCode": l.reason_code,
                "restockLocationId": l.restock_location_id,
            });
            if let (Some(alt), Some(map)) = (&l.description_alt, line.as_object_mut()) {
                map.insert("descriptionAlt".into(), json!(alt));
            }
            line
        })
        .collect();
    let payments: Vec<Value> = doc
        .payments
        .iter()
        .map(|p| {
            json!({
                "method": p.method,
                "amount": p.amount,
                "reference": p.reference,
                "chequeBank": p.cheque_bank,
                "chequeDate": p.cheque_date,
            })
        })
        .collect();
    let mut value = json!({
        "v": CANONICAL_VERSION,
        "id": doc.id,
        "docType": doc.doc_type,
        "number": doc.number,
        "series": doc.series,
        "fiscalYear": doc.fiscal_year,
        "businessDate": doc.business_date,
        "customerId": doc.customer_id,
        "parentId": doc.parent_id,
        "validatedAt": doc.validated_at,
        "taxRegime": doc.tax_regime,
        "priceMode": doc.price_mode,
        "stampMode": doc.stamp_mode,
        "terminalId": doc.terminal_id,
        "userId": doc.user_id,
        "cashSessionId": doc.cash_session_id,
        "lang": doc.lang,
        "note": doc.note,
        "lines": lines,
        "payments": payments,
        "totals": {
            "subtotalHt": doc.subtotal_ht,
            "discountTotal": doc.discount_total,
            "taxTotal": doc.tax_total,
            "totalTtc": doc.total_ttc,
            "stampDuty": doc.stamp_duty,
            "rounding": doc.rounding,
            "totalDue": doc.total_due,
            "paidTotal": doc.paid_total,
            "changeGiven": doc.change_given,
            "creditAmount": doc.credit_amount,
        },
    });
    if let (Some(ext), Some(map)) = (extension(doc), value.as_object_mut()) {
        map.insert("ext".into(), ext);
    }
    value
}

/// The M3 part of the hash: only on documents that have one of these fields, so every document
/// validated before M3 hashes exactly as it did.
fn extension(doc: &Document) -> Option<Value> {
    let mut ext = serde_json::Map::new();
    if let Some(v) = &doc.valid_until {
        ext.insert("validUntil".into(), json!(v));
    }
    if let Some(v) = &doc.due_date {
        ext.insert("dueDate".into(), json!(v));
    }
    if let Some(v) = doc.second_lang {
        ext.insert("secondLang".into(), json!(v));
    }
    if let Some(v) = doc.credit_kind {
        ext.insert("creditKind".into(), json!(v));
    }
    if let Some(v) = doc.origin {
        ext.insert("origin".into(), json!(v));
    }
    if !doc.parties.is_empty() {
        ext.insert(
            "parties".into(),
            Value::Array(doc.parties.iter().map(parties::party_json).collect()),
        );
    }
    (!ext.is_empty()).then_some(Value::Object(ext))
}

/// `hex(SHA-256(prev_hash ␟ canonical_json(canonical(doc))))`, ␟ = U+001F.
pub fn document_hash(prev_hash: &str, doc: &Document) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update([0x1f_u8]);
    hasher.update(canonical_json(&canonical(doc)).as_bytes());
    hex::encode(hasher.finalize().as_slice())
}

/// The last link of a chain: `(chain_seq, hash)`, or `(0, GENESIS_HASH)` for an empty chain.
pub fn anchor(
    conn: &Connection,
    doc_type: DocType,
    series: &str,
) -> Result<(i64, String), AppError> {
    let last: Option<(i64, String)> = conn
        .prepare_cached(
            "SELECT chain_seq, hash FROM documents WHERE doc_type = ?1 AND series = ?2
               AND chain_seq IS NOT NULL ORDER BY chain_seq DESC LIMIT 1",
        )?
        .query_row(params![doc_type, series], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    Ok(last.unwrap_or_else(|| (0, GENESIS_HASH.to_owned())))
}

/// Validates a draft inside the caller's transaction: assigns the next number of its series,
/// links it to the previous validated document of the same type and series, and hashes it.
/// The caller audits the business operation (`sale.created`, `return.created`, …).
pub fn validate(tx: &Transaction<'_>, id: &str, now: i64) -> Result<Document, AppError> {
    let mut doc = get(tx, id)?;
    if doc.status != DocStatus::Draft {
        return Err(AppError::conflict("document_already_validated"));
    }
    if doc.lines.is_empty() {
        return Err(AppError::validation("lines", "document_empty"));
    }
    // Finding 39: the demo shop issues no invoice and no credit note (its tickets carry the
    // "DÉMO – sans valeur fiscale" mark; an invoice is the document a buyer deducts VAT with).
    if matches!(doc.doc_type, DocType::Invoice | DocType::CreditNote) {
        crate::demo::refuse_in_demo(tx)?;
    }
    let business_date = NaiveDate::parse_from_str(&doc.business_date, "%Y-%m-%d")
        .map_err(|_| AppError::Internal(format!("bad business date on document {id}")))?;
    let assigned = numbering::assign(tx, doc.doc_type, &doc.series, business_date)?;
    let (prev_seq, prev_hash) = anchor(tx, doc.doc_type, &doc.series)?;
    doc.number = Some(assigned.number);
    doc.fiscal_year = Some(assigned.fiscal_year);
    doc.validated_at = Some(now);
    doc.chain_seq = Some(prev_seq + 1);
    let hash = document_hash(&prev_hash, &doc);
    let changed = tx
        .prepare_cached(
            "UPDATE documents SET status = 'validated', number = ?2, fiscal_year = ?3, validated_at = ?4,
                 chain_seq = ?5, prev_hash = ?6, hash = ?7, updated_at = ?4
             WHERE id = ?1 AND status = 'draft'",
        )?
        .execute(params![id, doc.number, doc.fiscal_year, now, doc.chain_seq, prev_hash, hash])?;
    if changed != 1 {
        return Err(AppError::conflict("document_already_validated"));
    }
    get(tx, id)
}

/// Cancels a draft (a validated document is corrected by a return or an avoir instead).
pub fn cancel(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let doc = get(tx, id)?;
    if doc.status != DocStatus::Draft {
        return Err(AppError::conflict("document_already_validated"));
    }
    tx.execute(
        "UPDATE documents SET status = 'cancelled', updated_at = ?2 WHERE id = ?1",
        params![id, crate::time::now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "document.cancelled",
            "document",
            Some(id),
            json!({ "docType": doc.doc_type }),
        ),
    )?;
    Ok(())
}

/// Checks the chain of (`doc_type`, `series`) after the anchor `(from_seq, from_hash)` (use
/// `(0, GENESIS_HASH)` for the whole chain): consecutive `chain_seq`, `prev_hash` equal to the
/// previous hash, and each hash recomputed from the stored document. Documents deleted at the
/// end are caught by the counters: every validation took exactly one number of the series, and
/// the only numbers no document took are the journaled skips of a restore
/// (`numbering::issued`).
pub fn verify_chain_from(
    conn: &Connection,
    doc_type: DocType,
    series: &str,
    from_seq: i64,
    from_hash: &str,
) -> Result<DocChainReport, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM documents WHERE doc_type = ?1 AND series = ?2 AND chain_seq > ?3
         ORDER BY chain_seq",
    )?;
    let ids = stmt
        .query_map(params![doc_type, series, from_seq], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut report = DocChainReport {
        doc_type,
        series: series.to_owned(),
        rows: 0,
        first_broken_seq: None,
        first_broken_number: None,
        last_seq: from_seq,
        last_hash: from_hash.to_owned(),
    };
    for id in ids {
        report.rows += 1;
        if report.first_broken_seq.is_some() {
            continue;
        }
        let doc = get(conn, &id)?;
        let seq = doc.chain_seq.unwrap_or(0);
        let intact = seq == report.last_seq + 1
            && doc.prev_hash.as_deref() == Some(report.last_hash.as_str())
            && doc.hash.as_deref() == Some(document_hash(&report.last_hash, &doc).as_str());
        if !intact {
            report.first_broken_seq = Some(seq);
            report.first_broken_number = doc.number.clone();
            continue;
        }
        report.last_seq = seq;
        report.last_hash = doc.hash.unwrap_or_default();
    }
    let issued = numbering::issued(conn, doc_type.as_str(), series)?;
    if report.first_broken_seq.is_none() && issued > report.last_seq {
        report.first_broken_seq = Some(report.last_seq + 1);
    }
    Ok(report)
}

/// The whole chain of one (type, series).
pub fn verify_chain(
    conn: &Connection,
    doc_type: DocType,
    series: &str,
) -> Result<DocChainReport, AppError> {
    verify_chain_from(conn, doc_type, series, 0, GENESIS_HASH)
}

/// Every chain in the database (the "Vérifier l'intégrité" screen, AUD-004): the chains that
/// have documents and the ones that only have a counter, so a chain whose documents were all
/// deleted is still checked (and reported broken at its first number). The perpetual Z counter
/// and the counters that are not a document type (customer codes `next_code`, versement
/// receipts `assign_custom`) are not document chains.
pub fn verify_all(conn: &Connection) -> Result<Vec<DocChainReport>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT doc_type, series FROM documents WHERE chain_seq IS NOT NULL
         UNION
         SELECT doc_type, series FROM counters WHERE doc_type <> ?1
         ORDER BY doc_type, series",
    )?;
    let chains = stmt
        .query_map([crate::numbering::Z_COUNTER], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    chains
        .into_iter()
        .filter_map(|(t, s)| DocType::parse(&t).map(|t| (t, s)))
        .map(|(t, s)| verify_chain(conn, t, &s))
        .collect()
}

/// Validated documents, newest first, filtered (SAL-056 history, RET-002 search by date). Sales
/// documents only, whatever the type asked: supplier documents (returns to suppliers `BR…-F-`
/// included) are listed by `purchases::list`.
pub fn list(conn: &Connection, q: &DocumentQuery) -> Result<Vec<DocumentSummary>, AppError> {
    list_of(conn, q, &DocType::ALL)
}

/// [`list`] of the kinds `kinds` only (what the viewer may see): the filter is in the query, so
/// a page is always whole whatever the kinds left out.
pub fn list_of(
    conn: &Connection,
    q: &DocumentQuery,
    kinds: &[DocType],
) -> Result<Vec<DocumentSummary>, AppError> {
    let kinds = serde_json::to_string(kinds)?;
    let limit = i64::from(q.limit.unwrap_or(50).clamp(1, 200));
    let prefix = q
        .number_prefix
        .as_deref()
        .map(|p| format!("{}%", p.trim().to_uppercase()));
    let mut stmt = conn.prepare_cached(
        "SELECT id, doc_type, number, status, business_date, validated_at, total_due, user_id, terminal_id,
                parent_id
         FROM documents
         WHERE status = 'validated'
           AND (?1 IS NULL OR doc_type = ?1)
           AND supplier_id IS NULL
           AND (?2 IS NULL OR terminal_id = ?2)
           AND (?3 IS NULL OR cash_session_id = ?3)
           AND (?4 IS NULL OR business_date = ?4)
           AND (?5 IS NULL OR number LIKE ?5)
           AND (?6 IS NULL OR validated_at < ?6 OR (validated_at = ?6 AND id < ?8))
           AND doc_type IN (SELECT value FROM json_each(?9))
         ORDER BY validated_at DESC, id DESC LIMIT ?7",
    )?;
    let rows = stmt
        .query_map(
            params![
                q.doc_type,
                q.terminal_id,
                q.cash_session_id,
                q.business_date,
                prefix,
                q.before_validated_at,
                limit,
                q.before_id,
                kinds
            ],
            |r| {
                Ok(DocumentSummary {
                    id: r.get(0)?,
                    doc_type: r.get(1)?,
                    number: r.get(2)?,
                    status: r.get(3)?,
                    business_date: r.get(4)?,
                    validated_at: r.get(5)?,
                    total_due: r.get(6)?,
                    user_id: r.get(7)?,
                    terminal_id: r.get(8)?,
                    parent_id: r.get(9)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// How many times a document was reprinted (a reprint is marked DUPLICATA, DOC-018).
pub fn reprint_count(conn: &Connection, id: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'document.reprinted' AND entity_id = ?1",
        [id],
        |r| r.get(0),
    )?)
}

/// Records a reprint (SAL-056) and returns the number of reprints so far.
pub fn record_reprint(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<i64, AppError> {
    let doc = get(tx, id)?;
    if doc.status != DocStatus::Validated {
        return Err(AppError::conflict("document_not_validated"));
    }
    audit::append(
        tx,
        &actor.event(
            "document.reprinted",
            "document",
            Some(id),
            json!({ "number": doc.number }),
        ),
    )?;
    reprint_count(tx, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tax::tva::{Discount, LineInput, compute_line};
    use crate::testkit::{NOON, shop};

    fn line(desc: &str, unit_price: i64, qty: i64) -> DraftLine {
        let amounts = compute_line(
            PriceMode::Ttc,
            &LineInput {
                unit_price: Money(unit_price),
                qty: Qty(qty),
                discount: Discount::None,
                rate: Rate(1_900),
            },
        )
        .unwrap();
        DraftLine {
            id: new_id(),
            product_id: None,
            description: desc.into(),
            qty: Qty(qty),
            unit_code: "pc".into(),
            unit_price: Money(unit_price),
            price_overridden: false,
            tax_rate_bp: Rate(1_900),
            amounts,
            cost_price: Money::ZERO,
            parent_line_id: None,
            origin_line_id: None,
            restock_location_id: None,
            reason_code: None,
            note: None,
        }
    }

    fn draft(lines: Vec<DraftLine>, paid: i64, day: &str) -> DraftDocument {
        DraftDocument {
            doc_type: DocType::Ticket,
            terminal_code: "C1".into(),
            business_date: NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap(),
            customer_id: None,
            supplier_id: None,
            user_id: "u1".into(),
            terminal_id: "t1".into(),
            cash_session_id: None,
            price_level_id: None,
            lang: Lang::Fr,
            tax_regime: TaxRegime::Reel,
            price_mode: PriceMode::Ttc,
            stamp_mode: StampMode::ChargeCustomer,
            stamp_duty: Money::ZERO,
            stamp_charged: Money::ZERO,
            rounding: Money::ZERO,
            paid_total: Money(paid),
            change_given: Money::ZERO,
            credit_amount: Money::ZERO,
            note: None,
            parent_id: None,
            source_ref: None,
            lines,
            payments: vec![DraftPayment {
                method: PaymentMethod::Cash,
                amount: Money(paid),
                reference: None,
                cheque_bank: None,
                cheque_date: None,
            }],
        }
    }

    /// One line of `product`, as a ticket, or as a purchase order of supplier "X" (no payment).
    fn sample_draft(
        shop: &crate::testkit::Shop,
        product: &crate::catalog::products::Product,
        doc_type: DocType,
    ) -> DraftDocument {
        let mut l = line(&product.name_fr, product.sell_price.0, 1_000);
        l.product_id = Some(product.id.clone());
        let mut d = draft(vec![l], product.sell_price.0, "2026-09-28");
        d.doc_type = doc_type;
        if doc_type == DocType::PurchaseOrder {
            d.supplier_id = Some(shop.supplier("X").id);
            d.paid_total = Money::ZERO;
            d.payments = Vec::new();
        }
        d
    }

    fn validated(db: &crate::db::Db, d: DraftDocument) -> Document {
        db.write(move |tx| {
            let id = insert_draft(tx, &d, NOON)?;
            validate(tx, &id, NOON)
        })
        .unwrap()
    }

    #[test]
    fn a_validated_ticket_has_number_totals_taxes_and_hash() {
        let shop = shop();
        let doc = validated(
            &shop.db,
            draft(
                vec![line("Lait", 11_900, 2_000), line("Pain", 1_000, 1_000)],
                24_800,
                "2026-09-28",
            ),
        );
        assert_eq!(doc.number.as_deref(), Some("TK2026-C1-000001"));
        assert_eq!(
            (doc.status, doc.fiscal_year, doc.chain_seq),
            (DocStatus::Validated, Some(2026), Some(1))
        );
        assert_eq!(
            (doc.total_ttc, doc.tax_total, doc.subtotal_ht, doc.total_due),
            (Money(24_800), Money(3_960), Money(20_840), Money(24_800))
        );
        assert_eq!(
            doc.taxes,
            [DocumentTax {
                rate_bp: Rate(1_900),
                base_ht: Money(20_840),
                tax_amount: Money(3_960)
            }]
        );
        assert_eq!(
            doc.lines.iter().map(|l| l.line_no).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(doc.prev_hash.as_deref(), Some(GENESIS_HASH));
        assert_eq!(doc.hash.clone().unwrap(), document_hash(GENESIS_HASH, &doc));
        let second = validated(
            &shop.db,
            draft(vec![line("Sucre", 9_000, 1_000)], 9_000, "2026-09-28"),
        );
        assert_eq!(second.number.as_deref(), Some("TK2026-C1-000002"));
        assert_eq!(second.prev_hash, doc.hash);
        let found = shop
            .db
            .read(|c| find_by_number(c, " TK2026-C1-000002 "))
            .unwrap()
            .unwrap();
        assert_eq!(found.id, second.id);
    }

    #[test]
    fn validated_documents_cannot_change_and_drafts_can_be_cancelled_only_once() {
        let shop = shop();
        let doc = validated(
            &shop.db,
            draft(vec![line("Lait", 11_900, 1_000)], 11_900, "2026-09-28"),
        );
        let id = doc.id.clone();
        let err = shop
            .db
            .write(move |tx| validate(tx, &id, NOON))
            .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
        let id = doc.id.clone();
        let actor = shop.owner.actor();
        let err = shop
            .db
            .write(move |tx| cancel(tx, &actor, &id))
            .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
        let id = doc.id.clone();
        let err = shop
            .db
            .write(move |tx| {
                Ok(tx.execute(
                    "UPDATE document_lines SET qty = 5000 WHERE document_id = ?1",
                    [&id],
                )?)
            })
            .unwrap_err();
        assert_eq!(err.code(), "constraint_violation");
        let d = draft(vec![line("Pain", 1_000, 1_000)], 1_000, "2026-09-28");
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                let id = insert_draft(tx, &d, NOON)?;
                cancel(tx, &actor, &id)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM documents WHERE status = 'cancelled' AND number IS NULL"
            ),
            1
        );
        let empty = draft(Vec::new(), 0, "2026-09-28");
        let err = shop
            .db
            .write(move |tx| insert_draft(tx, &empty, NOON))
            .unwrap_err();
        assert_eq!(err.code(), "document_empty");
    }

    #[test]
    fn a_draft_is_replaced_in_place_and_a_validated_document_is_not() {
        let shop = crate::testkit::shop();
        let p = shop.product("Lait", Money(15_000), &shop.tva9);
        let mut draft = sample_draft(&shop, &p, DocType::PurchaseOrder);
        let id = shop
            .db
            .write({
                let d = draft.clone();
                move |tx| insert_draft(tx, &d, 1)
            })
            .unwrap();
        draft.note = Some("urgent".into());
        shop.db
            .write({
                let (id, d) = (id.clone(), draft.clone());
                move |tx| replace_draft(tx, &id, &d, 2)
            })
            .unwrap();
        assert_eq!(
            shop.db
                .read({
                    let id = id.clone();
                    move |c| get(c, &id)
                })
                .unwrap()
                .note
                .as_deref(),
            Some("urgent")
        );
        shop.db
            .write({
                let id = id.clone();
                move |tx| validate(tx, &id, 3).map(drop)
            })
            .unwrap();
        let err = shop
            .db
            .write(move |tx| replace_draft(tx, &id, &draft, 4))
            .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
    }

    #[test]
    fn the_chain_verifies_and_reports_tampering_and_truncation() {
        let shop = shop();
        for price in [1_000, 2_000, 3_000] {
            validated(
                &shop.db,
                draft(vec![line("Article", price, 1_000)], price, "2026-09-28"),
            );
        }
        let report = shop
            .db
            .read(|c| verify_chain(c, DocType::Ticket, "C1"))
            .unwrap();
        assert_eq!(
            (report.rows, report.first_broken_seq, report.last_seq),
            (3, None, 3)
        );
        let all = shop.db.read(verify_all).unwrap();
        assert_eq!(all.len(), 1);
        // Someone with the database file drops the trigger and edits a total.
        shop.db
            .write(|tx| {
                tx.execute_batch(
                    "DROP TRIGGER document_lines_draft_update;
                     UPDATE document_lines SET description = 'Autre' WHERE document_id =
                       (SELECT id FROM documents WHERE chain_seq = 2);",
                )?;
                Ok(())
            })
            .unwrap();
        let report = shop
            .db
            .read(|c| verify_chain(c, DocType::Ticket, "C1"))
            .unwrap();
        assert_eq!(report.first_broken_seq, Some(2));
        assert_eq!(
            report.first_broken_number.as_deref(),
            Some("TK2026-C1-000002")
        );
        assert_eq!(report.last_seq, 1);
    }

    /// Every field that decides how a ticket prints or where it counts fiscally is hashed: a
    /// change to any of them breaks the chain (spec §5.3, the `v` 1 form).
    #[test]
    fn every_printed_and_fiscal_field_is_hashed() {
        let doc = {
            let shop = shop();
            validated(
                &shop.db,
                draft(vec![line("Article", 1_000, 1_000)], 1_000, "2026-09-28"),
            )
        };
        assert_eq!(canonical(&doc)["v"], json!(CANONICAL_VERSION));
        for sql in [
            "UPDATE documents SET tax_regime = 'ifu'",
            "UPDATE documents SET price_mode = 'ht'",
            "UPDATE documents SET stamp_mode = 'off'",
            "UPDATE documents SET terminal_id = 'elsewhere'",
            "UPDATE documents SET user_id = 'someone-else'",
            "UPDATE documents SET cash_session_id = (SELECT id FROM cash_sessions)",
            "UPDATE documents SET lang = 'ar'",
            "UPDATE documents SET note = 'hors caisse'",
            "UPDATE payments SET cheque_bank = 'BNA'",
            "UPDATE payments SET cheque_date = '2026-10-01'",
            "UPDATE document_lines SET price_overridden = 1",
            "UPDATE document_lines SET reason_code = 'defective'",
            "UPDATE document_lines SET restock_location_id = (SELECT id FROM stock_locations LIMIT 1)",
        ] {
            let shop = shop();
            shop.open_till(0);
            validated(
                &shop.db,
                draft(vec![line("Article", 1_000, 1_000)], 1_000, "2026-09-28"),
            );
            let tamper = format!(
                "DROP TRIGGER documents_final_once_validated;
                 DROP TRIGGER document_lines_draft_update;
                 DROP TRIGGER payments_draft_update;
                 {sql};"
            );
            shop.db
                .write(move |tx| Ok(tx.execute_batch(&tamper)?))
                .unwrap();
            let report = shop
                .db
                .read(|c| verify_chain(c, DocType::Ticket, "C1"))
                .unwrap();
            assert_eq!(report.first_broken_seq, Some(1), "{sql}");
        }
    }

    #[test]
    fn a_document_deleted_at_the_end_breaks_the_chain() {
        let shop = shop();
        for price in [1_000, 2_000] {
            validated(
                &shop.db,
                draft(vec![line("Article", price, 1_000)], price, "2026-09-28"),
            );
        }
        shop.db
            .write(|tx| {
                tx.execute_batch(
                    "DROP TRIGGER documents_kept_once_validated;
                     DROP TRIGGER document_lines_draft_delete;
                     DROP TRIGGER document_taxes_draft_delete;
                     DROP TRIGGER payments_draft_delete;
                     DELETE FROM document_lines WHERE document_id = (SELECT id FROM documents WHERE chain_seq = 2);
                     DELETE FROM document_taxes WHERE document_id = (SELECT id FROM documents WHERE chain_seq = 2);
                     DELETE FROM payments WHERE document_id = (SELECT id FROM documents WHERE chain_seq = 2);
                     DELETE FROM documents WHERE chain_seq = 2;",
                )?;
                Ok(())
            })
            .unwrap();
        let report = shop
            .db
            .read(|c| verify_chain(c, DocType::Ticket, "C1"))
            .unwrap();
        assert_eq!((report.rows, report.first_broken_seq), (1, Some(2)));
    }

    #[test]
    fn a_chain_deleted_whole_is_still_verified_through_its_counter() {
        let shop = shop();
        validated(
            &shop.db,
            draft(vec![line("Article", 1_000, 1_000)], 1_000, "2026-09-28"),
        );
        for price in [2_000, 3_000] {
            let mut d = draft(vec![line("Article", price, 1_000)], price, "2026-09-28");
            d.terminal_code = "C2".into();
            validated(&shop.db, d);
        }
        // The perpetual Z counter shares the table but is not a document chain.
        shop.db
            .write(|tx| crate::numbering::assign_z(tx, "C1"))
            .unwrap();
        // Someone with the database file drops the triggers and deletes every ticket of C2.
        shop.db
            .write(|tx| {
                tx.execute_batch(
                    "DROP TRIGGER documents_kept_once_validated;
                     DROP TRIGGER document_lines_draft_delete;
                     DROP TRIGGER document_taxes_draft_delete;
                     DROP TRIGGER payments_draft_delete;
                     DELETE FROM document_lines WHERE document_id IN (SELECT id FROM documents WHERE series = 'C2');
                     DELETE FROM document_taxes WHERE document_id IN (SELECT id FROM documents WHERE series = 'C2');
                     DELETE FROM payments WHERE document_id IN (SELECT id FROM documents WHERE series = 'C2');
                     DELETE FROM documents WHERE series = 'C2';",
                )?;
                Ok(())
            })
            .unwrap();
        let all = shop.db.read(verify_all).unwrap();
        assert_eq!(
            all.iter()
                .map(|r| (
                    r.doc_type,
                    r.series.as_str(),
                    r.rows,
                    r.first_broken_seq,
                    r.last_seq
                ))
                .collect::<Vec<_>>(),
            [
                (DocType::Ticket, "C1", 1, None, 1),
                (DocType::Ticket, "C2", 0, Some(1), 0)
            ]
        );
        assert_eq!(all[1].first_broken_number, None);
        assert_eq!(all[1].last_hash, GENESIS_HASH);
    }

    #[test]
    fn code_and_receipt_counters_are_not_document_chains() {
        let shop = shop();
        let day = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        // A customer code and a versement receipt share the counters table with the documents.
        shop.db
            .write(|tx| numbering::next_code(tx, "customer_code", "C", 5))
            .unwrap();
        shop.db
            .write(move |tx| numbering::assign_custom(tx, "credit_receipt", "RV", day))
            .unwrap();
        assert_eq!(shop.db.read(verify_all).unwrap(), []);
        validated(
            &shop.db,
            draft(vec![line("Article", 1_000, 1_000)], 1_000, "2026-09-28"),
        );
        let all = shop.db.read(verify_all).unwrap();
        assert_eq!(
            all.iter()
                .map(|r| (r.doc_type, r.series.as_str(), r.rows, r.first_broken_seq))
                .collect::<Vec<_>>(),
            [(DocType::Ticket, "C1", 1, None)]
        );
    }

    #[test]
    fn incremental_verification_starts_at_an_anchor() {
        let shop = shop();
        validated(
            &shop.db,
            draft(vec![line("A", 1_000, 1_000)], 1_000, "2026-09-28"),
        );
        let (seq, hash) = shop.db.read(|c| anchor(c, DocType::Ticket, "C1")).unwrap();
        validated(
            &shop.db,
            draft(vec![line("B", 2_000, 1_000)], 2_000, "2026-09-28"),
        );
        let report = shop
            .db
            .read(move |c| verify_chain_from(c, DocType::Ticket, "C1", seq, &hash))
            .unwrap();
        assert_eq!(
            (report.rows, report.first_broken_seq, report.last_seq),
            (1, None, 2)
        );
        let wrong = shop
            .db
            .read(|c| verify_chain_from(c, DocType::Ticket, "C1", 1, GENESIS_HASH))
            .unwrap();
        assert_eq!(
            wrong.first_broken_seq,
            Some(2),
            "a wrong anchor does not verify"
        );
    }

    #[test]
    fn history_and_reprints() {
        let shop = shop();
        let a = validated(
            &shop.db,
            draft(vec![line("A", 1_000, 1_000)], 1_000, "2026-09-27"),
        );
        let b = validated(
            &shop.db,
            draft(vec![line("B", 2_000, 1_000)], 2_000, "2026-09-28"),
        );
        let all = shop
            .db
            .read(|c| list(c, &DocumentQuery::default()))
            .unwrap();
        assert_eq!(all.len(), 2);
        let q = DocumentQuery {
            business_date: Some("2026-09-27".into()),
            ..DocumentQuery::default()
        };
        let on_27 = shop.db.read(move |c| list(c, &q)).unwrap();
        assert_eq!(on_27[0].id, a.id);
        let q = DocumentQuery {
            number_prefix: Some("tk2026-c1-000002".into()),
            ..DocumentQuery::default()
        };
        assert_eq!(shop.db.read(move |c| list(c, &q)).unwrap()[0].id, b.id);
        let (id, actor) = (b.id.clone(), shop.cashier.actor());
        assert_eq!(
            shop.db
                .write(move |tx| record_reprint(tx, &actor, &id))
                .unwrap(),
            1
        );
        let (id, actor) = (b.id.clone(), shop.cashier.actor());
        assert_eq!(
            shop.db
                .write(move |tx| record_reprint(tx, &actor, &id))
                .unwrap(),
            2
        );
    }

    #[test]
    fn history_pages_through_documents_validated_at_the_same_instant() {
        // An exchange validates its return note and its new ticket with the same `now`.
        let shop = shop();
        let a = validated(
            &shop.db,
            draft(vec![line("A", 1_000, 1_000)], 1_000, "2026-09-28"),
        );
        let b = validated(
            &shop.db,
            draft(vec![line("B", 2_000, 1_000)], 2_000, "2026-09-28"),
        );
        assert_eq!(a.validated_at, b.validated_at);
        let page = |before: Option<&DocumentSummary>| {
            let q = DocumentQuery {
                before_validated_at: before.and_then(|s| s.validated_at),
                before_id: before.map(|s| s.id.clone()),
                limit: Some(1),
                ..DocumentQuery::default()
            };
            shop.db.read(move |c| list(c, &q)).unwrap()
        };
        let first = page(None);
        assert_eq!(first.len(), 1);
        let second = page(first.first());
        assert_eq!(second.len(), 1);
        assert!(page(second.first()).is_empty());
        let mut seen = [first[0].id.clone(), second[0].id.clone()];
        seen.sort();
        let mut expected = [a.id, b.id];
        expected.sort();
        assert_eq!(seen, expected);
    }

    #[test]
    fn the_return_notes_of_the_sales_history_are_the_tills_only() {
        // A return to a supplier is a return note of the `F` series with a supplier: it belongs
        // to the purchases, never to the tills' history, even filtered on return notes.
        let shop = shop();
        let mut till = draft(vec![line("A", 1_000, 1_000)], 1_000, "2026-09-28");
        till.doc_type = DocType::ReturnNote;
        let till = validated(&shop.db, till);
        let mut supplier = draft(vec![line("B", 2_000, 1_000)], 0, "2026-09-28");
        supplier.doc_type = DocType::ReturnNote;
        supplier.terminal_code = "F".into();
        supplier.supplier_id = Some(shop.supplier("Ifri").id);
        supplier.payments.clear();
        let supplier = validated(&shop.db, supplier);
        assert_eq!(supplier.number.as_deref(), Some("BR2026-F-000001"));
        let q = DocumentQuery {
            doc_type: Some(DocType::ReturnNote),
            ..DocumentQuery::default()
        };
        let listed = shop.db.read(move |c| list(c, &q)).unwrap();
        assert_eq!(
            listed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            [till.id.as_str()]
        );
    }

    fn quote_draft(db: &crate::db::Db) -> String {
        let mut d = draft(vec![line("Ciment 25 kg", 90_000, 10_000)], 0, "2026-09-28");
        d.doc_type = DocType::Quote;
        d.payments.clear();
        d.paid_total = Money::ZERO;
        db.write(move |tx| insert_draft(tx, &d, NOON)).unwrap()
    }

    #[test]
    fn m0_to_m2_documents_hash_as_before() {
        let shop = shop();
        let doc = validated(
            &shop.db,
            draft(vec![line("Lait", 11_900, 2_000)], 23_800, "2026-09-28"),
        );
        let value = canonical(&doc);
        assert!(value.get("ext").is_none(), "no ext without M3 fields");
        assert!(value["lines"][0].get("descriptionAlt").is_none());
        assert_eq!(doc.parties, Vec::<DocumentParty>::new());
        assert_eq!(
            (
                doc.valid_until.clone(),
                doc.due_date.clone(),
                doc.second_lang,
                doc.credit_kind,
                doc.origin
            ),
            (None, None, None, None, None)
        );
        assert_eq!(doc.hash.clone().unwrap(), document_hash(GENESIS_HASH, &doc));
        assert!(
            shop.db
                .read(verify_all)
                .unwrap()
                .iter()
                .all(|r| r.first_broken_seq.is_none())
        );
    }

    #[test]
    fn m3_fields_and_parties_are_hashed_and_tampering_is_found() {
        let shop = shop();
        let id = quote_draft(&shop.db);
        let for_tx = id.clone();
        let doc = shop
            .db
            .write(move |tx| {
                set_extras(
                    tx,
                    &for_tx,
                    &DraftExtras {
                        valid_until: Some("2026-10-28".into()),
                        second_lang: Some(Lang::Ar),
                        ..DraftExtras::default()
                    },
                )?;
                let mut buyer = DocumentParty::named(PartyRole::Buyer, "SARL Amine");
                buyer.nif = Some("000016001234567".into());
                parties::record(tx, &for_tx, &buyer)?;
                let line_id = get(tx, &for_tx)?.lines[0].id.clone();
                set_alt_descriptions(tx, &for_tx, &[(line_id, "إسمنت 25 كغ".into())])?;
                validate(tx, &for_tx, NOON)
            })
            .unwrap();
        assert_eq!(doc.number.as_deref(), Some("DV2026-000001"));
        let value = canonical(&doc);
        assert_eq!(value["ext"]["validUntil"], "2026-10-28");
        assert_eq!(value["ext"]["secondLang"], "ar");
        assert_eq!(value["ext"]["parties"][0]["nif"], "000016001234567");
        assert_eq!(value["lines"][0]["descriptionAlt"], "إسمنت 25 كغ");
        let report = shop
            .db
            .read(|c| verify_chain(c, DocType::Quote, ""))
            .unwrap();
        assert_eq!(report.first_broken_seq, None);
        // Someone with the database file drops the trigger and edits the buyer's NIF.
        shop.db
            .write(|tx| {
                tx.execute_batch(
                    "DROP TRIGGER document_parties_draft_update;
                     UPDATE document_parties SET nif = '000016009999999';",
                )?;
                Ok(())
            })
            .unwrap();
        let report = shop
            .db
            .read(|c| verify_chain(c, DocType::Quote, ""))
            .unwrap();
        assert_eq!(report.first_broken_seq, Some(1));
    }

    #[test]
    fn extras_fit_their_kind_and_only_drafts_take_them() {
        let shop = shop();
        let id = quote_draft(&shop.db);
        let wrong = id.clone();
        let err = shop
            .db
            .write(move |tx| {
                set_extras(
                    tx,
                    &wrong,
                    &DraftExtras {
                        due_date: Some("2026-10-28".into()),
                        ..DraftExtras::default()
                    },
                )
            })
            .unwrap_err();
        assert_eq!(err.payload().field.as_deref(), Some("dueDate"));
        assert_eq!(err.code(), "value_invalid");
        let day = id.clone();
        shop.db
            .write(move |tx| {
                restamp(
                    tx,
                    &day,
                    NaiveDate::from_ymd_opt(2026, 10, 2).unwrap(),
                    NOON,
                )?;
                validate(tx, &day, NOON).map(drop)
            })
            .unwrap();
        let doc = shop.db.read({
            let id = id.clone();
            move |c| get(c, &id)
        });
        assert_eq!(doc.unwrap().business_date, "2026-10-02");
        let late = id.clone();
        let err = shop
            .db
            .write(move |tx| set_extras(tx, &late, &DraftExtras::default()))
            .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
        let err = shop
            .db
            .write(move |tx| restamp(tx, &id, NaiveDate::from_ymd_opt(2026, 10, 3).unwrap(), NOON))
            .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
    }

    #[test]
    fn a_draft_keeps_its_cart_until_validation() {
        let shop = shop();
        let id = quote_draft(&shop.db);
        let for_tx = id.clone();
        shop.db
            .write(move |tx| {
                store_cart(tx, &for_tx, &json!({ "lines": [1] }), NOON)?;
                store_cart(tx, &for_tx, &json!({ "lines": [2] }), NOON)?;
                validate(tx, &for_tx, NOON).map(drop)
            })
            .unwrap();
        let read = id.clone();
        assert_eq!(
            shop.db.read(move |c| stored_cart(c, &read)).unwrap(),
            Some(json!({ "lines": [2] }))
        );
        let err = shop
            .db
            .write(move |tx| store_cart(tx, &id, &json!({}), NOON))
            .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
    }

    #[test]
    fn a_draft_takes_its_line_costs_and_gives_back_its_extras() {
        let shop = shop();
        let id = tests_support::invoice_draft(&shop.db);
        let for_tx = id.clone();
        let doc = shop
            .db
            .write(move |tx| {
                let extras = DraftExtras {
                    due_date: Some("2026-10-28".into()),
                    origin: Some(DocOrigin::Direct),
                    ..DraftExtras::default()
                };
                set_extras(tx, &for_tx, &extras)?;
                let line_id = get(tx, &for_tx)?.lines[0].id.clone();
                set_line_costs(tx, &for_tx, &[(line_id, Money(7_500))])?;
                validate(tx, &for_tx, NOON)
            })
            .unwrap();
        assert_eq!(doc.lines[0].cost_price, Money(7_500));
        assert_eq!(
            extras_of(&doc),
            DraftExtras {
                due_date: Some("2026-10-28".into()),
                origin: Some(DocOrigin::Direct),
                ..DraftExtras::default()
            }
        );
        let err = shop
            .db
            .write(move |tx| set_line_costs(tx, &id, &[]))
            .unwrap_err();
        assert_eq!(err.code(), "document_already_validated");
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use crate::db::Db;
    use crate::tax::tva::{Discount, LineInput, compute_line};
    use crate::testkit::NOON;

    pub(crate) fn draft_of(doc_type: DocType) -> DraftDocument {
        let amounts = compute_line(
            PriceMode::Ttc,
            &LineInput {
                unit_price: Money(11_900),
                qty: Qty(1_000),
                discount: Discount::None,
                rate: Rate(1_900),
            },
        )
        .unwrap();
        DraftDocument {
            doc_type,
            terminal_code: "C1".into(),
            business_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
            customer_id: None,
            supplier_id: None,
            user_id: "u1".into(),
            terminal_id: "t1".into(),
            cash_session_id: None,
            price_level_id: None,
            lang: Lang::Fr,
            tax_regime: TaxRegime::Reel,
            price_mode: PriceMode::Ttc,
            stamp_mode: StampMode::ChargeCustomer,
            stamp_duty: Money::ZERO,
            stamp_charged: Money::ZERO,
            rounding: Money::ZERO,
            paid_total: Money::ZERO,
            change_given: Money::ZERO,
            credit_amount: Money::ZERO,
            note: None,
            parent_id: None,
            source_ref: None,
            lines: vec![DraftLine {
                id: new_id(),
                product_id: None,
                description: "Article".into(),
                qty: Qty(1_000),
                unit_code: "pc".into(),
                unit_price: Money(11_900),
                price_overridden: false,
                tax_rate_bp: Rate(1_900),
                amounts,
                cost_price: Money::ZERO,
                parent_line_id: None,
                origin_line_id: None,
                restock_location_id: None,
                reason_code: None,
                note: None,
            }],
            payments: Vec::new(),
        }
    }

    pub(crate) fn invoice_draft(db: &Db) -> String {
        db.write(|tx| insert_draft(tx, &draft_of(DocType::Invoice), NOON))
            .unwrap()
    }

    pub(crate) fn validated_of(db: &Db, doc_type: DocType) -> String {
        db.write(move |tx| {
            let id = insert_draft(tx, &draft_of(doc_type), NOON)?;
            validate(tx, &id, NOON)?;
            Ok(id)
        })
        .unwrap()
    }
}
