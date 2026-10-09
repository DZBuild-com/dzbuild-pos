//! Goods receipts (PUR-003/004): against a validated order or direct, into a storage location,
//! with lots, expiry dates and serials; each line moves the stock and the CMUP. The supplier's
//! invoice can be recorded (and paid) in the same transaction.

use super::invoices::{self, PaymentNow};
use super::lines::{self, PurchaseLineInput};
use crate::audit;
use crate::company;
use crate::documents::{self, DocStatus, DocType, Document};
use crate::error::AppError;
use crate::ledger::LedgerEntry;
use crate::money::Money;
use crate::stock::{self, ReceiveLine};
use crate::till::TillContext;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceNow {
    pub supplier_number: String,
    pub payment: Option<PaymentNow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptInput {
    pub supplier_id: String,
    pub order_id: Option<String>,
    /// Default: the shop's default location.
    pub location_id: Option<String>,
    pub lines: Vec<PurchaseLineInput>,
    /// The supplier's delivery note number.
    pub supplier_ref: Option<String>,
    pub note: Option<String>,
    pub invoice: Option<InvoiceNow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CostMove {
    pub product_id: String,
    pub cost_before: Money,
    pub cost_after: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptDone {
    pub receipt: Document,
    pub invoice: Option<Document>,
    pub payment: Option<LedgerEntry>,
    pub cost_changes: Vec<CostMove>,
}

pub fn receive(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ReceiptInput,
) -> Result<ReceiptDone, AppError> {
    let supplier = crate::suppliers::get(tx, &input.supplier_id)?;
    if supplier.archived {
        return Err(AppError::not_found("supplier"));
    }
    let order = match &input.order_id {
        Some(id) => {
            let o = documents::get(tx, id)
                .map_err(|_| AppError::validation("orderId", "purchase_order_invalid"))?;
            if o.doc_type != DocType::PurchaseOrder
                || o.status != DocStatus::Validated
                || o.supplier_id.as_deref() != Some(supplier.id.as_str())
            {
                return Err(AppError::validation("orderId", "purchase_order_invalid"));
            }
            Some(o)
        }
        None => None,
    };
    let location = match &input.location_id {
        Some(id) => stock::location(tx, id)
            .map_err(|_| AppError::validation("locationId", "stock_location_not_found"))?,
        None => stock::default_location(tx)?,
    };
    let company = company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let built = lines::build(
        tx,
        company.default_doc_lang,
        &input.lines,
        true,
        Some(&location.id),
    )?;
    for (i, b) in built.iter().enumerate() {
        match (&order, &b.draft.origin_line_id) {
            (Some(o), Some(origin)) => {
                let ok = o.lines.iter().any(|l| {
                    &l.id == origin && l.product_id.as_deref() == Some(b.product.id.as_str())
                });
                if !ok {
                    return Err(AppError::validation(
                        &format!("lines.{i}.originLineId"),
                        "purchase_line_invalid",
                    ));
                }
            }
            (None, Some(_)) => {
                return Err(AppError::validation(
                    &format!("lines.{i}.originLineId"),
                    "purchase_line_invalid",
                ));
            }
            _ => {}
        }
    }
    let draft = super::draft(
        ctx,
        &company,
        DocType::GoodsReceipt,
        &supplier.id,
        built.iter().map(|b| b.draft.clone()).collect(),
        crate::catalog::clean_opt(input.note.as_deref(), "note", 500)?,
        order.as_ref().map(|o| o.id.clone()),
        crate::catalog::clean_opt(input.supplier_ref.as_deref(), "supplierRef", 48)?,
        &ctx.session.terminal_code,
    );
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    let actor = ctx.actor();
    let mut cost_changes = Vec::new();
    let mut lot_ids = Vec::new();
    for b in &built {
        let received = stock::receive(
            tx,
            &actor,
            &ReceiveLine {
                product_id: b.product.id.clone(),
                location_id: location.id.clone(),
                qty: b.units,
                unit_cost: b.net_unit_cost,
                lot: b.lot.clone(),
                serials: b.serials.clone(),
                supplier_id: Some(supplier.id.clone()),
                ref_type: "document".into(),
                ref_id: id.clone(),
                note: None,
            },
            ctx.now,
        )?;
        lot_ids.push((b.draft.id.clone(), received.lot_id.clone()));
        cost_changes.push(CostMove {
            product_id: b.product.id.clone(),
            cost_before: received.cost_before,
            cost_after: received.cost_after,
        });
    }
    // The receipt's lines name their lot while the receipt is still a draft (then it is frozen).
    for (line_id, lot_id) in lot_ids.iter().filter(|(_, l)| l.is_some()) {
        tx.execute(
            "UPDATE document_lines SET lot_id = ?2 WHERE id = ?1",
            params![line_id, lot_id],
        )?;
    }
    let receipt = documents::validate(tx, &id, ctx.now)?;
    let (invoice, payment) = match input.invoice {
        Some(now) => {
            let done = invoices::record_invoice(
                tx,
                ctx,
                invoices::InvoiceInput {
                    supplier_id: supplier.id.clone(),
                    supplier_number: now.supplier_number,
                    receipt_ids: vec![receipt.id.clone()],
                    lines: Vec::new(),
                    note: None,
                    payment: now.payment,
                },
            )?;
            (Some(done.invoice), done.payment)
        }
        None => (None, None),
    };
    audit::append(
        tx,
        &actor.event(
            "goods_receipt.validated",
            "document",
            Some(&receipt.id),
            json!({ "number": receipt.number, "supplierId": supplier.id, "orderId": input.order_id,
                    "locationId": location.id, "totalHt": receipt.subtotal_ht,
                    "costChanges": cost_changes.iter().map(|c| json!({ "productId": c.product_id, "before": c.cost_before, "after": c.cost_after })).collect::<Vec<_>>() }),
        ),
    )?;
    Ok(ReceiptDone {
        receipt,
        invoice,
        payment,
        cost_changes,
    })
}
