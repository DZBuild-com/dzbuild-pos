//! Deposits (acompte, arrhes — CRD-007) held for a customer order or a service ticket: taken with
//! their own numbered receipt `AC…` and the droit de timbre on their cash (TAX-007, M2's rule),
//! kept apart from the kridi account so they never settle old debts, spent only by their owner's
//! sale (a `voucher` tender naming the owner), refunded never beyond what is held. Append-only.
//!
//! LAN: what is held is a balance, so its writes belong to the host — `take` and
//! `refund` are single functions the app registers as host services; `apply` runs inside
//! the owner's sale.

use crate::audit;
use crate::cash::{self, DrawerShare, MovementKind};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OwnerType {
    CustomerOrder,
    ServiceTicket,
}

impl OwnerType {
    pub const fn as_str(self) -> &'static str {
        match self {
            OwnerType::CustomerOrder => "customer_order",
            OwnerType::ServiceTicket => "service_ticket",
        }
    }

    pub fn parse(s: &str) -> Option<OwnerType> {
        [OwnerType::CustomerOrder, OwnerType::ServiceTicket]
            .into_iter()
            .find(|o| o.as_str() == s)
    }

    /// The pack module whose owner this is: a deposit is taken only while it is on.
    const fn module(self) -> crate::packs::Module {
        match self {
            OwnerType::CustomerOrder => crate::packs::Module::Orders,
            OwnerType::ServiceTicket => crate::packs::Module::Repairs,
        }
    }
}

sql_text_enum!(OwnerType);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DepositKind {
    Deposit,
    Applied,
    Refund,
}

