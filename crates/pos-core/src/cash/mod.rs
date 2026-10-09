//! Cash sessions (spec §7.3; SAL-001, CSH-001/002): one open session per terminal, opened with
//! a counted float; every cash in or out of the drawer is an append-only movement.
//! Closing, the X and Z reports are in [`closing`].

use crate::audit;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::payments::PaymentMethod;
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

pub mod closing;
#[cfg(test)]
mod drawer_tests;

pub use closing::{
    CloseInput, Closed, CountLine, SessionSummary, ZReport, close_session,
    close_session_with_backup, x_report, z_report,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Open,
    Closed,
}

impl SessionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Open => "open",
            SessionStatus::Closed => "closed",
        }
    }

    pub fn parse(s: &str) -> Option<SessionStatus> {
        [SessionStatus::Open, SessionStatus::Closed]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(SessionStatus);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum MovementKind {
    Float,
    Sale,
    Refund,
    PaidIn,
    PaidOut,
    Expense,
    CreditPayment,
    /// Cash taken from the drawer to the safe.
    Drop,
}

impl MovementKind {
    pub const ALL: [MovementKind; 8] = [
        MovementKind::Float,
        MovementKind::Sale,
        MovementKind::Refund,
        MovementKind::PaidIn,
        MovementKind::PaidOut,
        MovementKind::Expense,
        MovementKind::CreditPayment,
        MovementKind::Drop,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            MovementKind::Float => "float",
            MovementKind::Sale => "sale",
            MovementKind::Refund => "refund",
            MovementKind::PaidIn => "paid_in",
            MovementKind::PaidOut => "paid_out",
            MovementKind::Expense => "expense",
            MovementKind::CreditPayment => "credit_payment",
            MovementKind::Drop => "drop",
        }
    }

    pub fn parse(s: &str) -> Option<MovementKind> {
        MovementKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(MovementKind);

/// Till expense categories (CSH-002).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseCategory {
    Rent,
    Electricity,
    Water,
    Wages,
    Transport,
    Supplies,
    Maintenance,
    Other,
}

impl ExpenseCategory {
    pub const ALL: [ExpenseCategory; 8] = [
        ExpenseCategory::Rent,
        ExpenseCategory::Electricity,
        ExpenseCategory::Water,
        ExpenseCategory::Wages,
        ExpenseCategory::Transport,
        ExpenseCategory::Supplies,
        ExpenseCategory::Maintenance,
        ExpenseCategory::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ExpenseCategory::Rent => "rent",
            ExpenseCategory::Electricity => "electricity",
            ExpenseCategory::Water => "water",
            ExpenseCategory::Wages => "wages",
            ExpenseCategory::Transport => "transport",
            ExpenseCategory::Supplies => "supplies",
            ExpenseCategory::Maintenance => "maintenance",
            ExpenseCategory::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<ExpenseCategory> {
        ExpenseCategory::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

sql_text_enum!(ExpenseCategory);

/// Manual cash movements of the cashier (CSH-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ManualKind {
    PaidIn,
    PaidOut,
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CashSession {
    pub id: String,
    pub terminal_id: String,
    pub user_id: String,
    pub business_date: String,
    pub opened_at: i64,
    pub opening_float: Money,
    pub closed_at: Option<i64>,
    pub closed_by: Option<String>,
    pub expected_cash: Option<Money>,
    pub counted_cash: Option<Money>,
    pub variance: Option<Money>,
    pub status: SessionStatus,
    pub z_number: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CashMovement {
    pub id: String,
    pub session_id: String,
    pub kind: MovementKind,
    pub method: PaymentMethod,
    /// Signed: positive into the drawer, negative out of it.
    pub amount: Money,
    pub category: Option<String>,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
    pub user_id: String,
}

const SESSION_SELECT: &str =
    "SELECT id, terminal_id, user_id, business_date, opened_at, opening_float,
        closed_at, closed_by, expected_cash, counted_cash, variance, status, z_number, notes
     FROM cash_sessions";

fn map_session(r: &Row<'_>) -> rusqlite::Result<CashSession> {
    Ok(CashSession {
        id: r.get(0)?,
        terminal_id: r.get(1)?,
        user_id: r.get(2)?,
        business_date: r.get(3)?,
        opened_at: r.get(4)?,
        opening_float: r.get(5)?,
        closed_at: r.get(6)?,
        closed_by: r.get(7)?,
        expected_cash: r.get(8)?,
        counted_cash: r.get(9)?,
        variance: r.get(10)?,
        status: r.get(11)?,
        z_number: r.get(12)?,
        notes: r.get(13)?,
    })
}

pub fn get_session(conn: &Connection, id: &str) -> Result<CashSession, AppError> {
    conn.prepare_cached(&format!("{SESSION_SELECT} WHERE id = ?1"))?
        .query_row([id], map_session)
        .optional()?
        .ok_or_else(|| AppError::not_found("cash_session"))
}

/// The open session of a terminal, if any.
pub fn current(conn: &Connection, terminal_id: &str) -> Result<Option<CashSession>, AppError> {
    Ok(conn
        .prepare_cached(&format!(
            "{SESSION_SELECT} WHERE terminal_id = ?1 AND status = 'open'"
        ))?
        .query_row([terminal_id], map_session)
        .optional()?)
}

/// The open session of a terminal, or `cash_session_closed` ("Open a cash session to sell").
pub fn require_open(conn: &Connection, terminal_id: &str) -> Result<CashSession, AppError> {
    current(conn, terminal_id)?.ok_or_else(|| AppError::conflict("cash_session_closed"))
}

/// Sessions of a terminal, newest first (history screen, `cash.view_history`).
pub fn list_sessions(
    conn: &Connection,
    terminal_id: &str,
    limit: u32,
) -> Result<Vec<CashSession>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{SESSION_SELECT} WHERE terminal_id = ?1 ORDER BY opened_at DESC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(
            params![terminal_id, i64::from(limit.clamp(1, 500))],
            map_session,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// A new movement in an open session (the trigger refuses a closed one). Used by sales,
/// returns and the manual movements below; the caller audits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    session_id: &str,
    kind: MovementKind,
    amount: Money,
    category: Option<ExpenseCategory>,
    reference: Option<(&str, &str)>,
    note: Option<&str>,
) -> Result<String, AppError> {
    let id = new_id();
    tx.prepare_cached(
        "INSERT INTO cash_movements (id, session_id, kind, method, amount, category, ref_type, ref_id, note,
                                     created_at, user_id)
         VALUES (?1, ?2, ?3, 'cash', ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?
    .execute(params![
        id,
        session_id,
        kind,
        amount,
        category.map(ExpenseCategory::as_str),
        reference.map(|r| r.0),
        reference.map(|r| r.1),
        note,
        ctx.now,
        ctx.session.user_id
    ])?;
    Ok(id)
}

/// The open session that takes the money of `ctx`'s operation (`cash_session_closed` without
/// one): this PC's, or the calling till's for a host service run here
/// ([`TillContext::drawer_till`]).
pub fn drawer(conn: &Connection, ctx: &TillContext) -> Result<CashSession, AppError> {
    require_open(conn, ctx.drawer_terminal())
}

/// [`drawer`] when it is open (a non-cash operation books on it when there is one).
pub fn drawer_if_open(
    conn: &Connection,
    ctx: &TillContext,
) -> Result<Option<CashSession>, AppError> {
    current(conn, ctx.drawer_terminal())
}

/// One drawer movement of an operation, as the PC whose drawer it is records it: the cash of a
/// ticket or an invoice (`sale`), of a deposit or a versement (`credit_payment`), of a gift
/// voucher sold or an advance repaid (`paid_in`), a refund (`refund`), float paid from the till
/// (`paid_out`) or a salary advance paid from it (`expense`, with its category). A host service
/// run on the main PC for a till reports it in its answer instead of recording it
/// ([`TillContext::drawer_till`]); the till records it in its own copy with [`record_share`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawerShare {
    pub kind: MovementKind,
    /// Signed: positive into the drawer, negative out of it.
    pub amount: Money,
    pub ref_type: String,
    pub ref_id: String,
    pub note: Option<String>,
    /// The till expense category of an `expense` (CSH-002); `None` for every other kind.
    #[serde(default)]
    pub category: Option<ExpenseCategory>,
}

impl DrawerShare {
    /// The cash a ticket or an invoice kept, net of the change; `None` when it took none.
    pub fn of_settlement(
        document_id: &str,
        settlement: &crate::payments::Settlement,
    ) -> Result<Option<DrawerShare>, AppError> {
        let net_cash = settlement
            .cash_tendered
            .checked_sub(settlement.change)
            .ok_or_else(|| AppError::validation("payments", "amount_invalid"))?;
        Ok(net_cash.is_positive().then(|| DrawerShare {
            kind: MovementKind::Sale,
            amount: net_cash,
            ref_type: "document".into(),
            ref_id: document_id.to_owned(),
            note: None,
            category: None,
        }))
    }

    /// Money in (a sale, a deposit, a versement, a voucher sold, an advance repaid) is positive,
    /// money out (a refund, float paid, an advance paid) negative; only an expense has a category
    /// and it must have one; no other kind is an operation's share.
    fn is_valid(&self) -> bool {
        let direction = match self.kind {
            MovementKind::Sale | MovementKind::CreditPayment | MovementKind::PaidIn => {
                self.amount.is_positive()
            }
            MovementKind::Refund | MovementKind::PaidOut | MovementKind::Expense => {
                self.amount.is_negative()
            }
            _ => false,
        };
        direction && (self.kind == MovementKind::Expense) == self.category.is_some()
    }
}

/// Records `share` of `ctx`'s operation in `session_id` when this PC holds its drawer; a host
/// service run for a till records nothing (`None`): the till does, from the answer.
pub(crate) fn book(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    session_id: &str,
    share: &DrawerShare,
) -> Result<Option<String>, AppError> {
    if !ctx.moves_drawer() {
        return Ok(None);
    }
    if !share.is_valid() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    record(
        tx,
        ctx,
        session_id,
        share.kind,
        share.amount,
        share.category,
        Some((&share.ref_type, &share.ref_id)),
        share.note.as_deref(),
    )
    .map(Some)
}

/// The till's half of a host service (Task 20 review): the money the main PC booked for this
/// till goes into this till's open drawer (`cash_session_closed` without one), as the main PC
/// would have recorded it in its own. One `cash.host_share` row.
pub fn record_share(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    share: &DrawerShare,
) -> Result<CashMovement, AppError> {
    if !share.is_valid() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let session = require_open(tx, &ctx.session.terminal_id)?;
    let id = record(
        tx,
        ctx,
        &session.id,
        share.kind,
        share.amount,
        share.category,
        Some((&share.ref_type, &share.ref_id)),
        share.note.as_deref(),
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "cash.host_share",
            "cash_session",
            Some(&session.id),
            json!({
                "kind": share.kind,
                "amount": share.amount,
                "refType": share.ref_type,
                "refId": share.ref_id,
            }),
        ),
    )?;
    movement_by_id(tx, &id)
}

/// Opens the terminal's session with the counted float (SAL-001); one per terminal
/// (`cash_session_already_open`).
pub fn open_session(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    opening_float: Money,
) -> Result<CashSession, AppError> {
    if opening_float.is_negative() {
        return Err(AppError::validation("openingFloat", "amount_invalid"));
    }
    if current(tx, &ctx.session.terminal_id)?.is_some() {
        return Err(AppError::conflict("cash_session_already_open"));
    }
    let id = new_id();
    tx.execute(
        "INSERT INTO cash_sessions (id, terminal_id, user_id, business_date, opened_at, opening_float)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, ctx.session.terminal_id, ctx.session.user_id, ctx.business_day(), ctx.now, opening_float],
    )?;
    record(
        tx,
        ctx,
        &id,
        MovementKind::Float,
        opening_float,
        None,
        None,
        None,
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "cash.session_opened",
            "cash_session",
            Some(&id),
            json!({ "openingFloat": opening_float }),
        ),
    )?;
    get_session(tx, &id)
}

fn clean_note(note: Option<&str>, required: bool) -> Result<Option<String>, AppError> {
    let note = crate::catalog::clean_opt(note, "note", 200)?;
    if required && note.is_none() {
        return Err(AppError::validation("note", "required"));
    }
    Ok(note)
}

/// Paid in, paid out or a drop to the safe, with the reason written by the cashier (CSH-001).
pub fn move_cash(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    kind: ManualKind,
    amount: Money,
    note: Option<&str>,
) -> Result<CashMovement, AppError> {
    if !amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let session = require_open(tx, &ctx.session.terminal_id)?;
    let note = clean_note(note, true)?;
    let (movement, signed, action) = match kind {
        ManualKind::PaidIn => (MovementKind::PaidIn, amount, "cash.paid_in"),
        ManualKind::PaidOut => (
            MovementKind::PaidOut,
            amount.checked_neg().unwrap_or(Money::ZERO),
            "cash.paid_out",
        ),
        ManualKind::Drop => (
            MovementKind::Drop,
            amount.checked_neg().unwrap_or(Money::ZERO),
            "cash.dropped",
        ),
    };
    let id = record(
        tx,
        ctx,
        &session.id,
        movement,
        signed,
        None,
        None,
        note.as_deref(),
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            action,
            "cash_session",
            Some(&session.id),
            json!({ "amount": amount, "note": note }),
        ),
    )?;
    movement_by_id(tx, &id)
}

/// A till expense by category (CSH-002, FIN-001 till side).
pub fn expense(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    category: ExpenseCategory,
    amount: Money,
    note: Option<&str>,
) -> Result<CashMovement, AppError> {
    if !amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let session = require_open(tx, &ctx.session.terminal_id)?;
    let note = clean_note(note, false)?;
    let signed = amount.checked_neg().unwrap_or(Money::ZERO);
    let id = record(
        tx,
        ctx,
        &session.id,
        MovementKind::Expense,
        signed,
        Some(category),
        None,
        note.as_deref(),
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "cash.expense",
            "cash_session",
            Some(&session.id),
            json!({ "category": category, "amount": amount, "note": note }),
        ),
    )?;
    movement_by_id(tx, &id)
}

const MOVEMENT_SELECT: &str =
    "SELECT id, session_id, kind, method, amount, category, ref_type, ref_id, note,
        created_at, user_id FROM cash_movements";

fn map_movement(r: &Row<'_>) -> rusqlite::Result<CashMovement> {
    Ok(CashMovement {
        id: r.get(0)?,
        session_id: r.get(1)?,
        kind: r.get(2)?,
        method: r.get(3)?,
        amount: r.get(4)?,
        category: r.get(5)?,
        ref_type: r.get(6)?,
        ref_id: r.get(7)?,
        note: r.get(8)?,
        created_at: r.get(9)?,
        user_id: r.get(10)?,
    })
}

fn movement_by_id(conn: &Connection, id: &str) -> Result<CashMovement, AppError> {
    conn.query_row(
        &format!("{MOVEMENT_SELECT} WHERE id = ?1"),
        [id],
        map_movement,
    )
    .optional()?
    .ok_or_else(|| AppError::Internal("cash movement missing after insert".into()))
}

/// The movements of a session in order.
pub fn movements(conn: &Connection, session_id: &str) -> Result<Vec<CashMovement>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{MOVEMENT_SELECT} WHERE session_id = ?1 ORDER BY created_at, rowid"
    ))?;
    let rows = stmt
        .query_map([session_id], map_movement)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Cash that should be in the drawer: float + cash sales − refunds + paid in − paid out −
