//! Supplier invoices (PUR-005): `FF…` numbered, the supplier's own number kept, built from
//! goods receipts (each receipt invoiced once) and/or extra lines (transport, services); the
//! TVA per rate stays in `document_taxes` for the deductible TVA of M3; the invoice's TTC goes
//! on the payables, and may be paid at once.

use super::lines::{self, PurchaseLineInput};
use crate::audit;
use crate::company;
use crate::documents::{self, DocStatus, DocType, Document, DraftLine};
use crate::error::AppError;
use crate::ids::new_id;
use crate::ledger::{EntryKind, LedgerEntry};
use crate::money::Money;
use crate::payments::PaymentMethod;
use crate::suppliers::{self, SupplierPaymentInput};
use crate::till::TillContext;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaymentNow {
    pub amount: Money,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub from_till: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceInput {
    pub supplier_id: String,
    pub supplier_number: String,
    pub receipt_ids: Vec<String>,
    pub lines: Vec<PurchaseLineInput>,
    pub note: Option<String>,
    pub payment: Option<PaymentNow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceDone {
    pub invoice: Document,
    pub entry: LedgerEntry,
    pub payment: Option<LedgerEntry>,
}

pub fn record_invoice(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: InvoiceInput,
) -> Result<InvoiceDone, AppError> {
    let supplier = suppliers::get(tx, &input.supplier_id)?;
    if supplier.archived {
        return Err(AppError::not_found("supplier"));
    }
    let number = crate::catalog::clean_opt(Some(&input.supplier_number), "supplierNumber", 48)?
        .ok_or_else(|| AppError::validation("supplierNumber", "required"))?;
    let duplicate: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM documents WHERE doc_type = 'supplier_invoice' AND supplier_id = ?1
                        AND source_ref = ?2 AND status = 'validated')",
        params![supplier.id, number],
        |r| r.get(0),
    )?;
    if duplicate {
        return Err(AppError::validation(
            "supplierNumber",
            "supplier_invoice_duplicate",
        ));
    }
    let company = company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let mut draft_lines: Vec<DraftLine> = Vec::new();
    // A receipt listed twice in this call is invoiced once (its lines are not copied twice).
    let mut seen: HashSet<&str> = HashSet::with_capacity(input.receipt_ids.len());
    for (i, receipt_id) in input.receipt_ids.iter().enumerate() {
        let field = format!("receiptIds.{i}");
        if !seen.insert(receipt_id.as_str()) {
            return Err(AppError::validation(&field, "receipt_already_invoiced"));
        }
        let rc = documents::get(tx, receipt_id)
            .map_err(|_| AppError::validation(&field, "document_not_found"))?;
        if rc.doc_type != DocType::GoodsReceipt
            || rc.status != DocStatus::Validated
            || rc.supplier_id.as_deref() != Some(supplier.id.as_str())
        {
            return Err(AppError::validation(&field, "purchase_line_invalid"));
        }
        let invoiced: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM document_lines l JOIN documents f ON f.id = l.document_id
                            JOIN document_lines rl ON rl.id = l.origin_line_id
                            WHERE f.doc_type = 'supplier_invoice' AND f.status = 'validated' AND rl.document_id = ?1)",
            [receipt_id],
            |r| r.get(0),
        )?;
        if invoiced {
            return Err(AppError::validation(&field, "receipt_already_invoiced"));
        }
        for l in rc.lines.iter().filter(|l| l.parent_line_id.is_none()) {
            draft_lines.push(DraftLine {
                id: new_id(),
                product_id: l.product_id.clone(),
                description: l.description.clone(),
                qty: l.qty,
                unit_code: l.unit_code.clone(),
                unit_price: l.unit_price,
                price_overridden: false,
                tax_rate_bp: l.tax_rate_bp,
                amounts: crate::tax::tva::LineAmounts {
                    gross: l
                        .total_ht
                        .checked_add(l.discount_amount)
                        .unwrap_or(l.total_ht),
                    discount_bp: l.discount_bp,
                    discount_amount: l.discount_amount,
                    total_ht: l.total_ht,
                    tax: l.tax_amount,
                    total_ttc: l.total_ttc,
                },
                cost_price: l.cost_price,
                parent_line_id: None,
                origin_line_id: Some(l.id.clone()),
                restock_location_id: None,
                reason_code: None,
                note: l.note.clone(),
            });
        }
    }
    draft_lines.extend(
        lines::build(tx, company.default_doc_lang, &input.lines, false, None)?
            .into_iter()
            .map(|b| b.draft),
    );
    let parent = (input.receipt_ids.len() == 1).then(|| input.receipt_ids[0].clone());
    let draft = super::draft(
        ctx,
        &company,
        DocType::SupplierInvoice,
        &supplier.id,
        draft_lines,
        crate::catalog::clean_opt(input.note.as_deref(), "note", 500)?,
        parent,
        Some(number.clone()),
        &ctx.session.terminal_code,
    );
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    let invoice = documents::validate(tx, &id, ctx.now)?;
    let entry = suppliers::book(
        tx,
        ctx,
        &supplier.id,
        EntryKind::Purchase,
        invoice.total_ttc,
        &invoice.id,
    )?;
    let payment = match input.payment {
        Some(p) => Some(suppliers::pay(
            tx,
            ctx,
            SupplierPaymentInput {
                supplier_id: supplier.id.clone(),
                amount: p.amount,
                method: p.method,
                reference: p.reference,
                cheque_bank: p.cheque_bank,
                cheque_date: p.cheque_date,
                from_till: p.from_till,
                allocate_to: vec![entry.id.clone()],
                note: None,
            },
        )?),
        None => None,
    };
    audit::append(
        tx,
        &ctx.actor().event(
            "supplier_invoice.validated",
            "document",
            Some(&invoice.id),
            json!({ "number": invoice.number, "supplierNumber": number, "supplierId": supplier.id,
                    "totalHt": invoice.subtotal_ht, "tax": invoice.tax_total, "totalTtc": invoice.total_ttc,
                    "receipts": input.receipt_ids, "paid": payment.as_ref().map(|p| p.amount) }),
        ),
    )?;
    Ok(InvoiceDone {
        invoice,
        entry,
        payment,
    })
}
