//! Open tickets (RST-003, RST-004, RST-009, SAL-044): a ticket belongs to a table (sur place), to
//! a predefined label ("Comptoir") or to nobody (à emporter, livraison, with a daily order number
//! per terminal); its lines are M1a cart lines kept in `dining_lines`, priced by the till's own
//! pipeline at every change (the cached `total_ttc` is what the board shows). Lines are never
//! deleted: a line removed before it reached the kitchen is voided with reason `mistake`.

use super::modifiers;
use crate::audit;
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::packs::{self, Module};
use crate::sales::cart::{self, Cart, CartLine, PricedCart};
use crate::sqlenum::sql_text_enum;
use crate::tax::tva::Discount;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DiningMode {
    DineIn,
    Takeaway,
    Delivery,
}

impl DiningMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            DiningMode::DineIn => "dine_in",
            DiningMode::Takeaway => "takeaway",
            DiningMode::Delivery => "delivery",
        }
    }

    pub fn parse(s: &str) -> Option<DiningMode> {
        [
            DiningMode::DineIn,
            DiningMode::Takeaway,
            DiningMode::Delivery,
        ]
        .into_iter()
        .find(|m| m.as_str() == s)
    }
}

sql_text_enum!(DiningMode);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Open,
    Billed,
    Paid,
    Void,
}

impl TicketStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            TicketStatus::Open => "open",
            TicketStatus::Billed => "billed",
            TicketStatus::Paid => "paid",
            TicketStatus::Void => "void",
        }
    }

    pub fn parse(s: &str) -> Option<TicketStatus> {
        [
            TicketStatus::Open,
            TicketStatus::Billed,
            TicketStatus::Paid,
            TicketStatus::Void,
        ]
        .into_iter()
        .find(|m| m.as_str() == s)
    }
}

sql_text_enum!(TicketStatus);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LineStatus {
    Active,
    Voided,
}

impl LineStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            LineStatus::Active => "active",
            LineStatus::Voided => "voided",
        }
    }

    pub fn parse(s: &str) -> Option<LineStatus> {
        [LineStatus::Active, LineStatus::Voided]
            .into_iter()
            .find(|m| m.as_str() == s)
    }
}

sql_text_enum!(LineStatus);

/// Why a line was voided (RST-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VoidReason {
    Mistake,
    CustomerChanged,
    Quality,
    Other,
}

impl VoidReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            VoidReason::Mistake => "mistake",
            VoidReason::CustomerChanged => "customer_changed",
            VoidReason::Quality => "quality",
            VoidReason::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<VoidReason> {
        [
            VoidReason::Mistake,
            VoidReason::CustomerChanged,
            VoidReason::Quality,
            VoidReason::Other,
        ]
        .into_iter()
        .find(|m| m.as_str() == s)
    }
}

sql_text_enum!(VoidReason);

/// Why a line was offered (RST-008 comp/gift).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CompReason {
    Offered,
    Staff,
    Quality,
    Other,
}

impl CompReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            CompReason::Offered => "offered",
            CompReason::Staff => "staff",
            CompReason::Quality => "quality",
            CompReason::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<CompReason> {
        [
            CompReason::Offered,
            CompReason::Staff,
            CompReason::Quality,
            CompReason::Other,
        ]
        .into_iter()
        .find(|m| m.as_str() == s)
    }
}