/// expenses − drops (the sum of the session's movements).
pub fn expected_cash(conn: &Connection, session_id: &str) -> Result<Money, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM cash_movements WHERE session_id = ?1",
        [session_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn one_session_per_terminal_with_its_float() {
        let shop = shop();
        let session = shop.open_till(500_000);
        assert_eq!(
            (
                session.status,
                session.opening_float,
                session.business_date.as_str()
            ),
            (SessionStatus::Open, Money(500_000), "2026-09-28")
        );
        let c = shop.till();
        let err = shop
            .db
            .write(move |tx| open_session(tx, &c, Money::ZERO))
            .unwrap_err();
        assert_eq!(err.code(), "cash_session_already_open");
        let terminal = shop.terminal.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| require_open(c, &terminal))
                .unwrap()
                .id,
            session.id
        );
        let id = session.id.clone();
        assert_eq!(
            shop.db.read(move |c| expected_cash(c, &id)).unwrap(),
            Money(500_000)
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'cash.session_opened'"),
            1
        );
    }

    #[test]
    fn nothing_moves_without_an_open_session() {
        let shop = shop();
        let terminal = shop.terminal.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| require_open(c, &terminal))
                .unwrap_err()
                .code(),
            "cash_session_closed"
        );
        let c = shop.till();
        let err = shop
            .db
            .write(move |tx| move_cash(tx, &c, ManualKind::PaidIn, Money(100), Some("x")))
            .unwrap_err();
        assert_eq!(err.code(), "cash_session_closed");
    }

    #[test]
    fn manual_movements_and_expenses_change_the_expected_cash() {
        let shop = shop();
        let session = shop.open_till(200_000);
        let c = shop.till();
        shop.db
            .write(move |tx| {
                move_cash(
                    tx,
                    &c,
                    ManualKind::PaidIn,
                    Money(50_000),
                    Some("Monnaie de la banque"),
                )?;
                move_cash(
                    tx,
                    &c,
                    ManualKind::PaidOut,
                    Money(30_000),
                    Some("Fournisseur pain"),
                )?;
                move_cash(tx, &c, ManualKind::Drop, Money(100_000), Some("Coffre"))?;
                expense(tx, &c, ExpenseCategory::Electricity, Money(15_000), None)?;
                Ok(())
            })
            .unwrap();
        let id = session.id.clone();
        assert_eq!(
            shop.db.read(move |c| expected_cash(c, &id)).unwrap(),
            Money(105_000)
        );
        let id = session.id.clone();
        let kinds: Vec<MovementKind> = shop
            .db
            .read(move |c| movements(c, &id))
            .unwrap()
            .into_iter()
            .map(|m| m.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                MovementKind::Float,
                MovementKind::PaidIn,
                MovementKind::PaidOut,
                MovementKind::Drop,
                MovementKind::Expense
            ]
        );
        let c = shop.till();
        let err = shop
            .db
            .write(move |tx| move_cash(tx, &c, ManualKind::PaidOut, Money(100), Some("  ")))
            .unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            ("required".into(), Some("note".into()))
        );
        let c = shop.till();
        let err = shop
            .db
            .write(move |tx| expense(tx, &c, ExpenseCategory::Rent, Money::ZERO, None))
            .unwrap_err();
        assert_eq!(err.code(), "amount_invalid");
        let actions: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT group_concat(action) FROM (SELECT action FROM audit_log WHERE action LIKE 'cash.%' ORDER BY seq)",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            actions,
            "cash.session_opened,cash.paid_in,cash.paid_out,cash.dropped,cash.expense"
        );
    }
}
