//! Rooms and tables (RST-001, RST-002): a room is a grid of at most 12 × 12 cells and a table sits
//! on one free cell; the board shows each table free, occupied or with its addition printed, and
//! the minutes since it was opened and since its last kitchen ticket.

use super::tickets::{DiningMode, TicketStatus};
use crate::audit::{self, Actor};
use crate::catalog::{clean_code, clean_names};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::sqlenum::sql_text_enum;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// Longest room code (`dining_rooms.code`).
const ROOM_CODE_MAX: usize = 12;
/// Longest table code (`dining_tables.code`): what fits on a floor tile and a kitchen ticket.
const TABLE_CODE_MAX: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TableShape {
    Square,
    Round,
    Rect,
}

impl TableShape {
    pub const fn as_str(self) -> &'static str {
        match self {
            TableShape::Square => "square",
            TableShape::Round => "round",
            TableShape::Rect => "rect",
        }
    }

    pub fn parse(s: &str) -> Option<TableShape> {
        [TableShape::Square, TableShape::Round, TableShape::Rect]
            .into_iter()
            .find(|t| t.as_str() == s)
    }
}

sql_text_enum!(TableShape);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub grid_cols: u8,
    pub grid_rows: u8,
    pub sort_order: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomInput {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub grid_cols: u8,
    pub grid_rows: u8,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiningTable {
    pub id: String,
    pub room_id: String,
    pub code: String,
    pub seats: u8,
    pub shape: TableShape,
    pub grid_col: u8,
    pub grid_row: u8,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TableInput {
    pub room_id: String,
    pub code: String,
    pub seats: u8,
    pub shape: TableShape,
    pub grid_col: u8,
    pub grid_row: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TableState {
    Free,
    Occupied,
    /// Every live ticket of the table has its addition printed.
    Billed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TicketSummary {
    pub id: String,
    pub order_no: Option<i64>,
    pub mode: DiningMode,
    pub table_id: Option<String>,
    pub table_code: Option<String>,
    pub label: Option<String>,
    pub covers: u8,
    pub waiter_name: String,
    pub total_ttc: Money,
    pub status: TicketStatus,
    pub opened_at: i64,
    pub minutes_open: i64,
    pub last_kitchen_at: Option<i64>,
    pub minutes_since_kitchen: Option<i64>,
    /// Kitchen tickets of this ticket not printed yet (a failed print waits for its reprint).
    pub unprinted_kitchen: u32,
    /// Its lines not sent to the kitchen yet, wholly or in part (final review: the floor marks
    /// them "À envoyer", so a forgotten F6 does not leave a table unserved).
    pub unsent_lines: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TableStatus {
    pub table: DiningTable,
    pub state: TableState,
    pub tickets: Vec<TicketSummary>,
}

const ROOM_SELECT: &str =
    "SELECT id, code, name_ar, name_fr, name_en, grid_cols, grid_rows, sort_order,
        archived_at IS NOT NULL FROM dining_rooms";
const TABLE_SELECT: &str = "SELECT id, room_id, code, seats, shape, grid_col, grid_row,
        archived_at IS NOT NULL FROM dining_tables";

fn map_room(r: &Row<'_>) -> rusqlite::Result<Room> {
    Ok(Room {
        id: r.get(0)?,
        code: r.get(1)?,
        name_ar: r.get(2)?,
        name_fr: r.get(3)?,
        name_en: r.get(4)?,
        grid_cols: r.get(5)?,
        grid_rows: r.get(6)?,
        sort_order: r.get(7)?,
        archived: r.get(8)?,
    })
}

fn map_table(r: &Row<'_>) -> rusqlite::Result<DiningTable> {
    Ok(DiningTable {
        id: r.get(0)?,
        room_id: r.get(1)?,
        code: r.get(2)?,
        seats: r.get(3)?,
        shape: r.get(4)?,
        grid_col: r.get(5)?,
        grid_row: r.get(6)?,
        archived: r.get(7)?,
    })
}

pub fn room(conn: &Connection, id: &str) -> Result<Room, AppError> {
    conn.prepare_cached(&format!("{ROOM_SELECT} WHERE id = ?1"))?
        .query_row([id], map_room)
        .optional()?
        .ok_or_else(|| AppError::not_found("dining_room"))
}

pub fn rooms(conn: &Connection, include_archived: bool) -> Result<Vec<Room>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{ROOM_SELECT} WHERE (?1 OR archived_at IS NULL) ORDER BY sort_order, code"
    ))?;
    let rows = stmt
        .query_map([include_archived], map_room)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn table(conn: &Connection, id: &str) -> Result<DiningTable, AppError> {
    conn.prepare_cached(&format!("{TABLE_SELECT} WHERE id = ?1"))?
        .query_row([id], map_table)
        .optional()?
        .ok_or_else(|| AppError::not_found("dining_table"))
}

pub fn tables(conn: &Connection, room_id: Option<&str>) -> Result<Vec<DiningTable>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{TABLE_SELECT} WHERE archived_at IS NULL AND (?1 IS NULL OR room_id = ?1)
         ORDER BY room_id, grid_row, grid_col"
    ))?;
    let rows = stmt
        .query_map([room_id], map_table)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn save_room(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: Option<&str>,
    input: RoomInput,
) -> Result<Room, AppError> {
    let code = clean_code(&input.code, "code")?;
    if code.chars().count() > ROOM_CODE_MAX {
        return Err(AppError::validation("code", "too_long"));
    }
    let (ar, fr, en) = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    if !(1..=12).contains(&input.grid_cols) || !(1..=12).contains(&input.grid_rows) {
        return Err(AppError::validation("gridCols", "value_invalid"));
    }
    let code_taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM dining_rooms WHERE code = ?1 AND archived_at IS NULL AND id <> ?2)",
        params![code, id.unwrap_or("")],
        |r| r.get(0),
    )?;
    if code_taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    let now = now_ms();
    let id = match id {
        Some(id) => {
            room(tx, id)?;
            let outside: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM dining_tables WHERE room_id = ?1 AND archived_at IS NULL
                                AND (grid_col >= ?2 OR grid_row >= ?3))",
                params![id, input.grid_cols, input.grid_rows],
                |r| r.get(0),
            )?;
            if outside {
                return Err(AppError::validation("gridCols", "table_cell_invalid"));
            }
            tx.execute(
                "UPDATE dining_rooms SET code = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5, grid_cols = ?6,
                                         grid_rows = ?7, sort_order = ?8, updated_at = ?9 WHERE id = ?1",
                params![id, code, ar, fr, en, input.grid_cols, input.grid_rows, input.sort_order, now],
            )?;
            id.to_owned()
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO dining_rooms (id, code, name_ar, name_fr, name_en, grid_cols, grid_rows, sort_order,
                                           created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                params![id, code, ar, fr, en, input.grid_cols, input.grid_rows, input.sort_order, now],
            )?;
            id
        }
    };
    audit::append(
        tx,
        &actor.event(
            "dining.room_saved",
            "dining_room",
            Some(&id),
            json!({ "code": code }),
        ),
    )?;
    room(tx, &id)
}

