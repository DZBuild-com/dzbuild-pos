//! Credit notes (avoirs; RET-012, DOC-028, DOC-007): the only correction of a validated invoice.
//! A goods credit note brings goods back — pro rata of the invoiced line, the last one taking
//! the exact rest, like M1a returns — into the shop or the damaged-goods location; a financial
//! one reduces the price, spread over the invoice's VAT rates. Goods coming back after a
//! financial credit note are worth their price net of the reductions booked on their VAT rate,
//! and the last goods of a rate take exactly what it still holds. The refund leaves the drawer,
//! goes back by card, transfer or cheque, lowers the customer's account or becomes store
//! credit; a refund carries no stamp duty. Own series (AV), citing the invoice, never above what
//! the invoice still holds. A fully credited invoice stays in its series, validated, and reads
//! as cancelled ("FACTURE ANNULÉE", [`super::state::DocState::Credited`]).

use super::mentions;
use super::validate::seller_party;
use crate::audit;
use crate::cash::{self, MovementKind};
use crate::catalog::products::{self, Product, ProductKind};
use crate::catalog::{UNIT_PIECE_ID, clean_opt, units};
use crate::company::PriceMode;
use crate::documents::links::{self, LinkKind};
use crate::documents::{
    self, CreditKind, DocStatus, DocType, Document, DocumentLine, DraftDocument, DraftExtras,
    DraftLine, DraftPayment, PartyRole, parties,
};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::payments::PaymentMethod;
use crate::stock::{self, MovementReason, NewMovement};
use crate::tax::timbre::StampMode;
use crate::tax::tva::{self, Discount, LineAmounts, LineInput, Totals};
use crate::till::TillContext;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeSet;

/// Why the invoice is corrected (printed on the credit note).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CreditReason {
    Defective,
    WrongItem,
    PriceAdjustment,
    CommercialGesture,
    Cancellation,
    Other,
}

impl CreditReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            CreditReason::Defective => "defective",
            CreditReason::WrongItem => "wrong_item",
            CreditReason::PriceAdjustment => "price_adjustment",
            CreditReason::CommercialGesture => "commercial_gesture",
            CreditReason::Cancellation => "cancellation",
            CreditReason::Other => "other",
        }
    }
}

/// A quantity of an invoiced line coming back (goods credit notes).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreditLineInput {
    pub origin_line_id: String,
    /// Positive, at most what is left of the line.
    pub qty: Qty,
    /// Back to sellable stock, else to the damaged-goods location.
    pub restock: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreditNoteInput {
    pub invoice_id: String,
    pub kind: CreditKind,
    /// Goods credit notes: the lines coming back.
    #[serde(default)]
    pub lines: Vec<CreditLineInput>,
    /// Financial credit notes: the TTC amount taken off.
    #[serde(default)]
    #[specta(optional)]
    pub amount: Option<Money>,
    pub reason: CreditReason,
    /// Cash, card CIB, Edahabia, BaridiMob, transfer, cheque, the account (`credit`) or store
    /// credit (`voucher`).
    pub refund_method: PaymentMethod,
    /// Required for a transfer or a cheque.
    #[serde(default)]
    #[specta(optional)]
    pub refund_reference: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub note: Option<String>,
    /// The invoice's second language when absent.
    #[serde(default)]
    #[specta(optional)]
    pub second_lang: Option<Lang>,
}

