//! Returns to a supplier (PUR-008): expired, damaged or wrong goods leave the stock (from a
//! named lot, never beyond what it holds at that location, or FEFO), their serials go back (one
//! per piece, from that location), the value (at CMUP unless given) is credited on the
//! payables when the supplier accepts it. Numbered in the `F` series of bons de retour so it never
//! mixes with the tills' `BR…-C1-` return notes.

use super::lines::{self, PurchaseLineInput};
use crate::audit;
use crate::company;
use crate::documents::{self, DocType, Document};
use crate::error::AppError;
use crate::ledger::{EntryKind, LedgerEntry};
use crate::money::{Money, Qty};
use crate::sales::cart::at_line;
use crate::stock::serials::SerialStatus;
use crate::stock::{self, MovementReason, NewMovement, Shortage, StockNeed, lots, serials};
use crate::till::TillContext;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The series of supplier returns (`BR2026-F-000001`).
pub const SUPPLIER_RETURN_SERIES: &str = "F";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupplierReturnLine {
    pub product_id: String,
    pub qty: Qty,
    /// HT value per unit; default: the product's CMUP.
    pub unit_cost: Option<Money>,
    pub lot_number: Option<String>,
    #[serde(default)]
    pub serials: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupplierReturnInput {
    pub supplier_id: String,
    pub location_id: Option<String>,
    pub lines: Vec<SupplierReturnLine>,
    /// The supplier credits the value on the account.
    pub credit_supplier: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupplierReturnDone {
    pub document: Document,
    pub entry: Option<LedgerEntry>,
}

pub fn return_to_supplier(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SupplierReturnInput,
) -> Result<SupplierReturnDone, AppError> {
    let supplier = crate::suppliers::get(tx, &input.supplier_id)?;
    if supplier.archived {
        return Err(AppError::not_found("supplier"));
    }
    let location = match &input.location_id {
        Some(id) => stock::location(tx, id)
            .map_err(|_| AppError::validation("locationId", "stock_location_not_found"))?,
        None => stock::default_location(tx)?,
    };
    let company = company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let as_purchase: Vec<PurchaseLineInput> = input
        .lines
        .iter()
        .map(|l| -> Result<PurchaseLineInput, AppError> {
            let cost = match l.unit_cost {
                Some(c) => c,
                None => crate::catalog::products::get(tx, &l.product_id)?.cost_price,
            };
            Ok(PurchaseLineInput {
                product_id: l.product_id.clone(),
                qty: l.qty,
                unit_cost: cost,
                pack_qty: None,
                tax_rate_id: None,
                discount: crate::tax::tva::Discount::None,
                origin_line_id: None,
                lot_number: l.lot_number.clone(),
                expiry_date: None,
                serials: l.serials.clone(),
                note: l.note.clone(),
            })
        })
        .collect::<Result<_, _>>()?;
    let built = lines::build(
        tx,
        company.default_doc_lang,
        &as_purchase,
        true,
        Some(&location.id),
    )?;
    let needs: Vec<StockNeed> = built
        .iter()
        .map(|b| StockNeed {
            product_id: b.product.id.clone(),
            location_id: location.id.clone(),
            qty: b.units,
        })
        .collect();
    stock::check_available(tx, &needs)?;
    let draft = super::draft(
        ctx,
        &company,
        DocType::ReturnNote,
        &supplier.id,
        built.iter().map(|b| b.draft.clone()).collect(),
        crate::catalog::clean_opt(input.note.as_deref(), "note", 500)?,
        None,
        None,
        SUPPLIER_RETURN_SERIES,
    );
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    let document = documents::validate(tx, &id, ctx.now)?;
    let actor = ctx.actor();
    for (i, b) in built.iter().enumerate() {
        let f = |name: &str| format!("lines.{i}.{name}");
        // A product tracked by serial number sends back exactly one serial per whole piece;
        // no line ever sends back more serials than its whole pieces.
        let pieces = usize::try_from(b.units.0 / 1_000).unwrap_or(usize::MAX);
        let exact = crate::catalog::extras::get(tx, &b.product.id)?.track_serials;
        if b.serials.len() > pieces
            || (exact && (b.units.0 % 1_000 != 0 || b.serials.len() != pieces))
        {
            return Err(AppError::validation(
                &f("serials"),
                "serials_count_mismatch",
            ));
        }
        let out = NewMovement {
            product_id: b.product.id.clone(),
            location_id: location.id.clone(),
            qty_delta: b
                .units
                .checked_neg()
                .ok_or_else(|| AppError::validation("qty", "qty_invalid"))?,
            reason: MovementReason::Purchase,
            reason_code: None,
            ref_type: Some("document".into()),
            ref_id: Some(document.id.clone()),
            unit_cost: Some(b.net_unit_cost),
            note: None,
        };
        match &b.lot {
            Some(lot) => {
                let found = lots::find(tx, &b.product.id, &lot.lot_number)?
                    .ok_or_else(|| AppError::validation(&f("lotNumber"), "lot_not_found"))?;
                // The named lot leaves only what it holds here: a lot never goes below zero,
                // nor appears where it never was (the product's total cannot tell).
                let held: Option<Qty> = tx
                    .prepare_cached(
                        "SELECT qty FROM lot_levels WHERE lot_id = ?1 AND location_id = ?2",
                    )?
                    .query_row(params![found.id, location.id], |r| r.get(0))
                    .optional()?;
                let held =
                    held.ok_or_else(|| AppError::validation(&f("lotNumber"), "lot_not_found"))?;
                if held < b.units {
                    let shortage = Shortage {
                        product_id: b.product.id.clone(),
                        name_ar: b.product.name_ar.clone(),
                        name_fr: b.product.name_fr.clone(),
                        name_en: b.product.name_en.clone(),
                        location_id: location.id.clone(),
                        available: held,
                        needed: b.units,
                    };
                    return Err(AppError::rejected(
                        json!({ "shortages": [shortage], "lotNumber": found.lot_number }),
                        "stock_insufficient",
                    ));
                }
                stock::append_with_lot(tx, &actor, &out, Some(&found.id), ctx.now)?;
            }
            None => {
                lots::take(tx, &actor, &out, &ctx.business_day(), ctx.now)?;
            }
        }
        for serial_no in &b.serials {
            let serial = serials::find(tx, serial_no)
                .map_err(|e| at_line(i, e))?
                .into_iter()
                .find(|s| s.product_id == b.product.id)
                .ok_or_else(|| AppError::validation(&f("serials"), "serial_not_found"))?;
            // A piece leaves from the location the return takes its stock out of.
            if serial.status == SerialStatus::InStock
                && serial.location_id.as_deref() != Some(location.id.as_str())
            {
                return Err(AppError::validation(
                    &f("serials"),
                    "serial_not_at_location",
                ));
            }
            serials::send_back(tx, &serial.id, ctx.now).map_err(|e| at_line(i, e))?;
        }
    }
    let entry = if input.credit_supplier && document.total_ttc.is_positive() {
        let credit = document
            .total_ttc
            .checked_neg()
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
        Some(crate::suppliers::book(
            tx,
            ctx,
            &supplier.id,
            EntryKind::SupplierReturn,
            credit,
            &document.id,
        )?)
    } else {
        None
    };
    audit::append(
        tx,
        &actor.event(
            "supplier_return.validated",
            "document",
            Some(&document.id),
            json!({ "number": document.number, "supplierId": supplier.id, "totalTtc": document.total_ttc,
                    "credited": entry.is_some(), "locationId": location.id }),
        ),
    )?;
    Ok(SupplierReturnDone { document, entry })
}