sql_text_enum!(CompReason);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TicketLine {
    pub id: String,
    pub position: i64,
    pub product_id: Option<String>,
    /// The product's name in the ticket's language, or the open item's description.
    pub name: String,
    pub cart_line: CartLine,
    pub qty: Qty,
    pub sent_qty: Qty,
    pub seat: Option<u8>,
    pub course: Option<u8>,
    pub parent_line_id: Option<String>,
    pub modifier_option_id: Option<String>,
    pub status: LineStatus,
    pub void_reason: Option<VoidReason>,
    pub comp_reason: Option<CompReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub id: String,
    pub mode: DiningMode,
    pub order_no: Option<i64>,
    pub business_date: String,
    pub table_id: Option<String>,
    pub table_code: Option<String>,
    pub label: Option<String>,
    pub covers: u8,
    pub waiter_id: String,
    pub waiter_name: String,
    pub customer_id: Option<String>,
    pub delivery_address: Option<String>,
    pub price_level_id: Option<String>,
    pub lang: Lang,
    pub note: Option<String>,
    pub status: TicketStatus,
    pub total_ttc: Money,
    pub parent_id: Option<String>,
    pub document_id: Option<String>,
    pub opened_at: i64,
    pub billed_at: Option<i64>,
    pub closed_at: Option<i64>,
    /// Active lines only, in position order.
    pub lines: Vec<TicketLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenInput {
    pub mode: DiningMode,
    pub table_id: Option<String>,
    pub label: Option<String>,
    pub covers: u8,
    pub customer_id: Option<String>,
    pub delivery_address: Option<String>,
    pub lang: Lang,
    pub price_level_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AddLineInput {
    pub cart_line: CartLine,
    pub seat: Option<u8>,
    pub course: Option<u8>,
    /// Chosen modifier option ids (the product's groups decide what is valid).
    pub modifiers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TicketDetails {
    pub covers: u8,
    pub note: Option<String>,
    pub customer_id: Option<String>,
    pub delivery_address: Option<String>,
    pub label: Option<String>,
}

/// Where an unsent line goes and what the kitchen reads on it (RST-002): its seat (`None` =
/// shared), its course and its note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LineEdit {
    pub seat: Option<u8>,
    pub course: Option<u8>,
    pub note: Option<String>,
}

const TICKET_SELECT: &str = "SELECT t.id, t.mode, t.order_no, t.business_date, t.table_id, d.code, t.label, t.covers,
        t.waiter_id, u.display_name, t.customer_id, t.delivery_address, t.price_level_id, t.lang, t.note,
        t.status, t.total_ttc, t.parent_id, t.document_id, t.opened_at, t.billed_at, t.closed_at
     FROM dining_tickets t JOIN users u ON u.id = t.waiter_id
     LEFT JOIN dining_tables d ON d.id = t.table_id";

fn map_ticket(r: &Row<'_>) -> rusqlite::Result<Ticket> {
    Ok(Ticket {
        id: r.get(0)?,
        mode: r.get(1)?,
        order_no: r.get(2)?,
        business_date: r.get(3)?,
        table_id: r.get(4)?,
        table_code: r.get(5)?,
        label: r.get(6)?,
        covers: r.get(7)?,
        waiter_id: r.get(8)?,
        waiter_name: r.get(9)?,
        customer_id: r.get(10)?,
        delivery_address: r.get(11)?,
        price_level_id: r.get(12)?,
        lang: r.get(13)?,
        note: r.get(14)?,
        status: r.get(15)?,
        total_ttc: r.get(16)?,
        parent_id: r.get(17)?,
        document_id: r.get(18)?,
        opened_at: r.get(19)?,
        billed_at: r.get(20)?,
        closed_at: r.get(21)?,
        lines: Vec::new(),
    })
}

fn lines_of(conn: &Connection, ticket: &Ticket) -> Result<Vec<TicketLine>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT l.id, l.position, l.product_id, l.cart_line, l.qty, l.sent_qty, l.seat, l.course,
                l.parent_line_id, l.modifier_option_id, l.status, l.void_reason, l.comp_reason
         FROM dining_lines l WHERE l.ticket_id = ?1 AND l.status = 'active' ORDER BY l.position",
    )?;
    let raw = stmt
        .query_map([&ticket.id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Qty>(4)?,
                r.get::<_, Qty>(5)?,
                r.get::<_, Option<u8>>(6)?,
                r.get::<_, Option<u8>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, LineStatus>(10)?,
                r.get::<_, Option<VoidReason>>(11)?,
                r.get::<_, Option<CompReason>>(12)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(raw.len());
    for (
        id,
        position,
        product_id,
        json_line,
        qty,
        sent_qty,
        seat,
        course,
        parent,
        option,
        status,
        void_reason,
        comp_reason,
    ) in raw
    {
        let cart_line: CartLine = serde_json::from_str(&json_line)?;
        let name = match &product_id {
            Some(pid) => crate::catalog::products::get(conn, pid)?
                .name_in(ticket.lang)
                .to_owned(),
            None => cart_line.description.clone().unwrap_or_default(),
        };
        out.push(TicketLine {
            id,
            position,
            product_id,
            name,
            cart_line,
            qty,
            sent_qty,
            seat,
            course,
            parent_line_id: parent,
            modifier_option_id: option,
            status,
            void_reason,
            comp_reason,
        });
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: &str) -> Result<Ticket, AppError> {
    let mut ticket = conn
        .prepare_cached(&format!("{TICKET_SELECT} WHERE t.id = ?1"))?
        .query_row([id], map_ticket)
        .optional()?
        .ok_or_else(|| AppError::not_found("dining_ticket"))?;
    ticket.lines = lines_of(conn, &ticket)?;
    Ok(ticket)
}

pub(crate) fn require_live(ticket: &Ticket) -> Result<(), AppError> {
    if matches!(ticket.status, TicketStatus::Open | TicketStatus::Billed) {
        Ok(())
    } else {
        Err(AppError::conflict("ticket_not_open"))
    }
}

/// The ticket as a till cart (its active lines in order, customer, level, language, note).
pub fn cart_of(ticket: &Ticket) -> Cart {
    let mut cart = Cart::new(
        ticket.lang,
        ticket.lines.iter().map(|l| l.cart_line.clone()).collect(),
    );
    cart.customer_id = ticket.customer_id.clone();
    cart.price_level_id = ticket.price_level_id.clone();
    cart.note = ticket.note.clone();
    cart
}

/// For each line of [`cart_of`], the reason code it keeps on the paid ticket when it was offered
/// (`comp_<reason>`, RST-008). Its 100 % discount was checked against the right and limit of
/// whoever gave it (`bill::comp_line`) and journaled with its approval, so repricing the ticket
/// does not ask it again of the waiter or cashier who works on the table later.
pub(crate) fn granted_of(ticket: &Ticket) -> Vec<Option<String>> {
    ticket
        .lines
        .iter()
        .map(|l| match l.comp_reason {
            Some(reason) if l.cart_line.discount == Discount::Percent(Rate::FULL) => {
                Some(format!("comp_{}", reason.as_str()))
            }
            _ => None,
        })
        .collect()
}

/// Prices `cart` (the ticket's lines first, as [`cart_of`] builds them, then any line the bill
/// adds) under the operator's rights, the discount of the offered lines excepted.
pub(crate) fn price(
    conn: &Connection,
    ctx: &TillContext,
    ticket: &Ticket,
    cart: &Cart,
) -> Result<PricedCart, AppError> {
    let granted: Vec<bool> = granted_of(ticket).iter().map(Option::is_some).collect();
    cart::price_cart_granted(conn, ctx, cart, &granted)
}

/// Checks `line` as the till would sell it under the operator's rights, its discount included
/// (offering a line, or offering more of it, is the discount right's to give).
pub(crate) fn check_line_rights(
    conn: &Connection,
    ctx: &TillContext,
    ticket: &Ticket,
    line: &CartLine,
) -> Result<(), AppError> {
    let mut alone = cart_of(ticket);
    alone.lines = vec![line.clone()];
    cart::price_cart(conn, ctx, &alone).map(drop)
}

/// Prices the ticket with the till's pipeline under the operator's rights (an override the
/// waiter may not make is refused here, as at the till; an offered line's discount was approved
/// when it was given) and caches the total.
pub(crate) fn refresh_total(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
) -> Result<Money, AppError> {
    let ticket = get(tx, ticket_id)?;
    let total = if ticket.lines.is_empty() {
        Money::ZERO
    } else {
        price(tx, ctx, &ticket, &cart_of(&ticket))?.totals.total_ttc
    };
    tx.execute(
        "UPDATE dining_tickets SET total_ttc = ?2, updated_at = ?3 WHERE id = ?1",
        params![ticket_id, total, ctx.now],
    )?;
    Ok(total)
}

fn clean_label(value: Option<&str>) -> Result<Option<String>, AppError> {
    crate::catalog::clean_opt(value, "label", 40)
}

pub fn open(tx: &Transaction<'_>, ctx: &TillContext, input: OpenInput) -> Result<Ticket, AppError> {
    packs::require(tx, Module::Restaurant)?;
    let label = clean_label(input.label.as_deref())?;
    let address =
        crate::catalog::clean_opt(input.delivery_address.as_deref(), "deliveryAddress", 200)?;
    if input.covers > 200 {
        return Err(AppError::validation("covers", "value_invalid"));
    }
    if let Some(table_id) = &input.table_id {
        let table = super::floor::table(tx, table_id)
            .map_err(|_| AppError::validation("tableId", "dining_table_not_found"))?;
        if table.archived || input.mode != DiningMode::DineIn {
            return Err(AppError::validation("tableId", "value_invalid"));
        }
    }
    if input.mode == DiningMode::DineIn && input.table_id.is_none() && label.is_none() {
        return Err(AppError::validation("tableId", "ticket_needs_place"));
    }
    if input.mode == DiningMode::Delivery && address.is_none() && input.customer_id.is_none() {
        return Err(AppError::validation("deliveryAddress", "required"));
    }
    if let Some(c) = &input.customer_id {
        crate::customers::selectable(tx, c)?;
    }
    let day = ctx.business_day();
    let terminal = ctx.session.terminal_id.clone();
    let order_no: Option<i64> = if input.mode == DiningMode::DineIn {
        None
    } else {
        Some(tx.query_row(
            "SELECT COALESCE(MAX(order_no), 0) + 1 FROM dining_tickets WHERE business_date = ?1 AND terminal_id = ?2",
            params![day, terminal],
            |r| r.get(0),
        )?)
    };
    let id = new_id();
    tx.execute(
        "INSERT INTO dining_tickets (id, mode, order_no, business_date, table_id, label, covers, waiter_id, customer_id,
                                     delivery_address, price_level_id, lang, status, opened_at, terminal_id, user_id,
                                     created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'open', ?13, ?14, ?8, ?13, ?13)",
        params![
            id, input.mode, order_no, day, input.table_id, label, input.covers, ctx.session.user_id,
            input.customer_id, address, input.price_level_id, input.lang, ctx.now, terminal
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.ticket_opened",
            "dining_ticket",
            Some(&id),
            json!({ "mode": input.mode, "tableId": input.table_id, "orderNo": order_no, "covers": input.covers }),
        ),
    )?;
    get(tx, &id)
}

fn next_position(tx: &Transaction<'_>, ticket_id: &str) -> Result<i64, AppError> {
    Ok(tx.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM dining_lines WHERE ticket_id = ?1",
        [ticket_id],
        |r| r.get(0),
    )?)
}

#[allow(clippy::too_many_arguments)]
fn insert_line(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
    line: &CartLine,
    seat: Option<u8>,
    course: Option<u8>,
    parent: Option<&str>,
    option: Option<&str>,
) -> Result<String, AppError> {
    let id = new_id();
    tx.execute(
        "INSERT INTO dining_lines (id, ticket_id, position, product_id, cart_line, qty, seat, course, parent_line_id,
                                   modifier_option_id, user_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        params![
            id, ticket_id, next_position(tx, ticket_id)?, line.product_id, serde_json::to_string(line)?,
            line.qty, seat, course, parent, option, ctx.session.user_id, ctx.now
        ],
    )?;
    Ok(id)
}

/// A billed ticket that changes goes back to open (its addition no longer tells the truth).
pub(crate) fn reopen_if_billed(
    tx: &Transaction<'_>,
    ticket: &Ticket,
    now: i64,
) -> Result<(), AppError> {
    if ticket.status == TicketStatus::Billed {
        tx.execute(
            "UPDATE dining_tickets SET status = 'open', billed_at = NULL, updated_at = ?2 WHERE id = ?1",
            params![ticket.id, now],
        )?;
    }
    Ok(())
}

pub fn add_line(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
    input: AddLineInput,
) -> Result<Ticket, AppError> {
    let ticket = get(tx, ticket_id)?;
    require_live(&ticket)?;
    if input.seat.is_some_and(|s| !(1..=99).contains(&s))
        || input.course.is_some_and(|c| !(1..=5).contains(&c))
    {
        return Err(AppError::validation("seat", "value_invalid"));
    }
    // What the line's own row would refuse (a quantity of zero, an unknown product) is said on
    // its field before the row is written.
    if !input.cart_line.qty.is_positive() {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    if let Some(pid) = &input.cart_line.product_id {
        crate::catalog::products::sellable(tx, pid)?;
    }
    let mut main = input.cart_line.clone();
    let applied = match &main.product_id {
        Some(pid) => modifiers::apply(tx, pid, ticket.lang, &input.modifiers)?,
        None if input.modifiers.is_empty() => modifiers::Applied::default(),
        None => return Err(AppError::validation("modifiers", "modifier_choice_invalid")),
    };
    if let Some(note) = &applied.note {
        main.note = Some(match main.note.take() {
            Some(own) if !own.trim().is_empty() => format!("{note}; {own}"),
            _ => note.clone(),
        });
    }
    reopen_if_billed(tx, &ticket, ctx.now)?;
    let main_id = insert_line(
        tx,
        ctx,
        ticket_id,
        &main,
        input.seat,
        input.course,
        None,
        None,
    )?;
    let mut priced_options = 0;
    for option in &applied.options {
        let Some(product_id) = &option.product_id else {
            continue;
        };
        let child = CartLine::product(product_id, main.qty);
        insert_line(
            tx,
            ctx,
            ticket_id,
            &child,
            input.seat,
            input.course,
            Some(&main_id),
            Some(&option.option_id),
        )?;
        priced_options += 1;
    }
    let total = refresh_total(tx, ctx, ticket_id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.line_added",
            "dining_ticket",
            Some(ticket_id),
            json!({ "lineId": main_id, "productId": main.product_id, "qty": main.qty, "options": applied.options.len(), "pricedOptions": priced_options, "totalTtc": total }),
        ),
    )?;
    get(tx, ticket_id)
}

fn line_row(tx: &Transaction<'_>, line_id: &str) -> Result<(String, Qty, Qty, CartLine), AppError> {
    let (ticket_id, qty, sent, json_line): (String, Qty, Qty, String) = tx
        .query_row(
            "SELECT ticket_id, qty, sent_qty, cart_line FROM dining_lines WHERE id = ?1 AND status = 'active'",
            [line_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("dining_line"))?;
    Ok((ticket_id, qty, sent, serde_json::from_str(&json_line)?))
}

/// Changes a line's quantity (and its priced options'); lowering it below what the kitchen
/// already received is a void (Task 5 `bill::void_line`), refused here with `line_already_sent`.
pub fn set_qty(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    line_id: &str,
    qty: Qty,
) -> Result<Ticket, AppError> {
    let (ticket_id, before, sent, mut line) = line_row(tx, line_id)?;
    let ticket = get(tx, &ticket_id)?;
    require_live(&ticket)?;
    if !qty.is_positive() {
        return Err(AppError::validation("qty", "qty_invalid"));
    }
    if qty < sent {
        return Err(AppError::conflict("line_already_sent"));
    }
    line.qty = qty;
    // An offered line's approval covered the quantity offered: offering more asks it again.
    let offered = ticket
        .lines
        .iter()
        .any(|l| l.id == line_id && l.comp_reason.is_some());
    if offered && qty > before {
        check_line_rights(tx, ctx, &ticket, &line)?;
    }
    reopen_if_billed(tx, &ticket, ctx.now)?;
    tx.execute(
        "UPDATE dining_lines SET cart_line = ?2, qty = ?3, updated_at = ?4 WHERE id = ?1",
        params![line_id, serde_json::to_string(&line)?, qty, ctx.now],
    )?;
    let mut children = tx.prepare(
        "SELECT id, cart_line FROM dining_lines WHERE parent_line_id = ?1 AND status = 'active'",
    )?;
    let kids = children
        .query_map([line_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (kid, json_line) in kids {
        let mut k: CartLine = serde_json::from_str(&json_line)?;
        k.qty = qty;
        tx.execute(
            "UPDATE dining_lines SET cart_line = ?2, qty = ?3, updated_at = ?4 WHERE id = ?1 AND sent_qty <= ?3",
            params![kid, serde_json::to_string(&k)?, qty, ctx.now],
        )?;
    }
    let total = refresh_total(tx, ctx, &ticket_id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.line_changed",
            "dining_ticket",
            Some(&ticket_id),
            json!({ "lineId": line_id, "before": before, "after": qty, "totalTtc": total }),
        ),
    )?;
    get(tx, &ticket_id)
}

/// Removes a line the kitchen never received (and its priced options): voided `mistake`.
pub fn remove_unsent(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    line_id: &str,
) -> Result<Ticket, AppError> {
    let (ticket_id, qty, sent, _) = line_row(tx, line_id)?;
    let ticket = get(tx, &ticket_id)?;
    require_live(&ticket)?;
    if sent.is_positive() {
        return Err(AppError::conflict("line_already_sent"));
    }
    reopen_if_billed(tx, &ticket, ctx.now)?;
    tx.execute(
        "UPDATE dining_lines SET status = 'voided', void_reason = 'mistake', updated_at = ?2
         WHERE (id = ?1 OR parent_line_id = ?1) AND status = 'active'",
        params![line_id, ctx.now],
    )?;
    let total = refresh_total(tx, ctx, &ticket_id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.line_removed",
            "dining_ticket",
            Some(&ticket_id),
            json!({ "lineId": line_id, "qty": qty, "totalTtc": total }),
        ),
    )?;
    get(tx, &ticket_id)
}

/// Moves a line the kitchen has not received to another seat or course and sets its note (its
/// priced options follow it); nothing priced changes, so a printed addition stays true. A sent
/// line is voided and added again instead (`line_already_sent`); an option moves with its dish.
pub fn edit_unsent(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    line_id: &str,
    edit: LineEdit,
) -> Result<Ticket, AppError> {
    let (ticket_id, _, sent, mut line) = line_row(tx, line_id)?;
    let ticket = get(tx, &ticket_id)?;
    require_live(&ticket)?;
    if sent.is_positive() {
        return Err(AppError::conflict("line_already_sent"));
    }
    let option = ticket
        .lines
        .iter()
        .any(|l| l.id == line_id && l.parent_line_id.is_some());
    if option {
        return Err(AppError::validation("lineId", "value_invalid"));
    }
    if edit.seat.is_some_and(|s| !(1..=99).contains(&s))
        || edit.course.is_some_and(|c| !(1..=5).contains(&c))
    {
        return Err(AppError::validation("seat", "value_invalid"));
    }
    line.note = crate::catalog::clean_opt(edit.note.as_deref(), "note", 200)?;
    tx.execute(
        "UPDATE dining_lines SET cart_line = ?2, seat = ?3, course = ?4, updated_at = ?5 WHERE id = ?1",
        params![
            line_id,
            serde_json::to_string(&line)?,
            edit.seat,
            edit.course,
            ctx.now
        ],
    )?;
    tx.execute(
        "UPDATE dining_lines SET seat = ?2, course = ?3, updated_at = ?4
         WHERE parent_line_id = ?1 AND status = 'active'",
        params![line_id, edit.seat, edit.course, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.line_changed",
            "dining_ticket",
            Some(&ticket_id),
            json!({ "lineId": line_id, "seat": edit.seat, "course": edit.course, "note": line.note.is_some() }),
        ),
    )?;
    get(tx, &ticket_id)
}

pub fn set_details(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
    d: TicketDetails,
) -> Result<Ticket, AppError> {
    let ticket = get(tx, ticket_id)?;
    require_live(&ticket)?;
    let note = crate::catalog::clean_opt(d.note.as_deref(), "note", 200)?;
    let label = clean_label(d.label.as_deref())?;
    let address = crate::catalog::clean_opt(d.delivery_address.as_deref(), "deliveryAddress", 200)?;
    if ticket.mode == DiningMode::DineIn && ticket.table_id.is_none() && label.is_none() {
        return Err(AppError::validation("label", "ticket_needs_place"));
    }
    if let Some(c) = &d.customer_id {
        crate::customers::selectable(tx, c)?;
    }
    tx.execute(
        "UPDATE dining_tickets SET covers = ?2, note = ?3, customer_id = ?4, delivery_address = ?5, label = ?6,
                                   updated_at = ?7 WHERE id = ?1",
        params![ticket_id, d.covers.min(200), note, d.customer_id, address, label, ctx.now],
    )?;
    refresh_total(tx, ctx, ticket_id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.ticket_updated",
            "dining_ticket",
            Some(ticket_id),
            json!({ "covers": d.covers, "customerId": d.customer_id }),
        ),
    )?;
    get(tx, ticket_id)
}

/// Moves a table's ticket to another table (RST-004).
pub fn transfer(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
    table_id: &str,
) -> Result<Ticket, AppError> {
    let ticket = get(tx, ticket_id)?;
    require_live(&ticket)?;
    if ticket.mode != DiningMode::DineIn {
        return Err(AppError::validation("tableId", "value_invalid"));
    }
    let table = super::floor::table(tx, table_id)
        .map_err(|_| AppError::validation("tableId", "dining_table_not_found"))?;
    if table.archived {
        return Err(AppError::validation("tableId", "dining_table_not_found"));
    }
    tx.execute(
        "UPDATE dining_tickets SET table_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![ticket_id, table_id, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.ticket_transferred",
            "dining_ticket",
            Some(ticket_id),
            json!({ "from": ticket.table_id, "to": table_id }),
        ),
    )?;
    get(tx, ticket_id)
}