impl DepositKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            DepositKind::Deposit => "deposit",
            DepositKind::Applied => "applied",
            DepositKind::Refund => "refund",
        }
    }

    pub fn parse(s: &str) -> Option<DepositKind> {
        [
            DepositKind::Deposit,
            DepositKind::Applied,
            DepositKind::Refund,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

sql_text_enum!(DepositKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DepositInput {
    pub owner_type: OwnerType,
    pub owner_id: String,
    pub amount: Money,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DepositQuote {
    pub amount: Money,
    pub stamp_duty: Money,
    pub stamp_charged: Money,
    /// What the customer hands over: amount + charged duty.
    pub to_receive: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    pub id: String,
    pub owner_type: OwnerType,
    pub owner_id: String,
    pub customer_id: Option<String>,
    pub kind: DepositKind,
    /// Signed: money in (deposit) or out (applied, refund).
    pub amount: Money,
    pub method: Option<PaymentMethod>,
    pub reference: Option<String>,
    pub stamp_duty: Money,
    pub stamp_charged: Money,
    pub receipt_number: Option<String>,
    pub document_id: Option<String>,
    pub business_date: String,
    pub created_at: i64,
}

const SELECT: &str =
    "SELECT id, owner_type, owner_id, customer_id, kind, amount, method, reference, stamp_duty,
        stamp_charged, receipt_number, document_id, business_date, created_at FROM deposits";

fn map(r: &rusqlite::Row<'_>) -> rusqlite::Result<Deposit> {
    Ok(Deposit {
        id: r.get(0)?,
        owner_type: r.get(1)?,
        owner_id: r.get(2)?,
        customer_id: r.get(3)?,
        kind: r.get(4)?,
        amount: r.get(5)?,
        method: r.get(6)?,
        reference: r.get(7)?,
        stamp_duty: r.get(8)?,
        stamp_charged: r.get(9)?,
        receipt_number: r.get(10)?,
        document_id: r.get(11)?,
        business_date: r.get(12)?,
        created_at: r.get(13)?,
    })
}

fn get(conn: &Connection, id: &str) -> Result<Deposit, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("deposit"))
}

/// The owner's customer, when it can still take a deposit: an open order (validated, not
/// fulfilled nor closed) or a service ticket not closed.
fn live_owner(
    conn: &Connection,
    owner_type: OwnerType,
    owner_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<Option<String>> = match owner_type {
        OwnerType::CustomerOrder => conn
            .query_row(
                "SELECT d.customer_id FROM documents d JOIN customer_orders o ON o.document_id = d.id
                 WHERE d.id = ?1 AND d.status = 'validated' AND o.status IN ('open', 'ready')",
                [owner_id],
                |r| r.get(0),
            )
            .optional()?,
        OwnerType::ServiceTicket => conn
            .query_row(
                "SELECT customer_id FROM service_tickets WHERE id = ?1
                 AND status NOT IN ('delivered', 'returned_unrepaired', 'cancelled')",
                [owner_id],
                |r| r.get(0),
            )
            .optional()?,
    };
    row.ok_or_else(|| AppError::validation("ownerId", "deposit_owner_invalid"))
}

/// The owner behind a printed number (`CC…` of a customer order, `RP…` of a service ticket):
/// its id and customer.
pub fn owner_of_number(
    conn: &Connection,
    owner_type: OwnerType,
    number: &str,
) -> Result<Option<(String, Option<String>)>, AppError> {
    let sql = match owner_type {
        OwnerType::CustomerOrder => {
            "SELECT id, customer_id FROM documents WHERE doc_type = 'customer_order' AND number = ?1"
        }
        OwnerType::ServiceTicket => "SELECT id, customer_id FROM service_tickets WHERE number = ?1",
    };
    Ok(conn
        .query_row(sql, [number], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

/// The printed number of a deposit's owner, for its receipt: the `CC…` of a customer order or the
/// `RP…` of a service ticket (`order_not_found` / `service_ticket_not_found` otherwise).
pub fn owner_number(
    conn: &Connection,
    owner_type: OwnerType,
    owner_id: &str,
) -> Result<String, AppError> {
    let (sql, missing) = match owner_type {
        OwnerType::CustomerOrder => (
            "SELECT number FROM documents WHERE id = ?1 AND doc_type = 'customer_order'",
            "order",
        ),
        OwnerType::ServiceTicket => (
            "SELECT number FROM service_tickets WHERE id = ?1",
            "service_ticket",
        ),
    };
    conn.query_row(sql, [owner_id], |r| r.get::<_, Option<String>>(0))
        .optional()?
        .flatten()
        .ok_or_else(|| AppError::not_found(missing))
}

/// The name of a deposit's customer, for its receipt; `None` without a customer.
pub fn customer_name(
    conn: &Connection,
    customer_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(id) = customer_id else {
        return Ok(None);
    };
    Ok(conn
        .query_row("SELECT name FROM customers WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?)
}

/// What is held for the owner: the sum of its ledger.
pub fn held(conn: &Connection, owner_type: OwnerType, owner_id: &str) -> Result<Money, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT COALESCE(SUM(amount), 0) FROM deposits WHERE owner_type = ?1 AND owner_id = ?2",
        )?
        .query_row(params![owner_type, owner_id], |r| r.get(0))?)
}

pub fn list(
    conn: &Connection,
    owner_type: OwnerType,
    owner_id: &str,
) -> Result<Vec<Deposit>, AppError> {
    let mut stmt = conn.prepare_cached(&format!(
        "{SELECT} WHERE owner_type = ?1 AND owner_id = ?2 ORDER BY created_at, rowid"
    ))?;
    let rows = stmt
        .query_map(params![owner_type, owner_id], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The checked tender of a deposit: M2's versement rules (no kridi nor voucher, cheque details).
fn tender(input: &DepositInput) -> Result<PaymentInput, AppError> {
    crate::customers::credit::clean_versement(&crate::customers::credit::CollectInput {
        customer_id: String::new(),
        amount: input.amount,
        method: input.method,
        reference: input.reference.clone(),
        cheque_bank: input.cheque_bank.clone(),
        cheque_date: input.cheque_date.clone(),
        allocate_to: Vec::new(),
        note: None,
    })
}

/// The figures of a deposit before it is taken: the droit de timbre on its cash through M2's
/// `versement_duty` (`tax::timbre::for_cash_amount`), the part charged and what to receive.
pub fn quote(
    conn: &Connection,
    ctx: &TillContext,
    input: &DepositInput,
) -> Result<DepositQuote, AppError> {
    let t = tender(input)?;
    let (stamp_duty, stamp_charged, _) = crate::customers::credit::versement_duty(conn, ctx, &t)?;
    Ok(DepositQuote {
        amount: t.amount,
        stamp_duty,
        stamp_charged,
        to_receive: t
            .amount
            .checked_add(stamp_charged)
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?,
    })
}

/// Takes a deposit for a live owner: its `AC…` receipt, the duty, the cash in the drawer as a
/// `credit_payment`. One `deposit.taken` row.
pub fn take(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: DepositInput,
) -> Result<Deposit, AppError> {
    crate::packs::require(tx, input.owner_type.module())?;
    let customer_id = live_owner(tx, input.owner_type, &input.owner_id)?;
    let t = tender(&input)?;
    let q = quote(tx, ctx, &input)?;
    let note = crate::catalog::clean_opt(input.note.as_deref(), "note", 200)?;
    // The drawer that takes the cash: this till's, or the calling till's (Task 20 review).
    let session = if t.method.is_cash() {
        Some(cash::drawer(tx, ctx)?)
    } else {
        cash::drawer_if_open(tx, ctx)?
    };
    let receipt = crate::numbering::assign_custom(tx, "deposit", "AC", ctx.business_date())?;
    let id = new_id();
    tx.execute(
        "INSERT INTO deposits (id, owner_type, owner_id, customer_id, kind, amount, method, reference, cheque_bank,
                               cheque_date, stamp_duty, stamp_charged, receipt_number, cash_session_id, business_date,
                               note, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, 'deposit', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            id,
            input.owner_type,
            input.owner_id,
            customer_id,
            t.amount,
            t.method,
            t.reference,
            t.cheque_bank,
            t.cheque_date,
            q.stamp_duty,
            q.stamp_charged,
            receipt.number,
            session.as_ref().map(|s| s.id.clone()),
            ctx.business_day(),
            note,
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    let deposit = get(tx, &id)?;
    if let (Some(share), Some(s)) = (drawer_share(&deposit), &session) {
        cash::book(tx, ctx, &s.id, &share)?;
    }
    audit::append(
        tx,
        &ctx.actor().event(
            "deposit.taken",
            input.owner_type.as_str(),
            Some(&input.owner_id),
            json!({
                "depositId": id,
                "receiptNumber": receipt.number,
                "amount": t.amount,
                "method": t.method,
                "stampDuty": q.stamp_duty,
            }),
        ),
    )?;
    Ok(deposit)
}

/// Gives back `amount` of what is held (an order or a repair called off), never more. One
/// `deposit.refunded` row.
pub fn refund(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    owner_type: OwnerType,
    owner_id: &str,
    amount: Money,
    method: PaymentMethod,
    reference: Option<String>,
) -> Result<Deposit, AppError> {
    if !amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    if !matches!(
        method,
        PaymentMethod::Cash | PaymentMethod::Transfer | PaymentMethod::Cheque
    ) {
        return Err(AppError::validation("method", "payment_method_invalid"));
    }
    let reference = crate::catalog::clean_opt(reference.as_deref(), "reference", 48)?;
    let available = held(tx, owner_type, owner_id)?;
    if available < amount {
        return Err(AppError::rejected(
            json!({ "held": available }),
            "deposit_insufficient",
        ));
    }
    let out = amount
        .checked_neg()
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
    let customer_id: Option<String> = tx
        .query_row(
            "SELECT customer_id FROM deposits WHERE owner_type = ?1 AND owner_id = ?2 LIMIT 1",
            params![owner_type, owner_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let session = if method.is_cash() {
        Some(cash::drawer(tx, ctx)?)
    } else {
        None
    };
    let id = new_id();
    tx.execute(
        "INSERT INTO deposits (id, owner_type, owner_id, customer_id, kind, amount, method, reference, cash_session_id,
                               business_date, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, 'refund', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            owner_type,
            owner_id,
            customer_id,
            out,
            method,
            reference,
            session.as_ref().map(|s| s.id.clone()),
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    let refunded = get(tx, &id)?;
    if let (Some(share), Some(s)) = (drawer_share(&refunded), &session) {
        cash::book(tx, ctx, &s.id, &share)?;
    }
    audit::append(
        tx,
        &ctx.actor().event(
            "deposit.refunded",
            owner_type.as_str(),
            Some(owner_id),
            json!({ "depositId": id, "amount": amount, "method": method }),
        ),
    )?;
    Ok(refunded)
}

/// The drawer movement of a deposit taken or refunded in cash: what the customer handed over
/// (the amount and the duty charged) or what was given back. `None` for any other tender or a
/// deposit spent by its owner's sale. A host service for a till returns the deposit and the
/// till records this share in its own drawer (Task 20 review).
pub fn drawer_share(deposit: &Deposit) -> Option<DrawerShare> {
    if deposit.method != Some(PaymentMethod::Cash) {
        return None;
    }
    let (kind, amount) = match deposit.kind {
        DepositKind::Deposit => (
            MovementKind::CreditPayment,
            deposit.amount.checked_add(deposit.stamp_charged)?,
        ),
        DepositKind::Refund => (MovementKind::Refund, deposit.amount),
        DepositKind::Applied => return None,
    };
    Some(DrawerShare {
        kind,
        amount,
        ref_type: "deposit".into(),
        ref_id: deposit.id.clone(),
        note: None,
        category: None,
    })
}

/// The owner's sale spends `amount` of its deposits (the `voucher` tender, `vouchers::book`).
/// Journaled by the sale's own `sale.created` row, whose ticket lists the tender.
pub(crate) fn apply(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    owner_type: OwnerType,
    owner_id: &str,
    amount: Money,
    document_id: &str,
) -> Result<Deposit, AppError> {
    let id = new_id();
    tx.execute(
        "INSERT INTO deposits (id, owner_type, owner_id, kind, amount, method, document_id, business_date, created_at,
                               user_id, terminal_id)
         VALUES (?1, ?2, ?3, 'applied', ?4, 'voucher', ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            owner_type,
            owner_id,
            amount
                .checked_neg()
                .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?,
            document_id,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    get(tx, &id)
}
