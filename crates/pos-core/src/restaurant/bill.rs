//! The table's bill (RST-005, RST-007, RST-008, RST-009): voids and comps with a reason, the
//! addition ("ce document n'est pas un ticket de caisse") with the cash amount and its droit de
//! timbre, splits by line or by seat (lines move, never copy), even shares for guests paying
//! apart, and the payment: the ticket becomes one validated M1a ticket through `create_sale`
//! (its receipt, stock, drawer, timbre and hash chain are the till's), the kitchen gets what it
//! had not received yet, and the table ticket is closed with a link to the receipt.

use super::kitchen::{self, KitchenTicket};
use super::tickets::{self, CompReason, DiningMode, Ticket, VoidReason};
use crate::audit;
use crate::auth::Permission;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty, Rate};
use crate::payments::{self, PaymentInput};
use crate::sales::cart::{self, Cart, CartLine, PaymentPreview, PricedCart};
use crate::sales::sale::create_sale_granted;
use crate::sales::{SaleExtras, SaleInput, ValidatedSale};
use crate::tax::tva::Discount;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoidOutcome {
    pub ticket: Ticket,
    /// Cancellation tickets to print (what the kitchen had already received).
    pub cancels: Vec<KitchenTicket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreBill {
    pub ticket: Ticket,
    pub priced: PricedCart,
    /// The droit de timbre if the whole bill is paid in cash.
    pub stamp_duty: Money,
    /// What the table pays in cash: TTC + charged duty.
    pub cash_due: Money,
    /// Even shares of the TTC when the guests pay apart (empty for one payer).
    pub shares: Vec<Money>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum SplitBy {
    #[serde(rename_all = "camelCase")]
    Lines {
        line_ids: Vec<String>,
    },
    Seat {
        seat: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SplitDone {
    pub from: Ticket,
    pub to: Ticket,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PayInput {
    pub ticket_id: String,
    pub payments: Vec<PaymentInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaidTicket {
    pub ticket: Ticket,
    pub sale: ValidatedSale,
    /// Kitchen tickets sent at the payment (to print).
    pub kitchen: Vec<KitchenTicket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiningInfo {
    pub mode: DiningMode,
    pub order_no: Option<i64>,
    pub table_code: Option<String>,
    pub label: Option<String>,
    pub covers: u8,
}

fn line_of<'a>(ticket: &'a Ticket, line_id: &str) -> Result<&'a tickets::TicketLine, AppError> {
    ticket
        .lines
        .iter()
        .find(|l| l.id == line_id)
        .ok_or_else(|| AppError::not_found("dining_line"))
}

fn ticket_of_line(conn: &Connection, line_id: &str) -> Result<Ticket, AppError> {
    let ticket_id: String = conn
        .query_row(
            "SELECT ticket_id FROM dining_lines WHERE id = ?1",
            [line_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("dining_line"))?;
    tickets::get(conn, &ticket_id)
}

/// The value of `qty` of a line (and its priced options) as the till prices it (AUD-001).
fn value_of(
    conn: &Connection,
    ctx: &TillContext,
    ticket: &Ticket,
    line_id: &str,
    qty: Qty,
) -> Result<Money, AppError> {
    let mut lines: Vec<CartLine> = Vec::new();
    for l in ticket
        .lines
        .iter()
        .filter(|l| l.id == line_id || l.parent_line_id.as_deref() == Some(line_id))
    {
        let mut c = l.cart_line.clone();
        c.qty = qty;
        lines.push(c);
    }
    let mut cart = tickets::cart_of(ticket);
    cart.lines = lines;
    Ok(cart::value_cart(conn, ctx, &cart)?.totals.total_ttc)
}

/// The cart the table pays: its lines, then the charge per cover on a dine-in ticket when
/// `restaurant.cover_product_id` names a product. The addition and the payment use the same
/// cart, so the addition shows what the table will pay.
fn bill_cart(conn: &Connection, ticket: &Ticket) -> Result<Cart, AppError> {
    let mut cart = tickets::cart_of(ticket);
    let cover: Option<String> = crate::settings::get(conn, "restaurant.cover_product_id")?;
    if let (Some(product_id), DiningMode::DineIn, true) = (cover, ticket.mode, ticket.covers > 0) {
        cart.lines.push(CartLine::product(
            &product_id,
            Qty(i64::from(ticket.covers) * 1_000),
        ));
    }
    Ok(cart)
}

/// Voids `qty` of a line (all of it with `None`) and its priced options, with a reason; what the
/// kitchen had received gets a cancellation ticket. Needs `sales.void_line` (or its approval).
pub fn void_line(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    line_id: &str,
    qty: Option<Qty>,
    reason: VoidReason,
) -> Result<VoidOutcome, AppError> {
    if !ctx.allows(Permission::SalesVoidLine) {
        return Err(AppError::Permission {
            permission: Permission::SalesVoidLine.as_str().to_owned(),
        });
    }
    let ticket = ticket_of_line(tx, line_id)?;
    tickets::require_live(&ticket)?;
    let line = line_of(&ticket, line_id)?.clone();
    let voided = qty.unwrap_or(line.qty);
    if !voided.is_positive() || voided > line.qty {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    let value = value_of(tx, ctx, &ticket, line_id, voided)?;
    let remaining = line.qty.checked_sub(voided).unwrap_or(Qty::ZERO);
    let still_sent = line.sent_qty.min(remaining);
    let cancel_qty = line.sent_qty.checked_sub(still_sent).unwrap_or(Qty::ZERO);
    let reason_s = reason.as_str();
    if remaining.is_positive() {
        // The priced options keep their own product in their JSON line: only the quantity moves.
        let mut kids = tx.prepare(
            "SELECT id, cart_line FROM dining_lines WHERE parent_line_id = ?1 AND status = 'active'",
        )?;
        let rows = kids
            .query_map([line_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (kid, json_line) in rows {
            let mut k: CartLine = serde_json::from_str(&json_line)?;
            k.qty = remaining;
            tx.execute(
                "UPDATE dining_lines SET cart_line = ?2, qty = ?3, sent_qty = min(sent_qty, ?3), updated_at = ?4 WHERE id = ?1",
                params![kid, serde_json::to_string(&k)?, remaining, ctx.now],
            )?;
        }
        let mut c = line.cart_line.clone();
        c.qty = remaining;
        tx.execute(
            "UPDATE dining_lines SET cart_line = ?2, qty = ?3, sent_qty = ?4, updated_at = ?5 WHERE id = ?1",
            params![line_id, serde_json::to_string(&c)?, remaining, still_sent, ctx.now],
        )?;
    } else {
        tx.execute(
            "UPDATE dining_lines SET status = 'voided', void_reason = ?2, updated_at = ?3
             WHERE (id = ?1 OR parent_line_id = ?1) AND status = 'active'",
            params![line_id, reason_s, ctx.now],
        )?;
    }
    let cancels = if cancel_qty.is_positive() {
        vec![kitchen::cancel(tx, ctx, &ticket, &line, cancel_qty)?]
    } else {
        Vec::new()
    };
    tickets::reopen_if_billed(tx, &ticket, ctx.now)?;
    let total = tickets::refresh_total(tx, ctx, &ticket.id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.line_voided",
            "dining_ticket",
            Some(&ticket.id),
            json!({ "lineId": line_id, "qty": voided, "value": value, "reason": reason_s, "cancelled": cancel_qty, "totalTtc": total, "approvals": ctx.approvers() }),
        ),
    )?;
    Ok(VoidOutcome {
        ticket: tickets::get(tx, &ticket.id)?,
        cancels,
    })
}

/// Voids a whole live ticket (the customer left): every line voided with `reason`, one
/// cancellation per line the kitchen received.
pub fn void_ticket(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
    reason: VoidReason,
) -> Result<VoidOutcome, AppError> {
    if !ctx.allows(Permission::SalesVoidLine) {
        return Err(AppError::Permission {
            permission: Permission::SalesVoidLine.as_str().to_owned(),
        });
    }
    let ticket = tickets::get(tx, ticket_id)?;
    tickets::require_live(&ticket)?;
    let mut cancels = Vec::new();
    for line in ticket
        .lines
        .iter()
        .filter(|l| l.parent_line_id.is_none() && l.sent_qty.is_positive())
    {
        cancels.push(kitchen::cancel(tx, ctx, &ticket, line, line.sent_qty)?);
    }
    tx.execute(
        "UPDATE dining_lines SET status = 'voided', void_reason = ?2, updated_at = ?3 WHERE ticket_id = ?1 AND status = 'active'",
        params![ticket_id, reason.as_str(), ctx.now],
    )?;
    tx.execute(
        "UPDATE dining_tickets SET status = 'void', total_ttc = 0, closed_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![ticket_id, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.ticket_voided",
            "dining_ticket",
            Some(ticket_id),
            json!({ "reason": reason.as_str(), "value": ticket.total_ttc, "lines": ticket.lines.len(), "approvals": ctx.approvers() }),
        ),
    )?;
    Ok(VoidOutcome {
        ticket: tickets::get(tx, ticket_id)?,
        cancels,
    })
}

/// Offers a line (100 % discount) with a reason; the discount right and limit apply as at the
/// till (a cashier needs a manager). Once given, the comp is not asked again of whoever works on
/// the table later (`tickets::granted_of`); the paid ticket keeps its reason.
pub fn comp_line(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    line_id: &str,
    reason: CompReason,
) -> Result<Ticket, AppError> {
    let ticket = ticket_of_line(tx, line_id)?;
    tickets::require_live(&ticket)?;
    let line = line_of(&ticket, line_id)?;
    let value = value_of(tx, ctx, &ticket, line_id, line.qty)?;
    let mut c = line.cart_line.clone();
    c.discount = Discount::Percent(Rate::FULL);
    tickets::check_line_rights(tx, ctx, &ticket, &c)?;
    tx.execute(
        "UPDATE dining_lines SET cart_line = ?2, comp_reason = ?3, updated_at = ?4 WHERE id = ?1",
        params![
            line_id,
            serde_json::to_string(&c)?,
            reason.as_str(),
            ctx.now
        ],
    )?;
    tickets::reopen_if_billed(tx, &ticket, ctx.now)?;
    let total = tickets::refresh_total(tx, ctx, &ticket.id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.line_comped",
            "dining_ticket",
            Some(&ticket.id),
            json!({ "lineId": line_id, "value": value, "reason": reason.as_str(), "totalTtc": total, "approvals": ctx.approvers() }),
        ),
    )?;
    tickets::get(tx, &ticket.id)
}

/// `total` in `parts` shares, the first ones floored to the centime and the last one taking the
/// rest (every share within one centime of the others per part).
pub fn even_shares(total: Money, parts: u8) -> Result<Vec<Money>, AppError> {
    if !(2..=20).contains(&parts) || total.is_negative() {
        return Err(AppError::validation("parts", "value_invalid"));
    }
    let n = i64::from(parts);
    let base = total.0 / n;
    let mut shares = vec![Money(base); usize::from(parts)];
    let rest = total.0 - base * n;
    if let Some(last) = shares.last_mut() {
        *last = Money(base + rest);
    }
    Ok(shares)
}

/// The addition (RST-007): the ticket priced as the till would, its cash amount with the droit
/// de timbre, optional even shares; the ticket becomes `billed` (a new line reopens it).
pub fn prebill(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
    parts: Option<u8>,
) -> Result<PreBill, AppError> {
    let ticket = tickets::get(tx, ticket_id)?;
    tickets::require_live(&ticket)?;
    if ticket.lines.is_empty() {
        return Err(AppError::validation("lines", "cart_empty"));
    }
    let priced = tickets::price(tx, ctx, &ticket, &bill_cart(tx, &ticket)?)?;
    let total = priced.totals.total_ttc;
    let settled = payments::preview(
        &ctx.pack,
        total,
        &[PaymentInput::cash(total)],
        cart::settle_rules(tx)?,
    )?;
    let shares = match parts {
        Some(p) if p >= 2 => even_shares(total, p)?,
        _ => Vec::new(),
    };
    tx.execute(
        "UPDATE dining_tickets SET status = 'billed', billed_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![ticket_id, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.bill_printed",
            "dining_ticket",
            Some(ticket_id),
            json!({ "totalTtc": total, "parts": parts }),
        ),
    )?;
    Ok(PreBill {
        ticket: tickets::get(tx, ticket_id)?,
        priced,
        stamp_duty: settled.stamp_duty,
        cash_due: settled.total_due,
        shares,
    })
}

/// The payment dialog's live figures (SAL-048, SAL-057) for a table ticket: the bill the payment
/// will take (its cover charge, its offered lines without asking their discount again) and the
/// settlement of the tenders being typed. Writes nothing.
pub fn preview(
    conn: &Connection,
    ctx: &TillContext,
    ticket_id: &str,
    tenders: &[PaymentInput],
) -> Result<PaymentPreview, AppError> {
    let ticket = tickets::get(conn, ticket_id)?;
    tickets::require_live(&ticket)?;
    if ticket.lines.is_empty() {
        return Err(AppError::validation("lines", "cart_empty"));
    }
    let priced = tickets::price(conn, ctx, &ticket, &bill_cart(conn, &ticket)?)?;
    let settlement = payments::preview(
        &ctx.pack,
        priced.totals.total_ttc,
        tenders,
        cart::settle_rules(conn)?,
    )?;
    Ok(PaymentPreview {
        cart: priced,
        settlement,
    })
}

/// Moves the chosen lines (with their options) or one seat's lines to a new ticket at the same
/// place (RST-005). Refused when nothing moves or nothing would stay.
pub fn split(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
    by: SplitBy,
) -> Result<SplitDone, AppError> {
    let ticket = tickets::get(tx, ticket_id)?;
    tickets::require_live(&ticket)?;
    if ticket.mode != DiningMode::DineIn {
        return Err(AppError::validation("by", "split_invalid"));
    }
    let mains: Vec<&tickets::TicketLine> = ticket
        .lines
        .iter()
        .filter(|l| l.parent_line_id.is_none())
        .collect();
    let moving: Vec<String> = match &by {
        SplitBy::Lines { line_ids } => mains
            .iter()
            .filter(|l| line_ids.contains(&l.id))
            .map(|l| l.id.clone())
            .collect(),
        SplitBy::Seat { seat } => mains
            .iter()
            .filter(|l| l.seat == Some(*seat))
            .map(|l| l.id.clone())
            .collect(),
    };
    if moving.is_empty() || moving.len() == mains.len() {
        return Err(AppError::validation("by", "split_invalid"));
    }
    let id = new_id();
    // A seat's guest takes one of the table's covers with them (never a new one): the covers of
    // both tickets add up to the table's, so the cover charge and RPT-065 count each guest once.
    let covers: u8 = match by {
        SplitBy::Seat { .. } if ticket.covers >= 2 => 1,
        _ => 0,
    };
    tx.execute(
        "INSERT INTO dining_tickets (id, mode, business_date, table_id, label, covers, waiter_id, customer_id,
                                     price_level_id, lang, status, parent_id, opened_at, terminal_id, user_id,
                                     created_at, updated_at)
         SELECT ?2, mode, business_date, table_id, label, ?3, waiter_id, NULL, price_level_id, lang, 'open', id, ?4,
                terminal_id, ?5, ?4, ?4 FROM dining_tickets WHERE id = ?1",
        params![ticket_id, id, covers, ctx.now, ctx.session.user_id],
    )?;
    let mut position = 0_i64;
    for main in &moving {
        position += 1;
        tx.execute(
            "UPDATE dining_lines SET ticket_id = ?2, position = ?3, updated_at = ?4 WHERE id = ?1",
            params![main, id, position, ctx.now],
        )?;
        let mut kids = tx.prepare("SELECT id FROM dining_lines WHERE parent_line_id = ?1")?;
        let rows = kids
            .query_map([main], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for kid in rows {
            position += 1;
            tx.execute(
                "UPDATE dining_lines SET ticket_id = ?2, position = ?3, updated_at = ?4 WHERE id = ?1",
                params![kid, id, position, ctx.now],
            )?;
        }
    }
    tx.execute(
        "UPDATE dining_tickets SET status = 'open', billed_at = NULL WHERE id = ?1",
        [ticket_id],
    )?;
    if covers > 0 {
        let moved = tx.execute(
            "UPDATE dining_tickets SET covers = covers - ?3, updated_at = ?2 WHERE id = ?1 AND covers > ?3",
            params![ticket_id, ctx.now, covers],
        )?;
        if moved != 1 {
            return Err(AppError::validation("by", "split_invalid"));
        }
    }
    tickets::refresh_total(tx, ctx, ticket_id)?;
    tickets::refresh_total(tx, ctx, &id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.ticket_split",
            "dining_ticket",
            Some(ticket_id),
            json!({ "to": id, "lines": moving, "covers": covers }),
        ),
    )?;
    Ok(SplitDone {
        from: tickets::get(tx, ticket_id)?,
        to: tickets::get(tx, &id)?,
    })
}

/// Pays the ticket (RST-005 even shares are several tenders of this one payment): unsent lines go
/// to the kitchen first when `restaurant.send_on_pay`, a per-cover charge is added when set, and
/// the ticket becomes one validated M1a ticket. A ticket is paid once.
pub fn pay(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: PayInput,
) -> Result<PaidTicket, AppError> {
    pay_with(tx, ctx, input, &SaleExtras::default())
}

/// [`pay`] with the till's pack extras of the sale (register entries, serials, buyer, payment
/// types): the table's sale refuses or records them exactly like a till sale.
pub fn pay_with(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: PayInput,
    extras: &SaleExtras,
) -> Result<PaidTicket, AppError> {
    let ticket = tickets::get(tx, &input.ticket_id)?;
    tickets::require_live(&ticket)?;
    if ticket.lines.is_empty() {
        return Err(AppError::validation("lines", "cart_empty"));
    }
    let unsent = ticket
        .lines
        .iter()
        .any(|l| l.parent_line_id.is_none() && l.sent_qty < l.qty);
    let kitchen = if unsent && crate::settings::get::<bool>(tx, "restaurant.send_on_pay")? {
        kitchen::send(tx, ctx, &ticket.id)?
    } else {
        Vec::new()
    };
    let ticket = tickets::get(tx, &input.ticket_id)?;
    let cart = bill_cart(tx, &ticket)?;
    let sale = create_sale_granted(
        tx,
        ctx,
        SaleInput {
            cart,
            payments: input.payments,
        },
        &tickets::granted_of(&ticket),
        extras,
    )?;
    let changed = tx.execute(
        "UPDATE dining_tickets SET status = 'paid', document_id = ?2, closed_at = ?3, updated_at = ?3
         WHERE id = ?1 AND status IN ('open', 'billed')",
        params![ticket.id, sale.document.id, ctx.now],
    )?;
    if changed != 1 {
        return Err(AppError::conflict("ticket_not_open"));
    }
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.ticket_paid",
            "dining_ticket",
            Some(&ticket.id),
            json!({ "documentId": sale.document.id, "number": sale.document.number, "totalDue": sale.document.total_due, "covers": ticket.covers }),
        ),
    )?;
    Ok(PaidTicket {
        ticket: tickets::get(tx, &ticket.id)?,
        sale,
        kitchen,
    })
}

/// The dining block of a receipt (RST-009): the takeaway number or the table, and the covers.
pub fn dining_of_document(
    conn: &Connection,
    document_id: &str,
) -> Result<Option<DiningInfo>, AppError> {
    Ok(conn
        .query_row(
            "SELECT t.mode, t.order_no, d.code, t.label, t.covers FROM dining_tickets t
             LEFT JOIN dining_tables d ON d.id = t.table_id WHERE t.document_id = ?1",
            [document_id],
            |r| {
                Ok(DiningInfo {
                    mode: r.get(0)?,
                    order_no: r.get(1)?,
                    table_code: r.get(2)?,
                    label: r.get(3)?,
                    covers: r.get(4)?,
                })
            },
        )
        .optional()?)
}
