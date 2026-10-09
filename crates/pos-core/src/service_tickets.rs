//! Service tickets (ORD-005 repairs, ORD-006 pressing/tailoring): intake with the item, its state,
//! accessories and the fault (the numbered "bon de réparation" / "وصل دخول"), a history of
//! statuses, the technician, the parts and labour planned (sold at pickup with the deposit
//! applied), a "ready" text for SMS/WhatsApp in the customer's language, and the list of what
//! waits too long. A closed ticket is final (migration trigger).

use crate::audit;
use crate::billing::settings::Trilingual;
use crate::catalog::clean_opt;
use crate::deposits::{self, OwnerType};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::party::clean_day;
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, ValidatedSale};
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Repair,
    Pressing,
    Tailoring,
    Other,
}

impl ServiceKind {
    pub const ALL: [ServiceKind; 4] = [
        ServiceKind::Repair,
        ServiceKind::Pressing,
        ServiceKind::Tailoring,
        ServiceKind::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ServiceKind::Repair => "repair",
            ServiceKind::Pressing => "pressing",
            ServiceKind::Tailoring => "tailoring",
            ServiceKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<ServiceKind> {
        ServiceKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(ServiceKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Received,
    Diagnosing,
    WaitingParts,
    InProgress,
    Ready,
    Delivered,
    ReturnedUnrepaired,
    Cancelled,
}

impl ServiceStatus {
    pub const ALL: [ServiceStatus; 8] = [
        ServiceStatus::Received,
        ServiceStatus::Diagnosing,
        ServiceStatus::WaitingParts,
        ServiceStatus::InProgress,
        ServiceStatus::Ready,
        ServiceStatus::Delivered,
        ServiceStatus::ReturnedUnrepaired,
        ServiceStatus::Cancelled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ServiceStatus::Received => "received",
            ServiceStatus::Diagnosing => "diagnosing",
            ServiceStatus::WaitingParts => "waiting_parts",
            ServiceStatus::InProgress => "in_progress",
            ServiceStatus::Ready => "ready",
            ServiceStatus::Delivered => "delivered",
            ServiceStatus::ReturnedUnrepaired => "returned_unrepaired",
            ServiceStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<ServiceStatus> {
        ServiceStatus::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(ServiceStatus);

/// A part or the labour planned on a ticket, sold at pickup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceItem {
    pub id: String,
    pub position: i64,
    pub product_id: String,
    /// In the ticket's language.
    pub name: String,
    pub qty: Qty,
    /// The price typed for an open-price service (labour); `None` sells at the catalogue price.
    pub unit_price: Option<Money>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceItemInput {
    pub product_id: String,
    pub qty: Qty,
    pub unit_price: Option<Money>,
}

/// One step of the ticket's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEvent {
    pub status: ServiceStatus,
    pub note: Option<String>,
    pub created_at: i64,
    pub user_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTicket {
    pub id: String,
    pub number: String,
    pub kind: ServiceKind,
    pub customer_id: String,
    pub customer_name: String,
    pub item: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_no: Option<String>,
    pub accessories: Option<String>,
    pub condition: Option<String>,
    pub fault: String,
    pub estimate: Option<Money>,
    pub promised_on: Option<String>,
    pub technician_id: Option<String>,
    pub technician_name: Option<String>,
    pub status: ServiceStatus,
    pub ready_at: Option<i64>,
    pub closed_at: Option<i64>,
    /// The pickup ticket.
    pub document_id: Option<String>,
    pub lang: Lang,
    pub business_date: String,
    pub created_at: i64,
    pub items: Vec<ServiceItem>,
    /// What is held (the sum of its deposits ledger).
    pub deposits: Money,
    pub events: Vec<ServiceEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IntakeInput {
    pub kind: ServiceKind,
    pub customer_id: String,
    pub item: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_no: Option<String>,
    pub accessories: Option<String>,
    pub condition: Option<String>,
    pub fault: String,
    pub estimate: Option<Money>,
    /// `YYYY-MM-DD`.
    pub promised_on: Option<String>,
    pub technician_id: Option<String>,
    /// The customer's language: the intake slip and the "ready" text.
    pub lang: Lang,
    #[serde(default)]
    pub items: Vec<ServiceItemInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct BoardFilter {
    pub status: Option<ServiceStatus>,
    pub kind: Option<ServiceKind>,
    /// Matches the number, the item, the serial number or the customer's name.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    pub id: String,
    pub number: String,
    pub kind: ServiceKind,
    pub customer_name: String,
    pub item: String,
    pub status: ServiceStatus,
    pub technician_name: Option<String>,
    pub promised_on: Option<String>,
    pub ready_at: Option<i64>,
    /// Days since the intake.
    pub days_open: i64,
    /// Days since it was ready; `None` until then.
    pub ready_days: Option<i64>,
    pub deposits: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickupInput {
    pub id: String,
    /// What the customer pays on top of the deposit (applied first, by itself).
    pub payments: Vec<PaymentInput>,
    #[serde(default)]
    pub extras: SaleExtras,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickedUp {
    pub ticket: ServiceTicket,
    pub sale: ValidatedSale,
}

const TRANSITIONS: &[(ServiceStatus, &[ServiceStatus])] = &[
    (
        ServiceStatus::Received,
        &[
            ServiceStatus::Diagnosing,
            ServiceStatus::InProgress,
            ServiceStatus::WaitingParts,
            ServiceStatus::Ready,
            ServiceStatus::ReturnedUnrepaired,
            ServiceStatus::Cancelled,
        ],
    ),
    (
        ServiceStatus::Diagnosing,
        &[
            ServiceStatus::WaitingParts,
            ServiceStatus::InProgress,
            ServiceStatus::Ready,
            ServiceStatus::ReturnedUnrepaired,
            ServiceStatus::Cancelled,
        ],
    ),
    (
        ServiceStatus::WaitingParts,
        &[
            ServiceStatus::InProgress,
            ServiceStatus::Ready,
            ServiceStatus::ReturnedUnrepaired,
            ServiceStatus::Cancelled,
        ],
    ),
    (
        ServiceStatus::InProgress,
        &[
            ServiceStatus::WaitingParts,
            ServiceStatus::Ready,
            ServiceStatus::ReturnedUnrepaired,
            ServiceStatus::Cancelled,
        ],
    ),
    (
        ServiceStatus::Ready,
        &[
            ServiceStatus::InProgress,
            ServiceStatus::ReturnedUnrepaired,
            ServiceStatus::Cancelled,
        ],
    ),
];

fn allowed(from: ServiceStatus, to: ServiceStatus) -> bool {
    TRANSITIONS
        .iter()
        .any(|(f, tos)| *f == from && tos.contains(&to))
}

const TICKET_SELECT: &str = "SELECT t.id, t.number, t.kind, t.customer_id, c.name, t.item, t.brand, t.model,
        t.serial_no, t.accessories, t.condition, t.fault, t.estimate, t.promised_on, t.technician_id,
        u.display_name, t.status, t.ready_at, t.closed_at, t.document_id, t.lang, t.business_date,
        t.created_at
     FROM service_tickets t
     JOIN customers c ON c.id = t.customer_id
     LEFT JOIN users u ON u.id = t.technician_id";

fn map_ticket(r: &Row<'_>) -> rusqlite::Result<ServiceTicket> {
    Ok(ServiceTicket {
        id: r.get(0)?,
        number: r.get(1)?,
        kind: r.get(2)?,
        customer_id: r.get(3)?,
        customer_name: r.get(4)?,
        item: r.get(5)?,
        brand: r.get(6)?,
        model: r.get(7)?,
        serial_no: r.get(8)?,
        accessories: r.get(9)?,
        condition: r.get(10)?,
        fault: r.get(11)?,
        estimate: r.get(12)?,
        promised_on: r.get(13)?,
        technician_id: r.get(14)?,
        technician_name: r.get(15)?,
        status: r.get(16)?,
        ready_at: r.get(17)?,
        closed_at: r.get(18)?,
        document_id: r.get(19)?,
        lang: r.get(20)?,
        business_date: r.get(21)?,
        created_at: r.get(22)?,
        items: Vec::new(),
        deposits: Money::ZERO,
        events: Vec::new(),
    })
}

fn items(conn: &Connection, id: &str, lang: Lang) -> Result<Vec<ServiceItem>, AppError> {
    let name = crate::reports::name_of("p", lang);
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT i.id, i.position, i.product_id, {name}, i.qty, i.unit_price
         FROM service_ticket_items i JOIN products p ON p.id = i.product_id
         WHERE i.ticket_id = ?1 ORDER BY i.position"
    ))?;
    let rows = stmt
        .query_map([id], |r| {
            Ok(ServiceItem {
                id: r.get(0)?,
                position: r.get(1)?,
                product_id: r.get(2)?,
                name: r.get(3)?,
                qty: r.get(4)?,
                unit_price: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn events(conn: &Connection, id: &str) -> Result<Vec<ServiceEvent>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT e.status, e.note, e.created_at, COALESCE(u.display_name, '')
         FROM service_ticket_events e LEFT JOIN users u ON u.id = e.user_id
         WHERE e.ticket_id = ?1 ORDER BY e.created_at, e.rowid",
    )?;
    let rows = stmt
        .query_map([id], |r| {
            Ok(ServiceEvent {
                status: r.get(0)?,
                note: r.get(1)?,
                created_at: r.get(2)?,
                user_name: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// A ticket with its items (named in its language), its deposits held and its history.
pub fn get(conn: &Connection, id: &str) -> Result<ServiceTicket, AppError> {
    let mut t = conn
        .prepare_cached(&format!("{TICKET_SELECT} WHERE t.id = ?1"))?
        .query_row([id], map_ticket)
        .optional()?
        .ok_or_else(|| AppError::not_found("service_ticket"))?;
    t.items = items(conn, id, t.lang)?;
    t.deposits = deposits::held(conn, OwnerType::ServiceTicket, id)?;
    t.events = events(conn, id)?;
    Ok(t)
}

fn parse_day(day: &str, field: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| AppError::validation(field, "date_invalid"))
}

/// The service board: by status, kind and a text (number, item, serial number, customer), newest
/// first, at most 500 tickets; the days are counted to `today`.
pub fn board(
    conn: &Connection,
    filter: &BoardFilter,
    today: &str,
) -> Result<Vec<ServiceSummary>, AppError> {
    let today = parse_day(today, "today")?;
    let text = filter.text.trim();
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, t.number, t.kind, c.name, t.item, t.status, u.display_name, t.promised_on,
                t.ready_at, t.business_date,
                (SELECT COALESCE(SUM(d.amount), 0) FROM deposits d
                 WHERE d.owner_type = 'service_ticket' AND d.owner_id = t.id)
         FROM service_tickets t
         JOIN customers c ON c.id = t.customer_id
         LEFT JOIN users u ON u.id = t.technician_id
         WHERE (?1 IS NULL OR t.status = ?1) AND (?2 IS NULL OR t.kind = ?2)
           AND (?3 = '' OR t.number LIKE '%' || ?3 || '%' OR t.item LIKE '%' || ?3 || '%'
                OR t.serial_no LIKE '%' || ?3 || '%' OR c.name LIKE '%' || ?3 || '%')
         ORDER BY t.created_at DESC, t.rowid DESC
         LIMIT 500",
    )?;
    let rows = stmt
        .query_map(params![filter.status, filter.kind, text], |r| {
            Ok((
                ServiceSummary {
                    id: r.get(0)?,
                    number: r.get(1)?,
                    kind: r.get(2)?,
                    customer_name: r.get(3)?,
                    item: r.get(4)?,
                    status: r.get(5)?,
                    technician_name: r.get(6)?,
                    promised_on: r.get(7)?,
                    ready_at: r.get(8)?,
                    days_open: 0,
                    ready_days: None,
                    deposits: r.get(10)?,
                },
                r.get::<_, String>(9)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(mut s, business_date)| {
            s.days_open = (today - parse_day(&business_date, "businessDate")?).num_days();
            s.ready_days = s
                .ready_at
                .map(|ms| (today - crate::time::business_date(ms)).num_days());
            Ok(s)
        })
        .collect()
}

/// Opens a ticket (the numbered intake slip `RP…`). One `service.intake` row.
pub fn intake(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: IntakeInput,
) -> Result<ServiceTicket, AppError> {
    packs::require(tx, Module::Repairs)?;
    crate::customers::selectable(tx, &input.customer_id)?;
    let item = clean_opt(Some(&input.item), "item", 120)?
        .ok_or_else(|| AppError::validation("item", "required"))?;
    let fault = clean_opt(Some(&input.fault), "fault", 500)?
        .ok_or_else(|| AppError::validation("fault", "required"))?;
    let serial = clean_opt(input.serial_no.as_deref(), "serialNo", 40)?;
    if let Some(s) = &serial
        && s.len() == 15
        && s.bytes().all(|b| b.is_ascii_digit())
        && !crate::stock::serials::imei_ok(s)
    {
        return Err(AppError::validation("serialNo", "imei_invalid"));
    }
    let promised = clean_day(input.promised_on.as_deref(), "promisedOn")?;
    if input.estimate.is_some_and(Money::is_negative) {
        return Err(AppError::validation("estimate", "amount_invalid"));
    }
    if let Some(t) = &input.technician_id {
        technician(tx, t)?;
    }
    let brand = clean_opt(input.brand.as_deref(), "brand", 60)?;
    let model = clean_opt(input.model.as_deref(), "model", 60)?;
    let accessories = clean_opt(input.accessories.as_deref(), "accessories", 200)?;
    let condition = clean_opt(input.condition.as_deref(), "condition", 300)?;
    let number =
        crate::numbering::assign_custom(tx, "service_ticket", "RP", ctx.business_date())?.number;
    let id = new_id();
    tx.execute(
        "INSERT INTO service_tickets (id, number, kind, customer_id, item, brand, model, serial_no, accessories, condition,
                                      fault, estimate, promised_on, technician_id, status, lang, business_date,
                                      created_at, updated_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'received', ?15, ?16, ?17, ?17, ?18, ?19)",
        params![
            id,
            number,
            input.kind,
            input.customer_id,
            item,
            brand,
            model,
            serial,
            accessories,
            condition,
            fault,
            input.estimate,
            promised,
            input.technician_id,
            input.lang,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    event(tx, ctx, &id, ServiceStatus::Received, None)?;
    write_items(tx, ctx, &id, &input.items)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "service.intake",
            "service_ticket",
            Some(&id),
            json!({ "number": number, "kind": input.kind, "customerId": input.customer_id }),
        ),
    )?;
    get(tx, &id)
}

/// A ticket still open: a delivered, returned or cancelled one is final.
fn still_open(conn: &Connection, id: &str) -> Result<ServiceTicket, AppError> {
    let t = get(conn, id)?;
    if matches!(
        t.status,
        ServiceStatus::Delivered | ServiceStatus::ReturnedUnrepaired | ServiceStatus::Cancelled
    ) {
        return Err(AppError::conflict("service_status_invalid"));
    }
    Ok(t)
}

fn technician(conn: &Connection, id: &str) -> Result<(), AppError> {
    crate::auth::users::get(conn, id)
        .map(|_| ())
        .map_err(|_| AppError::validation("technicianId", "user_not_found"))
}

fn event(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    status: ServiceStatus,
    note: Option<&str>,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO service_ticket_events (id, ticket_id, status, note, created_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![new_id(), id, status, note, ctx.now, ctx.session.user_id],
    )?;
    Ok(())
}

fn write_items(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    items: &[ServiceItemInput],
) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM service_ticket_items WHERE ticket_id = ?1",
        [id],
    )?;
    for (i, item) in items.iter().enumerate() {
        crate::catalog::products::sellable(tx, &item.product_id).map_err(|_| {
            AppError::validation(&format!("items.{i}.productId"), "product_not_found")
        })?;
        if !item.qty.is_positive() {
            return Err(AppError::validation(
                &format!("items.{i}.qty"),
                "qty_invalid",
            ));
        }
        if item.unit_price.is_some_and(Money::is_negative) {
            return Err(AppError::validation(
                &format!("items.{i}.unitPrice"),
                "amount_invalid",
            ));
        }
        tx.execute(
            "INSERT INTO service_ticket_items (id, ticket_id, position, product_id, qty, unit_price, created_at, user_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                new_id(),
                id,
                i64::try_from(i).unwrap_or(i64::MAX - 1) + 1,
                item.product_id,
                item.qty,
                item.unit_price,
                ctx.now,
                ctx.session.user_id
            ],
        )?;
    }
    Ok(())
}

/// Moves a ticket along its statuses (never to `delivered`: that is the pickup). Closing it
/// unrepaired or cancelled asks for its deposit to be refunded first. One `service.status_set`
/// row.
pub fn set_status(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    status: ServiceStatus,
    note: Option<String>,
) -> Result<ServiceTicket, AppError> {
    packs::require(tx, Module::Repairs)?;
    let t = get(tx, id)?;
    if !allowed(t.status, status) {
        return Err(AppError::validation("status", "service_status_invalid"));
    }
    let closing = matches!(
        status,
        ServiceStatus::ReturnedUnrepaired | ServiceStatus::Cancelled
    );
    if closing && t.deposits.is_positive() {
        return Err(AppError::conflict("deposit_held"));
    }
    let note = clean_opt(note.as_deref(), "note", 200)?;
    tx.execute(
        "UPDATE service_tickets SET status = ?2, ready_at = CASE WHEN ?2 = 'ready' THEN ?3 ELSE ready_at END,
                closed_at = CASE WHEN ?4 THEN ?3 ELSE closed_at END, updated_at = ?3 WHERE id = ?1",
        params![id, status, ctx.now, closing],
    )?;
    event(tx, ctx, id, status, note.as_deref())?;
    audit::append(
        tx,
        &ctx.actor().event(
            "service.status_set",
            "service_ticket",
            Some(id),
            json!({ "from": t.status, "to": status }),
        ),
    )?;
    get(tx, id)
}

/// Names the technician (or none). One `service.assigned` row.
pub fn assign(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    technician_id: Option<&str>,
) -> Result<ServiceTicket, AppError> {
    packs::require(tx, Module::Repairs)?;
    still_open(tx, id)?;
    if let Some(t) = technician_id {
        technician(tx, t)?;
    }
    tx.execute(
        "UPDATE service_tickets SET technician_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, technician_id, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "service.assigned",
            "service_ticket",
            Some(id),
            json!({ "technicianId": technician_id }),
        ),
    )?;
    get(tx, id)
}

/// Replaces the parts and labour planned. One `service.items_set` row.
pub fn set_items(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    items: Vec<ServiceItemInput>,
) -> Result<ServiceTicket, AppError> {
    packs::require(tx, Module::Repairs)?;
    still_open(tx, id)?;
    write_items(tx, ctx, id, &items)?;
    tx.execute(
        "UPDATE service_tickets SET updated_at = ?2 WHERE id = ?1",
        params![id, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "service.items_set",
            "service_ticket",
            Some(id),
            json!({ "items": items.len() }),
        ),
    )?;
    get(tx, id)
}

/// The "your device is ready" text (SMS/WhatsApp, copied by the cashier: nothing is sent from the
/// app) in the customer's language, from `repairs.ready_message`.
pub fn ready_message(conn: &Connection, id: &str, lang: Lang) -> Result<String, AppError> {
    let t = get(conn, id)?;
    let templates: Trilingual = crate::settings::get(conn, "repairs.ready_message")?;
    let shop = crate::company::name_in(conn, lang)?;
    let due = t
        .estimate
        .unwrap_or(Money::ZERO)
        .checked_sub(t.deposits)
        .unwrap_or(Money::ZERO)
        .max(Money::ZERO);
    Ok(templates
        .in_lang(lang)
        .unwrap_or_default()
        .replace("{shop}", &shop)
        .replace("{number}", &t.number)
        .replace("{item}", &t.item)
        .replace("{amount}", &due.to_string()))
}

/// Hands a ready item back: its parts and labour are sold on one ticket to its customer, the
/// deposit held applied first as a `voucher` tender naming the ticket, and the ticket is
/// delivered. One `service.delivered` row (the sale writes its own).
pub fn pickup(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: PickupInput,
) -> Result<PickedUp, AppError> {
    packs::require(tx, Module::Repairs)?;
    let t = get(tx, &input.id)?;
    if t.status != ServiceStatus::Ready {
        return Err(AppError::conflict("service_not_ready"));
    }
    if t.items.is_empty() {
        return Err(AppError::validation("items", "required"));
    }
    let lines: Vec<CartLine> = t
        .items
        .iter()
        .map(|i| {
            let mut l = CartLine::product(&i.product_id, i.qty);
            l.unit_price = i.unit_price;
            l
        })
        .collect();
    let mut cart = Cart::new(t.lang, lines);
    cart.customer_id = Some(t.customer_id.clone());
    cart.note = Some(t.number.clone());
    let total = crate::sales::cart::price_cart(tx, ctx, &cart)?
        .totals
        .total_ttc;
    // A deposit above the final price is refunded first (`deposits::refund` on the open
    // ticket): a delivered ticket never keeps money held for the customer.
    if t.deposits > total {
        return Err(AppError::rejected(
            json!({ "held": t.deposits, "total": total }),
            "deposit_held",
        ));
    }
    let applied = t.deposits;
    let mut payments = input.payments;
    let mut extras = input.extras;
    if applied.is_positive() {
        payments.insert(
            0,
            PaymentInput {
                method: PaymentMethod::Voucher,
                amount: applied,
                reference: Some(t.number.clone()),
                cheque_bank: None,
                cheque_date: None,
            },
        );
        // the payment types name the payments sent: each moves one place down (final review)
        for used in &mut extras.payment_types {
            used.payment = used.payment.saturating_add(1);
        }
    }
    let sale = crate::sales::create_sale_with(tx, ctx, SaleInput { cart, payments }, &extras)?;
    tx.execute(
        "UPDATE service_tickets SET status = 'delivered', document_id = ?2, closed_at = ?3, updated_at = ?3
         WHERE id = ?1",
        params![t.id, sale.document.id, ctx.now],
    )?;
    event(tx, ctx, &t.id, ServiceStatus::Delivered, None)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "service.delivered",
            "service_ticket",
            Some(&t.id),
            json!({ "documentId": sale.document.id, "deposit": applied }),
        ),
    )?;
    Ok(PickedUp {
        ticket: get(tx, &t.id)?,
        sale,
    })
}

/// Ready tickets waiting more than `repairs.uncollected_days` on `today` (RPT-069).
pub fn uncollected(conn: &Connection, today: &str) -> Result<Vec<ServiceSummary>, AppError> {
    let days: i64 = crate::settings::get(conn, "repairs.uncollected_days")?;
    let filter = BoardFilter {
        status: Some(ServiceStatus::Ready),
        kind: None,
        text: String::new(),
    };
    Ok(board(conn, &filter, today)?
        .into_iter()
        .filter(|s| s.ready_days.is_some_and(|d| d > days))
        .collect())
}

/// A fixed `Received` repair ticket for the slip and printer tests of `pos-docs` and `pos-hw`.
#[doc(hidden)]
pub fn tests_fixture() -> ServiceTicket {
    ServiceTicket {
        id: "00000000-0000-7000-8000-000000000001".into(),
        number: "RP2026-000001".into(),
        kind: ServiceKind::Repair,
        customer_id: "00000000-0000-7000-8000-000000000002".into(),
        customer_name: "Nassim Ould Ali".into(),
        item: "Samsung A54".into(),
        brand: Some("Samsung".into()),
        model: Some("A54".into()),
        serial_no: Some("356938035643809".into()),
        accessories: Some("Chargeur, coque".into()),
        condition: Some("Écran fissuré".into()),
        fault: "Remplacement écran".into(),
        estimate: Some(Money(1_500_000)),
        promised_on: Some("2026-10-01".into()),
        technician_id: None,
        technician_name: None,
        status: ServiceStatus::Received,
        ready_at: None,
        closed_at: None,
        document_id: None,
        lang: Lang::Ar,
        business_date: "2026-09-28".into(),
        created_at: 1_790_593_200_000,
        items: Vec::new(),
        deposits: Money::ZERO,
        events: Vec::new(),
    }
}