fn busy_tables(conn: &Connection, filter: &str, id: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM dining_tickets t JOIN dining_tables d ON d.id = t.table_id
                            WHERE t.status IN ('open', 'billed') AND {filter} = ?1)"
        ),
        [id],
        |r| r.get(0),
    )?)
}

pub fn archive_room(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    room(tx, id)?;
    if busy_tables(tx, "d.room_id", id)? {
        return Err(AppError::conflict("table_busy"));
    }
    let now = now_ms();
    tx.execute("UPDATE dining_tables SET archived_at = ?2, updated_at = ?2 WHERE room_id = ?1 AND archived_at IS NULL", params![id, now])?;
    tx.execute(
        "UPDATE dining_rooms SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    audit::append(
        tx,
        &actor.event("dining.room_archived", "dining_room", Some(id), json!({})),
    )?;
    Ok(())
}

pub fn save_table(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: Option<&str>,
    input: TableInput,
) -> Result<DiningTable, AppError> {
    let r = room(tx, &input.room_id)
        .ok()
        .filter(|r| !r.archived)
        .ok_or_else(|| AppError::validation("roomId", "dining_room_not_found"))?;
    let code = clean_code(&input.code, "code")?;
    if code.chars().count() > TABLE_CODE_MAX {
        return Err(AppError::validation("code", "too_long"));
    }
    if !(1..=40).contains(&input.seats) {
        return Err(AppError::validation("seats", "value_invalid"));
    }
    if input.grid_col >= r.grid_cols || input.grid_row >= r.grid_rows {
        return Err(AppError::validation("gridCol", "table_cell_invalid"));
    }
    let other = id.unwrap_or("");
    let cell_taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM dining_tables WHERE room_id = ?1 AND grid_col = ?2 AND grid_row = ?3
                        AND archived_at IS NULL AND id <> ?4)",
        params![input.room_id, input.grid_col, input.grid_row, other],
        |r| r.get(0),
    )?;
    if cell_taken {
        return Err(AppError::validation("gridCol", "table_cell_taken"));
    }
    let code_taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM dining_tables WHERE lower(code) = lower(?1) AND archived_at IS NULL AND id <> ?2)",
        params![code, other],
        |r| r.get(0),
    )?;
    if code_taken {
        return Err(AppError::validation("code", "table_code_taken"));
    }
    let now = now_ms();
    let id = match id {
        Some(id) => {
            table(tx, id)?;
            tx.execute(
                "UPDATE dining_tables SET room_id = ?2, code = ?3, seats = ?4, shape = ?5, grid_col = ?6,
                                          grid_row = ?7, updated_at = ?8 WHERE id = ?1",
                params![id, input.room_id, code, input.seats, input.shape, input.grid_col, input.grid_row, now],
            )?;
            id.to_owned()
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO dining_tables (id, room_id, code, seats, shape, grid_col, grid_row, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![id, input.room_id, code, input.seats, input.shape, input.grid_col, input.grid_row, now],
            )?;
            id
        }
    };
    audit::append(
        tx,
        &actor.event(
            "dining.table_saved",
            "dining_table",
            Some(&id),
            json!({ "code": code, "roomId": input.room_id }),
        ),
    )?;
    table(tx, &id)
}

