//! Delivery rounds, tournées (VRT-100, DOC-030): a round (`TR…`) carries validated delivery notes
//! and invoices to their customers in a driver's vehicle; each stop is reported delivered or
//! failed with what was collected; closing the round at the shop turns every collection into an
//! M2 versement settling that stop's document first (its droit de timbre on cash, its receipt,
//! its cash in the drawer of the closing till). A van is a stock location loaded and unloaded by
//! M2 transfers; the till sells from it with `SaleExtras.location_id` (VRT-101).
//!
//! Rounds are host-only writes; each write below is one function the app
//! can register as a host service.

use crate::audit::{self, Actor};
use crate::catalog::clean_opt;
use crate::customers::credit::{self, CollectInput, Collected};
use crate::documents::{self, DocStatus, DocType};
use crate::error::AppError;
use crate::ids::new_id;
use crate::ledger::{self, Side};
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::party::clean_day;
use crate::payments::PaymentMethod;
use crate::sqlenum::sql_text_enum;
use crate::stock::transfers::{self, TransferDone, TransferInput, TransferLine};
use crate::stock::{self, LocationInput, LocationKind, StockLocation};
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// Most stops on one round.
const MAX_STOPS: usize = 200;
/// Most rounds a list returns.
const LIST_LIMIT: i64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    Planned,
    Out,
    Closed,
}

impl RoundStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            RoundStatus::Planned => "planned",
            RoundStatus::Out => "out",
            RoundStatus::Closed => "closed",
        }
    }

    pub fn parse(s: &str) -> Option<RoundStatus> {
        [RoundStatus::Planned, RoundStatus::Out, RoundStatus::Closed]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(RoundStatus);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StopStatus {
    Pending,
    Delivered,
    Failed,
}

impl StopStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            StopStatus::Pending => "pending",
            StopStatus::Delivered => "delivered",
            StopStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<StopStatus> {
        [
            StopStatus::Pending,
            StopStatus::Delivered,
            StopStatus::Failed,
        ]
        .into_iter()
        .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(StopStatus);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoundInput {
    /// `YYYY-MM-DD`; `None` = today's business date.
    pub business_date: Option<String>,
    pub driver_name: String,
    pub driver_user_id: Option<String>,
    pub vehicle_plate: Option<String>,
    pub van_location_id: Option<String>,
    pub note: Option<String>,
}

/// One delivery of a round: a validated delivery note or invoice and what came of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Stop {
    /// 1-based, the order of the route.
    pub seq: u32,
    pub document_id: String,
    pub document_number: Option<String>,
    pub doc_type: DocType,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    /// What the customer owed on the document when the stop was planned.
    pub amount_due: Money,
    pub status: StopStatus,
    pub collected: Money,
    pub method: Option<PaymentMethod>,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    /// The versement's receipt (`RV…`) once the round is closed.
    pub receipt_number: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    pub id: String,
    /// `TR2026-000001`.
    pub number: String,
    pub business_date: String,
    pub driver_name: String,
    pub driver_user_id: Option<String>,
    pub vehicle_plate: Option<String>,
    pub van_location_id: Option<String>,
    /// The van's French name (the screen shows it in its own language from `stock::locations`).
    pub van_location_name: Option<String>,
    pub status: RoundStatus,
    pub note: Option<String>,
    pub started_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub stops: Vec<Stop>,
    pub total_due: Money,
    pub total_collected: Money,
    /// The part collected in cash.
    pub cash_collected: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoundFilter {
    pub status: Option<RoundStatus>,
    /// `YYYY-MM-DD`, inclusive.
    pub from: String,
    /// `YYYY-MM-DD`, inclusive.
    pub to: String,
}

/// What the driver reports for a stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StopReport {
    pub seq: u32,
    pub status: StopStatus,
    pub collected: Money,
    pub method: Option<PaymentMethod>,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub note: Option<String>,
}

/// A closed round and the versements its collections became, in stop order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClosedRound {
    pub round: Round,
    pub versements: Vec<Collected>,
}

