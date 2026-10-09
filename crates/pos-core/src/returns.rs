//! Returns and exchanges (spec §7.2; RET-001/002/003/004/005/006/007/010/011): a bon de retour
//! with negative lines, in its own gap-free series per terminal, linked to the original ticket;
//! the goods go back to the shop or to the damaged-goods location; the refund leaves the drawer
//! (or goes back to the card). An exchange is a return and a sale in one transaction, the
//! returned value paying the new ticket as store credit (no stamp duty on it).

use crate::audit;
use crate::auth::Permission;
use crate::cash::{self, MovementKind};
use crate::catalog::products::{self, Product, ProductKind};
use crate::catalog::{UNIT_PIECE_ID, clean_opt, tax_rates, units};
use crate::company;
use crate::documents::{
    self, DocStatus, DocType, Document, DocumentLine, DraftDocument, DraftLine, DraftPayment,
};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentMethod;
use crate::sales::SaleExtras;
use crate::sales::extensions::ReturnExtras;
use crate::sales::sale::{self, SaleInput, StoreCredit, ValidatedSale};
use crate::sqlenum::sql_text_enum;
use crate::stock::{self, MovementReason, NewMovement};
use crate::tax::timbre::StampMode;
use crate::tax::tva::{self, Discount, LineAmounts, LineInput};
use crate::till::TillContext;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeSet;

/// Mandatory reason of a returned line (RET-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReturnReason {
    Defective,
    WrongItem,
    ChangedMind,
    Damaged,
    Expired,
    Other,
}

