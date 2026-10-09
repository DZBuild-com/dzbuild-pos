//! The controlled-medicine register (tableau / psychotropes): one numbered entry per controlled
//! line of a sale, with the prescriber, the prescription and the patient — health data, so it is
//! written only with the pharmacy module on and `pharmacy.register`, read with the same right, and
//! its audit rows carry ids and counts only. Append-only, gap-free per calendar year, never deleted.

use crate::auth::Permission;
use crate::catalog::clean_opt;
use crate::documents::{Document, DocumentLine};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::Qty;
use crate::packs::{self, Module};
use crate::party::clean_day;
use crate::table::{Cell, ColumnKind as K, Table};
use crate::till::TillContext;
use chrono::Datelike;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The counter of the register's numbers (`counters`, one key per calendar year).
const COUNTER: &str = "controlled_register";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrescriptionInput {
    /// The cart line (0-based) of the controlled medicine.
    pub line: u32,
    pub prescriber_name: String,
    pub prescriber_ref: Option<String>,
    pub prescription_no: Option<String>,
    /// `YYYY-MM-DD`.
    pub prescription_date: String,
    pub patient_name: String,
    pub patient_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RegisterEntry {
    pub id: String,
    /// `2026-000012`.
    pub number: String,
    pub business_date: String,
    pub document_id: String,
    pub document_number: Option<String>,
    pub line_id: String,
    pub product_id: String,
    pub product_name: String,
    pub qty: Qty,
    pub prescriber_name: String,
    pub prescriber_ref: Option<String>,
    pub prescription_no: Option<String>,
    pub prescription_date: String,
    pub patient_name: String,
    pub patient_ref: Option<String>,
    pub created_at: i64,
    pub user_name: String,
}

/// One entry as the sale writes it down, before the register numbers it (a till's sale
/// sends its entries to the main PC, which keeps the register; elsewhere the sale writes them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlledRow {
    pub business_date: String,
    pub document_id: String,
    pub line_id: String,
    pub product_id: String,
    pub qty: Qty,
    pub prescriber_name: String,
    pub prescriber_ref: Option<String>,
    pub prescription_no: Option<String>,
    pub prescription_date: String,
    pub patient_name: String,
    pub patient_ref: Option<String>,
    pub created_at: i64,
    pub user_id: String,
    pub terminal_id: String,
}

pub(crate) fn controlled(conn: &Connection, product_id: &str) -> Result<bool, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM pharmacy_products WHERE product_id = ?1 AND controlled = 1)",
        )?
        .query_row([product_id], |r| r.get(0))?)
}

fn required(value: &str, field: &str, max: usize) -> Result<String, AppError> {
    clean_opt(Some(value), field, max)?.ok_or_else(|| AppError::validation(field, "required"))
}

/// The ticket line of cart line `i`.
fn line_of<'a>(
    document: &'a Document,
    main_line_ids: &[String],
    i: usize,
) -> Option<&'a DocumentLine> {
    main_line_ids
        .get(i)
        .and_then(|id| document.lines.iter().find(|l| &l.id == id))
}

/// The entries of a validated ticket (called by `sales::extensions::after_sale`), checked and
/// written down but not numbered yet: one per controlled line, each naming its cart line.
pub(crate) fn plan(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    document: &Document,
    main_line_ids: &[String],
    prescriptions: &[PrescriptionInput],
) -> Result<Vec<ControlledRow>, AppError> {
    if !packs::enabled(tx, Module::Pharmacy)? {
        if !prescriptions.is_empty() {
            return packs::require(tx, Module::Pharmacy).map(|()| Vec::new());
        }
        return Ok(Vec::new());
    }
    let mut needed: Vec<usize> = Vec::new();
    for i in 0..main_line_ids.len() {
        if let Some(pid) = line_of(document, main_line_ids, i).and_then(|l| l.product_id.as_deref())
            && controlled(tx, pid)?
        {
            needed.push(i);
        }
    }
    for (k, p) in prescriptions.iter().enumerate() {
        let line = p.line as usize;
        // An entry names a controlled line of the ticket, and a line has one entry.
        let named_before = prescriptions[..k].iter().any(|q| q.line == p.line);
        if !needed.contains(&line) || named_before {
            return Err(AppError::validation(
                &format!("prescriptions.{k}.line"),
                "value_invalid",
            ));
        }
    }
    let missing: Vec<usize> = needed
        .iter()
        .copied()
        .filter(|i| !prescriptions.iter().any(|p| p.line as usize == *i))
        .collect();
    if !missing.is_empty() {
        return Err(AppError::rejected(
            json!({ "lines": missing }),
            "register_entry_required",
        ));
    }
    if needed.is_empty() {
        return Ok(Vec::new());
    }
    if !ctx.allows(Permission::PharmacyRegister) {
        return Err(AppError::Permission {
            permission: Permission::PharmacyRegister.as_str().to_owned(),
        });
    }
    let mut rows = Vec::with_capacity(prescriptions.len());
    for (k, p) in prescriptions.iter().enumerate() {
        let f = |n: &str| format!("prescriptions.{k}.{n}");
        let Some(line) = line_of(document, main_line_ids, p.line as usize) else {
            continue;
        };
        let prescriber = required(&p.prescriber_name, &f("prescriberName"), 120)?;
        let patient = required(&p.patient_name, &f("patientName"), 120)?;
        let date = clean_day(Some(p.prescription_date.as_str()), &f("prescriptionDate"))?
            .ok_or_else(|| AppError::validation(&f("prescriptionDate"), "required"))?;
        let prescriber_ref = clean_opt(p.prescriber_ref.as_deref(), &f("prescriberRef"), 40)?;
        let prescription_no = clean_opt(p.prescription_no.as_deref(), &f("prescriptionNo"), 40)?;
        let patient_ref = clean_opt(p.patient_ref.as_deref(), &f("patientRef"), 40)?;
        rows.push(ControlledRow {
            business_date: ctx.business_day(),
            document_id: document.id.clone(),
            line_id: line.id.clone(),
            product_id: line.product_id.clone().unwrap_or_default(),
            qty: line.qty,
            prescriber_name: prescriber,
            prescriber_ref,
            prescription_no,
            prescription_date: date,
            patient_name: patient,
            patient_ref,
            created_at: ctx.now,
            user_id: ctx.session.user_id.clone(),
            terminal_id: ctx.session.terminal_id.clone(),
        });
    }
    Ok(rows)
}