fn load_round(conn: &Connection, id: &str) -> Result<(RoundStatus, Option<String>), AppError> {
    conn.query_row(
        "SELECT status, van_location_id FROM delivery_rounds WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("round"))
}

/// The round's van, refused unless the round stands in `status`.
fn expect(conn: &Connection, id: &str, status: RoundStatus) -> Result<Option<String>, AppError> {
    let (s, van) = load_round(conn, id)?;
    if s != status {
        return Err(AppError::conflict("round_status_invalid"));
    }
    Ok(van)
}

fn touch(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<(), AppError> {
    tx.execute(
        "UPDATE delivery_rounds SET updated_at = ?2 WHERE id = ?1",
        params![id, ctx.now],
    )?;
    Ok(())
}

/// A new planned round (`TR…`), numbered in its business year.
pub fn create(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: RoundInput,
) -> Result<Round, AppError> {
    packs::require(tx, Module::DeliveryRounds)?;
    let driver = clean_opt(Some(&input.driver_name), "driverName", 80)?
        .ok_or_else(|| AppError::validation("driverName", "required"))?;
    let plate = clean_opt(input.vehicle_plate.as_deref(), "vehiclePlate", 20)?;
    let note = clean_opt(input.note.as_deref(), "note", 200)?;
    let day = clean_day(input.business_date.as_deref(), "businessDate")?
        .unwrap_or_else(|| ctx.business_day());
    let date = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|_| AppError::validation("businessDate", "date_invalid"))?;
    if let Some(u) = &input.driver_user_id {
        crate::auth::users::get(tx, u)
            .map_err(|_| AppError::validation("driverUserId", "user_not_found"))?;
    }
    if let Some(v) = &input.van_location_id {
        van_location(tx, v)?;
    }
    let number = crate::numbering::assign_custom(tx, "delivery_round", "TR", date)?.number;
    let id = new_id();
    tx.execute(
        "INSERT INTO delivery_rounds (id, number, business_date, driver_name, driver_user_id, vehicle_plate,
                                      van_location_id, note, created_at, updated_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11)",
        params![
            id,
            number,
            day,
            driver,
            input.driver_user_id,
            plate,
            input.van_location_id,
            note,
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "rounds.created",
            "delivery_round",
            Some(&id),
            json!({ "number": number, "driver": driver, "vanLocationId": input.van_location_id }),
        ),
    )?;
    get(tx, &id)
}

/// The open debt of a validated delivery note or invoice: its `sale_credit` entry's open part.
fn open_debt(
    conn: &Connection,
    customer_id: &str,
    document_id: &str,
) -> Result<(Money, Option<String>), AppError> {
    Ok(ledger::open_debits(conn, Side::Customer, customer_id)?
        .into_iter()
        .find(|e| {
            e.ref_type.as_deref() == Some("document") && e.ref_id.as_deref() == Some(document_id)
        })
        .map_or((Money::ZERO, None), |e| (e.open, Some(e.id))))
}

/// What a stop's customer owes on its document, and the ledger entry a collection settles first.
/// An invoice: its open debt. A delivery note carries no debt in M3 until it is invoiced: once
/// invoiced, its invoice's open debt (at most the note's total); before that, the note's total,
/// collected on delivery as a versement that settles the customer's oldest debts or waits on the
/// account as an advance the note's invoice will take.
fn debt_of(
    conn: &Connection,
    customer_id: &str,
    doc: &documents::Document,
) -> Result<(Money, Option<String>), AppError> {
    if doc.doc_type != DocType::DeliveryNote {
        return open_debt(conn, customer_id, &doc.id);
    }
    match documents::links::invoiced(conn, &doc.id)? {
        Some(invoice) if invoice.status == DocStatus::Validated => {
            let (open, entry) = open_debt(conn, customer_id, &invoice.link.to_id)?;
            Ok((open.min(doc.total_due), entry))
        }
        _ => Ok((doc.total_due, None)),
    }
}

