//! Employees lite (master spec §26 EMP-002/003 lite): a profile per user (phone, hire date,
//! monthly salary, commission rate), salary advances paid out as M2 wages expenses and followed
//! in an append-only ledger until repaid in cash or kept off a salary, shifts read from the cash
//! sessions, commissions in the reports. No payroll, no payslips, no social declarations, no
//! clock-in (EMP-004 is V1).
//!
//! What an employee owes is the sum of their `staff_ledger` rows, never stored; the ledger's
//! trigger refuses a row that would take it below zero.
//!
//! LAN: the advances are a balance-bearing ledger, so [`advance`], [`repay`] and
//! [`deduct`] are host-only writes, each one function the app registers as a host service;
//! [`set_profile`] is shared master data. The cash of a till's advance or repayment is in that
//! till's drawer: the main PC books it on the till's open session and the till records
//! the movement from the answer ([`drawer_share`]).

use crate::audit::{self, Actor};
use crate::cash::{self, DrawerShare, ExpenseCategory, MovementKind};
use crate::catalog::clean_opt;
use crate::error::AppError;
use crate::expenses::{self, NewExpense};
use crate::ids::new_id;
use crate::money::Money;
use crate::packs::{self, Module};
use crate::payments::PaymentMethod;
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// M2's seeded `wages` expense category.
const WAGES_CATEGORY_ID: &str = "00000000-0000-7000-8000-000000000504";
/// The highest commission rate of a cashier (50 %), as the table allows.
const COMMISSION_MAX_BP: i32 = 5_000;
/// The longest note of a profile or a ledger entry.
const NOTE_MAX: usize = 200;
/// The most rows `entries` and `shifts` return.
const PAGE_MAX: i64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StaffEntryKind {
    /// Money given to the employee ahead of their salary (positive).
    Advance,
    /// Money the employee paid back (negative).
    Repayment,
    /// An advance kept off a month's salary (negative).
    SalaryDeduction,
    /// A correction, either sign.
    Adjustment,
}

impl StaffEntryKind {
    pub const ALL: [StaffEntryKind; 4] = [
        StaffEntryKind::Advance,
        StaffEntryKind::Repayment,
        StaffEntryKind::SalaryDeduction,
        StaffEntryKind::Adjustment,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            StaffEntryKind::Advance => "advance",
            StaffEntryKind::Repayment => "repayment",
            StaffEntryKind::SalaryDeduction => "salary_deduction",
            StaffEntryKind::Adjustment => "adjustment",
        }
    }

    pub fn parse(s: &str) -> Option<StaffEntryKind> {
        StaffEntryKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(StaffEntryKind);

/// What the owner keeps on an employee. The inputs and results of the writes serialize both
/// ways: a till's writes run on the main PC as host services.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    pub phone: Option<String>,
    /// `YYYY-MM-DD`.
    pub hire_date: Option<String>,
    pub monthly_salary: Option<Money>,
    /// Commission on the cashier's net sales HT, basis points (0–5 000).
    pub commission_bp: i32,
    pub note: Option<String>,
}

/// A user with their staff profile (every field empty or zero without one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StaffProfile {
    pub user_id: String,
    pub display_name: String,
    /// The user's role, in French.
    pub role_name: String,
    pub active: bool,
    pub phone: Option<String>,
    pub hire_date: Option<String>,
    pub monthly_salary: Option<Money>,
    pub commission_bp: i32,
    pub note: Option<String>,
    /// What the employee still owes on their advances.
    pub owed: Money,
}

/// A salary advance paid to an employee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceInput {
    pub user_id: String,
    pub amount: Money,
    pub method: PaymentMethod,
    /// Paid from the drawer (cash only): the wages expense leaves the open session.
    pub from_till: bool,
    pub note: Option<String>,
}

/// One row of an employee's advances ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StaffEntry {
    pub id: String,
    pub user_id: String,
    pub kind: StaffEntryKind,
    /// Signed as it moves what is owed: an advance positive, a repayment or deduction negative.
    pub amount: Money,
    pub method: Option<PaymentMethod>,
    /// The wages expense of an advance.
    pub expense_id: Option<String>,
    /// The salary month of a deduction, `YYYY-MM`.
    pub month: Option<String>,
    pub business_date: String,
    pub note: Option<String>,
    pub created_at: i64,
    pub created_by_name: String,
}