/// An invoiced line with what earlier credit notes took from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreditableLine {
    pub line: DocumentLine,
    pub credited: Qty,
    pub remaining: Qty,
    pub remaining_ttc: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreditQuote {
    /// Negative, like the credit note.
    pub total_ttc: Money,
    pub tax_total: Money,
    /// What goes back to the customer (positive).
    pub refund: Money,
    /// TTC the invoice still holds after this credit note.
    pub invoice_remaining: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedCreditNote {
    pub document: Document,
    /// What went back to the customer (positive), with the refund method.
    pub refund: Money,
    /// The customer's balance after a refund to the account or to store credit.
    pub customer_balance: Option<Money>,
    /// Nothing is left on the invoice: it reads "FACTURE ANNULÉE".
    pub invoice_fully_credited: bool,
}

/// What earlier validated credit notes took from an invoiced line: quantity, TTC, tax, discount.
struct Credited {
    qty: Qty,
    ttc: Money,
    tax: Money,
    discount: Money,
}

fn bad() -> AppError {
    AppError::validation("amount", "amount_invalid")
}

fn neg(m: Money) -> Result<Money, AppError> {
    m.checked_neg().ok_or_else(bad)
}

/// The `reason_code` of a financial credit note's lines (one per VAT rate).
const FINANCIAL: &str = "credit_financial";

const ZERO: LineAmounts = LineAmounts {
    gross: Money::ZERO,
    discount_bp: Rate::ZERO,
    discount_amount: Money::ZERO,
    total_ht: Money::ZERO,
    tax: Money::ZERO,
    total_ttc: Money::ZERO,
};

fn already_credited(conn: &Connection, origin_line_id: &str) -> Result<Credited, AppError> {
    let (qty, ttc, tax, discount): (Qty, Money, Money, Money) = conn
        .prepare_cached(
            "SELECT COALESCE(SUM(-l.qty), 0), COALESCE(SUM(-l.total_ttc), 0), COALESCE(SUM(-l.tax_amount), 0),
                    COALESCE(SUM(-l.discount_amount), 0)
             FROM document_lines l JOIN documents d ON d.id = l.document_id
             WHERE l.origin_line_id = ?1 AND d.doc_type = 'credit_note' AND d.status = 'validated'",
        )?
        .query_row([origin_line_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
    Ok(Credited {
        qty,
        ttc,
        tax,
        discount,
    })
}

/// A validated invoice (`invoice_not_creditable` for any other document).
fn invoice_of(conn: &Connection, id: &str) -> Result<Document, AppError> {
    let doc = documents::get(conn, id)?;
    if doc.status != DocStatus::Validated || doc.doc_type != DocType::Invoice {
        return Err(AppError::conflict("invoice_not_creditable"));
    }
    Ok(doc)
}

/// TTC the invoice still holds: its TTC minus every validated credit note on it.
pub fn remaining_ttc(conn: &Connection, invoice: &Document) -> Result<Money, AppError> {
    let credited: Money = conn.query_row(
        "SELECT COALESCE(SUM(-total_ttc), 0) FROM documents
         WHERE parent_id = ?1 AND doc_type = 'credit_note' AND status = 'validated'",
        [&invoice.id],
        |r| r.get(0),
    )?;
    invoice.total_ttc.checked_sub(credited).ok_or_else(bad)
}

/// The lines of a validated invoice (not a kit's components) with what is left to credit:
/// `remaining_ttc` is what a goods credit note of the whole rest of the line would credit, net
/// of the financial credit notes on its VAT rate.
pub fn creditable(conn: &Connection, invoice_id: &str) -> Result<Vec<CreditableLine>, AppError> {
    let invoice = invoice_of(conn, invoice_id)?;
    let rates = held_per_rate(conn, &invoice)?;
    invoice
        .lines
        .into_iter()
        .filter(|l| l.parent_line_id.is_none())
        .map(|line| {
            let before = already_credited(conn, &line.id)?;
            let remaining = line.qty.checked_sub(before.qty).unwrap_or(Qty::ZERO);
            let rate = rate_held(&rates, line.tax_rate_bp);
            let remaining_ttc = if !remaining.is_positive() {
                Money::ZERO
            } else if !rate.at_par && rate.open_lines == 1 {
                // The last goods of a reduced rate take what it still holds.
                rate.ttc
            } else {
                goods_value(&line, &before, remaining, &rate)?.0
            };
            Ok(CreditableLine {
                remaining,
                remaining_ttc,
                credited: before.qty,
                line,
            })
        })
        .collect()
}

/// Stock coming back to a location, at the invoiced cost.
struct Restock {
    product_id: String,
    location_id: String,
    qty: Qty,
    cost: Money,
}

/// The lines of a credit note being built, the stock they bring back and their names in the
/// second language.
#[derive(Default)]
struct CreditLines {
    lines: Vec<DraftLine>,
    restocks: Vec<Restock>,
    alt_names: Vec<(String, String)>,
}

/// A credit note ready to write: the invoice, its lines and totals, and what the invoice held
/// before it.
struct Built {
    invoice: Document,
    second_lang: Option<Lang>,
    credit: CreditLines,
    totals: Totals,
    held: Money,
}

/// The second-language name of a credited line: the invoice's when the credit note prints the
/// invoice's second language, else the product's name in it.
fn alt_name(
    sold: &DocumentLine,
    product: Option<&Product>,
    second: Option<Lang>,
    invoice_second: Option<Lang>,
) -> Option<String> {
    let lang = second?;
    if Some(lang) == invoice_second {
        return sold.description_alt.clone();
    }
    product
        .map(|p| p.name_in(lang))
        .filter(|n| !n.trim().is_empty() && *n != sold.description)
        .map(str::to_owned)
}

fn goods_lines(
    conn: &Connection,
    invoice: &Document,
    input: &CreditNoteInput,
    second_lang: Option<Lang>,
    rates: &[RateHeld],
) -> Result<CreditLines, AppError> {
    if input.lines.is_empty() {
        return Err(AppError::validation("lines", "cart_empty"));
    }
    let shop_location = stock::default_location(conn)?.id;
    let damaged_location = stock::damaged_location(conn)?.id;
    let mut out = CreditLines::default();
    let mut seen = BTreeSet::new();
    // The credited lines (index in `out.lines`), with their rate and whether they take the
    // whole rest of their invoiced line.
    let mut credited: Vec<(usize, Rate, bool)> = Vec::new();
    for (i, l) in input.lines.iter().enumerate() {
        let f = |name: &str| format!("lines.{i}.{name}");
        let sold = invoice
            .lines
            .iter()
            .find(|x| x.id == l.origin_line_id && x.parent_line_id.is_none())
            .ok_or_else(|| AppError::validation(&f("originLineId"), "credit_line_invalid"))?;
        if !seen.insert(sold.id.as_str()) {
            return Err(AppError::validation(&f("originLineId"), "value_invalid"));
        }
        if !l.qty.is_positive() {
            return Err(AppError::validation(&f("qty"), "qty_invalid"));
        }
        let product = sold
            .product_id
            .as_deref()
            .map(|id| products::get(conn, id))
            .transpose()?;
        let decimals = match &product {
            Some(p) => p.unit_decimals,
            None => units::get(conn, UNIT_PIECE_ID)?.decimals,
        };
        let before = already_credited(conn, &sold.id)?;
        let remaining = sold.qty.checked_sub(before.qty).unwrap_or(Qty::ZERO);
        if l.qty > remaining {
            return Err(AppError::validation(&f("qty"), "credit_exceeds_invoice"));
        }
        // The whole rest of a line may always come back, whatever its decimals.
        let whole_rest = l.qty == remaining;
        if !l.qty.fits_decimals(decimals) && !whole_rest {
            return Err(AppError::validation(&f("qty"), "qty_invalid"));
        }
        let rate = rate_held(rates, sold.tax_rate_bp);
        let (ttc, tax) = goods_value(sold, &before, l.qty, &rate)?;
        // The invoice's own discount on the line, pro rata (the whole rest taking its rest).
        let discount = if whole_rest {
            sold.discount_amount
                .checked_sub(before.discount)
                .ok_or_else(bad)?
        } else {
            sold.discount_amount
                .mul_div(l.qty.0, sold.qty.0)
                .ok_or_else(bad)?
        };
        let location = if l.restock {
            &shop_location
        } else {
            &damaged_location
        };
        let line_id = new_id();
        if let Some(alt) = alt_name(sold, product.as_ref(), second_lang, invoice.second_lang) {
            out.alt_names.push((line_id.clone(), alt));
        }
        credited.push((out.lines.len(), sold.tax_rate_bp, whole_rest));
        out.lines.push(DraftLine {
            id: line_id.clone(),
            product_id: sold.product_id.clone(),
            description: sold.description.clone(),
            qty: l.qty.checked_neg().ok_or_else(bad)?,
            unit_code: sold.unit_code.clone(),
            unit_price: sold.unit_price,
            price_overridden: sold.price_overridden,
            tax_rate_bp: sold.tax_rate_bp,
            amounts: credit_amounts(invoice.price_mode, sold.discount_bp, ttc, tax, discount)?,
            cost_price: sold.cost_price,
            parent_line_id: None,
            origin_line_id: Some(sold.id.clone()),
            restock_location_id: product.as_ref().map(|_| location.clone()),
            reason_code: Some(input.reason.as_str().to_owned()),
            note: None,
        });
        let Some(product) = product else { continue };
        if product.kind != ProductKind::Composite {
            if product.track_stock {
                out.restocks.push(Restock {
                    product_id: product.id,
                    location_id: location.clone(),
                    qty: l.qty,
                    cost: sold.cost_price,
                });
            }
            continue;
        }
        // A kit's components come back in proportion, the last credit taking their rest.
        for child in invoice
            .lines
            .iter()
            .filter(|c| c.parent_line_id.as_deref() == Some(sold.id.as_str()))
        {
            let share = if whole_rest {
                child
                    .qty
                    .checked_sub(already_credited(conn, &child.id)?.qty)
                    .unwrap_or(Qty::ZERO)
            } else {
                let part =
                    i128::from(child.qty.0) * i128::from(l.qty.0) / i128::from(sold.qty.0.max(1));
                Qty(i64::try_from(part).map_err(|_| bad())?)
            };
            if !share.is_positive() {
                continue;
            }
            let component = child
                .product_id
                .as_deref()
                .map(|id| products::get(conn, id))
                .transpose()?;
            let child_id = new_id();
            if let Some(alt) = alt_name(child, component.as_ref(), second_lang, invoice.second_lang)
            {
                out.alt_names.push((child_id.clone(), alt));
            }
            out.lines.push(DraftLine {
                id: child_id,
                product_id: child.product_id.clone(),
                description: child.description.clone(),
                qty: share.checked_neg().ok_or_else(bad)?,
                unit_code: child.unit_code.clone(),
                unit_price: Money::ZERO,
                price_overridden: false,
                tax_rate_bp: child.tax_rate_bp,
                amounts: ZERO,
                cost_price: child.cost_price,
                parent_line_id: Some(line_id.clone()),
                origin_line_id: Some(child.id.clone()),
                restock_location_id: Some(location.clone()),
                reason_code: None,
                note: None,
            });
            if let Some(component) = component.filter(|c| c.track_stock) {
                out.restocks.push(Restock {
                    product_id: component.id,
                    location_id: location.clone(),
                    qty: share,
                    cost: child.cost_price,
                });
            }
        }
    }
    // Every good of a rate reduced by a financial credit note comes back: the last line takes
    // exactly what the rate still holds, TTC and TVA.
    for rate in rates.iter().filter(|r| !r.at_par && r.open_lines > 0) {
        let at_rate: Vec<(usize, bool)> = credited
            .iter()
            .filter(|(_, r, _)| *r == rate.rate)
            .map(|&(index, _, whole)| (index, whole))
            .collect();
        if at_rate.iter().filter(|(_, whole)| *whole).count() != rate.open_lines {
            continue;
        }
        let Some(((last, _), others)) = at_rate.split_last() else {
            continue;
        };
        let (mut ttc, mut tax) = (rate.ttc, rate.tax);
        for (index, _) in others {
            let amounts = &out.lines[*index].amounts;
            ttc = ttc.checked_add(amounts.total_ttc).ok_or_else(bad)?;
            tax = tax.checked_add(amounts.tax).ok_or_else(bad)?;
        }
        let line = &mut out.lines[*last];
        let discount = neg(line.amounts.discount_amount)?;
        line.amounts = credit_amounts(
            invoice.price_mode,
            line.amounts.discount_bp,
            ttc,
            tax,
            discount,
        )?;
    }
    Ok(out)
}

/// What one VAT rate of the invoice still holds, and how its goods come back.
#[derive(Debug, Clone, Copy)]
struct RateHeld {
    rate: Rate,
    /// TTC and TVA the rate still holds: the invoice's minus every credit note's at that rate.
    ttc: Money,
    tax: Money,
    /// No financial credit note reduced the rate: its goods come back at their invoiced price.
    at_par: bool,
    /// Invoiced TTC of the goods at that rate not yet back (pro rata of their quantity).
    goods: Money,
    /// Invoice lines at that rate with goods not yet back.
    open_lines: usize,
}

impl RateHeld {
    /// A rate the invoice does not carry: nothing held, goods at their invoiced price.
    const fn none(rate: Rate) -> RateHeld {
        RateHeld {
            rate,
            ttc: Money::ZERO,
            tax: Money::ZERO,
            at_par: true,
            goods: Money::ZERO,
            open_lines: 0,
        }
    }
}

fn rate_held(rates: &[RateHeld], rate: Rate) -> RateHeld {
    rates
        .iter()
        .find(|r| r.rate == rate)
        .copied()
        .unwrap_or(RateHeld::none(rate))
}

/// The invoiced pro rata of `amount` (a line's TTC or TVA) for `qty` of the line, when `back`
/// already came back; the whole rest of the line takes the amount less the pro rata of `back`.
fn invoiced(amount: Money, sold: &DocumentLine, back: Qty, qty: Qty) -> Result<Money, AppError> {
    if back.checked_add(qty) == Some(sold.qty) {
        let gone = amount.mul_div(back.0, sold.qty.0).ok_or_else(bad)?;
        amount.checked_sub(gone).ok_or_else(bad)
    } else {
        amount.mul_div(qty.0, sold.qty.0).ok_or_else(bad)
    }
}

/// What each VAT rate of the invoice still holds, in the invoice's order.
fn held_per_rate(conn: &Connection, invoice: &Document) -> Result<Vec<RateHeld>, AppError> {
    // Per rate: TTC and TVA of the validated credit notes on the invoice, and the TTC of their
    // financial lines.
    let credited: Vec<(Rate, Money, Money, Money)> = conn
        .prepare_cached(
            "SELECT l.tax_rate_bp, COALESCE(SUM(-l.total_ttc), 0), COALESCE(SUM(-l.tax_amount), 0),
                    COALESCE(SUM(CASE WHEN l.reason_code = ?2 THEN -l.total_ttc ELSE 0 END), 0)
             FROM document_lines l JOIN documents d ON d.id = l.document_id
             WHERE d.parent_id = ?1 AND d.doc_type = 'credit_note' AND d.status = 'validated'
             GROUP BY l.tax_rate_bp",
        )?
        .query_map(params![invoice.id, FINANCIAL], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut held = Vec::with_capacity(invoice.taxes.len());
    for t in &invoice.taxes {
        let (ttc_before, tax_before, financial) = credited
            .iter()
            .find(|c| c.0 == t.rate_bp)
            .map_or((Money::ZERO, Money::ZERO, Money::ZERO), |c| (c.1, c.2, c.3));
        let mut goods = Money::ZERO;
        let mut open_lines = 0;
        for line in invoice
            .lines
            .iter()
            .filter(|l| l.parent_line_id.is_none() && l.tax_rate_bp == t.rate_bp)
        {
            let back = already_credited(conn, &line.id)?.qty;
            let Some(rest) = line.qty.checked_sub(back).filter(|q| q.is_positive()) else {
                continue;
            };
            open_lines += 1;
            goods = goods
                .checked_add(invoiced(line.total_ttc, line, back, rest)?)
                .ok_or_else(bad)?;
        }
        held.push(RateHeld {
            rate: t.rate_bp,
            ttc: t
                .base_ht
                .checked_add(t.tax_amount)
                .and_then(|ttc| ttc.checked_sub(ttc_before))
                .ok_or_else(bad)?,
            tax: t.tax_amount.checked_sub(tax_before).ok_or_else(bad)?,
            at_par: financial == Money::ZERO,
            goods,
            open_lines,
        });
    }
    Ok(held)
}

/// TTC and TVA of `qty` of an invoiced line coming back. While no financial credit note reduced
/// its rate: the pro rata of the line, the whole rest of the line taking its exact rest (like
/// M1a returns). After one: net of the reductions booked on the rate, the invoiced pro rata ×
/// what the rate still holds ÷ what its goods not yet back were invoiced — so the goods of a
/// rate, all back, take exactly what it holds.
fn goods_value(
    sold: &DocumentLine,
    before: &Credited,
    qty: Qty,
    rate: &RateHeld,
) -> Result<(Money, Money), AppError> {
    if rate.at_par {
        return if before.qty.checked_add(qty) == Some(sold.qty) {
            Ok((
                sold.total_ttc.checked_sub(before.ttc).ok_or_else(bad)?,
                sold.tax_amount.checked_sub(before.tax).ok_or_else(bad)?,
            ))
        } else {
            Ok((
                sold.total_ttc.mul_div(qty.0, sold.qty.0).ok_or_else(bad)?,
                sold.tax_amount.mul_div(qty.0, sold.qty.0).ok_or_else(bad)?,
            ))
        };
    }
    if !rate.goods.is_positive() {
        return Ok((Money::ZERO, Money::ZERO));
    }
    let net = |amount: Money| -> Result<Money, AppError> {
        invoiced(amount, sold, before.qty, qty)?
            .mul_div(rate.ttc.0, rate.goods.0)
            .ok_or_else(bad)
    };
    Ok((net(sold.total_ttc)?, net(sold.tax_amount)?))
}

/// The amounts of a credited line (negative) from its TTC, TVA and discount; `gross` is before
/// the discount, in the invoice's price mode (TTC, or HT).
fn credit_amounts(
    mode: PriceMode,
    discount_bp: Rate,
    ttc: Money,
    tax: Money,
    discount: Money,
) -> Result<LineAmounts, AppError> {
    let ht = ttc.checked_sub(tax).ok_or_else(bad)?;
    let net = if mode == PriceMode::Ht { ht } else { ttc };
    Ok(LineAmounts {
        gross: neg(net.checked_add(discount).ok_or_else(bad)?)?,
        discount_bp,
        discount_amount: neg(discount)?,
        total_ht: neg(ht)?,
        tax: neg(tax)?,
        total_ttc: neg(ttc)?,
    })
}

/// A price reduction of `amount` TTC, one line per VAT rate in proportion of what each rate
/// still holds (the last rate takes the rest), described by the invoice number.
fn financial_lines(
    invoice: &Document,
    input: &CreditNoteInput,
    held: Money,
    rates: &[RateHeld],
) -> Result<CreditLines, AppError> {
    let amount = input
        .amount
        .filter(|a| a.is_positive())
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
    if amount > held {
        return Err(AppError::validation("amount", "credit_exceeds_invoice"));
    }
    // The rates that still hold something, credited in full ones left out.
    let rates: Vec<(Rate, Money)> = rates
        .iter()
        .filter(|r| r.ttc.is_positive())
        .map(|r| (r.rate, r.ttc))
        .collect();
    let base = Money::checked_sum(rates.iter().map(|(_, m)| *m)).ok_or_else(bad)?;
    let number = invoice.number.clone().unwrap_or_default();
    let mut rest = amount;
    let mut out = CreditLines::default();
    for (i, (rate, ttc)) in rates.iter().enumerate() {
        let share = if i + 1 == rates.len() {
            rest
        } else {
            amount.mul_div(ttc.0, base.0).ok_or_else(bad)?
        };
        rest = rest.checked_sub(share).ok_or_else(bad)?;
        if !share.is_positive() {
            continue;
        }
        let amounts = tva::compute_line(
            PriceMode::Ttc,
            &LineInput {
                unit_price: share,
                qty: Qty(1_000),
                discount: Discount::None,
                rate: *rate,
            },
        )?;
        // The unit price in the invoice's price mode: TTC, or HT on an invoice priced HT.
        let unit_price = if invoice.price_mode == PriceMode::Ht {
            amounts.total_ht
        } else {
            share
        };
        out.lines.push(DraftLine {
            id: new_id(),
            product_id: None,
            description: number.clone(),
            qty: Qty(-1_000),
            unit_code: String::new(),
            unit_price,
            price_overridden: false,
            tax_rate_bp: *rate,
            amounts: LineAmounts {
                gross: unit_price,
                ..amounts
            }
            .negated()?,
            cost_price: Money::ZERO,
            parent_line_id: None,
            origin_line_id: None,
            restock_location_id: None,
            reason_code: Some(FINANCIAL.to_owned()),
            note: None,
        });
    }
    Ok(out)
}

fn build(conn: &Connection, input: &CreditNoteInput) -> Result<Built, AppError> {
    let invoice = invoice_of(conn, &input.invoice_id)?;
    let second_lang = input.second_lang.or(invoice.second_lang);
    if second_lang == Some(invoice.lang) {
        return Err(AppError::validation("secondLang", "value_invalid"));
    }
    let held = remaining_ttc(conn, &invoice)?;
    let rates = held_per_rate(conn, &invoice)?;
    let (credit, field) = match input.kind {
        CreditKind::Goods => (
            goods_lines(conn, &invoice, input, second_lang, &rates)?,
            "lines",
        ),
        CreditKind::Financial => (financial_lines(&invoice, input, held, &rates)?, "amount"),
    };
    let totals = tva::totals(credit.lines.iter().map(|l| (l.tax_rate_bp, &l.amounts)))?;
    // A guard: goods never take more than what the invoice, or one of its VAT rates, still
    // holds (their value is net of the financial credit notes, so only rounding could).
    let over_a_rate = || -> Result<bool, AppError> {
        for r in &totals.per_rate {
            if neg(r.total_ttc)? > rate_held(&rates, r.rate_bp).ttc {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if neg(totals.total_ttc)? > held || (input.kind == CreditKind::Goods && over_a_rate()?) {
        return Err(AppError::validation(field, "credit_exceeds_invoice"));
    }
    Ok(Built {
        invoice,
        second_lang,
        credit,
        totals,
        held,
    })
}

/// The figures of the credit note being typed (nothing written).
pub fn quote(
    conn: &Connection,
    _ctx: &TillContext,
    input: &CreditNoteInput,
) -> Result<CreditQuote, AppError> {
    let built = build(conn, input)?;
    Ok(CreditQuote {
        total_ttc: built.totals.total_ttc,
        tax_total: built.totals.tax_total,
        refund: neg(built.totals.total_ttc)?,
        invoice_remaining: built
            .held
            .checked_add(built.totals.total_ttc)
            .ok_or_else(bad)?,
    })
}

/// Validates a credit note on an invoice in the caller's transaction: numbered in the AV series,
/// the invoice's buyer and today's seller frozen, linked to the invoice, the goods back in
/// stock, the refund paid out (no stamp duty) and one `credit_note.created` audit row.
pub fn create_credit_note(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: CreditNoteInput,
) -> Result<ValidatedCreditNote, AppError> {
    let Built {
        invoice,
        second_lang,
        credit,
        totals,
        held: _,
    } = build(tx, &input)?;
    mentions::require(tx, DocType::CreditNote, invoice.customer_id.as_deref())?;
    let reference = clean_opt(input.refund_reference.as_deref(), "refundReference", 48)?;
    match input.refund_method {
        PaymentMethod::Cash
        | PaymentMethod::CardCib
        | PaymentMethod::Edahabia
        | PaymentMethod::Baridimob => {}
        PaymentMethod::Transfer | PaymentMethod::Cheque if reference.is_none() => {
            return Err(AppError::validation("refundReference", "required"));
        }
        PaymentMethod::Transfer | PaymentMethod::Cheque => {}
        PaymentMethod::Credit | PaymentMethod::Voucher if invoice.customer_id.is_none() => {
            return Err(AppError::validation("customerId", "customer_required"));
        }
        PaymentMethod::Credit | PaymentMethod::Voucher => {}
        PaymentMethod::Change => {
            return Err(AppError::validation(
                "refundMethod",
                "payment_method_invalid",
            ));
        }
    }
    let session = if input.refund_method == PaymentMethod::Cash {
        Some(cash::require_open(tx, &ctx.session.terminal_id)?)
    } else {
        cash::current(tx, &ctx.session.terminal_id)?
    };
    let refund = neg(totals.total_ttc)?;
    let payments = if refund.is_positive() {
        vec![DraftPayment {
            method: input.refund_method,
            amount: totals.total_ttc,
            reference: reference.clone(),
            cheque_bank: None,
            cheque_date: None,
        }]
    } else {
        Vec::new()
    };
    let CreditLines {
        lines,
        restocks,
        alt_names,
    } = credit;
    let draft = DraftDocument {
        doc_type: DocType::CreditNote,
        terminal_code: ctx.session.terminal_code.clone(),
        business_date: ctx.business_date(),
        customer_id: invoice.customer_id.clone(),
        supplier_id: None,
        user_id: ctx.session.user_id.clone(),
        terminal_id: ctx.session.terminal_id.clone(),
        cash_session_id: session.as_ref().map(|s| s.id.clone()),
        price_level_id: None,
        lang: invoice.lang,
        // The lines copy the invoice's amounts: its regime and price mode, whatever today's.
        tax_regime: invoice.tax_regime,
        price_mode: invoice.price_mode,
        stamp_mode: StampMode::Off,
        stamp_duty: Money::ZERO,
        stamp_charged: Money::ZERO,
        rounding: Money::ZERO,
        paid_total: totals.total_ttc,
        change_given: Money::ZERO,
        // What the note takes off the kridi, as the invoice records what it put on it (M2's
        // return notes): "on credit" in the sales reports nets it. Store credit is not kridi.
        credit_amount: if input.refund_method == PaymentMethod::Credit {
            totals.total_ttc
        } else {
            Money::ZERO
        },
        note: clean_opt(input.note.as_deref(), "note", 200)?,
        parent_id: Some(invoice.id.clone()),
        source_ref: None,
        lines,
        payments,
    };
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    documents::set_extras(
        tx,
        &id,
        &DraftExtras {
            second_lang,
            credit_kind: Some(input.kind),
            ..DraftExtras::default()
        },
    )?;
    documents::set_alt_descriptions(tx, &id, &alt_names)?;
    parties::record(tx, &id, &seller_party(tx, invoice.lang, second_lang)?)?;
    if let Some(buyer) = invoice.parties.iter().find(|p| p.role == PartyRole::Buyer) {
        parties::record(tx, &id, buyer)?;
    }
    links::add(tx, &invoice.id, &id, LinkKind::Credited, ctx.now)?;
    let document = documents::validate(tx, &id, ctx.now)?;
    let actor = ctx.actor();
    for r in &restocks {
        // Back without a lot, into the unlotted part of the level, like M1a returns.
        stock::append(
            tx,
            &actor,
            &NewMovement {
                product_id: r.product_id.clone(),
                location_id: r.location_id.clone(),
                qty_delta: r.qty,
                reason: MovementReason::Return,
                reason_code: None,
                ref_type: Some("document".into()),
                ref_id: Some(document.id.clone()),
                unit_cost: Some(r.cost),
                note: None,
            },
            ctx.now,
        )?;
    }
    if let (PaymentMethod::Cash, true, Some(s)) =
        (input.refund_method, refund.is_positive(), &session)
    {
        cash::record(
            tx,
            ctx,
            &s.id,
            MovementKind::Refund,
            totals.total_ttc,
            None,
            Some(("document", &document.id)),
            None,
        )?;
    }
    let customer_balance = match (input.refund_method, &invoice.customer_id) {
        (PaymentMethod::Credit | PaymentMethod::Voucher, Some(customer_id))
            if refund.is_positive() =>
        {
            Some(crate::customers::credit::book_refund(
                tx,
                ctx,
                customer_id,
                refund,
                input.refund_method,
                &document.id,
            )?)
        }
        _ => None,
    };
    let invoice_fully_credited = !remaining_ttc(tx, &invoice)?.is_positive();
    audit::append(
        tx,
        &actor.event(
            "credit_note.created",
            "document",
            Some(&document.id),
            json!({
                "number": document.number,
                "invoice": invoice.number,
                "kind": input.kind,
                "reason": input.reason,
                "refund": refund,
                "refundMethod": input.refund_method,
                "customerId": invoice.customer_id,
                "fullyCredited": invoice_fully_credited,
                "approvals": ctx.approvers(),
            }),
        ),
    )?;
    Ok(ValidatedCreditNote {
        document,
        refund,
        customer_balance,
        invoice_fully_credited,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::drafts::tests::{cart, fixture, input, save};
    use crate::billing::state::{DocState, state};
    use crate::billing::validate::{ValidateInput, validate};
    use crate::payments::PaymentInput;
    use crate::sales::cart::CartLine;
    use crate::testkit::{Shop, shop};
    use proptest::prelude::*;

    /// A validated invoice: 10 bags of cement (1 190.00) and 2 m3 of sand (5 450.00), on account.
    fn invoice(shop: &Shop) -> Document {
        let (amine, cement, sand) = fixture(shop);
        shop.stock_in(&cement.id, 100_000);
        shop.stock_in(&sand.id, 100_000);
        let draft = save(
            shop,
            None,
            input(
                DocType::Invoice,
                cart(
                    Some(&amine),
                    vec![
                        CartLine::product(&cement.id, Qty(10_000)),
                        CartLine::product(&sand.id, Qty(2_000)),
                    ],
                ),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let credit = PaymentInput {
            method: PaymentMethod::Credit,
            amount: Money(2_280_000),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        };
        shop.db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![credit],
                    },
                )
            })
            .unwrap()
            .document
    }

    fn goods(invoice: &Document, line: usize, qty: i64, method: PaymentMethod) -> CreditNoteInput {
        CreditNoteInput {
            invoice_id: invoice.id.clone(),
            kind: CreditKind::Goods,
            lines: vec![CreditLineInput {
                origin_line_id: invoice.lines[line].id.clone(),
                qty: Qty(qty),
                restock: true,
            }],
            amount: None,
            reason: CreditReason::Defective,
            refund_method: method,
            refund_reference: None,
            note: None,
            second_lang: None,
        }
    }

    fn create(shop: &Shop, input: CreditNoteInput) -> Result<ValidatedCreditNote, AppError> {
        let ctx = shop.owner_till();
        shop.db.write(move |tx| create_credit_note(tx, &ctx, input))
    }

    #[test]
    fn a_goods_credit_note_restocks_and_credits_the_pro_rata_of_the_invoice() {
        let shop = shop();
        let inv = invoice(&shop);
        let stock_before = shop.count("SELECT SUM(qty_delta) FROM stock_movements");
        let done = create(&shop, goods(&inv, 0, 3_000, PaymentMethod::Credit)).unwrap();
        let av = &done.document;
        assert_eq!(av.number.as_deref(), Some("AV2026-000001"));
        assert_eq!(
            (av.doc_type, av.credit_kind, av.parent_id.clone()),
            (
                DocType::CreditNote,
                Some(CreditKind::Goods),
                Some(inv.id.clone())
            )
        );
        assert_eq!(
            (av.total_ttc, av.tax_total, av.stamp_duty),
            (Money(-357_000), Money(-57_000), Money::ZERO)
        );
        assert_eq!(done.refund, Money(357_000));
        assert_eq!(
            done.customer_balance,
            Some(Money(2_280_000 - 357_000)),
            "the debt goes down"
        );
        assert_eq!(
            shop.count("SELECT SUM(qty_delta) FROM stock_movements"),
            stock_before + 3_000
        );
        assert_eq!(av.parties[1].nif, inv.parties[1].nif, "the invoice's buyer");
        let id = inv.id.clone();
        assert_eq!(
            shop.db.read(move |c| state(c, &id, "2026-09-28")).unwrap(),
            DocState::PartlyCredited {
                credited: Money(357_000),
                remaining: Money(2_280_000 - 357_000)
            }
        );
        let id = inv.id.clone();
        let left = shop.db.read(move |c| creditable(c, &id)).unwrap();
        assert_eq!(
            (left[0].credited, left[0].remaining),
            (Qty(3_000), Qty(7_000))
        );
    }

    /// M2's return notes, carried to credit notes: a credit note refunded to the kridi carries
    /// what it takes off the account, as the invoice carries what it put on it, so "on credit"
    /// in the sales reports nets it; store credit given back is not kridi.
    #[test]
    fn a_credit_note_to_the_kridi_takes_its_amount_off_the_credit_sales() {
        let shop = shop();
        let inv = invoice(&shop);
        assert_eq!(inv.credit_amount, Money(2_280_000));
        let to_kridi = create(&shop, goods(&inv, 0, 3_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(to_kridi.document.credit_amount, Money(-357_000));
        let voucher = create(&shop, goods(&inv, 0, 1_000, PaymentMethod::Voucher)).unwrap();
        assert_eq!(
            voucher.document.credit_amount,
            Money::ZERO,
            "store credit is not kridi"
        );
        let request = crate::reports::ReportRequest::new(
            crate::reports::ReportKind::SalesSummary,
            "2026-09-28",
            "2026-09-28",
        );
        let summary = shop
            .db
            .read(move |c| crate::reports::run(c, &request))
            .unwrap();
        let column = summary.table.column("onCredit").unwrap();
        assert_eq!(
            summary.table.totals.unwrap()[column],
            crate::table::Cell::int(2_280_000 - 357_000)
        );
    }

    #[test]
    fn a_financial_credit_note_spreads_over_the_invoice_rates() {
        let shop = shop();
        let inv = invoice(&shop); // 1 190 000 at 19 %, 1 090 000 at 9 %
        let done = create(
            &shop,
            CreditNoteInput {
                invoice_id: inv.id.clone(),
                kind: CreditKind::Financial,
                lines: Vec::new(),
                amount: Some(Money(228_000)),
                reason: CreditReason::CommercialGesture,
                refund_method: PaymentMethod::Credit,
                refund_reference: None,
                note: None,
                second_lang: None,
            },
        )
        .unwrap();
        let av = done.document;
        assert_eq!(av.total_ttc, Money(-228_000));
        assert_eq!(
            av.lines
                .iter()
                .map(|l| (l.tax_rate_bp, l.total_ttc))
                .collect::<Vec<_>>(),
            [(Rate(1_900), Money(-119_000)), (Rate(900), Money(-109_000))]
        );
        assert!(
            av.lines
                .iter()
                .all(|l| l.reason_code.as_deref() == Some("credit_financial"))
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE reason = 'return'"),
            0
        );
    }

    #[test]
    fn credit_notes_stop_at_what_the_invoice_holds_and_then_cancel_it() {
        let shop = shop();
        let inv = invoice(&shop);
        let err = create(&shop, goods(&inv, 0, 11_000, PaymentMethod::Credit)).unwrap_err();
        assert_eq!(err.code(), "credit_exceeds_invoice");
        create(&shop, goods(&inv, 0, 10_000, PaymentMethod::Credit)).unwrap();
        let mut rest = goods(&inv, 1, 2_000, PaymentMethod::Credit);
        rest.reason = CreditReason::Cancellation;
        let done = create(&shop, rest).unwrap();
        assert!(done.invoice_fully_credited);
        let id = inv.id.clone();
        assert!(matches!(
            shop.db.read(move |c| state(c, &id, "2026-09-28")).unwrap(),
            DocState::Credited { .. }
        ));
        let mut more = goods(&inv, 1, 1, PaymentMethod::Credit);
        more.kind = CreditKind::Financial;
        more.lines.clear();
        more.amount = Some(Money(1));
        assert_eq!(
            create(&shop, more).unwrap_err().code(),
            "credit_exceeds_invoice"
        );
    }

    #[test]
    fn cash_refunds_need_an_open_till_and_leave_the_drawer() {
        let shop = shop();
        let inv = invoice(&shop);
        let err = create(&shop, goods(&inv, 0, 1_000, PaymentMethod::Cash)).unwrap_err();
        assert_eq!(err.code(), "cash_session_closed");
        shop.open_till(500_000);
        create(&shop, goods(&inv, 0, 1_000, PaymentMethod::Cash)).unwrap();
        assert_eq!(
            shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'refund'"),
            -119_000
        );
        let mut by_transfer = goods(&inv, 0, 1_000, PaymentMethod::Transfer);
        assert_eq!(
            create(&shop, by_transfer.clone())
                .unwrap_err()
                .payload()
                .field
                .as_deref(),
            Some("refundReference")
        );
        by_transfer.refund_reference = Some("VIR-2026-1002".into());
        create(&shop, by_transfer).unwrap();
        let wrong = CreditNoteInput {
            invoice_id: inv.lines[0].id.clone(),
            ..goods(&inv, 0, 1_000, PaymentMethod::Credit)
        };
        assert_eq!(
            create(&shop, wrong).unwrap_err().code(),
            "document_not_found"
        );
    }

    /// A financial credit note of `amount` TTC on the invoice (228 000 is a 10 % gesture:
    /// 119 000 at 19 %, 109 000 at 9 %).
    fn gesture(inv: &Document, amount: i64) -> CreditNoteInput {
        CreditNoteInput {
            kind: CreditKind::Financial,
            lines: Vec::new(),
            amount: Some(Money(amount)),
            reason: CreditReason::CommercialGesture,
            ..goods(inv, 0, 1_000, PaymentMethod::Credit)
        }
    }

    /// TTC and TVA every credit note took at 19 % (the cement's rate), positive.
    fn credited_at_19(shop: &Shop) -> (i64, i64) {
        (
            shop.count(
                "SELECT -SUM(l.total_ttc) FROM document_lines l JOIN documents d ON d.id = l.document_id
                 WHERE d.doc_type = 'credit_note' AND l.tax_rate_bp = 1900",
            ),
            shop.count(
                "SELECT -SUM(l.tax_amount) FROM document_lines l JOIN documents d ON d.id = l.document_id
                 WHERE d.doc_type = 'credit_note' AND l.tax_rate_bp = 1900",
            ),
        )
    }

    #[test]
    fn after_a_financial_credit_note_goods_come_back_net_of_it() {
        let shop = shop();
        let inv = invoice(&shop);
        create(&shop, goods(&inv, 0, 10_000, PaymentMethod::Credit)).unwrap();
        let financial = CreditNoteInput {
            reason: CreditReason::PriceAdjustment,
            ..gesture(&inv, 109_000)
        };
        let av = create(&shop, financial).unwrap().document;
        assert_eq!(
            av.lines
                .iter()
                .map(|l| (l.tax_rate_bp, l.total_ttc))
                .collect::<Vec<_>>(),
            [(Rate(900), Money(-109_000))],
            "the 19 % rate was credited in full: the reduction is all at 9 %"
        );
        // 1 090 000 - 109 000 = 981 000 left at 9 %: each m3 of sand comes back 10 % off.
        let (ctx, one) = (
            shop.owner_till(),
            goods(&inv, 1, 1_000, PaymentMethod::Credit),
        );
        let figures = shop.db.read(move |c| quote(c, &ctx, &one)).unwrap();
        assert_eq!(
            figures,
            CreditQuote {
                total_ttc: Money(-490_500),
                tax_total: Money(-40_500),
                refund: Money(490_500),
                invoice_remaining: Money(981_000 - 490_500),
            }
        );
        let done = create(&shop, goods(&inv, 1, 1_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(
            (done.refund, done.document.total_ttc),
            (figures.refund, figures.total_ttc),
            "the quote showed what was credited"
        );
        let last = create(&shop, goods(&inv, 1, 1_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(
            (last.refund, last.document.tax_total),
            (Money(490_500), Money(-40_500)),
            "the second m3 takes what the 9 % rate still holds"
        );
        assert!(last.invoice_fully_credited);
    }

    #[test]
    fn after_a_gesture_the_ten_bags_come_back_for_what_their_rate_holds() {
        let shop = shop();
        let inv = invoice(&shop);
        create(&shop, gesture(&inv, 228_000)).unwrap();
        let bags = create(&shop, goods(&inv, 0, 10_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(
            (bags.refund, bags.document.tax_total),
            (Money(1_071_000), Money(-171_000)),
            "10 x 1 190.00 less the 119 000 of the gesture at 19 %"
        );
        assert_eq!(
            credited_at_19(&shop),
            (inv.lines[0].total_ttc.0, inv.lines[0].tax_amount.0),
            "the 19 % rate is credited in full, TTC and TVA"
        );
        assert_eq!(
            bags.customer_balance,
            Some(Money(2_280_000 - 228_000 - 1_071_000))
        );
    }

    #[test]
    fn after_a_gesture_nine_bags_then_the_last_one_come_back_net() {
        let shop = shop();
        let inv = invoice(&shop);
        create(&shop, gesture(&inv, 228_000)).unwrap();
        let id = inv.id.clone();
        let left = shop.db.read(move |c| creditable(c, &id)).unwrap();
        assert_eq!(
            (left[0].remaining_ttc, left[1].remaining_ttc),
            (Money(1_071_000), Money(981_000)),
            "what the goods are worth net of the gesture"
        );
        let nine = create(&shop, goods(&inv, 0, 9_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(
            (nine.refund, nine.document.tax_total),
            (Money(963_900), Money(-153_900)),
            "9 x 1 190.00 less 10 %"
        );
        let id = inv.id.clone();
        let left = shop.db.read(move |c| creditable(c, &id)).unwrap();
        assert_eq!(
            (left[0].remaining, left[0].remaining_ttc),
            (Qty(1_000), Money(107_100))
        );
        let last = create(&shop, goods(&inv, 0, 1_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(
            (last.refund, last.document.tax_total),
            (Money(107_100), Money(-17_100)),
            "the last bag takes what the 19 % rate still holds"
        );
        assert_eq!(
            credited_at_19(&shop),
            (inv.lines[0].total_ttc.0, inv.lines[0].tax_amount.0)
        );
        let id = inv.id.clone();
        let left = shop.db.read(move |c| creditable(c, &id)).unwrap();
        assert_eq!(
            (left[0].remaining, left[0].remaining_ttc),
            (Qty::ZERO, Money::ZERO)
        );
    }

    #[test]
    fn after_a_gesture_the_lines_of_one_rate_share_it_and_the_last_takes_the_rest() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let plaster = shop.product("Plâtre 5 kg", Money(59_500), &shop.tva19);
        shop.stock_in(&cement.id, 100_000);
        shop.stock_in(&plaster.id, 100_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::Invoice,
                cart(
                    Some(&amine),
                    vec![
                        CartLine::product(&cement.id, Qty(10_000)),
                        CartLine::product(&plaster.id, Qty(4_000)),
                    ],
                ),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let credit = PaymentInput {
            method: PaymentMethod::Credit,
            amount: Money(1_428_000),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        };
        let inv = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![credit],
                    },
                )
            })
            .unwrap()
            .document;
        create(&shop, gesture(&inv, 142_800)).unwrap(); // 10 %, all at 19 %
        let id = inv.id.clone();
        let left = shop.db.read(move |c| creditable(c, &id)).unwrap();
        assert_eq!(
            (left[0].remaining_ttc, left[1].remaining_ttc),
            (Money(1_071_000), Money(214_200))
        );
        let five = create(&shop, goods(&inv, 0, 5_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(five.refund, Money(535_500));
        let one = create(&shop, goods(&inv, 1, 1_000, PaymentMethod::Credit)).unwrap();
        assert_eq!(one.refund, Money(53_550));
        let mut rest = goods(&inv, 0, 5_000, PaymentMethod::Credit);
        rest.lines.push(CreditLineInput {
            origin_line_id: inv.lines[1].id.clone(),
            qty: Qty(3_000),
            restock: true,
        });
        let rest = create(&shop, rest).unwrap();
        assert_eq!(
            rest.document
                .lines
                .iter()
                .map(|l| (l.total_ttc, l.tax_amount))
                .collect::<Vec<_>>(),
            [
                (Money(-535_500), Money(-85_500)),
                (Money(-160_650), Money(-25_650))
            ],
            "the plaster, last of the rate, takes what the rate holds after the cement"
        );
        assert!(rest.invoice_fully_credited);
        assert_eq!(credited_at_19(&shop), (1_428_000, 228_000));
    }

    #[test]
    fn after_a_gesture_crediting_every_good_cancels_the_invoice() {
        let shop = shop();
        let inv = invoice(&shop);
        create(&shop, gesture(&inv, 228_000)).unwrap();
        let stock_before = shop.count("SELECT SUM(qty_delta) FROM stock_movements");
        let mut everything = goods(&inv, 0, 10_000, PaymentMethod::Credit);
        everything.lines.push(CreditLineInput {
            origin_line_id: inv.lines[1].id.clone(),
            qty: Qty(2_000),
            restock: true,
        });
        everything.reason = CreditReason::Cancellation;
        let done = create(&shop, everything).unwrap();
        assert_eq!(done.refund, Money(1_071_000 + 981_000));
        assert!(done.invoice_fully_credited);
        assert_eq!(done.customer_balance, Some(Money::ZERO));
        assert_eq!(
            shop.count("SELECT SUM(qty_delta) FROM stock_movements"),
            stock_before + 12_000,
            "every good is back in stock"
        );
        let id = inv.id.clone();
        assert!(matches!(
            shop.db.read(move |c| state(c, &id, "2026-09-28")).unwrap(),
            DocState::Credited { .. }
        ));
    }

    #[test]
    fn a_bilingual_credit_note_names_its_lines_in_the_second_language() {
        let shop = shop();
        let (amine, _, _) = fixture(&shop);
        let cement = shop.product_with({
            let mut p = crate::catalog::products::ProductInput::simple(
                "Ciment 25 kg",
                UNIT_PIECE_ID,
                &shop.tva19,
                Money(119_000),
            );
            p.name_ar = "إسمنت 25 كغ".into();
            p
        });
        shop.stock_in(&cement.id, 100_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::Invoice,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(2_000))],
                ),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let credit = PaymentInput {
            method: PaymentMethod::Credit,
            amount: Money(238_000),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        };
        let inv = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![credit],
                    },
                )
            })
            .unwrap()
            .document;
        let mut same = goods(&inv, 0, 1_000, PaymentMethod::Credit);
        same.second_lang = Some(Lang::Fr);
        let err = create(&shop, same).unwrap_err();
        assert_eq!(
            (err.payload().field.as_deref(), err.code().as_str()),
            (Some("secondLang"), "value_invalid")
        );
        let mut arabic = goods(&inv, 0, 1_000, PaymentMethod::Credit);
        arabic.second_lang = Some(Lang::Ar);
        let av = create(&shop, arabic).unwrap().document;
        assert_eq!(av.second_lang, Some(Lang::Ar));
        assert_eq!(av.lines[0].description_alt.as_deref(), Some("إسمنت 25 كغ"));
        assert_eq!(av.parties[0].name_alt.as_deref(), Some("مغازة الأمل"));
    }

    #[test]
    fn a_kit_comes_back_as_its_components_in_proportion() {
        use crate::catalog::products::{Component, ProductInput};
        let shop = shop();
        let (amine, _, _) = fixture(&shop);
        let pen = shop.product("Stylo", Money(5_000), &shop.tva19);
        let mut kit = ProductInput::simple("Kit", UNIT_PIECE_ID, &shop.tva19, Money(9_000));
        kit.kind = ProductKind::Composite;
        kit.components = vec![Component {
            component_id: pen.id.clone(),
            qty: Qty(2_000),
        }];
        let kit = shop.product_with(kit);
        shop.stock_in(&pen.id, 100_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::Invoice,
                cart(Some(&amine), vec![CartLine::product(&kit.id, Qty(3_000))]),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let credit = PaymentInput {
            method: PaymentMethod::Credit,
            amount: Money(27_000),
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        };
        let inv = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![credit],
                    },
                )
            })
            .unwrap()
            .document;
        let mut damaged = goods(&inv, 0, 1_000, PaymentMethod::Credit);
        damaged.lines[0].restock = false;
        let av = create(&shop, damaged).unwrap().document;
        assert_eq!(
            av.lines
                .iter()
                .map(|l| (l.qty, l.total_ttc, l.parent_line_id.is_some()))
                .collect::<Vec<_>>(),
            [
                (Qty(-1_000), Money(-9_000), false),
                (Qty(-2_000), Money::ZERO, true)
            ]
        );
        assert_eq!(av.lines[1].origin_line_id.as_ref(), Some(&inv.lines[1].id));
        create(&shop, goods(&inv, 0, 2_000, PaymentMethod::Credit)).unwrap();
        let back = |sql: &'static str| shop.count(sql);
        assert_eq!(
            (
                back(
                    "SELECT SUM(qty_delta) FROM stock_movements WHERE reason = 'return'
                     AND location_id IN (SELECT id FROM stock_locations WHERE kind = 'damaged')"
                ),
                back(
                    "SELECT SUM(qty_delta) FROM stock_movements WHERE reason = 'return'
                     AND location_id IN (SELECT id FROM stock_locations WHERE is_default = 1)"
                ),
            ),
            (2_000, 4_000),
            "one kit's two pens to the damaged goods, the other four back on the shelf"
        );
        let chains = shop.db.read(documents::verify_all).unwrap();
        assert!(
            chains.iter().all(|c| c.first_broken_seq.is_none()),
            "the AV chain verifies: {chains:?}"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]
        /// However the ten bags come back, the credits never exceed the line and the last one
        /// takes exactly what is left of its TTC and TVA.
        #[test]
        fn credit_notes_never_exceed_the_invoice(parts in proptest::collection::vec(1_i64..=4, 1..8)) {
            let shop = shop();
            let inv = invoice(&shop);
            let mut left = 10_000_i64;
            for part in parts {
                let qty = (part * 1_000).min(left);
                if qty == 0 { break; }
                create(&shop, goods(&inv, 0, qty, PaymentMethod::Credit)).unwrap();
                left -= qty;
            }
            if left > 0 {
                create(&shop, goods(&inv, 0, left, PaymentMethod::Credit)).unwrap();
            }
            let credited = shop.count(
                "SELECT -SUM(l.total_ttc) FROM document_lines l JOIN documents d ON d.id = l.document_id
                 WHERE d.doc_type = 'credit_note' AND l.parent_line_id IS NULL",
            );
            let tax = shop.count(
                "SELECT -SUM(l.tax_amount) FROM document_lines l JOIN documents d ON d.id = l.document_id
                 WHERE d.doc_type = 'credit_note' AND l.parent_line_id IS NULL",
            );
            prop_assert_eq!(credited, inv.lines[0].total_ttc.0);
            prop_assert_eq!(tax, inv.lines[0].tax_amount.0);
            prop_assert_eq!(
                create(&shop, goods(&inv, 0, 1_000, PaymentMethod::Credit)).unwrap_err().code(),
                "credit_exceeds_invoice"
            );
        }

        /// Whatever the financial credit note and however the ten bags then come back, every
        /// bag stays creditable and the 19 % rate ends credited exactly, TTC and TVA: never
        /// above what the invoice held at that rate.
        #[test]
        fn after_a_financial_credit_note_every_bag_still_comes_back(
            amount in 1_i64..=2_280_000,
            parts in proptest::collection::vec(1_i64..=4, 1..8),
        ) {
            let shop = shop();
            let inv = invoice(&shop);
            create(&shop, gesture(&inv, amount)).unwrap();
            let mut left = 10_000_i64;
            for part in parts {
                let qty = (part * 1_000).min(left);
                if qty == 0 { break; }
                create(&shop, goods(&inv, 0, qty, PaymentMethod::Credit)).unwrap();
                left -= qty;
            }
            if left > 0 {
                create(&shop, goods(&inv, 0, left, PaymentMethod::Credit)).unwrap();
            }
            prop_assert_eq!(
                credited_at_19(&shop),
                (inv.lines[0].total_ttc.0, inv.lines[0].tax_amount.0)
            );
            prop_assert_eq!(
                create(&shop, goods(&inv, 0, 1_000, PaymentMethod::Credit)).unwrap_err().code(),
                "credit_exceeds_invoice"
            );
        }
    }
}