impl ReturnReason {
    pub const ALL: [ReturnReason; 6] = [
        ReturnReason::Defective,
        ReturnReason::WrongItem,
        ReturnReason::ChangedMind,
        ReturnReason::Damaged,
        ReturnReason::Expired,
        ReturnReason::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ReturnReason::Defective => "defective",
            ReturnReason::WrongItem => "wrong_item",
            ReturnReason::ChangedMind => "changed_mind",
            ReturnReason::Damaged => "damaged",
            ReturnReason::Expired => "expired",
            ReturnReason::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<ReturnReason> {
        ReturnReason::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

sql_text_enum!(ReturnReason);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnLineInput {
    /// The sold line (with a receipt).
    pub origin_line_id: Option<String>,
    /// The product (without a receipt).
    pub product_id: Option<String>,
    /// Quantity brought back, positive.
    pub qty: Qty,
    /// Refund unit price without a receipt; the current price when absent.
    pub unit_price: Option<Money>,
    /// Back to sellable stock, or to the damaged-goods location (RET-006). Ignored for an open
    /// item: it has no stock.
    pub restock: bool,
    pub reason: ReturnReason,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnInput {
    /// The original ticket; `None` for a return without receipt (manager, RET-003).
    pub original_id: Option<String>,
    pub lines: Vec<ReturnLineInput>,
    /// Cash, card CIB, Edahabia, BaridiMob or transfer; or the customer's account: credit
    /// (kridi reduced) or voucher (store credit).
    pub refund_method: PaymentMethod,
    pub refund_reference: Option<String>,
    pub lang: Lang,
    pub note: Option<String>,
}

/// A sold line with what is left to return (RET-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnableLine {
    pub line: DocumentLine,
    pub returned: Qty,
    pub remaining: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedReturn {
    pub document: Document,
    /// Paid back to the customer with the refund method.
    pub refund: Money,
    /// Kept as store credit for the new ticket of an exchange.
    pub store_credit: Money,
    /// The exchange voucher a `voucher` refund without customer became (Task 13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub exchange_voucher: Option<crate::gift_vouchers::Voucher>,
}

impl ValidatedReturn {
    /// The cash this return paid out of the drawer that holds it, when refunded in `method`
    /// (a host service's answer: the till records it, [`cash::record_share`]).
    pub fn drawer_share(
        &self,
        method: PaymentMethod,
    ) -> Result<Option<cash::DrawerShare>, AppError> {
        if method != PaymentMethod::Cash || !self.refund.is_positive() {
            return Ok(None);
        }
        Ok(Some(cash::DrawerShare {
            kind: MovementKind::Refund,
            amount: neg(self.refund)?,
            ref_type: "document".into(),
            ref_id: self.document.id.clone(),
            note: None,
            category: None,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Exchange {
    pub returned: ValidatedReturn,
    pub sale: ValidatedSale,
    /// New ticket's amount due minus the returned value: what the customer paid (positive) or
    /// got back (negative).
    pub net_to_pay: Money,
}

/// What earlier validated returns took from a sold line: quantity, TTC, tax, discount.
struct Returned {
    qty: Qty,
    ttc: Money,
    tax: Money,
    discount: Money,
}

fn already_returned(conn: &Connection, origin_line_id: &str) -> Result<Returned, AppError> {
    let (qty, ttc, tax, discount): (Qty, Money, Money, Money) = conn
        .prepare_cached(
            "SELECT COALESCE(SUM(-l.qty), 0), COALESCE(SUM(-l.total_ttc), 0), COALESCE(SUM(-l.tax_amount), 0),
                    COALESCE(SUM(-l.discount_amount), 0)
             FROM document_lines l JOIN documents d ON d.id = l.document_id
             WHERE l.origin_line_id = ?1 AND d.status = 'validated'",
        )?
        .query_row([origin_line_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(Returned {
        qty,
        ttc,
        tax,
        discount,
    })
}

fn original_ticket(conn: &Connection, id: &str) -> Result<Document, AppError> {
    let doc =
        documents::get(conn, id).map_err(|_| AppError::conflict("return_original_invalid"))?;
    if doc.status != DocStatus::Validated || doc.doc_type != DocType::Ticket {
        return Err(AppError::conflict("return_original_invalid"));
    }
    // An invoiced ticket is corrected by a credit note on its invoice (M3), never twice.
    if documents::links::invoiced(conn, id)?.is_some() {
        return Err(AppError::conflict("ticket_invoiced"));
    }
    Ok(doc)
}

/// The lines of a ticket (catalogue products and open items, not kit components) with what
/// was already returned (RET-001/010).
pub fn returnable(conn: &Connection, original_id: &str) -> Result<Vec<ReturnableLine>, AppError> {
    let doc = original_ticket(conn, original_id)?;
    doc.lines
        .into_iter()
        .filter(|l| l.parent_line_id.is_none())
        .map(|line| {
            let returned = already_returned(conn, &line.id)?.qty;
            let remaining = line.qty.checked_sub(returned).unwrap_or(Qty::ZERO);
            Ok(ReturnableLine {
                line,
                returned,
                remaining,
            })
        })
        .collect()
}

/// Returns goods and refunds them (`sales.return` in the command). Without a receipt the
/// operator also needs `sales.price_override` or a manager's approval of it.
pub fn create_return(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ReturnInput,
) -> Result<ValidatedReturn, AppError> {
    create_return_for(tx, ctx, input, None)
}

/// A return whose refund may go to a customer's account (`credit`: kridi reduced, `voucher`:
/// store credit), RET-007. `customer_id` defaults to the original ticket's customer.
pub fn create_return_for(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ReturnInput,
    customer_id: Option<&str>,
) -> Result<ValidatedReturn, AppError> {
    create_return_inner(
        tx,
        ctx,
        input,
        Money::ZERO,
        customer_id,
        &ReturnExtras::default(),
    )
}

/// A return with the packs' extras (serials coming back).
pub fn create_return_with(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ReturnInput,
    customer_id: Option<&str>,
    extras: &ReturnExtras,
) -> Result<ValidatedReturn, AppError> {
    create_return_inner(tx, ctx, input, Money::ZERO, customer_id, extras)
}

fn neg(m: Money) -> Result<Money, AppError> {
    m.checked_neg()
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))
}

/// A component line of a composite being returned, and the stock it restocks.
struct Restock {
    product_id: String,
    location_id: String,
    qty: Qty,
    cost: Money,
}

const ZERO: LineAmounts = LineAmounts {
    gross: Money::ZERO,
    discount_bp: crate::money::Rate::ZERO,
    discount_amount: Money::ZERO,
    total_ht: Money::ZERO,
    tax: Money::ZERO,
    total_ttc: Money::ZERO,
};

/// The lines and restocks of a return being built.
struct ReturnBuild {
    lang: Lang,
    lines: Vec<DraftLine>,
    restocks: Vec<Restock>,
}

/// A returned quantity of a product, where it goes, and the return line it belongs to.
struct Bring<'a> {
    product: &'a Product,
    qty: Qty,
    location_id: &'a str,
    line_id: &'a str,
}

impl ReturnBuild {
    /// Puts `bring.qty` back in stock. A composite puts back its components as child lines,
    /// in proportion to the component lines of the ticket when there is one (`sold`: those
    /// lines and the quantity sold), else from its current recipe.
    fn restock(
        &mut self,
        conn: &Connection,
        bring: Bring<'_>,
        sold: Option<(&[&DocumentLine], Qty)>,
    ) -> Result<(), AppError> {
        let product = bring.product;
        if product.kind != ProductKind::Composite {
            if product.track_stock {
                self.restocks.push(Restock {
                    product_id: product.id.clone(),
                    location_id: bring.location_id.to_owned(),
                    qty: bring.qty,
                    cost: product.cost_price,
                });
            }
            return Ok(());
        }
        let parts: Vec<(Product, Qty, Option<String>)> = match sold {
            Some((children, sold_qty)) if !children.is_empty() => children
                .iter()
                .map(|child| {
                    let component =
                        products::get(conn, child.product_id.as_deref().unwrap_or_default())?;
                    let share = i128::from(child.qty.0) * i128::from(bring.qty.0)
                        / i128::from(sold_qty.0.max(1));
                    Ok((
                        component,
                        Qty(i64::try_from(share).unwrap_or(0)),
                        Some(child.id.clone()),
                    ))
                })
                .collect::<Result<_, AppError>>()?,
            _ => stock::expand(conn, product, bring.qty)?
                .into_iter()
                .map(|(p, q)| (p, q, None))
                .collect(),
        };
        for (component, part_qty, origin) in parts {
            self.lines.push(DraftLine {
                id: new_id(),
                product_id: Some(component.id.clone()),
                description: component.name_in(self.lang).to_owned(),
                qty: part_qty.checked_neg().unwrap_or(Qty::ZERO),
                unit_code: component.unit_code.clone(),
                unit_price: Money::ZERO,
                price_overridden: false,
                tax_rate_bp: component.tax_rate_bp,
                amounts: ZERO,
                cost_price: component.cost_price,
                parent_line_id: Some(bring.line_id.to_owned()),
                origin_line_id: origin,
                restock_location_id: Some(bring.location_id.to_owned()),
                reason_code: None,
                note: None,
            });
            if component.track_stock && part_qty.is_positive() {
                self.restocks.push(Restock {
                    product_id: component.id,
                    location_id: bring.location_id.to_owned(),
                    qty: part_qty,
                    cost: component.cost_price,
                });
            }
        }
        Ok(())
    }
}

/// Where `qty` of `product_id` brought back on a note of the ticket `original_id` goes (STK-004,
/// PRD-053): into the lots the ticket took it from, the soonest expiry first, never more than the
/// ticket took from a lot less what its earlier return notes put back; the rest (what the ticket
/// took without a lot, or goods returned without a receipt) without a lot. FEFO and the expiry
/// reports see the returned boxes in their lot again.
fn back_to_lots(
    tx: &Transaction<'_>,
    original_id: Option<&str>,
    product_id: &str,
    qty: Qty,
) -> Result<Vec<(Option<String>, Qty)>, AppError> {
    let mut parts = Vec::new();
    let mut rest = qty;
    if let Some(original) = original_id {
        let open: Vec<(String, Qty)> = tx
            .prepare_cached(
                "SELECT m.lot_id, SUM(-m.qty_delta)
                 FROM stock_movements m JOIN lots l ON l.id = m.lot_id
                 WHERE m.product_id = ?2 AND m.ref_type = 'document'
                   AND ((m.reason = 'sale' AND m.ref_id = ?1)
                        OR (m.reason = 'return' AND m.ref_id IN (
                            SELECT id FROM documents
                            WHERE parent_id = ?1 AND doc_type = 'return_note' AND status = 'validated')))
                 GROUP BY m.lot_id
                 HAVING SUM(-m.qty_delta) > 0
                 ORDER BY l.expiry_date IS NULL, l.expiry_date, l.created_at",
            )?
            .query_map(params![original, product_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (lot_id, left) in open {
            if !rest.is_positive() {
                break;
            }
            let back = rest.min(left);
            parts.push((Some(lot_id), back));
            rest = rest.checked_sub(back).unwrap_or(Qty::ZERO);
        }
    }
    if rest.is_positive() {
        parts.push((None, rest));
    }
    Ok(parts)
}

fn create_return_inner(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: ReturnInput,
    credit_cap: Money,
    customer_id: Option<&str>,
    extras: &ReturnExtras,
) -> Result<ValidatedReturn, AppError> {
    // This PC's drawer, or the calling till's for a host service.
    let session = cash::drawer(tx, ctx)?;
    if input.lines.is_empty() {
        return Err(AppError::validation("lines", "cart_empty"));
    }
    match input.refund_method {
        PaymentMethod::Cash
        | PaymentMethod::CardCib
        | PaymentMethod::Edahabia
        | PaymentMethod::Baridimob
        | PaymentMethod::Transfer
        // Accepted, checked against the account below.
        | PaymentMethod::Credit
        | PaymentMethod::Voucher => {}
        PaymentMethod::Cheque | PaymentMethod::Change => {
            return Err(AppError::validation(
                "refundMethod",
                "payment_method_invalid",
            ));
        }
    }
    let refund_reference = clean_opt(input.refund_reference.as_deref(), "refundReference", 48)?;
    if input.refund_method == PaymentMethod::Transfer && refund_reference.is_none() {
        return Err(AppError::validation(
            "refundReference",
            "transfer_reference_required",
        ));
    }
    let company = company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let original = match &input.original_id {
        Some(id) => Some(original_ticket(tx, id)?),
        None if ctx.allows(Permission::SalesPriceOverride) => None,
        None => return Err(AppError::conflict("return_without_receipt_not_allowed")),
    };
    let account_customer: Option<String> = customer_id
        .map(str::to_owned)
        .or_else(|| original.as_ref().and_then(|o| o.customer_id.clone()));
    // A customer named at the counter is sealed into the note whatever the refund method.
    if let Some(id) = customer_id {
        crate::customers::selectable(tx, id)?;
    }
    // A voucher refund with a customer stays M2's store credit; without one it is an exchange
    // voucher when asked (Task 13), and needs the gift vouchers module.
    let exchange_voucher = extras.exchange_voucher
        && input.refund_method == PaymentMethod::Voucher
        && account_customer.is_none();
    if exchange_voucher {
        crate::packs::require(tx, crate::packs::Module::GiftVouchers)?;
    }
    let to_account = matches!(
        input.refund_method,
        PaymentMethod::Credit | PaymentMethod::Voucher
    ) && !exchange_voucher;
    if to_account && account_customer.is_none() {
        return Err(AppError::validation("customerId", "customer_required"));
    }
    let shop_location = stock::default_location(tx)?.id;
    let damaged_location = stock::damaged_location(tx)?.id;
    let mut build = ReturnBuild {
        lang: input.lang,
        lines: Vec::new(),
        restocks: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for (i, l) in input.lines.iter().enumerate() {
        let f = |name: &str| format!("lines.{i}.{name}");
        if !l.qty.is_positive() {
            return Err(AppError::validation(&f("qty"), "qty_invalid"));
        }
        let note = clean_opt(l.note.as_deref(), &f("note"), 200)?;
        let location_id = if l.restock {
            shop_location.clone()
        } else {
            damaged_location.clone()
        };
        let line_id = new_id();
        match &original {
            Some(orig) => {
                let origin_id = l
                    .origin_line_id
                    .as_deref()
                    .ok_or_else(|| AppError::validation(&f("originLineId"), "required"))?;
                let sold = orig
                    .lines
                    .iter()
                    .find(|x| x.id == origin_id && x.parent_line_id.is_none())
                    .ok_or_else(|| {
                        AppError::validation(&f("originLineId"), "return_line_invalid")
                    })?;
                if !seen.insert(origin_id.to_owned()) {
                    return Err(AppError::validation(&f("originLineId"), "value_invalid"));
                }
                // A catalogue product goes back to stock; an open item (SAL-018, no product)
                // is only refunded, and counts in pieces.
                let product = match sold.product_id.as_deref() {
                    Some(id) => Some(products::get(tx, id)?),
                    None => None,
                };
                let decimals = match &product {
                    Some(p) => p.unit_decimals,
                    None => units::get(tx, UNIT_PIECE_ID)?.decimals,
                };
                let before = already_returned(tx, &sold.id)?;
                let remaining = sold.qty.checked_sub(before.qty).unwrap_or(Qty::ZERO);
                // The whole rest of a line may always go back: an open item may have been sold
                // in a fraction of a piece.
                if !l.qty.fits_decimals(decimals) && l.qty != remaining {
                    return Err(AppError::validation(&f("qty"), "qty_invalid"));
                }
                if l.qty > remaining {
                    return Err(AppError::validation(&f("qty"), "return_qty_exceeds_sold"));
                }
                let bad = || AppError::validation(&f("qty"), "amount_invalid");
                let (ttc, tax, discount) = if l.qty == remaining {
                    (
                        sold.total_ttc.checked_sub(before.ttc).ok_or_else(bad)?,
                        sold.tax_amount.checked_sub(before.tax).ok_or_else(bad)?,
                        sold.discount_amount
                            .checked_sub(before.discount)
                            .ok_or_else(bad)?,
                    )
                } else {
                    (
                        sold.total_ttc
                            .mul_div(l.qty.0, sold.qty.0)
                            .ok_or_else(bad)?,
                        sold.tax_amount
                            .mul_div(l.qty.0, sold.qty.0)
                            .ok_or_else(bad)?,
                        sold.discount_amount
                            .mul_div(l.qty.0, sold.qty.0)
                            .ok_or_else(bad)?,
                    )
                };
                let ht = ttc.checked_sub(tax).ok_or_else(bad)?;
                let amounts = LineAmounts {
                    gross: neg(ttc.checked_add(discount).ok_or_else(bad)?)?,
                    discount_bp: sold.discount_bp,
                    discount_amount: neg(discount)?,
                    total_ht: neg(ht)?,
                    tax: neg(tax)?,
                    total_ttc: neg(ttc)?,
                };
                build.lines.push(DraftLine {
                    id: line_id.clone(),
                    product_id: sold.product_id.clone(),
                    description: sold.description.clone(),
                    qty: l.qty.checked_neg().ok_or_else(bad)?,
                    unit_code: sold.unit_code.clone(),
                    unit_price: sold.unit_price,
                    price_overridden: sold.price_overridden,
                    tax_rate_bp: sold.tax_rate_bp,
                    amounts,
                    cost_price: sold.cost_price,
                    parent_line_id: None,
                    origin_line_id: Some(sold.id.clone()),
                    restock_location_id: product.as_ref().map(|_| location_id.clone()),
                    reason_code: Some(l.reason.as_str().to_owned()),
                    note,
                });
                if let Some(product) = &product {
                    let children: Vec<&DocumentLine> = orig
                        .lines
                        .iter()
                        .filter(|c| c.parent_line_id.as_deref() == Some(sold.id.as_str()))
                        .collect();
                    let bring = Bring {
                        product,
                        qty: l.qty,
                        location_id: &location_id,
                        line_id: &line_id,
                    };
                    build.restock(tx, bring, Some((&children, sold.qty)))?;
                }
            }
            None => {
                let product_id = l
                    .product_id
                    .as_deref()
                    .ok_or_else(|| AppError::validation(&f("productId"), "required"))?;
                let product = products::get(tx, product_id)
                    .map_err(|_| AppError::validation(&f("productId"), "product_not_found"))?;
                if !l.qty.fits_decimals(product.unit_decimals) {
                    return Err(AppError::validation(&f("qty"), "qty_invalid"));
                }
                let rate = tax_rates::get(tx, &product.tax_rate_id)?.rate_bp;
                let unit_price = l.unit_price.unwrap_or(product.sell_price);
                let computed = tva::compute_line(
                    company.price_mode,
                    &LineInput {
                        unit_price,
                        qty: l.qty,
                        discount: Discount::None,
                        rate,
                    },
                )
                .map_err(|e| crate::sales::cart::at_line(i, e))?;
                build.lines.push(DraftLine {
                    id: line_id.clone(),
                    product_id: Some(product.id.clone()),
                    description: product.name_in(input.lang).to_owned(),
                    qty: l
                        .qty
                        .checked_neg()
                        .ok_or_else(|| AppError::validation(&f("qty"), "qty_invalid"))?,
                    unit_code: product.unit_code.clone(),
                    unit_price,
                    price_overridden: unit_price != product.sell_price,
                    tax_rate_bp: rate,
                    amounts: computed.negated()?,
                    cost_price: product.cost_price,
                    parent_line_id: None,
                    origin_line_id: None,
                    restock_location_id: Some(location_id.clone()),
                    reason_code: Some(l.reason.as_str().to_owned()),
                    note,
                });
                let bring = Bring {
                    product: &product,
                    qty: l.qty,
                    location_id: &location_id,
                    line_id: &line_id,
                };
                build.restock(tx, bring, None)?;
            }
        }
    }
    let total = Money::checked_sum(build.lines.iter().map(|l| l.amounts.total_ttc))
        .ok_or_else(|| AppError::validation("lines", "amount_invalid"))?;
    let returned_value = neg(total)?;
    let store_credit = returned_value.min(credit_cap).max(Money::ZERO);
    let refund = returned_value
        .checked_sub(store_credit)
        .unwrap_or(Money::ZERO);
    let mut payments = Vec::new();
    if store_credit.is_positive() {
        payments.push(DraftPayment {
            method: PaymentMethod::Voucher,
            amount: neg(store_credit)?,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        });
    }
    if refund.is_positive() {
        payments.push(DraftPayment {
            method: input.refund_method,
            amount: neg(refund)?,
            reference: refund_reference.clone(),
            cheque_bank: None,
            cheque_date: None,
        });
    }
    let draft = DraftDocument {
        doc_type: DocType::ReturnNote,
        terminal_code: ctx.session.terminal_code.clone(),
        business_date: ctx.business_date(),
        customer_id: account_customer.clone(),
        supplier_id: None,
        user_id: ctx.session.user_id.clone(),
        terminal_id: ctx.session.terminal_id.clone(),
        cash_session_id: Some(session.id.clone()),
        price_level_id: None,
        lang: input.lang,
        // The lines copy the ticket's amounts, so the note keeps the ticket's regime and mode
        // (the company may have changed since); only a return without receipt takes today's.
        tax_regime: original
            .as_ref()
            .map_or(company.tax_regime, |o| o.tax_regime),
        price_mode: original
            .as_ref()
            .map_or(company.price_mode, |o| o.price_mode),
        stamp_mode: StampMode::Off,
        stamp_duty: Money::ZERO,
        stamp_charged: Money::ZERO,
        rounding: Money::ZERO,
        paid_total: total,
        change_given: Money::ZERO,
        // What the note takes off the kridi, as a ticket records what it put on it: the sales
        // reports net "on credit" like their other columns. Store credit is not kridi.
        credit_amount: if input.refund_method == PaymentMethod::Credit {
            neg(refund)?
        } else {
            Money::ZERO
        },
        note: clean_opt(input.note.as_deref(), "note", 200)?,
        parent_id: original.as_ref().map(|o| o.id.clone()),
        source_ref: None,
        lines: build.lines,
        payments,
    };
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    let document = documents::validate(tx, &id, ctx.now)?;
    let actor = ctx.actor();
    let original_id = original.as_ref().map(|o| o.id.as_str());
    for r in &build.restocks {
        for (lot_id, qty) in back_to_lots(tx, original_id, &r.product_id, r.qty)? {
            let movement = NewMovement {
                product_id: r.product_id.clone(),
                location_id: r.location_id.clone(),
                qty_delta: qty,
                reason: MovementReason::Return,
                reason_code: None,
                ref_type: Some("document".into()),
                ref_id: Some(document.id.clone()),
                unit_cost: Some(r.cost),
                note: None,
            };
            stock::append_with_lot(tx, &actor, &movement, lot_id.as_deref(), ctx.now)?;
        }
    }
    if input.refund_method == PaymentMethod::Cash && refund.is_positive() {
        let share = cash::DrawerShare {
            kind: MovementKind::Refund,
            amount: neg(refund)?,
            ref_type: "document".into(),
            ref_id: document.id.clone(),
            note: None,
            category: None,
        };
        cash::book(tx, ctx, &session.id, &share)?;
    }
    if let (true, true, Some(customer)) = (to_account, refund.is_positive(), &account_customer) {
        crate::customers::credit::book_refund(
            tx,
            ctx,
            customer,
            refund,
            input.refund_method,
            &document.id,
        )?;
    }
    let detail: Vec<_> = input
        .lines
        .iter()
        .map(|l| json!({ "originLineId": l.origin_line_id, "productId": l.product_id, "qty": l.qty, "restock": l.restock, "reason": l.reason }))
        .collect();
    let mut validated = ValidatedReturn {
        document,
        refund,
        store_credit,
        exchange_voucher: None,
    };
    // The packs' part (serials coming back, top-ups refused, points taken back, the exchange
    // voucher), before the return's journal row.
    let outcome = crate::sales::extensions::after_return(tx, ctx, &input, extras, &validated)?;
    validated.exchange_voucher = outcome.exchange_voucher;
    let document = &validated.document;
    audit::append(
        tx,
        &actor.event(
            "return.created",
            "document",
            Some(&document.id),
            json!({
                "number": document.number,
                "originalNumber": original.as_ref().and_then(|o| o.number.clone()),
                "withoutReceipt": original.is_none(),
                "refund": refund,
                "refundMethod": input.refund_method,
                "storeCredit": store_credit,
                "customerId": account_customer,
                "exchangeVoucher": validated.exchange_voucher.as_ref().map(|v| v.code.as_str()),
                "lines": detail,
                "approvals": ctx.approvers(),
            }),
        ),
    )?;
    Ok(validated)
}

/// Return and new sale in one transaction (RET-004): the returned value pays the new ticket
/// first (store credit, exempt from the duty); any rest is refunded with the return's refund
/// method, or paid by the sale's tenders.
pub fn exchange(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ret: ReturnInput,
    sale_input: SaleInput,
) -> Result<Exchange, AppError> {
    exchange_with(
        tx,
        ctx,
        ret,
        sale_input,
        &ReturnExtras::default(),
        &SaleExtras::default(),
    )
}

/// An exchange with the packs' extras on both sides: the serials coming back on the return, and
/// the new ticket's serials and register entries (a defective phone swapped for another).
/// Whether a return writes the main PC's ledgers, so that a till makes it on the main
/// PC (a host service, `host_required` offline): it brings back a serial-tracked piece sold by
/// its serial, takes back loyalty points its ticket earned, or becomes an exchange voucher.
pub fn needs_host(
    conn: &Connection,
    input: &ReturnInput,
    extras: &ReturnExtras,
) -> Result<bool, AppError> {
    if !extras.serials.is_empty()
        || (extras.exchange_voucher && input.refund_method == PaymentMethod::Voucher)
    {
        return Ok(true);
    }
    for line in &input.lines {
        let Some(origin) = &line.origin_line_id else {
            continue;
        };
        let sold: bool = conn
            .prepare_cached(
                "SELECT EXISTS (SELECT 1 FROM serials WHERE sale_line_id = ?1 AND status = 'sold')",
            )?
            .query_row([origin], |r| r.get(0))?;
        if sold {
            return Ok(true);
        }
    }
    match &input.original_id {
        Some(original) => crate::loyalty::earned_on(conn, original),
        None => Ok(false),
    }
}

pub fn exchange_with(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ret: ReturnInput,
    sale_input: SaleInput,
    return_extras: &ReturnExtras,
    sale_extras: &SaleExtras,
) -> Result<Exchange, AppError> {
    let priced = crate::sales::cart::price_cart(tx, ctx, &sale_input.cart)?;
    let returned = create_return_inner(tx, ctx, ret, priced.totals.total_ttc, None, return_extras)?;
    let credit = StoreCredit {
        amount: returned.store_credit,
        return_id: returned.document.id.clone(),
        return_number: returned.document.number.clone().unwrap_or_default(),
    };
    let sold = sale::create_sale_with_credit(tx, ctx, sale_input, Some(&credit), sale_extras)?;
    let returned_value = returned
        .refund
        .checked_add(returned.store_credit)
        .unwrap_or(Money::ZERO);
    let net_to_pay = sold
        .document
        .total_due
        .checked_sub(returned_value)
        .unwrap_or(Money::ZERO);
    Ok(Exchange {
        returned,
        sale: sold,
        net_to_pay,
    })
}

#[cfg(test)]
mod lot_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::auth::session::OverrideGrant;
    use crate::catalog::products::{Component, ProductInput};
    use crate::company::{CompanyInput, PriceMode, TaxRegime};
    use crate::payments::PaymentInput;
    use crate::sales::cart::{Cart, CartLine};
    use crate::sales::create_sale;
    use crate::stock::DAMAGED_LOCATION_ID;
    use crate::testkit::{NOON, Shop, shop};
    use std::sync::Arc;

    fn sell(shop: &Shop, lines: Vec<CartLine>, cash: i64) -> Document {
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, lines),
            payments: vec![PaymentInput::cash(Money(cash))],
        };
        shop.db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap()
            .document
    }

    fn back(origin: &DocumentLine, qty: i64, restock: bool) -> ReturnLineInput {
        ReturnLineInput {
            origin_line_id: Some(origin.id.clone()),
            product_id: None,
            qty: Qty(qty),
            unit_price: None,
            restock,
            reason: ReturnReason::ChangedMind,
            note: None,
        }
    }

    fn input(original: &Document, lines: Vec<ReturnLineInput>) -> ReturnInput {
        ReturnInput {
            original_id: Some(original.id.clone()),
            lines,
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            lang: Lang::Fr,
            note: None,
        }
    }

    fn do_return(
        shop: &Shop,
        ctx: TillContext,
        input: ReturnInput,
    ) -> Result<ValidatedReturn, AppError> {
        shop.db.write(move |tx| create_return(tx, &ctx, input))
    }

    fn expected_cash(shop: &Shop) -> Money {
        let terminal = shop.terminal.id.clone();
        shop.db
            .read(move |c| {
                let s = cash::require_open(c, &terminal)?;
                cash::expected_cash(c, &s.id)
            })
            .unwrap()
    }

    #[test]
    fn a_return_restocks_refunds_cash_and_links_the_ticket() {
        let shop = shop();
        let milk = shop.product("Lait", Money(45_000), &shop.tva19);
        shop.stock_in(&milk.id, 5_000);
        shop.open_till(0);
        let ticket = sell(&shop, vec![CartLine::product(&milk.id, Qty(2_000))], 91_000);
        let r = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]),
        )
        .unwrap();
        let doc = &r.document;
        assert_eq!(
            (
                doc.number.as_deref(),
                doc.doc_type,
                doc.parent_id.as_deref()
            ),
            (
                Some("BR2026-C1-000001"),
                DocType::ReturnNote,
                Some(ticket.id.as_str())
            )
        );
        assert_eq!(
            (doc.total_ttc, doc.tax_total, doc.total_due, r.refund),
            (Money(-45_000), Money(-7_185), Money(-45_000), Money(45_000))
        );
        assert_eq!(
            (doc.lines[0].qty, doc.lines[0].reason_code.as_deref()),
            (Qty(-1_000), Some("changed_mind"))
        );
        assert_eq!(
            (doc.payments[0].method, doc.payments[0].amount),
            (PaymentMethod::Cash, Money(-45_000))
        );
        let id = milk.id.clone();
        assert_eq!(
            shop.db.read(move |c| stock::on_hand(c, &id)).unwrap(),
            Qty(4_000)
        );
        // 909.00 DA kept for the sale (duty included), 450.00 DA refunded.
        assert_eq!(expected_cash(&shop), Money(45_900));
        let t = ticket.id.clone();
        let left = shop.db.read(move |c| returnable(c, &t)).unwrap();
        assert_eq!(
            (left[0].returned, left[0].remaining),
            (Qty(1_000), Qty(1_000))
        );
        let chain = shop
            .db
            .read(|c| documents::verify_chain(c, DocType::ReturnNote, "C1"))
            .unwrap();
        assert_eq!(chain.first_broken_seq, None);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'return.created'"),
            1
        );
    }

    #[test]
    fn partial_returns_never_exceed_the_sale_and_end_on_the_exact_total() {
        let shop = shop();
        let jam = shop.product("Confiture", Money(1_000), &shop.tva19);
        shop.open_till(0);
        let mut line = CartLine::product(&jam.id, Qty(3_000));
        line.discount = crate::tax::tva::Discount::Amount(Money(100));
        let owner = TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
        let input_sale = SaleInput {
            cart: Cart::new(Lang::Fr, vec![line]),
            payments: vec![PaymentInput::cash(Money(2_900))],
        };
        let ticket = shop
            .db
            .write(move |tx| create_sale(tx, &owner, input_sale))
            .unwrap()
            .document;
        assert_eq!(ticket.total_ttc, Money(2_900));
        let refunds: Vec<Money> = (0..3)
            .map(|_| {
                do_return(
                    &shop,
                    shop.till(),
                    input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]),
                )
                .unwrap()
                .refund
            })
            .collect();
        assert_eq!(refunds, [Money(967), Money(967), Money(966)]);
        let err = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]),
        )
        .unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            ("return_qty_exceeds_sold".into(), Some("lines.0.qty".into()))
        );
        let returned_tax: i64 =
            shop.count("SELECT -SUM(tax_amount) FROM document_lines WHERE qty < 0");
        assert_eq!(
            returned_tax, ticket.tax_total.0,
            "the tax is refunded exactly once"
        );
    }

    #[test]
    fn damaged_goods_go_to_their_own_location_and_card_refunds_leave_the_drawer_alone() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 5_000);
        shop.open_till(0);
        let ticket = sell(&shop, vec![CartLine::product(&milk.id, Qty(1_000))], 10_000);
        let mut i = input(&ticket, vec![back(&ticket.lines[0], 1_000, false)]);
        i.refund_method = PaymentMethod::CardCib;
        i.refund_reference = Some("RB-1".into());
        do_return(&shop, shop.till(), i).unwrap();
        let id = milk.id.clone();
        assert_eq!(
            shop.db.read(move |c| stock::on_hand(c, &id)).unwrap(),
            Qty(4_000)
        );
        let id = milk.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| stock::level(c, &id, DAMAGED_LOCATION_ID))
                .unwrap(),
            Qty(1_000)
        );
        assert_eq!(
            expected_cash(&shop),
            Money(10_000),
            "the card refund does not touch the drawer"
        );
    }

    #[test]
    fn a_return_without_receipt_needs_a_manager() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.open_till(0);
        let line = ReturnLineInput {
            origin_line_id: None,
            product_id: Some(milk.id.clone()),
            qty: Qty(1_000),
            unit_price: None,
            restock: true,
            reason: ReturnReason::Defective,
            note: None,
        };
        let no_receipt = ReturnInput {
            original_id: None,
            lines: vec![line],
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            lang: Lang::Fr,
            note: Some("Client habituel".into()),
        };
        let err = do_return(&shop, shop.till(), no_receipt.clone()).unwrap_err();
        assert_eq!(err.code(), "return_without_receipt_not_allowed");
        let grant = OverrideGrant {
            approver_id: shop.owner.user_id.clone(),
            approver_name: "Amine".into(),
            permission: Permission::SalesPriceOverride,
            granted_at: NOON,
            discount_limit_bp: 10_000,
        };
        let r = do_return(&shop, shop.till().with_approval(grant), no_receipt).unwrap();
        assert_eq!(
            (r.refund, r.document.parent_id.clone()),
            (Money(10_000), None)
        );
        let payload: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'return.created'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert!(
            payload.contains("\"withoutReceipt\":true") && payload.contains("sales.price_override"),
            "{payload}"
        );
    }

    #[test]
    fn only_validated_tickets_and_their_lines_can_be_returned() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.open_till(0);
        let ticket = sell(&shop, vec![CartLine::product(&milk.id, Qty(1_000))], 10_000);
        let r = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]),
        )
        .unwrap();
        let err = do_return(
            &shop,
            shop.till(),
            input(&r.document, vec![back(&r.document.lines[0], 1_000, true)]),
        )
        .unwrap_err();
        assert_eq!(err.code(), "return_original_invalid");
        let mut wrong = back(&ticket.lines[0], 1_000, true);
        wrong.origin_line_id = Some("not-a-line".into());
        let err = do_return(&shop, shop.till(), input(&ticket, vec![wrong])).unwrap_err();
        assert_eq!(err.code(), "return_line_invalid");
        let mut i = input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]);
        i.refund_method = PaymentMethod::Voucher;
        assert_eq!(
            do_return(&shop, shop.till(), i).unwrap_err().code(),
            "customer_required"
        );
    }

    #[test]
    fn a_returned_kit_restocks_its_components() {
        let shop = shop();
        let pen = shop.product("Stylo", Money(5_000), &shop.tva19);
        let mut kit = ProductInput::simple("Kit", UNIT_PIECE_ID, &shop.tva19, Money(9_000));
        kit.kind = ProductKind::Composite;
        kit.components = vec![Component {
            component_id: pen.id.clone(),
            qty: Qty(2_000),
        }];
        let kit = shop.product_with(kit);
        shop.stock_in(&pen.id, 10_000);
        shop.open_till(0);
        let ticket = sell(&shop, vec![CartLine::product(&kit.id, Qty(2_000))], 18_000);
        let id = pen.id.clone();
        assert_eq!(
            shop.db.read(move |c| stock::on_hand(c, &id)).unwrap(),
            Qty(6_000)
        );
        let r = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]),
        )
        .unwrap();
        assert_eq!(r.document.lines.len(), 2);
        assert_eq!(r.document.lines[1].qty, Qty(-2_000));
        let id = pen.id.clone();
        assert_eq!(
            shop.db.read(move |c| stock::on_hand(c, &id)).unwrap(),
            Qty(8_000)
        );
    }

    #[test]
    fn an_exchange_pays_the_new_ticket_with_the_returned_value() {
        let shop = shop();
        let small = shop.product("T-shirt S", Money(200_000), &shop.tva19);
        let large = shop.product("T-shirt XL", Money(250_000), &shop.tva19);
        shop.open_till(0);
        let ticket = sell(
            &shop,
            vec![CartLine::product(&small.id, Qty(1_000))],
            202_000,
        );
        let drawer_after_sale = expected_cash(&shop);
        let ret = input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]);
        let new_sale = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&large.id, Qty(1_000))]),
            payments: vec![PaymentInput::cash(Money(50_500))],
        };
        let ctx = shop.till();
        let ex = shop
            .db
            .write(move |tx| exchange(tx, &ctx, ret, new_sale))
            .unwrap();
        assert_eq!(
            (ex.returned.store_credit, ex.returned.refund),
            (Money(200_000), Money::ZERO)
        );
        // Only the 500 DA paid in cash carries the duty: 5 DA.
        assert_eq!(
            (
                ex.sale.document.stamp_duty,
                ex.sale.document.total_due,
                ex.net_to_pay
            ),
            (Money(500), Money(250_500), Money(50_500))
        );
        assert_eq!(
            ex.sale.document.parent_id.as_deref(),
            Some(ex.returned.document.id.as_str())
        );
        let voucher = &ex.sale.document.payments[0];
        assert_eq!(
            (voucher.method, voucher.amount, voucher.reference.as_deref()),
            (
                PaymentMethod::Voucher,
                Money(200_000),
                Some("BR2026-C1-000001")
            )
        );
        assert_eq!(
            expected_cash(&shop),
            drawer_after_sale.checked_add(Money(50_500)).unwrap()
        );
        // Cheaper new item: the rest comes back in cash.
        let ticket = ex.sale.document;
        let ret = input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]);
        let cheaper = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&small.id, Qty(1_000))]),
            payments: vec![],
        };
        let ctx = shop.till();
        let ex = shop
            .db
            .write(move |tx| exchange(tx, &ctx, ret, cheaper))
            .unwrap();
        assert_eq!(
            (ex.returned.store_credit, ex.returned.refund, ex.net_to_pay),
            (Money(200_000), Money(50_000), Money(-50_000))
        );
        assert_eq!(ex.sale.document.stamp_duty, Money::ZERO);
    }

    #[test]
    fn a_return_keeps_the_regime_and_price_mode_of_its_ticket() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.open_till(100_000);
        let ticket = sell(&shop, vec![CartLine::product(&milk.id, Qty(2_000))], 20_000);
        assert_eq!(
            (ticket.tax_regime, ticket.price_mode),
            (TaxRegime::Reel, PriceMode::Ttc)
        );
        // The shop moves to the IFU after the sale (its price mode is fixed once prices are in
        // use: `price_mode_locked`).
        let pack = Arc::clone(&shop.pack);
        shop.db
            .write(move |tx| {
                let mut c = CompanyInput::named(Lang::Fr, "Supérette El Amel");
                c.tax_regime = TaxRegime::Ifu;
                company::save(tx, &Actor::system(None), &pack, c)
            })
            .unwrap();
        let r = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]),
        )
        .unwrap();
        assert_eq!(
            (r.document.tax_regime, r.document.price_mode),
            (TaxRegime::Reel, PriceMode::Ttc),
            "the return note copies the ticket's lines, so it keeps the ticket's regime and mode"
        );
        // Without a receipt there is no ticket: the shop's current settings apply.
        let owner = TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
        let no_receipt = ReturnInput {
            original_id: None,
            lines: vec![ReturnLineInput {
                origin_line_id: None,
                product_id: Some(milk.id.clone()),
                qty: Qty(1_000),
                unit_price: None,
                restock: true,
                reason: ReturnReason::Defective,
                note: None,
            }],
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            lang: Lang::Fr,
            note: None,
        };
        let r = do_return(&shop, owner, no_receipt).unwrap();
        assert_eq!(
            (r.document.tax_regime, r.document.price_mode),
            (TaxRegime::Ifu, PriceMode::Ttc)
        );
    }

    #[test]
    fn an_open_item_is_refunded_without_any_stock_movement() {
        let shop = shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 5_000);
        shop.open_till(0);
        // Selling an open item needs sales.price_override: the owner sells it.
        let owner = TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
        let sale = SaleInput {
            cart: Cart::new(
                Lang::Fr,
                vec![
                    CartLine::product(&milk.id, Qty(1_000)),
                    CartLine::open_item("Article divers", Qty(1_500), Money(2_000), &shop.tva19),
                ],
            ),
            payments: vec![PaymentInput::cash(Money(13_000))],
        };
        let ticket = shop
            .db
            .write(move |tx| create_sale(tx, &owner, sale))
            .unwrap()
            .document;
        let open = ticket
            .lines
            .iter()
            .find(|l| l.product_id.is_none())
            .unwrap()
            .clone();
        assert_eq!(open.total_ttc, Money(3_000));
        let t = ticket.id.clone();
        let lines = shop.db.read(move |c| returnable(c, &t)).unwrap();
        assert_eq!(lines.len(), 2, "the open item is returnable too");
        // An open item counts in pieces: half a piece cannot go back on its own...
        let err = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&open, 500, true)]),
        )
        .unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            ("qty_invalid".into(), Some("lines.0.qty".into()))
        );
        let first = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&open, 1_000, true)]),
        )
        .unwrap();
        assert_eq!(first.refund, Money(2_000), "pro rata of the sold line");
        let line = &first.document.lines[0];
        assert_eq!(
            (
                line.product_id.as_deref(),
                line.origin_line_id.as_deref(),
                line.qty,
                line.restock_location_id.as_deref(),
                line.description.as_str()
            ),
            (
                None,
                Some(open.id.as_str()),
                Qty(-1_000),
                None,
                "Article divers"
            )
        );
        // ...but the rest of the line always can, and takes the exact rest of the amounts.
        let rest = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&open, 500, true)]),
        )
        .unwrap();
        assert_eq!(rest.refund, Money(1_000));
        assert_eq!(
            first
                .document
                .tax_total
                .checked_add(rest.document.tax_total)
                .unwrap(),
            open.tax_amount.checked_neg().unwrap()
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE reason = 'return'"),
            0,
            "an open item has no stock"
        );
        let id = milk.id.clone();
        assert_eq!(
            shop.db.read(move |c| stock::on_hand(c, &id)).unwrap(),
            Qty(4_000)
        );
        let t = ticket.id.clone();
        let left = shop.db.read(move |c| returnable(c, &t)).unwrap();
        let remaining: Vec<_> = left
            .iter()
            .map(|l| (l.line.product_id.is_some(), l.remaining))
            .collect();
        assert_eq!(remaining, [(true, Qty(1_000)), (false, Qty::ZERO)]);
    }

    #[test]
    fn an_invoiced_ticket_is_corrected_on_its_invoice_never_returned() {
        use crate::billing::convert::{TicketInvoiceInput, invoice_ticket};
        let shop = shop();
        let (amine, cement, _) = crate::billing::drafts::tests::fixture(&shop);
        shop.stock_in(&cement.id, 10_000);
        shop.open_till(0);
        let ticket = sell(
            &shop,
            vec![CartLine::product(&cement.id, Qty(1_000))],
            121_000,
        );
        let (ctx, ticket_id, customer_id) = (shop.owner_till(), ticket.id.clone(), amine.id);
        shop.db
            .write(move |tx| {
                invoice_ticket(
                    tx,
                    &ctx,
                    TicketInvoiceInput {
                        ticket_id,
                        customer_id,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap();
        let id = ticket.id.clone();
        let err = shop.db.read(move |c| returnable(c, &id)).unwrap_err();
        assert_eq!(err.code(), "ticket_invoiced", "nothing is returnable");
        let err = do_return(
            &shop,
            shop.till(),
            input(&ticket, vec![back(&ticket.lines[0], 1_000, true)]),
        )
        .unwrap_err();
        assert_eq!(err.code(), "ticket_invoiced");
    }
}