/// One cash session, as the shift of the user who opened it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShiftRow {
    pub user_id: String,
    pub name: String,
    pub session_id: String,
    pub terminal_code: String,
    pub opened_at: i64,
    pub closed_at: Option<i64>,
    /// From opening to closing, or to now for a session still open.
    pub minutes: i64,
    /// The session's validated tickets.
    pub tickets: i64,
    pub sales_ttc: Money,
}

const PROFILE_SELECT: &str =
    "SELECT u.id, u.display_name, r.name_fr, u.active, p.phone, p.hire_date,
        p.monthly_salary, COALESCE(p.commission_bp, 0), p.note,
        (SELECT COALESCE(SUM(l.amount), 0) FROM staff_ledger l WHERE l.user_id = u.id)
     FROM users u JOIN roles r ON r.id = u.role_id
     LEFT JOIN staff_profiles p ON p.user_id = u.id";

fn map_profile(r: &Row<'_>) -> rusqlite::Result<StaffProfile> {
    Ok(StaffProfile {
        user_id: r.get(0)?,
        display_name: r.get(1)?,
        role_name: r.get(2)?,
        active: r.get(3)?,
        phone: r.get(4)?,
        hire_date: r.get(5)?,
        monthly_salary: r.get(6)?,
        commission_bp: r.get(7)?,
        note: r.get(8)?,
        owed: r.get(9)?,
    })
}

fn profile(conn: &Connection, user_id: &str) -> Result<StaffProfile, AppError> {
    conn.query_row(
        &format!("{PROFILE_SELECT} WHERE u.id = ?1"),
        [user_id],
        map_profile,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("user"))
}

/// Every user, with or without a profile, active ones first.
pub fn list(conn: &Connection) -> Result<Vec<StaffProfile>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{PROFILE_SELECT} ORDER BY u.active DESC, u.display_name COLLATE NOCASE, u.id"
    ))?;
    let rows = stmt
        .query_map([], map_profile)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Writes the profile of `user_id` (created on first use). The journal records whether the
/// salary changed, never the salary itself.
pub fn set_profile(
    tx: &Transaction<'_>,
    actor: &Actor,
    user_id: &str,
    input: ProfileInput,
) -> Result<StaffProfile, AppError> {
    packs::require(tx, Module::Staff)?;
    crate::auth::users::get(tx, user_id)?;
    if !(0..=COMMISSION_MAX_BP).contains(&input.commission_bp) {
        return Err(AppError::validation("commissionBp", "staff_amount_invalid"));
    }
    if input.monthly_salary.is_some_and(Money::is_negative) {
        return Err(AppError::validation("monthlySalary", "amount_invalid"));
    }
    let hire_date = crate::party::clean_day(input.hire_date.as_deref(), "hireDate")?;
    let phone = crate::party::clean_phone(input.phone.as_deref(), "phone")?;
    let note = clean_opt(input.note.as_deref(), "note", NOTE_MAX)?;
    let before: Option<Option<Money>> = tx
        .query_row(
            "SELECT monthly_salary FROM staff_profiles WHERE user_id = ?1",
            [user_id],
            |r| r.get(0),
        )
        .optional()?;
    let salary_changed = before.flatten() != input.monthly_salary;
    tx.execute(
        "INSERT INTO staff_profiles (user_id, phone, hire_date, monthly_salary, commission_bp, note, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (user_id) DO UPDATE SET phone = excluded.phone, hire_date = excluded.hire_date,
             monthly_salary = excluded.monthly_salary, commission_bp = excluded.commission_bp,
             note = excluded.note, updated_at = excluded.updated_at",
        params![
            user_id,
            phone,
            hire_date,
            input.monthly_salary,
            input.commission_bp,
            note,
            crate::time::now_ms()
        ],
    )?;
    audit::append(
        tx,
        &actor.event(
            "staff.profile_set",
            "user",
            Some(user_id),
            json!({ "salaryChanged": salary_changed, "commissionBp": input.commission_bp }),
        ),
    )?;
    profile(tx, user_id)
}

