//! Kitchen and bar tickets (RST-006, HW-004): a station ("Cuisine", "Grill", "Bar") takes the
//! categories mapped to it; sending a table ticket prints, per station, what the kitchen has not
//! received yet (`qty − sent_qty` of each dish, its options underneath), and a void of something
//! already sent prints a cancellation ticket on the same station. Tickets are append-only; the
//! print path sets `printed_at` once. Printers per station are per PC (hardware.json, Task 18).

use super::tickets::{self, DiningMode, Ticket, TicketLine};
use crate::audit::{self, Actor};
use crate::catalog::{clean_code, clean_names, products};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::Qty;
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;

/// Longest station code (`kitchen_stations.code`).
const STATION_CODE_MAX: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub sort_order: i64,
    pub archived: bool,
    pub category_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StationInput {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub sort_order: i64,
    pub category_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum KitchenKind {
    Order,
    Cancel,
}

impl KitchenKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            KitchenKind::Order => "order",
            KitchenKind::Cancel => "cancel",
        }
    }

    pub fn parse(s: &str) -> Option<KitchenKind> {
        [KitchenKind::Order, KitchenKind::Cancel]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

sql_text_enum!(KitchenKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KitchenLine {
    pub name: String,
    pub qty: Qty,
    pub unit_code: String,
    pub note: Option<String>,
    pub seat: Option<u8>,
    pub course: Option<u8>,
    /// Priced options sold with the dish, printed under it.
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KitchenTicket {
    pub id: String,
    pub ticket_id: String,
    pub station_id: Option<String>,
    pub station_name: Option<String>,
    pub seq: i64,
    pub kind: KitchenKind,
    pub lines: Vec<KitchenLine>,
    pub mode: DiningMode,
    pub order_no: Option<i64>,
    pub table_code: Option<String>,
    pub label: Option<String>,
    pub covers: u8,
    pub waiter_name: String,
    pub created_at: i64,
    pub printed_at: Option<i64>,
}

const SELECT: &str = "SELECT k.id, k.ticket_id, k.station_id, COALESCE(NULLIF(s.name_fr, ''), s.name_ar, s.name_en),
        k.seq, k.kind, k.lines, t.mode, t.order_no, d.code, t.label, t.covers, u.display_name, k.created_at,
        k.printed_at
     FROM kitchen_tickets k JOIN dining_tickets t ON t.id = k.ticket_id JOIN users u ON u.id = t.waiter_id
     LEFT JOIN kitchen_stations s ON s.id = k.station_id LEFT JOIN dining_tables d ON d.id = t.table_id";

fn map(r: &Row<'_>) -> rusqlite::Result<(KitchenTicket, String)> {
    Ok((
        KitchenTicket {
            id: r.get(0)?,
            ticket_id: r.get(1)?,
            station_id: r.get(2)?,
            station_name: r.get(3)?,
            seq: r.get(4)?,
            kind: r.get(5)?,
            lines: Vec::new(),
            mode: r.get(7)?,
            order_no: r.get(8)?,
            table_code: r.get(9)?,
            label: r.get(10)?,
            covers: r.get(11)?,
            waiter_name: r.get(12)?,
            created_at: r.get(13)?,
            printed_at: r.get(14)?,
        },
        r.get(6)?,
    ))
}

fn query(conn: &Connection, where_: &str, arg: &str) -> Result<Vec<KitchenTicket>, AppError> {
    let mut stmt = conn.prepare(&format!("{SELECT} {where_} ORDER BY k.created_at, k.seq"))?;
    let raw = stmt.query_map([arg], map)?.collect::<Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(|(mut k, lines)| {
            k.lines = serde_json::from_str(&lines)?;
            Ok(k)
        })
        .collect()
}

pub fn get(conn: &Connection, id: &str) -> Result<KitchenTicket, AppError> {
    query(conn, "WHERE k.id = ?1", id)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::not_found("kitchen_ticket"))
}

pub fn of_ticket(conn: &Connection, ticket_id: &str) -> Result<Vec<KitchenTicket>, AppError> {
    query(conn, "WHERE k.ticket_id = ?1", ticket_id)
}

/// Tickets this terminal made that were never printed (a printer was off): the till retries them.
pub fn pending(conn: &Connection, terminal_id: &str) -> Result<Vec<KitchenTicket>, AppError> {
    query(
        conn,
        "WHERE k.printed_at IS NULL AND k.terminal_id = ?1",
        terminal_id,
    )
}

pub fn mark_printed(tx: &Transaction<'_>, ids: &[String], now: i64) -> Result<u32, AppError> {
    let mut done = 0;
    for id in ids {
        done += u32::try_from(tx.execute(
            "UPDATE kitchen_tickets SET printed_at = ?2 WHERE id = ?1 AND printed_at IS NULL",
            params![id, now],
        )?)
        .unwrap_or(0);
    }
    Ok(done)
}

fn station_of_product(conn: &Connection, product_id: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT cs.station_id FROM products p JOIN category_stations cs ON cs.category_id = p.category_id
             JOIN kitchen_stations s ON s.id = cs.station_id AND s.archived_at IS NULL WHERE p.id = ?1",
        )?
        .query_row([product_id], |r| r.get(0))
        .optional()?)
}

