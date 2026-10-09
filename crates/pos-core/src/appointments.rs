//! The appointment book (ORD-009, lightweight): salon, gym, optician, workshop. An appointment
//! names the customer (a file or just a name and phone), the service, the staff member and the
//! time; a staff member is never booked twice for overlapping times. No reminders are sent: the
//! screen shows the day's list with the phone numbers to call. Phone numbers are stored for the
//! booking screen only and never enter a report.

use crate::audit;
use crate::catalog::clean_opt;
use crate::error::AppError;
use crate::ids::new_id;
use crate::packs::{self, Module};
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The shortest and longest appointment, in minutes (the schema's bounds).
const MIN_MINUTES: u16 = 5;
const MAX_MINUTES: u16 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentStatus {
    Booked,
    Done,
    Cancelled,
    NoShow,
}

impl AppointmentStatus {
    pub const ALL: [AppointmentStatus; 4] = [
        AppointmentStatus::Booked,
        AppointmentStatus::Done,
        AppointmentStatus::Cancelled,
        AppointmentStatus::NoShow,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            AppointmentStatus::Booked => "booked",
            AppointmentStatus::Done => "done",
            AppointmentStatus::Cancelled => "cancelled",
            AppointmentStatus::NoShow => "no_show",
        }
    }

    pub fn parse(s: &str) -> Option<AppointmentStatus> {
        AppointmentStatus::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(AppointmentStatus);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppointmentInput {
    /// The customer's file, when there is one.
    pub customer_id: Option<String>,
    pub customer_name: String,
    pub phone: Option<String>,
    pub service_product_id: Option<String>,
    pub staff_user_id: Option<String>,
    /// Unix ms.
    pub starts_at: i64,
    pub duration_min: u16,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Appointment {
    pub id: String,
    pub customer_id: Option<String>,
    pub customer_name: String,
    pub phone: Option<String>,
    pub service_product_id: Option<String>,
    /// In French (the screen names it in its own language from the product list).
    pub service_name: Option<String>,
    pub staff_user_id: Option<String>,
    pub staff_name: Option<String>,
    pub starts_at: i64,
    pub duration_min: u16,
    pub status: AppointmentStatus,
    pub note: Option<String>,
    pub created_at: i64,
}

const SELECT: &str = "SELECT a.id, a.customer_id, a.customer_name, a.phone, a.service_product_id,
        p.name_fr, a.staff_user_id, u.display_name, a.starts_at, a.duration_min, a.status, a.note,
        a.created_at
    FROM appointments a
    LEFT JOIN products p ON p.id = a.service_product_id
    LEFT JOIN users u ON u.id = a.staff_user_id";

fn map(r: &rusqlite::Row<'_>) -> rusqlite::Result<Appointment> {
    Ok(Appointment {
        id: r.get(0)?,
        customer_id: r.get(1)?,
        customer_name: r.get(2)?,
        phone: r.get(3)?,
        service_product_id: r.get(4)?,
        service_name: r.get(5)?,
        staff_user_id: r.get(6)?,
        staff_name: r.get(7)?,
        starts_at: r.get(8)?,
        duration_min: r.get(9)?,
        status: r.get(10)?,
        note: r.get(11)?,
        created_at: r.get(12)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Appointment, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE a.id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("appointment"))
}

/// The cleaned fields of an appointment: name, phone, note; the customer, service and staff
/// member checked; the length within 5 minutes to 10 hours; no other booked appointment of the
/// staff member overlapping (`except` is the appointment being changed).
struct Clean {
    name: String,
    phone: Option<String>,
    note: Option<String>,
}

fn check(
    tx: &Transaction<'_>,
    input: &AppointmentInput,
    except: Option<&str>,
) -> Result<Clean, AppError> {
    let name = clean_opt(Some(&input.customer_name), "customerName", 120)?
        .ok_or_else(|| AppError::validation("customerName", "required"))?;
    let phone = crate::party::clean_phone(input.phone.as_deref(), "phone")?;
    let note = clean_opt(input.note.as_deref(), "note", 200)?;
    if let Some(id) = &input.customer_id {
        crate::customers::selectable(tx, id)?;
    }
    if let Some(id) = &input.service_product_id {
        crate::catalog::products::get(tx, id)
            .map_err(|_| AppError::validation("serviceProductId", "product_not_found"))?;
    }
    if let Some(id) = &input.staff_user_id {
        crate::auth::users::get(tx, id)
            .map_err(|_| AppError::validation("staffUserId", "user_not_found"))?;
    }
    if !(MIN_MINUTES..=MAX_MINUTES).contains(&input.duration_min) {
        return Err(AppError::validation(
            "durationMin",
            "appointment_time_invalid",
        ));
    }
    let end = input
        .starts_at
        .checked_add(i64::from(input.duration_min) * 60_000)
        .ok_or_else(|| AppError::validation("startsAt", "appointment_time_invalid"))?;
    if let Some(staff) = &input.staff_user_id {
        let clash: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM appointments
                            WHERE staff_user_id = ?1 AND status = 'booked' AND id IS NOT ?4
                              AND starts_at < ?3 AND starts_at + duration_min * 60000 > ?2)",
            params![staff, input.starts_at, end, except],
            |r| r.get(0),
        )?;
        if clash {
            return Err(AppError::validation("startsAt", "appointment_overlap"));
        }
    }
    Ok(Clean { name, phone, note })
}

/// Books an appointment. One `appointments.booked` row.
pub fn book(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: AppointmentInput,
) -> Result<Appointment, AppError> {
    packs::require(tx, Module::Appointments)?;
    let c = check(tx, &input, None)?;
    let id = new_id();
    tx.execute(
        "INSERT INTO appointments (id, customer_id, customer_name, phone, service_product_id, staff_user_id,
                                   starts_at, duration_min, note, created_at, updated_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11)",
        params![
            id,
            input.customer_id,
            c.name,
            c.phone,
            input.service_product_id,
            input.staff_user_id,
            input.starts_at,
            input.duration_min,
            c.note,
            ctx.now,
            ctx.session.user_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "appointments.booked",
            "appointment",
            Some(&id),
            json!({
                "startsAt": input.starts_at,
                "durationMin": input.duration_min,
                "staffUserId": input.staff_user_id,
                "serviceProductId": input.service_product_id,
            }),
        ),
    )?;
    get(tx, &id)
}

/// Changes a booked appointment (a done, cancelled or missed one is final:
/// `appointment_time_invalid`). One `appointments.updated` row.
pub fn update(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    input: AppointmentInput,
) -> Result<Appointment, AppError> {
    packs::require(tx, Module::Appointments)?;
    let before = get(tx, id)?;
    if before.status != AppointmentStatus::Booked {
        return Err(AppError::validation("id", "appointment_time_invalid"));
    }
    let c = check(tx, &input, Some(id))?;
    tx.execute(
        "UPDATE appointments SET customer_id = ?2, customer_name = ?3, phone = ?4, service_product_id = ?5,
                                 staff_user_id = ?6, starts_at = ?7, duration_min = ?8, note = ?9,
                                 updated_at = ?10
         WHERE id = ?1",
        params![
            id,
            input.customer_id,
            c.name,
            c.phone,
            input.service_product_id,
            input.staff_user_id,
            input.starts_at,
            input.duration_min,
            c.note,
            ctx.now
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "appointments.updated",
            "appointment",
            Some(id),
            json!({
                "before": { "startsAt": before.starts_at, "durationMin": before.duration_min,
                            "staffUserId": before.staff_user_id },
                "after": { "startsAt": input.starts_at, "durationMin": input.duration_min,
                           "staffUserId": input.staff_user_id },
            }),
        ),
    )?;
    get(tx, id)
}

/// Marks a booked appointment done, cancelled or missed, once (`appointment_time_invalid` for
/// any other move). One `appointments.status_set` row.
pub fn set_status(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    status: AppointmentStatus,
) -> Result<Appointment, AppError> {
    packs::require(tx, Module::Appointments)?;
    let before = get(tx, id)?;
    if before.status != AppointmentStatus::Booked || status == AppointmentStatus::Booked {
        return Err(AppError::validation("status", "appointment_time_invalid"));
    }
    tx.execute(
        "UPDATE appointments SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status, ctx.now],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "appointments.status_set",
            "appointment",
            Some(id),
            json!({ "status": status }),
        ),
    )?;
    get(tx, id)
}

/// Every appointment of an Algiers day (`YYYY-MM-DD`), whatever its status, by time.
pub fn day(conn: &Connection, date: &str) -> Result<Vec<Appointment>, AppError> {
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("date", "date_invalid"))?;
    let (from, to) = crate::time::algiers_day_bounds(day);
    range(conn, from, to, None)
}

/// The appointments starting in `[from_ms, to_ms)`, of one staff member or everyone, by time.
pub fn range(
    conn: &Connection,
    from_ms: i64,
    to_ms: i64,
    staff_user_id: Option<&str>,
) -> Result<Vec<Appointment>, AppError> {
    let mut stmt = conn.prepare_cached(&format!(
        "{SELECT} WHERE a.starts_at >= ?1 AND a.starts_at < ?2
           AND (?3 IS NULL OR a.staff_user_id = ?3)
         ORDER BY a.starts_at, a.rowid"
    ))?;
    let rows = stmt
        .query_map(params![from_ms, to_ms, staff_user_id], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