/// The round's stops, in route order (planned rounds only): validated delivery notes or invoices
/// with a customer, each on one open round at a time.
pub fn set_stops(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    document_ids: Vec<String>,
) -> Result<Round, AppError> {
    packs::require(tx, Module::DeliveryRounds)?;
    expect(tx, id, RoundStatus::Planned)?;
    if document_ids.len() > MAX_STOPS {
        return Err(AppError::validation("documentIds", "value_invalid"));
    }
    tx.execute("DELETE FROM delivery_round_stops WHERE round_id = ?1", [id])?;
    for (i, doc_id) in document_ids.iter().enumerate() {
        let field = format!("documentIds.{i}");
        let doc = documents::get(tx, doc_id)
            .map_err(|_| AppError::validation(&field, "round_stop_invalid"))?;
        let customer_id = match (doc.doc_type, doc.status, &doc.customer_id) {
            (DocType::DeliveryNote | DocType::Invoice, DocStatus::Validated, Some(c)) => c.clone(),
            _ => return Err(AppError::validation(&field, "round_stop_invalid")),
        };
        let riding: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM delivery_round_stops s JOIN delivery_rounds r ON r.id = s.round_id
                            WHERE s.document_id = ?1 AND r.status <> 'closed' AND r.id <> ?2)",
            params![doc_id, id],
            |r| r.get(0),
        )?;
        if riding || document_ids[..i].contains(doc_id) {
            return Err(AppError::validation(&field, "round_stop_invalid"));
        }
        let (due, _) = debt_of(tx, &customer_id, &doc)?;
        // At most `MAX_STOPS` stops: the position always fits.
        let seq = u32::try_from(i + 1).unwrap_or(u32::MAX);
        tx.execute(
            "INSERT INTO delivery_round_stops (round_id, seq, document_id, customer_id, amount_due, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, seq, doc_id, customer_id, due, ctx.now],
        )?;
    }
    touch(tx, ctx, id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "rounds.stops_set",
            "delivery_round",
            Some(id),
            json!({ "documentIds": document_ids }),
        ),
    )?;
    get(tx, id)
}

/// The round leaves the shop: its stops are fixed from now on.
pub fn start(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<Round, AppError> {
    packs::require(tx, Module::DeliveryRounds)?;
    expect(tx, id, RoundStatus::Planned)?;
    tx.execute(
        "UPDATE delivery_rounds SET status = 'out', started_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor()
            .event("rounds.started", "delivery_round", Some(id), json!({})),
    )?;
    get(tx, id)
}

/// Records what happened at a stop of a round on the road; it can be corrected until the close.
/// A collection is never above what the customer owed on the document, and its tender is checked
/// now as the versement will check it at the close.
pub fn report_stop(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    r: StopReport,
) -> Result<Round, AppError> {
    packs::require(tx, Module::DeliveryRounds)?;
    expect(tx, id, RoundStatus::Out)?;
    let (due, customer_id): (Money, Option<String>) = tx
        .query_row(
            "SELECT amount_due, customer_id FROM delivery_round_stops WHERE round_id = ?1 AND seq = ?2",
            params![id, r.seq],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::validation("seq", "round_stop_invalid"))?;
    if r.status == StopStatus::Pending {
        return Err(AppError::validation("status", "value_invalid"));
    }
    if r.collected.is_negative() {
        return Err(AppError::validation("collected", "amount_invalid"));
    }
    if r.collected > due {
        return Err(AppError::validation("collected", "collection_above_due"));
    }
    let method = match (r.collected.is_positive(), r.method) {
        (false, _) => None,
        (
            true,
            Some(
                m @ (PaymentMethod::Cash
                | PaymentMethod::CardCib
                | PaymentMethod::Edahabia
                | PaymentMethod::Baridimob
                | PaymentMethod::Cheque
                | PaymentMethod::Transfer),
            ),
        ) => Some(m),
        (true, _) => return Err(AppError::validation("method", "payment_method_invalid")),
    };
    let collected = method.map(|_| r.collected).unwrap_or(Money::ZERO);
    let (reference, cheque_bank, cheque_date) = match method {
        Some(_) => (
            clean_opt(r.reference.as_deref(), "reference", 60)?,
            clean_opt(r.cheque_bank.as_deref(), "chequeBank", 60)?,
            clean_day(r.cheque_date.as_deref(), "chequeDate")?,
        ),
        None => (None, None, None),
    };
    if let (Some(m), Some(customer_id)) = (method, &customer_id) {
        // The same checks as the versement of the close (cheque details, transfer reference…).
        credit::quote(
            tx,
            ctx,
            &CollectInput {
                customer_id: customer_id.clone(),
                amount: collected,
                method: m,
                reference: reference.clone(),
                cheque_bank: cheque_bank.clone(),
                cheque_date: cheque_date.clone(),
                allocate_to: Vec::new(),
                note: None,
            },
        )?;
    }
    let note = clean_opt(r.note.as_deref(), "note", 200)?;
    tx.execute(
        "UPDATE delivery_round_stops SET status = ?3, collected = ?4, method = ?5, reference = ?6, cheque_bank = ?7,
                cheque_date = ?8, note = ?9, updated_at = ?10
         WHERE round_id = ?1 AND seq = ?2",
        params![
            id,
            r.seq,
            r.status,
            collected,
            method,
            reference,
            cheque_bank,
            cheque_date,
            note,
            ctx.now
        ],
    )?;
    touch(tx, ctx, id)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "rounds.stop_reported",
            "delivery_round",
            Some(id),
            json!({ "seq": r.seq, "status": r.status, "collected": collected, "method": method }),
        ),
    )?;
    get(tx, id)
}