fn line_name(
    conn: &Connection,
    line: &TicketLine,
    lang: Lang,
) -> Result<(String, String), AppError> {
    match &line.product_id {
        Some(pid) => {
            let p = products::get(conn, pid)?;
            Ok((p.name_in(lang).to_owned(), p.unit_code))
        }
        None => Ok((
            line.cart_line.description.clone().unwrap_or_default(),
            String::new(),
        )),
    }
}

fn insert(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket: &Ticket,
    station: Option<&str>,
    kind: KitchenKind,
    lines: &[KitchenLine],
) -> Result<String, AppError> {
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM kitchen_tickets WHERE ticket_id = ?1",
        [&ticket.id],
        |r| r.get(0),
    )?;
    let id = new_id();
    tx.execute(
        "INSERT INTO kitchen_tickets (id, ticket_id, station_id, seq, kind, lines, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            ticket.id,
            station,
            seq,
            kind,
            serde_json::to_string(lines)?,
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    Ok(id)
}

/// Sends what the kitchen has not received yet, one ticket per station (RST-006).
pub fn send(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket_id: &str,
) -> Result<Vec<KitchenTicket>, AppError> {
    let ticket = tickets::get(tx, ticket_id)?;
    tickets::require_live(&ticket)?;
    let lang: Lang = crate::settings::get(tx, "restaurant.kitchen_lang")?;
    let mut groups: BTreeMap<Option<String>, Vec<KitchenLine>> = BTreeMap::new();
    let mut sent: Vec<(String, Qty)> = Vec::new();
    for line in ticket.lines.iter().filter(|l| l.parent_line_id.is_none()) {
        let delta = line.qty.checked_sub(line.sent_qty).unwrap_or(Qty::ZERO);
        if !delta.is_positive() {
            continue;
        }
        let (name, unit_code) = line_name(tx, line, lang)?;
        let mut options = Vec::new();
        for child in ticket
            .lines
            .iter()
            .filter(|c| c.parent_line_id.as_deref() == Some(line.id.as_str()))
        {
            options.push(line_name(tx, child, lang)?.0);
            sent.push((child.id.clone(), child.qty));
        }
        let station = match &line.product_id {
            Some(pid) => station_of_product(tx, pid)?,
            None => None,
        };
        groups.entry(station).or_default().push(KitchenLine {
            name,
            qty: delta,
            unit_code,
            note: line.cart_line.note.clone(),
            seat: line.seat,
            course: line.course,
            options,
        });
        sent.push((line.id.clone(), line.qty));
    }
    if groups.is_empty() {
        return Err(AppError::conflict("kitchen_nothing_to_send"));
    }
    let mut ids = Vec::new();
    for (station, lines) in &groups {
        ids.push(insert(
            tx,
            ctx,
            &ticket,
            station.as_deref(),
            KitchenKind::Order,
            lines,
        )?);
    }
    for (line_id, qty) in &sent {
        tx.execute(
            "UPDATE dining_lines SET sent_qty = ?2, updated_at = ?3 WHERE id = ?1",
            params![line_id, qty, ctx.now],
        )?;
    }
    audit::append(
        tx,
        &ctx.actor().event(
            "dining.kitchen_sent",
            "dining_ticket",
            Some(ticket_id),
            json!({ "tickets": ids.len(), "lines": sent.len() }),
        ),
    )?;
    ids.iter().map(|id| get(tx, id)).collect()
}

/// A cancellation ticket for `qty` of `line` already received by its station.
pub(crate) fn cancel(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    ticket: &Ticket,
    line: &TicketLine,
    qty: Qty,
) -> Result<KitchenTicket, AppError> {
    let lang: Lang = crate::settings::get(tx, "restaurant.kitchen_lang")?;
    let (name, unit_code) = line_name(tx, line, lang)?;
    let station = match &line.product_id {
        Some(pid) => station_of_product(tx, pid)?,
        None => None,
    };
    let lines = [KitchenLine {
        name,
        qty,
        unit_code,
        note: line.cart_line.note.clone(),
        seat: line.seat,
        course: line.course,
        options: Vec::new(),
    }];
    let id = insert(
        tx,
        ctx,
        ticket,
        station.as_deref(),
        KitchenKind::Cancel,
        &lines,
    )?;
    get(tx, &id)
}

