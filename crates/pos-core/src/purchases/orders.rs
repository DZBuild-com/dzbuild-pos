//! Purchase orders (PUR-002): drafts edited freely, validated once (`BC…`), then received in
//! one or several goods receipts; the status follows the receipts.

use super::lines::{self, PurchaseLineInput};
use crate::audit;
use crate::company;
use crate::documents::{self, DocStatus, DocType, Document, DocumentLine};
use crate::error::AppError;
use crate::money::Qty;
use crate::till::TillContext;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderInput {
    pub supplier_id: String,
    pub lines: Vec<PurchaseLineInput>,
    pub supplier_ref: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemainingLine {
    pub line: DocumentLine,
    pub received: Qty,
    pub remaining: Qty,
}

fn order(conn: &Connection, id: &str) -> Result<Document, AppError> {
    let doc = documents::get(conn, id)?;
    if doc.doc_type != DocType::PurchaseOrder {
        return Err(AppError::validation("orderId", "purchase_order_invalid"));
    }
    Ok(doc)
}

pub fn save_order(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: Option<&str>,
    input: OrderInput,
) -> Result<Document, AppError> {
    let supplier = crate::suppliers::get(tx, &input.supplier_id)?;
    if supplier.archived {
        return Err(AppError::not_found("supplier"));
    }
    let company = company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let built = lines::build(tx, company.default_doc_lang, &input.lines, true, None)?;
    let draft = super::draft(
        ctx,
        &company,
        DocType::PurchaseOrder,
        &supplier.id,
        built.into_iter().map(|b| b.draft).collect(),
        crate::catalog::clean_opt(input.note.as_deref(), "note", 500)?,
        None,
        crate::catalog::clean_opt(input.supplier_ref.as_deref(), "supplierRef", 48)?,
        &ctx.session.terminal_code,
    );
    let id = match id {
        Some(id) => {
            order(tx, id)?;
            documents::replace_draft(tx, id, &draft, ctx.now)?;
            id.to_owned()
        }
        None => documents::insert_draft(tx, &draft, ctx.now)?,
    };
    let saved = documents::get(tx, &id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "purchase_order.saved",
            "document",
            Some(&id),
            json!({ "supplierId": supplier.id, "lines": saved.lines.len(), "totalHt": saved.subtotal_ht }),
        ),
    )?;
    Ok(saved)
}

pub fn validate_order(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
) -> Result<Document, AppError> {
    let doc = order(tx, id)?;
    if doc.status != DocStatus::Draft {
        return Err(AppError::conflict("document_already_validated"));
    }
    let validated = documents::validate(tx, id, ctx.now)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "purchase_order.validated",
            "document",
            Some(id),
            json!({ "number": validated.number, "supplierId": validated.supplier_id, "totalTtc": validated.total_ttc }),
        ),
    )?;
    Ok(validated)
}

/// A draft order dropped (M1a's `documents::cancel` journals `document.cancelled`).
pub fn cancel_order(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<(), AppError> {
    order(tx, id)?;
    documents::cancel(tx, &ctx.actor(), id)
}

/// Each order line with what the validated receipts already brought.
pub fn remaining(conn: &Connection, order_id: &str) -> Result<Vec<RemainingLine>, AppError> {
    let doc = order(conn, order_id)?;
    let mut out = Vec::with_capacity(doc.lines.len());
    for line in doc.lines {
        let received: Qty = conn.query_row(
            "SELECT COALESCE(SUM(l.qty), 0) FROM document_lines l JOIN documents r ON r.id = l.document_id
             WHERE l.origin_line_id = ?1 AND r.doc_type = 'goods_receipt' AND r.status = 'validated'",
            [&line.id],
            |r| r.get(0),
        )?;
        let remaining = line
            .qty
            .checked_sub(received)
            .unwrap_or(Qty::ZERO)
            .max(Qty::ZERO);
        out.push(RemainingLine {
            line,
            received,
            remaining,
        });
    }
    Ok(out)
}