/// What closing a round will book, shown before it is (VRT-100): the collections by method in the
/// order of the route, the cash the driver hands in — each cash collection with the droit de
/// timbre its customer pays on it, as the versement of [`close`] computes it — and the stops not
/// delivered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClosePreview {
    pub collected: Vec<(PaymentMethod, Money)>,
    pub cash_to_hand_in: Money,
    pub failed: u32,
}

/// The [`ClosePreview`] of a round, read only: M2's versement quote of every collection, with the
/// same checks [`close`] makes.
pub fn close_preview(
    conn: &Connection,
    ctx: &TillContext,
    id: &str,
) -> Result<ClosePreview, AppError> {
    let round = get(conn, id)?;
    let overflow = || AppError::Internal("round total overflow".into());
    let mut collected: Vec<(PaymentMethod, Money)> = Vec::new();
    let mut cash_to_hand_in = Money::ZERO;
    for stop in round.stops.iter().filter(|s| s.collected.is_positive()) {
        let (Some(customer_id), Some(method)) = (stop.customer_id.clone(), stop.method) else {
            return Err(AppError::validation("seq", "round_stop_invalid"));
        };
        let quote = credit::quote(
            conn,
            ctx,
            &CollectInput {
                customer_id,
                amount: stop.collected,
                method,
                reference: stop.reference.clone(),
                cheque_bank: stop.cheque_bank.clone(),
                cheque_date: stop.cheque_date.clone(),
                allocate_to: Vec::new(),
                note: None,
            },
        )?;
        match collected.iter_mut().find(|(m, _)| *m == method) {
            Some((_, sum)) => *sum = sum.checked_add(stop.collected).ok_or_else(overflow)?,
            None => collected.push((method, stop.collected)),
        }
        if method.is_cash() {
            cash_to_hand_in = cash_to_hand_in
                .checked_add(quote.to_receive)
                .ok_or_else(overflow)?;
        }
    }
    let failed = round
        .stops
        .iter()
        .filter(|s| s.status == StopStatus::Failed)
        .count();
    Ok(ClosePreview {
        collected,
        cash_to_hand_in,
        // At most `MAX_STOPS` stops.
        failed: u32::try_from(failed).unwrap_or(u32::MAX),
    })
}

/// Books every collection as a versement settling its stop's document, in this till's session
/// (the calling till's when a till closes the round through the main PC: its drawer takes the
/// driver's cash, Task 20 review).
pub fn close(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<ClosedRound, AppError> {
    packs::require(tx, Module::DeliveryRounds)?;
    expect(tx, id, RoundStatus::Out)?;
    let session = crate::cash::drawer(tx, ctx)?;
    let round = get(tx, id)?;
    let mut versements = Vec::new();
    for stop in round.stops.iter().filter(|s| s.collected.is_positive()) {
        let (Some(customer_id), Some(method)) = (stop.customer_id.clone(), stop.method) else {
            return Err(AppError::validation("seq", "round_stop_invalid"));
        };
        let doc = documents::get(tx, &stop.document_id)?;
        let (_, entry) = debt_of(tx, &customer_id, &doc)?;
        let collected = credit::collect(
            tx,
            ctx,
            CollectInput {
                customer_id,
                amount: stop.collected,
                method,
                reference: stop.reference.clone(),
                cheque_bank: stop.cheque_bank.clone(),
                cheque_date: stop.cheque_date.clone(),
                allocate_to: entry.into_iter().collect(),
                note: Some(format!("{} #{}", round.number, stop.seq)),
            },
        )?;
        tx.execute(
            "UPDATE delivery_round_stops SET ledger_entry_id = ?3, updated_at = ?4 WHERE round_id = ?1 AND seq = ?2",
            params![id, stop.seq, collected.entry.id, ctx.now],
        )?;
        versements.push(collected);
    }
    tx.execute(
        "UPDATE delivery_rounds SET status = 'closed', closed_at = ?2, closed_session_id = ?3, updated_at = ?2
         WHERE id = ?1",
        params![id, ctx.now, session.id],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "rounds.closed",
            "delivery_round",
            Some(id),
            json!({
                "number": round.number,
                "collected": round.total_collected,
                "cash": round.cash_collected,
                "versements": versements.len(),
            }),
        ),
    )?;
    Ok(ClosedRound {
        round: get(tx, id)?,
        versements,
    })
}