/// What `user_id` still owes on their advances.
pub fn owed(conn: &Connection, user_id: &str) -> Result<Money, AppError> {
    Ok(conn
        .prepare_cached("SELECT COALESCE(SUM(amount), 0) FROM staff_ledger WHERE user_id = ?1")?
        .query_row([user_id], |r| r.get(0))?)
}

/// The cash, bank and card methods a wages payment or a repayment can use.
fn check_method(method: PaymentMethod) -> Result<(), AppError> {
    if matches!(
        method,
        PaymentMethod::Cash
            | PaymentMethod::CardCib
            | PaymentMethod::Edahabia
            | PaymentMethod::Baridimob
            | PaymentMethod::Cheque
            | PaymentMethod::Transfer
    ) {
        Ok(())
    } else {
        Err(AppError::validation("method", "payment_method_invalid"))
    }
}

/// The ledger's links of a row: the wages expense, the cash session and movement.
#[derive(Default)]
struct Links<'a> {
    expense_id: Option<&'a str>,
    cash_session_id: Option<&'a str>,
    cash_movement_id: Option<&'a str>,
}

#[allow(clippy::too_many_arguments)]
fn insert(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    user_id: &str,
    kind: StaffEntryKind,
    amount: Money,
    method: Option<PaymentMethod>,
    links: Links<'_>,
    month: Option<&str>,
    note: Option<&str>,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO staff_ledger (id, user_id, kind, amount, method, expense_id, cash_session_id, cash_movement_id,
                                   month, business_date, note, created_at, created_by, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            user_id,
            kind,
            amount,
            method,
            links.expense_id,
            links.cash_session_id,
            links.cash_movement_id,
            month,
            ctx.business_day(),
            note,
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    Ok(())
}

/// The amount of a repayment or a deduction: above zero and no more than what is owed.
fn check_reduction(tx: &Transaction<'_>, user_id: &str, amount: Money) -> Result<Money, AppError> {
    if !amount.is_positive() {
        return Err(AppError::validation("amount", "staff_amount_invalid"));
    }
    if amount > owed(tx, user_id)? {
        return Err(AppError::validation("amount", "advance_repay_too_much"));
    }
    amount
        .checked_neg()
        .ok_or_else(|| AppError::validation("amount", "staff_amount_invalid"))
}

/// Pays an advance: an M2 wages expense (out of the open drawer when `from_till`) and the
/// `advance` entry pointing at it, in the caller's transaction. Paid from a till's drawer through
/// the main PC (a host service), it is booked on the till's open session with no expense
/// row here: the till records the wages expense movement in its own drawer ([`drawer_share`]),
/// which the expenses report counts like any till expense.
pub fn advance(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: AdvanceInput,
) -> Result<StaffEntry, AppError> {
    packs::require(tx, Module::Staff)?;
    let user = crate::auth::users::get(tx, &input.user_id)?;
    if !input.amount.is_positive() {
        return Err(AppError::validation("amount", "staff_amount_invalid"));
    }
    check_method(input.method)?;
    if input.from_till && input.method != PaymentMethod::Cash {
        return Err(AppError::validation("method", "payment_method_invalid"));
    }
    let note = clean_opt(input.note.as_deref(), "note", NOTE_MAX)?;
    let id = new_id();
    let (expense_id, session_id, movement) = if input.from_till && !ctx.moves_drawer() {
        let session = cash::drawer(tx, ctx)?;
        (None, Some(session.id), None)
    } else {
        let expense_note = Some(match &note {
            Some(n) => format!("{} — {n}", user.display_name),
            None => user.display_name.clone(),
        });
        let expense = expenses::record(
            tx,
            ctx,
            NewExpense {
                category_id: WAGES_CATEGORY_ID.into(),
                amount: input.amount,
                method: input.method,
                from_till: input.from_till,
                reference: None,
                supplier_id: None,
                business_date: None,
                note: expense_note,
            },
        )?;
        let movement: Option<String> = tx.query_row(
            "SELECT cash_movement_id FROM expenses WHERE id = ?1",
            [&expense.id],
            |r| r.get(0),
        )?;
        (Some(expense.id), expense.cash_session_id, movement)
    };
    insert(
        tx,
        ctx,
        &id,
        &input.user_id,
        StaffEntryKind::Advance,
        input.amount,
        Some(input.method),
        Links {
            expense_id: expense_id.as_deref(),
            cash_session_id: session_id.as_deref(),
            cash_movement_id: movement.as_deref(),
        },
        None,
        note.as_deref(),
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "staff.advance_given",
            "user",
            Some(&input.user_id),
            json!({ "entryId": id, "amount": input.amount, "method": input.method, "expenseId": expense_id }),
        ),
    )?;
    entry(tx, &id)
}