pub fn archive_table(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    table(tx, id)?;
    if busy_tables(tx, "d.id", id)? {
        return Err(AppError::conflict("table_busy"));
    }
    tx.execute(
        "UPDATE dining_tables SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event("dining.table_archived", "dining_table", Some(id), json!({})),
    )?;
    Ok(())
}

const SUMMARY: &str =
    "SELECT t.id, t.order_no, t.mode, t.table_id, d.code, t.label, t.covers, u.display_name,
        t.total_ttc, t.status, t.opened_at,
        (SELECT MAX(k.created_at) FROM kitchen_tickets k WHERE k.ticket_id = t.id),
        (SELECT COUNT(*) FROM kitchen_tickets k WHERE k.ticket_id = t.id AND k.printed_at IS NULL),
        (SELECT COUNT(*) FROM dining_lines l WHERE l.ticket_id = t.id AND l.parent_line_id IS NULL
                AND l.status = 'active' AND l.sent_qty < l.qty)
     FROM dining_tickets t JOIN users u ON u.id = t.waiter_id
     LEFT JOIN dining_tables d ON d.id = t.table_id
     WHERE t.status IN ('open', 'billed')";

fn summaries(conn: &Connection, extra: &str, now: i64) -> Result<Vec<TicketSummary>, AppError> {
    let mut stmt = conn.prepare(&format!("{SUMMARY} {extra} ORDER BY t.opened_at"))?;
    let minutes = |since: i64| (now - since).max(0) / 60_000;
    let rows = stmt
        .query_map([], |r| {
            let opened_at: i64 = r.get(10)?;
            let last: Option<i64> = r.get(11)?;
            Ok(TicketSummary {
                id: r.get(0)?,
                order_no: r.get(1)?,
                mode: r.get(2)?,
                table_id: r.get(3)?,
                table_code: r.get(4)?,
                label: r.get(5)?,
                covers: r.get(6)?,
                waiter_name: r.get(7)?,
                total_ttc: r.get(8)?,
                status: r.get(9)?,
                opened_at,
                minutes_open: minutes(opened_at),
                last_kitchen_at: last,
                minutes_since_kitchen: last.map(minutes),
                unprinted_kitchen: r.get(12)?,
                unsent_lines: r.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every live table of `room_id` (or of every room) with its tickets (RST-002).
pub fn board(
    conn: &Connection,
    room_id: Option<&str>,
    now: i64,
) -> Result<Vec<TableStatus>, AppError> {
    let live = summaries(conn, "AND t.table_id IS NOT NULL", now)?;
    Ok(tables(conn, room_id)?
        .into_iter()
        .map(|table| {
            let tickets: Vec<TicketSummary> = live
                .iter()
                .filter(|s| s.table_id.as_deref() == Some(table.id.as_str()))
                .cloned()
                .collect();
            let state = if tickets.is_empty() {
                TableState::Free
            } else if tickets.iter().all(|t| t.status == TicketStatus::Billed) {
                TableState::Billed
            } else {
                TableState::Occupied
            };
            TableStatus {
                table,
                state,
                tickets,
            }
        })
        .collect())
}

/// The live tickets without a table: takeaway, delivery and predefined tickets ("Comptoir").
pub fn counter(conn: &Connection, now: i64) -> Result<Vec<TicketSummary>, AppError> {
    summaries(conn, "AND t.table_id IS NULL", now)
}

/// The restaurant template's room: "Salle", 4 × 3 cells, tables T1–T8 on the first two rows.
pub fn seed_room(tx: &Transaction<'_>, actor: &Actor) -> Result<u32, AppError> {
    let any: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM dining_rooms)", [], |r| {
        r.get(0)
    })?;
    if any {
        return Ok(0);
    }
    let room = save_room(
        tx,
        actor,
        None,
        RoomInput {
            code: "SALLE".into(),
            name_ar: "القاعة".into(),
            name_fr: "Salle".into(),
            name_en: "Dining room".into(),
            grid_cols: 4,
            grid_rows: 3,
            sort_order: 1,
        },
    )?;
    for i in 0..8_u8 {
        save_table(
            tx,
            actor,
            None,
            TableInput {
                room_id: room.id.clone(),
                code: format!("T{}", i + 1),
                seats: 4,
                shape: if i % 2 == 0 {
                    TableShape::Square
                } else {
                    TableShape::Round
                },
                grid_col: i % 4,
                grid_row: i / 4,
            },
        )?;
    }
    Ok(9)
}