fn map_stop(r: &Row<'_>) -> rusqlite::Result<Stop> {
    Ok(Stop {
        seq: r.get(0)?,
        document_id: r.get(1)?,
        document_number: r.get(2)?,
        doc_type: r.get(3)?,
        customer_id: r.get(4)?,
        customer_name: r.get(5)?,
        address: r.get(6)?,
        phone: r.get(7)?,
        amount_due: r.get(8)?,
        status: r.get(9)?,
        collected: r.get(10)?,
        method: r.get(11)?,
        reference: r.get(12)?,
        cheque_bank: r.get(13)?,
        cheque_date: r.get(14)?,
        receipt_number: r.get(15)?,
        note: r.get(16)?,
    })
}

fn stops(conn: &Connection, id: &str) -> Result<Vec<Stop>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT s.seq, s.document_id, d.number, d.doc_type, s.customer_id, c.name, c.address, c.phone,
                s.amount_due, s.status, s.collected, s.method, s.reference, s.cheque_bank, s.cheque_date,
                l.receipt_number, s.note
         FROM delivery_round_stops s
         JOIN documents d ON d.id = s.document_id
         LEFT JOIN customers c ON c.id = s.customer_id
         LEFT JOIN customer_ledger l ON l.id = s.ledger_entry_id
         WHERE s.round_id = ?1 ORDER BY s.seq",
    )?;
    let rows = stmt
        .query_map([id], map_stop)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn sum(stops: &[Stop], f: impl Fn(&Stop) -> Money) -> Result<Money, AppError> {
    Money::checked_sum(stops.iter().map(f))
        .ok_or_else(|| AppError::Internal("round total overflow".into()))
}

