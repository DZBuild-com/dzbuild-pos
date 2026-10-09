//! Purchasing (spec §5.1, §5.2; PUR-002…008): purchase orders `BC`, goods receipts `RC`,
//! supplier invoices `FF` and returns to suppliers `BR…-F-` are ordinary `documents` rows with a
//! `supplier_id`, numbered and hash-chained like every document, immutable once validated. Lines
//! are HT (suppliers invoice HT + TVA). Stock and CMUP move only with goods receipts and returns;
//! the payables move with invoices, returns and payments.

pub mod invoices;
pub mod lines;
pub mod orders;
pub mod receipts;
pub mod returns;
#[cfg(test)]
mod tests;

pub use lines::PurchaseLineInput;

use crate::company::{Company, PriceMode, TaxRegime};
use crate::documents::{DocStatus, DocType, DraftDocument, DraftLine};
use crate::error::AppError;
use crate::money::Money;
use crate::tax::timbre::StampMode;
use crate::till::TillContext;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Draft,
    Sent,
    Partial,
    Received,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseSummary {
    pub id: String,
    pub doc_type: DocType,
    pub number: Option<String>,
    pub status: DocStatus,
    pub order_status: Option<OrderStatus>,
    pub business_date: String,
    pub supplier_id: Option<String>,
    pub supplier_name: Option<String>,
    /// The supplier's own number (BL or invoice).
    pub supplier_ref: Option<String>,
    pub parent_id: Option<String>,
    pub total_ht: Money,
    pub tax_total: Money,
    pub total_ttc: Money,
    pub validated_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseQuery {
    pub doc_type: Option<DocType>,
    pub supplier_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CostChange {
    pub product_id: String,
    pub cost_before: Money,
    pub cost_after: Money,
    pub source_type: String,
    pub source_id: Option<String>,
    pub source_number: Option<String>,
    pub created_at: i64,
}

/// PUR-002, line by line as [`orders::remaining`] counts: `open_lines` are the order lines whose
/// validated receipts (through `origin_line_id`) brought less than ordered; `delivered` is true
/// once a validated receipt was made against the order.
fn status_of(status: DocStatus, open_lines: i64, delivered: bool) -> OrderStatus {
    match status {
        DocStatus::Draft => OrderStatus::Draft,
        DocStatus::Cancelled => OrderStatus::Cancelled,
        DocStatus::Validated if !delivered => OrderStatus::Sent,
        DocStatus::Validated if open_lines > 0 => OrderStatus::Partial,
        DocStatus::Validated => OrderStatus::Received,
    }
}

const LIST: &str = "SELECT d.id, d.doc_type, d.number, d.status, d.business_date, d.supplier_id, s.name,
        d.source_ref, d.parent_id, d.subtotal_ht, d.tax_total, d.total_ttc, d.validated_at,
        (SELECT COUNT(*) FROM document_lines l WHERE l.document_id = d.id
          AND l.qty > (SELECT COALESCE(SUM(rl.qty), 0) FROM document_lines rl
                        JOIN documents r ON r.id = rl.document_id
                        WHERE rl.origin_line_id = l.id AND r.doc_type = 'goods_receipt'
                          AND r.status = 'validated')),
        EXISTS (SELECT 1 FROM documents r
                 WHERE r.parent_id = d.id AND r.doc_type = 'goods_receipt' AND r.status = 'validated')
     FROM documents d LEFT JOIN suppliers s ON s.id = d.supplier_id";

fn summaries(
    conn: &Connection,
    clause: &str,
    p: impl rusqlite::Params,
) -> Result<Vec<PurchaseSummary>, AppError> {
    let mut stmt = conn.prepare_cached(&format!("{LIST} {clause}"))?;
    let rows = stmt
        .query_map(p, |r| {
            let doc_type: DocType = r.get(1)?;
            let status: DocStatus = r.get(3)?;
            let open_lines: i64 = r.get(13)?;
            let delivered: bool = r.get(14)?;
            Ok(PurchaseSummary {
                id: r.get(0)?,
                doc_type,
                number: r.get(2)?,
                status,
                order_status: (doc_type == DocType::PurchaseOrder)
                    .then(|| status_of(status, open_lines, delivered)),
                business_date: r.get(4)?,
                supplier_id: r.get(5)?,
                supplier_name: r.get(6)?,
                supplier_ref: r.get(7)?,
                parent_id: r.get(8)?,
                total_ht: r.get(9)?,
                tax_total: r.get(10)?,
                total_ttc: r.get(11)?,
                validated_at: r.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Supplier documents, newest first (orders, receipts, invoices, returns).
pub fn list(conn: &Connection, q: &PurchaseQuery) -> Result<Vec<PurchaseSummary>, AppError> {
    summaries(
        conn,
        "WHERE d.supplier_id IS NOT NULL AND (?1 IS NULL OR d.doc_type = ?1) AND (?2 IS NULL OR d.supplier_id = ?2)
           AND (?3 IS NULL OR d.business_date >= ?3) AND (?4 IS NULL OR d.business_date <= ?4)
         ORDER BY d.created_at DESC LIMIT ?5",
        params![
            q.doc_type,
            q.supplier_id,
            q.from,
            q.to,
            q.limit.unwrap_or(500).clamp(1, 5_000)
        ],
    )
}

pub fn order_status(conn: &Connection, id: &str) -> Result<OrderStatus, AppError> {
    summaries(
        conn,
        "WHERE d.id = ?1 AND d.doc_type = 'purchase_order'",
        [id],
    )?
    .into_iter()
    .next()
    .and_then(|s| s.order_status)
    .ok_or_else(|| AppError::validation("orderId", "purchase_order_invalid"))
}

/// Every CMUP change of a product, newest first, with the number of what caused it.
pub fn cost_history(
    conn: &Connection,
    product_id: &str,
    limit: u32,
) -> Result<Vec<CostChange>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT h.product_id, h.cost_before, h.cost_after, h.source_type, h.source_id,
                (SELECT d.number FROM documents d WHERE d.id = h.source_id), h.created_at
         FROM cost_history h WHERE h.product_id = ?1 ORDER BY h.created_at DESC, h.rowid DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![product_id, limit.clamp(1, 500)], |r| {
            Ok(CostChange {
                product_id: r.get(0)?,
                cost_before: r.get(1)?,
                cost_after: r.get(2)?,
                source_type: r.get(3)?,
                source_id: r.get(4)?,
                source_number: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// A supplier document draft: HT lines, no stamp duty, no cash session, the company's document
/// language. `series_code` is the terminal code, or `F` for supplier returns (their own `BR`
/// series).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draft(
    ctx: &TillContext,
    company: &Company,
    doc_type: DocType,
    supplier_id: &str,
    lines: Vec<DraftLine>,
    note: Option<String>,
    parent_id: Option<String>,
    source_ref: Option<String>,
    series_code: &str,
) -> DraftDocument {
    DraftDocument {
        doc_type,
        terminal_code: series_code.to_owned(),
        business_date: ctx.business_date(),
        customer_id: None,
        supplier_id: Some(supplier_id.to_owned()),
        user_id: ctx.session.user_id.clone(),
        terminal_id: ctx.session.terminal_id.clone(),
        cash_session_id: None,
        price_level_id: None,
        lang: company.default_doc_lang,
        tax_regime: TaxRegime::Reel,
        price_mode: PriceMode::Ht,
        stamp_mode: StampMode::Off,
        stamp_duty: Money::ZERO,
        stamp_charged: Money::ZERO,
        rounding: Money::ZERO,
        paid_total: Money::ZERO,
        change_given: Money::ZERO,
        credit_amount: Money::ZERO,
        note,
        parent_id,
        source_ref,
        lines,
        payments: Vec::new(),
    }
}