/// Numbers and writes the entries of the ticket `document_id` (gap-free per calendar year of
/// their business date) and journals them (ids and counts only). Returns how many were written.
pub(crate) fn write(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    document_id: &str,
    rows: &[ControlledRow],
) -> Result<u32, AppError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let written = write_rows(tx, rows)?;
    crate::audit::append(
        tx,
        &ctx.actor().event(
            "pharmacy.register_recorded",
            "document",
            Some(document_id),
            json!({ "entries": written, "approvals": ctx.approvers() }),
        ),
    )?;
    Ok(written)
}

/// The entries numbered and written without their own audit row: the main PC's booking of a
/// till's ticket journals them in its one `registers.host_booked` row (final review).
pub(crate) fn write_rows(tx: &Transaction<'_>, rows: &[ControlledRow]) -> Result<u32, AppError> {
    for row in rows {
        let year = chrono::NaiveDate::parse_from_str(&row.business_date, "%Y-%m-%d")
            .map_err(|_| AppError::validation("businessDate", "value_invalid"))?
            .year();
        let seq = crate::numbering::next_number(tx, COUNTER, "", year)?;
        tx.execute(
            "INSERT INTO controlled_register (id, year, seq, business_date, document_id, line_id, product_id, qty,
                 prescriber_name, prescriber_ref, prescription_no, prescription_date, patient_name, patient_ref,
                 created_at, user_id, terminal_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                new_id(),
                year,
                seq,
                row.business_date,
                row.document_id,
                row.line_id,
                row.product_id,
                row.qty,
                row.prescriber_name,
                row.prescriber_ref,
                row.prescription_no,
                row.prescription_date,
                row.patient_name,
                row.patient_ref,
                row.created_at,
                row.user_id,
                row.terminal_id
            ],
        )?;
    }
    Ok(u32::try_from(rows.len()).unwrap_or(u32::MAX))
}

/// The entries of the business days `from`..=`to`, in register order.
pub fn entries(conn: &Connection, from: &str, to: &str) -> Result<Vec<RegisterEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.year, r.seq, r.business_date, r.document_id, d.number, r.line_id, r.product_id,
                COALESCE(NULLIF(p.name_fr, ''), NULLIF(p.name_ar, ''), p.name_en), r.qty, r.prescriber_name,
                r.prescriber_ref, r.prescription_no, r.prescription_date, r.patient_name, r.patient_ref,
                r.created_at, u.display_name
         FROM controlled_register r JOIN documents d ON d.id = r.document_id
         JOIN products p ON p.id = r.product_id JOIN users u ON u.id = r.user_id
         WHERE r.business_date BETWEEN ?1 AND ?2 ORDER BY r.year, r.seq",
    )?;
    let rows = stmt
        .query_map(params![from, to], |r| {
            let (year, seq): (i64, i64) = (r.get(1)?, r.get(2)?);
            Ok(RegisterEntry {
                id: r.get(0)?,
                number: format!("{year}-{seq:06}"),
                business_date: r.get(3)?,
                document_id: r.get(4)?,
                document_number: r.get(5)?,
                line_id: r.get(6)?,
                product_id: r.get(7)?,
                product_name: r.get(8)?,
                qty: r.get(9)?,
                prescriber_name: r.get(10)?,
                prescriber_ref: r.get(11)?,
                prescription_no: r.get(12)?,
                prescription_date: r.get(13)?,
                patient_name: r.get(14)?,
                patient_ref: r.get(15)?,
                created_at: r.get(16)?,
                user_name: r.get(17)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The register as a paged table (printed through M3's table print; names in their own script).
pub fn table(conn: &Connection, from: &str, to: &str, _lang: Lang) -> Result<Table, AppError> {
    table_of(entries(conn, from, to)?)
}

/// The register's table from its entries (a till builds it from the entries the main PC
/// answers, its own copy holding none).
pub fn table_of(entries: Vec<RegisterEntry>) -> Result<Table, AppError> {
    let mut t = Table::new(&[
        ("registerNo", K::Code),
        ("date", K::Date),
        ("documentNumber", K::Code),
        ("product", K::Text),
        ("qty", K::Qty),
        ("prescriber", K::Text),
        ("prescriptionNo", K::Code),
        ("prescriptionDate", K::Date),
        ("patient", K::Text),
    ]);
    for e in entries {
        t.push(vec![
            Cell::text(e.number),
            Cell::text(e.business_date),
            Cell::opt_text(e.document_number),
            Cell::text(e.product_name),
            Cell::qty(e.qty),
            Cell::text(match &e.prescriber_ref {
                Some(r) => format!("{} ({r})", e.prescriber_name),
                None => e.prescriber_name.clone(),
            }),
            Cell::opt_text(e.prescription_no),
            Cell::text(e.prescription_date),
            Cell::text(e.patient_name),
        ])?;
    }
    Ok(t)
}