/// The employee pays back `amount` of their advances; in cash it comes into the open drawer
/// as a `paid_in` referencing the entry (the calling till's, through the main PC).
pub fn repay(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    user_id: &str,
    amount: Money,
    method: PaymentMethod,
    note: Option<String>,
) -> Result<StaffEntry, AppError> {
    packs::require(tx, Module::Staff)?;
    crate::auth::users::get(tx, user_id)?;
    check_method(method)?;
    let signed = check_reduction(tx, user_id, amount)?;
    let note = clean_opt(note.as_deref(), "note", NOTE_MAX)?;
    let id = new_id();
    let (session, movement) = if method.is_cash() {
        let s = cash::drawer(tx, ctx)?;
        let share = DrawerShare {
            kind: MovementKind::PaidIn,
            amount,
            ref_type: "staff_ledger".into(),
            ref_id: id.clone(),
            note: note.clone(),
            category: None,
        };
        let m = cash::book(tx, ctx, &s.id, &share)?;
        (Some(s.id), m)
    } else {
        (None, None)
    };
    insert(
        tx,
        ctx,
        &id,
        user_id,
        StaffEntryKind::Repayment,
        signed,
        Some(method),
        Links {
            expense_id: None,
            cash_session_id: session.as_deref(),
            cash_movement_id: movement.as_deref(),
        },
        None,
        note.as_deref(),
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "staff.advance_repaid",
            "user",
            Some(user_id),
            json!({ "entryId": id, "amount": amount, "method": method }),
        ),
    )?;
    entry(tx, &id)
}

/// `amount` of the advances kept off the salary of `month` (`YYYY-MM`): no cash moves.
pub fn deduct(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    user_id: &str,
    amount: Money,
    month: &str,
    note: Option<String>,
) -> Result<StaffEntry, AppError> {
    packs::require(tx, Module::Staff)?;
    let month = month.trim();
    let valid = month.len() == 7
        && chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").is_ok();
    if !valid {
        return Err(AppError::validation("month", "month_invalid"));
    }
    crate::auth::users::get(tx, user_id)?;
    let signed = check_reduction(tx, user_id, amount)?;
    let note = clean_opt(note.as_deref(), "note", NOTE_MAX)?;
    let id = new_id();
    insert(
        tx,
        ctx,
        &id,
        user_id,
        StaffEntryKind::SalaryDeduction,
        signed,
        None,
        Links::default(),
        Some(month),
        note.as_deref(),
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "staff.salary_deducted",
            "user",
            Some(user_id),
            json!({ "entryId": id, "amount": amount, "month": month }),
        ),
    )?;
    entry(tx, &id)
}

const ENTRY_SELECT: &str =
    "SELECT l.id, l.user_id, l.kind, l.amount, l.method, l.expense_id, l.month,
        l.business_date, l.note, l.created_at, COALESCE(u.display_name, l.created_by)
     FROM staff_ledger l LEFT JOIN users u ON u.id = l.created_by";

fn map_entry(r: &Row<'_>) -> rusqlite::Result<StaffEntry> {
    Ok(StaffEntry {
        id: r.get(0)?,
        user_id: r.get(1)?,
        kind: r.get(2)?,
        amount: r.get(3)?,
        method: r.get(4)?,
        expense_id: r.get(5)?,
        month: r.get(6)?,
        business_date: r.get(7)?,
        note: r.get(8)?,
        created_at: r.get(9)?,
        created_by_name: r.get(10)?,
    })
}

