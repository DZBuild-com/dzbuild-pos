//! Purchase lines: HT unit costs, supplier discounts, packs (PRD-031: 5 cartons of 12 at
//! 360 DA = 60 bottles at 30 DA, priced 5 × 360 DA), lots and serials carried to the receipt.

use crate::catalog::products::{self, Product};
use crate::catalog::{clean_opt, tax_rates};
use crate::company::PriceMode;
use crate::documents::DraftLine;
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::sales::cart::at_line;
use crate::stock::lots::LotInput;
use crate::tax::tva::{self, Discount, LineInput};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseLineInput {
    pub product_id: String,
    /// Units, or packs when `pack_qty` is set.
    pub qty: Qty,
    /// HT cost per unit, or per pack when `pack_qty` is set.
    pub unit_cost: Money,
    pub pack_qty: Option<Qty>,
    /// Default: the product's VAT rate.
    pub tax_rate_id: Option<String>,
    #[serde(default)]
    pub discount: Discount,
    /// The order line (receipts) or receipt line (invoices) it comes from.
    pub origin_line_id: Option<String>,
    pub lot_number: Option<String>,
    pub expiry_date: Option<String>,
    #[serde(default)]
    pub serials: Vec<String>,
    pub note: Option<String>,
}

pub(crate) struct BuiltLine {
    pub draft: DraftLine,
    pub product: Product,
    pub units: Qty,
    /// HT cost per unit after the supplier's discount (what the CMUP takes).
    pub net_unit_cost: Money,
    pub lot: Option<LotInput>,
    pub serials: Vec<String>,
}

pub(crate) fn build(
    conn: &Connection,
    lang: Lang,
    lines: &[PurchaseLineInput],
    stocked_only: bool,
    location_id: Option<&str>,
) -> Result<Vec<BuiltLine>, AppError> {
    let mut out = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        let f = |name: &str| format!("lines.{i}.{name}");
        let product = products::get(conn, &l.product_id)
            .ok()
            .filter(|p| !p.archived)
            .ok_or_else(|| AppError::validation(&f("productId"), "product_not_found"))?;
        if stocked_only && !product.track_stock {
            return Err(AppError::validation(&f("productId"), "product_not_stocked"));
        }
        if !l.qty.is_positive() || l.unit_cost.is_negative() {
            return Err(AppError::validation(&f("qty"), "qty_invalid"));
        }
        // The line keeps its quantity in units; for a pack line, `unit_cost` (the pack cost over
        // its units, rounded) is only what the line shows.
        let (units, unit_cost) = match l.pack_qty {
            Some(pack) if pack.is_positive() => (
                l.qty
                    .mul_qty(pack)
                    .ok_or_else(|| AppError::validation(&f("qty"), "qty_invalid"))?,
                l.unit_cost
                    .mul_div(1_000, pack.0)
                    .ok_or_else(|| AppError::validation(&f("unitCost"), "amount_invalid"))?,
            ),
            Some(_) => return Err(AppError::validation(&f("packQty"), "qty_invalid")),
            None => (l.qty, l.unit_cost),
        };
        if !units.fits_decimals(product.unit_decimals) {
            return Err(AppError::validation(&f("qty"), "qty_invalid"));
        }
        let rate_id = l
            .tax_rate_id
            .clone()
            .unwrap_or_else(|| product.tax_rate_id.clone());
        let rate = tax_rates::get(conn, &rate_id)
            .map_err(|_| AppError::validation(&f("taxRateId"), "tax_rate_not_found"))?
            .rate_bp;
        // Priced as the supplier's document is: units × unit cost, or packs × pack cost, so a
        // pack line's HT, TVA and TTC never drift by the rounding of its cost per unit.
        let amounts = tva::compute_line(
            PriceMode::Ht,
            &LineInput {
                unit_price: l.unit_cost,
                qty: l.qty,
                discount: l.discount,
                rate,
            },
        )
        .map_err(|e| at_line(i, e))?;
        let net_unit_cost = amounts
            .total_ht
            .mul_div(1_000, units.0)
            .ok_or_else(|| AppError::validation(&f("unitCost"), "amount_invalid"))?;
        let note = clean_opt(l.note.as_deref(), &f("note"), 200)?;
        let lot =
            clean_opt(l.lot_number.as_deref(), &f("lotNumber"), 40)?.map(|lot_number| LotInput {
                lot_number,
                expiry_date: l.expiry_date.clone(),
            });
        out.push(BuiltLine {
            draft: DraftLine {
                id: new_id(),
                product_id: Some(product.id.clone()),
                description: product.name_in(lang).to_owned(),
                qty: units,
                unit_code: product.unit_code.clone(),
                unit_price: unit_cost,
                price_overridden: false,
                tax_rate_bp: rate,
                amounts,
                cost_price: net_unit_cost,
                parent_line_id: None,
                origin_line_id: l.origin_line_id.clone(),
                restock_location_id: location_id.map(str::to_owned),
                reason_code: None,
                note,
            },
            product,
            units,
            net_unit_cost,
            lot,
            serials: l.serials.clone(),
        });
    }
    Ok(out)
}