pub fn stations(conn: &Connection, include_archived: bool) -> Result<Vec<Station>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, code, name_ar, name_fr, name_en, sort_order, archived_at IS NOT NULL FROM kitchen_stations
         WHERE (?1 OR archived_at IS NULL) ORDER BY sort_order, code",
    )?;
    let mut out = stmt
        .query_map([include_archived], |r| {
            Ok(Station {
                id: r.get(0)?,
                code: r.get(1)?,
                name_ar: r.get(2)?,
                name_fr: r.get(3)?,
                name_en: r.get(4)?,
                sort_order: r.get(5)?,
                archived: r.get(6)?,
                category_ids: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut cats = conn.prepare_cached(
        "SELECT category_id FROM category_stations WHERE station_id = ?1 ORDER BY category_id",
    )?;
    for s in &mut out {
        s.category_ids = cats
            .query_map([&s.id], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(out)
}

pub fn save_station(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: Option<&str>,
    input: StationInput,
) -> Result<Station, AppError> {
    let code = clean_code(&input.code, "code")?;
    // What the table's CHECK and unique index would refuse with a bare `constraint_violation`
    // is said on the field first.
    if code.chars().count() > STATION_CODE_MAX {
        return Err(AppError::validation("code", "too_long"));
    }
    let (ar, fr, en) = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    let code_taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM kitchen_stations WHERE code = ?1 AND archived_at IS NULL AND id <> ?2)",
        params![code, id.unwrap_or("")],
        |r| r.get(0),
    )?;
    if code_taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    let now = now_ms();
    let id = match id {
        Some(id) => {
            let changed = tx.execute(
                "UPDATE kitchen_stations SET code = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5, sort_order = ?6,
                                             updated_at = ?7 WHERE id = ?1",
                params![id, code, ar, fr, en, input.sort_order, now],
            )?;
            if changed == 0 {
                return Err(AppError::not_found("kitchen_station"));
            }
            id.to_owned()
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO kitchen_stations (id, code, name_ar, name_fr, name_en, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![id, code, ar, fr, en, input.sort_order, now],
            )?;
            id
        }
    };
    tx.execute("DELETE FROM category_stations WHERE station_id = ?1", [&id])?;
    for (i, category) in input.category_ids.iter().enumerate() {
        crate::catalog::categories::get(tx, category)
            .map_err(|_| AppError::validation(&format!("categoryIds.{i}"), "category_not_found"))?;
        tx.execute(
            "INSERT INTO category_stations (category_id, station_id, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (category_id) DO UPDATE SET station_id = excluded.station_id, updated_at = excluded.updated_at",
            params![category, id, now],
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "kitchen_station.saved",
            "kitchen_station",
            Some(&id),
            json!({ "code": code, "categories": input.category_ids }),
        ),
    )?;
    stations(tx, true)?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| AppError::not_found("kitchen_station"))
}

pub fn archive_station(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let changed = tx.execute(
        "UPDATE kitchen_stations SET archived_at = ?2, updated_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
        params![id, now_ms()],
    )?;
    if changed == 0 {
        return Err(AppError::not_found("kitchen_station"));
    }
    tx.execute("DELETE FROM category_stations WHERE station_id = ?1", [id])?;
    audit::append(
        tx,
        &actor.event(
            "kitchen_station.archived",
            "kitchen_station",
            Some(id),
            json!({}),
        ),
    )?;
    Ok(())
}

/// The template's stations: "Cuisine" for dishes, sandwiches, pizzas and desserts, "Bar" for
/// drinks — mapped by the seeded categories' French names when they exist.
pub fn seed_stations(tx: &Transaction<'_>, actor: &Actor) -> Result<u32, AppError> {
    let any: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM kitchen_stations)", [], |r| {
        r.get(0)
    })?;
    if any {
        return Ok(0);
    }
    let ids_of = |names: &[&str]| -> Result<Vec<String>, AppError> {
        let mut out = Vec::new();
        for n in names {
            if let Some(id) = tx
                .query_row(
                    "SELECT id FROM categories WHERE lower(name_fr) = lower(?1) AND archived_at IS NULL",
                    [n],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                out.push(id);
            }
        }
        Ok(out)
    };
    save_station(
        tx,
        actor,
        None,
        StationInput {
            code: "CUISINE".into(),
            name_ar: "المطبخ".into(),
            name_fr: "Cuisine".into(),
            name_en: "Kitchen".into(),
            sort_order: 1,
            category_ids: ids_of(&["Plats", "Sandwichs", "Pizzas", "Desserts"])?,
        },
    )?;
    save_station(
        tx,
        actor,
        None,
        StationInput {
            code: "BAR".into(),
            name_ar: "البار".into(),
            name_fr: "Bar".into(),
            name_en: "Bar".into(),
            sort_order: 2,
            category_ids: ids_of(&["Boissons chaudes", "Boissons fraîches"])?,
        },
    )?;
    Ok(2)
}