/// Merges `from_id` into `into_id` (RST-004 merge tables): every line moves once (positions after
/// the target's), covers add up, and the emptied ticket is voided.
pub fn merge(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    from_id: &str,
    into_id: &str,
) -> Result<Ticket, AppError> {
    if from_id == into_id {
        return Err(AppError::validation("intoId", "value_invalid"));
    }
    let from = get(tx, from_id)?;
    let into = get(tx, into_id)?;
    require_live(&from)?;
    require_live(&into)?;
    let offset = next_position(tx, into_id)? - 1;
    tx.execute(
        "UPDATE dining_lines SET ticket_id = ?2, position = position + ?3, updated_at = ?4 WHERE ticket_id = ?1",
        params![from_id, into_id, offset, ctx.now],
    )?;
    let covers = u16::from(from.covers) + u16::from(into.covers);
    tx.execute(
        "UPDATE dining_tickets SET covers = ?2, status = 'open', billed_at = NULL, updated_at = ?3 WHERE id = ?1",
        params![into_id, covers.min(200), ctx.now],
    )?;
    tx.execute(
        "UPDATE dining_tickets SET status = 'void', total_ttc = 0, closed_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![from_id, ctx.now],
    )?;
    let total = refresh_total(tx, ctx, into_id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.tickets_merged",
            "dining_ticket",
            Some(into_id),
            json!({ "from": from_id, "lines": from.lines.len(), "totalTtc": total }),
        ),
    )?;
    get(tx, into_id)
}