pub fn get(conn: &Connection, id: &str) -> Result<Round, AppError> {
    let round = conn
        .query_row(
            "SELECT r.id, r.number, r.business_date, r.driver_name, r.driver_user_id, r.vehicle_plate,
                    r.van_location_id, l.name_fr, r.status, r.note, r.started_at, r.closed_at
             FROM delivery_rounds r LEFT JOIN stock_locations l ON l.id = r.van_location_id
             WHERE r.id = ?1",
            [id],
            |r| {
                Ok(Round {
                    id: r.get(0)?,
                    number: r.get(1)?,
                    business_date: r.get(2)?,
                    driver_name: r.get(3)?,
                    driver_user_id: r.get(4)?,
                    vehicle_plate: r.get(5)?,
                    van_location_id: r.get(6)?,
                    van_location_name: r.get(7)?,
                    status: r.get(8)?,
                    note: r.get(9)?,
                    started_at: r.get(10)?,
                    closed_at: r.get(11)?,
                    stops: Vec::new(),
                    total_due: Money::ZERO,
                    total_collected: Money::ZERO,
                    cash_collected: Money::ZERO,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("round"))?;
    let stops = stops(conn, id)?;
    Ok(Round {
        total_due: sum(&stops, |s| s.amount_due)?,
        total_collected: sum(&stops, |s| s.collected)?,
        cash_collected: sum(&stops, |s| {
            if s.method == Some(PaymentMethod::Cash) {
                s.collected
            } else {
                Money::ZERO
            }
        })?,
        stops,
        ..round
    })
}

/// The rounds of the period (`business_date` from `from` to `to`), newest first, at most 500.
pub fn list(conn: &Connection, filter: &RoundFilter) -> Result<Vec<Round>, AppError> {
    let from = clean_day(Some(&filter.from), "from")?
        .ok_or_else(|| AppError::validation("from", "required"))?;
    let to =
        clean_day(Some(&filter.to), "to")?.ok_or_else(|| AppError::validation("to", "required"))?;
    let mut stmt = conn.prepare(
        "SELECT id FROM delivery_rounds
         WHERE business_date BETWEEN ?1 AND ?2 AND (?3 IS NULL OR status = ?3)
         ORDER BY business_date DESC, number DESC LIMIT ?4",
    )?;
    let ids = stmt
        .query_map(params![from, to, filter.status, LIST_LIMIT], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter().map(|id| get(conn, id)).collect()
}

/// A delivery van: a storage location of its own (STK-003) the rounds load and the till sells
/// from.
pub fn create_van(
    tx: &Transaction<'_>,
    actor: &Actor,
    code: &str,
    names: (&str, &str, &str),
) -> Result<StockLocation, AppError> {
    packs::require(tx, Module::DeliveryRounds)?;
    let (name_ar, name_fr, name_en) = names;
    let van = stock::create_location(
        tx,
        actor,
        LocationInput {
            code: code.to_owned(),
            name_ar: name_ar.to_owned(),
            name_fr: name_fr.to_owned(),
            name_en: name_en.to_owned(),
        },
    )?;
    audit::append(
        tx,
        &actor.event(
            "rounds.van_created",
            "stock_location",
            Some(&van.id),
            json!({ "code": van.code }),
        ),
    )?;
    Ok(van)
}

/// A location a van sale or a round may use: active, storage, not the shop's default location,
/// while the delivery rounds module is on (`locationId` / `van_location_invalid`).
pub(crate) fn van_location(conn: &Connection, id: &str) -> Result<StockLocation, AppError> {
    let invalid = || AppError::validation("locationId", "van_location_invalid");
    if !packs::enabled(conn, Module::DeliveryRounds)? {
        return Err(invalid());
    }
    let location = stock::location(conn, id).map_err(|_| invalid())?;
    if location.kind == LocationKind::Damaged || location.is_default {
        return Err(invalid());
    }
    Ok(location)
}

/// The van of a round, refused when the round has none.
fn round_van(
    conn: &Connection,
    id: &str,
    statuses: &[RoundStatus],
) -> Result<StockLocation, AppError> {
    let (status, van) = load_round(conn, id)?;
    if !statuses.contains(&status) {
        return Err(AppError::conflict("round_status_invalid"));
    }
    let van = van.ok_or_else(|| AppError::validation("vanLocationId", "van_location_invalid"))?;
    van_location(conn, &van)
}

/// The round's driver and plate, for the transfer documents.
fn driver_of(conn: &Connection, id: &str) -> Result<(String, Option<String>, String), AppError> {
    Ok(conn.query_row(
        "SELECT driver_name, vehicle_plate, number FROM delivery_rounds WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?)
}

/// Loads the van of a planned or out round from the default location (one M2 transfer, which
/// journals itself).
pub fn load(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    lines: Vec<TransferLine>,
) -> Result<TransferDone, AppError> {
    let van = round_van(tx, id, &[RoundStatus::Planned, RoundStatus::Out])?;
    let (driver, plate, number) = driver_of(tx, id)?;
    transfers::transfer(
        tx,
        ctx,
        TransferInput {
            from_location_id: stock::default_location(tx)?.id,
            to_location_id: van.id,
            lines,
            driver_name: Some(driver),
            vehicle: plate,
            note: Some(number),
        },
    )
}

/// Brings everything left on the round's van back to the default location in one M2 transfer
/// (any status); `None` when the van is empty.
pub fn unload(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
) -> Result<Option<TransferDone>, AppError> {
    let van = round_van(
        tx,
        id,
        &[RoundStatus::Planned, RoundStatus::Out, RoundStatus::Closed],
    )?;
    let left: Vec<(String, Qty)> = {
        let mut stmt = tx.prepare(
            "SELECT product_id, qty FROM stock_levels WHERE location_id = ?1 AND qty > 0 ORDER BY product_id",
        )?;
        stmt.query_map([&van.id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    if left.is_empty() {
        return Ok(None);
    }
    let mut lines = Vec::with_capacity(left.len());
    for (product_id, qty) in left {
        // The serial numbers in stock on the van travel back with their pieces.
        let pieces = qty.0 / Qty::ONE.0;
        let serials = {
            let mut stmt = tx.prepare_cached(
                "SELECT serial_no FROM serials
                 WHERE product_id = ?1 AND location_id = ?2 AND status = 'in_stock'
                 ORDER BY serial_no LIMIT ?3",
            )?;
            stmt.query_map(params![product_id, van.id, pieces], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
        };
        lines.push(TransferLine {
            product_id,
            qty,
            lot_number: None,
            serials,
        });
    }
    let (driver, plate, number) = driver_of(tx, id)?;
    Ok(Some(transfers::transfer(
        tx,
        ctx,
        TransferInput {
            from_location_id: van.id,
            to_location_id: stock::default_location(tx)?.id,
            lines,
            driver_name: Some(driver),
            vehicle: plate,
            note: Some(number),
        },
    )?))
}