/// One ledger entry.
/// The drawer movement of an advance paid from the till (`from_till`: a wages expense of the
/// wages category's cash group, CSH-002) or of a repayment in cash (`paid_in`); `None` for
/// anything else. A host service for a till answers the entry and the till records this share in
/// its own drawer; on a single PC it is what the write recorded.
pub fn drawer_share(
    conn: &Connection,
    entry: &StaffEntry,
    from_till: bool,
) -> Result<Option<DrawerShare>, AppError> {
    if entry.method != Some(PaymentMethod::Cash) {
        return Ok(None);
    }
    let share = |kind, amount, note: Option<String>, category| DrawerShare {
        kind,
        amount,
        ref_type: "staff_ledger".into(),
        ref_id: entry.id.clone(),
        note,
        category,
    };
    Ok(match entry.kind {
        StaffEntryKind::Advance if from_till => {
            let group: ExpenseCategory = expenses::category(conn, WAGES_CATEGORY_ID)?.cash_group;
            let out = entry
                .amount
                .checked_neg()
                .ok_or_else(|| AppError::validation("amount", "staff_amount_invalid"))?;
            Some(share(
                MovementKind::Expense,
                out,
                entry.note.clone(),
                Some(group),
            ))
        }
        StaffEntryKind::Repayment => {
            let back = entry
                .amount
                .checked_neg()
                .ok_or_else(|| AppError::validation("amount", "staff_amount_invalid"))?;
            Some(share(MovementKind::PaidIn, back, entry.note.clone(), None))
        }
        _ => None,
    })
}

pub fn entry(conn: &Connection, id: &str) -> Result<StaffEntry, AppError> {
    conn.query_row(&format!("{ENTRY_SELECT} WHERE l.id = ?1"), [id], map_entry)
        .optional()?
        .ok_or_else(|| AppError::Internal("staff entry missing after insert".into()))
}

/// The ledger of `user_id`, newest first.
pub fn entries(conn: &Connection, user_id: &str) -> Result<Vec<StaffEntry>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{ENTRY_SELECT} WHERE l.user_id = ?1 ORDER BY l.created_at DESC, l.id DESC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(params![user_id, PAGE_MAX], map_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// One row per cash session opened between `from` and `to` (business days, `YYYY-MM-DD`), of
/// `user_id` or of everyone, in opening order.
pub fn shifts(
    conn: &Connection,
    from: &str,
    to: &str,
    user_id: Option<&str>,
) -> Result<Vec<ShiftRow>, AppError> {
    let from = crate::party::clean_day(Some(from), "from")?
        .ok_or_else(|| AppError::validation("from", "date_invalid"))?;
    let to = crate::party::clean_day(Some(to), "to")?
        .ok_or_else(|| AppError::validation("to", "date_invalid"))?;
    let now = crate::time::now_ms();
    let mut stmt = conn.prepare(
        "SELECT s.user_id, u.display_name, s.id, t.code, s.opened_at, s.closed_at,
                (SELECT COUNT(*) FROM documents d
                 WHERE d.cash_session_id = s.id AND d.doc_type = 'ticket' AND d.status = 'validated'),
                (SELECT COALESCE(SUM(d.total_ttc), 0) FROM documents d
                 WHERE d.cash_session_id = s.id AND d.doc_type = 'ticket' AND d.status = 'validated')
         FROM cash_sessions s
         JOIN users u ON u.id = s.user_id
         JOIN terminals t ON t.id = s.terminal_id
         WHERE s.business_date BETWEEN ?1 AND ?2 AND (?3 IS NULL OR s.user_id = ?3)
         ORDER BY s.opened_at, s.id LIMIT ?4",
    )?;
    let rows = stmt
        .query_map(params![from, to, user_id, PAGE_MAX], |r| {
            let opened_at: i64 = r.get(4)?;
            let closed_at: Option<i64> = r.get(5)?;
            let minutes = closed_at.unwrap_or(now).saturating_sub(opened_at).max(0) / 60_000;
            Ok(ShiftRow {
                user_id: r.get(0)?,
                name: r.get(1)?,
                session_id: r.get(2)?,
                terminal_code: r.get(3)?,
                opened_at,
                closed_at,
                minutes,
                tickets: r.get(6)?,
                sales_ttc: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
