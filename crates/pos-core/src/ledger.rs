//! Append-only party ledgers (spec §5.1 `customer_ledger`, `supplier_ledger`; NFR-013;
//! CRD-001/003/004/005/006, PUR-006/007). One shape for customers (positive = the customer owes
//! the shop; negative = store credit or an advance) and suppliers (positive = the shop owes the
//! supplier). The cached balance of the party moves in the INSERT statement (migration 0003
//! triggers refuse a `balance_after` that does not follow it) and `rebuild_balances` repairs it.
//! Every entry is allocated at once: a credit (payment, refund, return) settles the chosen debts
//! first, then the oldest; a new debt first consumes any advance left open.
//!
//! The SQL of both sides is built at compile time from the table names (`concat!`): no value is
//! ever formatted into SQL.

use crate::audit::{self, Actor};
use crate::catalog::clean_opt;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::payments::PaymentMethod;
use crate::sqlenum::sql_text_enum;
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Customer,
    Supplier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    SaleCredit,
    VoucherRedeemed,
    Purchase,
    Payment,
    Refund,
    #[serde(rename = "return")]
    SupplierReturn,
    Adjustment,
    Opening,
}

impl EntryKind {
    pub const ALL: [EntryKind; 8] = [
        EntryKind::SaleCredit,
        EntryKind::VoucherRedeemed,
        EntryKind::Purchase,
        EntryKind::Payment,
        EntryKind::Refund,
        EntryKind::SupplierReturn,
        EntryKind::Adjustment,
        EntryKind::Opening,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            EntryKind::SaleCredit => "sale_credit",
            EntryKind::VoucherRedeemed => "voucher_redeemed",
            EntryKind::Purchase => "purchase",
            EntryKind::Payment => "payment",
            EntryKind::Refund => "refund",
            EntryKind::SupplierReturn => "return",
            EntryKind::Adjustment => "adjustment",
            EntryKind::Opening => "opening",
        }
    }

    pub fn parse(s: &str) -> Option<EntryKind> {
        EntryKind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    fn allowed_on(self, side: Side) -> bool {
        match side {
            Side::Customer => matches!(
                self,
                EntryKind::SaleCredit
                    | EntryKind::VoucherRedeemed
                    | EntryKind::Payment
                    | EntryKind::Refund
                    | EntryKind::Adjustment
                    | EntryKind::Opening
            ),
            Side::Supplier => matches!(
                self,
                EntryKind::Purchase
                    | EntryKind::Payment
                    | EntryKind::SupplierReturn
                    | EntryKind::Adjustment
                    | EntryKind::Opening
            ),
        }
    }

    fn sign_ok(self, amount: Money) -> bool {
        match self {
            EntryKind::SaleCredit | EntryKind::VoucherRedeemed | EntryKind::Purchase => {
                amount.is_positive()
            }
            EntryKind::Payment | EntryKind::Refund | EntryKind::SupplierReturn => {
                amount.is_negative()
            }
            EntryKind::Adjustment | EntryKind::Opening => amount != Money::ZERO,
        }
    }
}

sql_text_enum!(EntryKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    pub id: String,
    pub party_id: String,
    pub kind: EntryKind,
    /// Signed: positive increases what the party owes (customer) or what is owed to it
    /// (supplier).
    pub amount: Money,
    pub balance_after: Money,
    pub method: Option<PaymentMethod>,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub stamp_duty: Money,
    pub stamp_charged: Money,
    pub receipt_number: Option<String>,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    /// The number of the referenced document (`TK…`, `FF…`) when there is one.
    pub ref_number: Option<String>,
    pub cash_session_id: Option<String>,
    pub business_date: String,
    pub note: Option<String>,
    pub created_at: i64,
    pub user_id: Option<String>,
    pub terminal_id: Option<String>,
    /// What is still unallocated (always ≥ 0).
    pub open: Money,
}

#[cfg(any(test, feature = "testkit"))]
impl LedgerEntry {
    /// An entry of `amount` on `business_date`, every other field empty or zero (statement
    /// tests of pos-docs, feature `testkit`).
    pub fn for_tests(
        id: &str,
        party_id: &str,
        kind: EntryKind,
        amount: Money,
        business_date: &str,
    ) -> LedgerEntry {
        LedgerEntry {
            id: id.to_owned(),
            party_id: party_id.to_owned(),
            kind,
            amount,
            balance_after: Money::ZERO,
            method: None,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            stamp_duty: Money::ZERO,
            stamp_charged: Money::ZERO,
            receipt_number: None,
            ref_type: None,
            ref_id: None,
            ref_number: None,
            cash_session_id: None,
            business_date: business_date.to_owned(),
            note: None,
            created_at: 0,
            user_id: None,
            terminal_id: None,
            open: Money::ZERO,
        }
    }
}

/// An entry to append. `allocate_to`: debts to settle first, in this order (credits only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntry {
    pub party_id: String,
    pub kind: EntryKind,
    pub amount: Money,
    pub method: Option<PaymentMethod>,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub stamp_duty: Money,
    pub stamp_charged: Money,
    pub receipt_number: Option<String>,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub cash_session_id: Option<String>,
    pub note: Option<String>,
    pub allocate_to: Vec<String>,
}

impl NewEntry {
    pub fn new(party_id: &str, kind: EntryKind, amount: Money) -> NewEntry {
        NewEntry {
            party_id: party_id.to_owned(),
            kind,
            amount,
            method: None,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            stamp_duty: Money::ZERO,
            stamp_charged: Money::ZERO,
            receipt_number: None,
            ref_type: None,
            ref_id: None,
            cash_session_id: None,
            note: None,
            allocate_to: Vec::new(),
        }
    }

    pub fn referencing(mut self, ref_type: &str, ref_id: &str) -> NewEntry {
        self.ref_type = Some(ref_type.to_owned());
        self.ref_id = Some(ref_id.to_owned());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Allocation {
    pub debit_id: String,
    pub credit_id: String,
    pub amount: Money,
    /// The number of the debt's document or receipt (`TK…`, `RV…`), for receipts.
    pub debit_number: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EntryQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    /// 1..=5000, default 1000.
    pub limit: Option<u32>,
    /// Keyset paging: the entries older than this one (the last entry of the page before).
    #[serde(default)]
    #[specta(optional)]
    pub before: Option<String>,
}

/// A page of a party's history, newest first: the entries the cashier checks (the latest sales
/// and versements) come first whatever the length of the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EntryPage {
    /// Newest first.
    pub entries: Vec<LedgerEntry>,
    /// Older entries exist: ask again with `before` set to the id of the last entry.
    pub truncated: bool,
    /// An opening balance is set and not undone (CRD-003, PUR-007), whichever page is loaded.
    pub has_opening: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Statement {
    pub party_id: String,
    pub from: String,
    pub to: String,
    /// Balance at the start of `from`.
    pub opening: Money,
    pub entries: Vec<LedgerEntry>,
    /// Sum of the positive entries of the period.
    pub debits: Money,
    /// Sum of the negative entries of the period, as a positive amount.
    pub credits: Money,
    pub closing: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgingRow {
    pub party_id: String,
    pub code: String,
    pub name: String,
    pub phone: Option<String>,
    pub balance: Money,
    /// Open debts 0–30 days old.
    pub current: Money,
    pub days_31_60: Money,
    pub days_61_90: Money,
    pub over_90: Money,
    /// Unallocated payments (store credit, advances).
    pub advance: Money,
    pub oldest_open: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRebuild {
    pub parties: i64,
    pub corrected: i64,
}

struct Sql {
    insert: &'static str,
    select: &'static str,
    by_id: &'static str,
    party_entries: &'static str,
    party_balance: &'static str,
    open_debits: &'static str,
    open_credits: &'static str,
    insert_allocation: &'static str,
    allocations_of: &'static str,
    has_opening: &'static str,
    balance_before: &'static str,
    period: &'static str,
    aging_debits: &'static str,
    aging_parties: &'static str,
    rebuild: &'static str,
    parties: &'static str,
}

macro_rules! side_sql {
    ($ledger:literal, $party:literal, $parties:literal, $alloc:literal) => {
        Sql {
            insert: concat!(
                "INSERT INTO ", $ledger, " (id, ", $party, ", kind, amount, balance_after, method,
                   reference, cheque_bank, cheque_date, stamp_duty, stamp_charged, receipt_number,
                   ref_type, ref_id, cash_session_id, business_date, note, created_at, user_id,
                   terminal_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                         ?18, ?19, ?20)"
            ),
            select: concat!(
                "SELECT e.id, e.", $party, ", e.kind, e.amount, e.balance_after, e.method,
                   e.reference, e.cheque_bank, e.cheque_date, e.stamp_duty, e.stamp_charged,
                   e.receipt_number, e.ref_type, e.ref_id,
                   (SELECT d.number FROM documents d WHERE d.id = e.ref_id),
                   e.cash_session_id, e.business_date, e.note, e.created_at, e.user_id,
                   e.terminal_id,
                   abs(e.amount)
                     - COALESCE((SELECT SUM(a.amount) FROM ", $alloc, " a WHERE a.debit_id = e.id), 0)
                     - COALESCE((SELECT SUM(a.amount) FROM ", $alloc, " a WHERE a.credit_id = e.id), 0)
                 FROM ", $ledger, " e"
            ),
            by_id: " WHERE e.id = ?1",
            party_entries: concat!(
                " WHERE e.", $party, " = ?1 AND (?2 IS NULL OR e.business_date >= ?2)
                   AND (?3 IS NULL OR e.business_date <= ?3)
                   AND (?5 IS NULL OR (e.created_at, e.rowid) <
                        (SELECT b.created_at, b.rowid FROM ", $ledger, " b WHERE b.id = ?5))
                 ORDER BY e.created_at DESC, e.rowid DESC LIMIT ?4"
            ),
            party_balance: concat!("SELECT balance FROM ", $parties, " WHERE id = ?1"),
            open_debits: concat!(
                " WHERE e.", $party, " = ?1 AND e.amount > 0
                 ORDER BY e.business_date, e.created_at, e.rowid"
            ),
            open_credits: concat!(
                " WHERE e.", $party, " = ?1 AND e.amount < 0
                 ORDER BY e.business_date, e.created_at, e.rowid"
            ),
            insert_allocation: concat!(
                "INSERT INTO ", $alloc, " (id, ", $party, ", debit_id, credit_id, amount, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
            ),
            allocations_of: concat!(
                "SELECT a.debit_id, a.credit_id, a.amount,
                        COALESCE((SELECT d.number FROM documents d WHERE d.id = l.ref_id), l.receipt_number)
                 FROM ", $alloc, " a JOIN ", $ledger, " l ON l.id = a.debit_id
                 WHERE a.debit_id = ?1 OR a.credit_id = ?1 ORDER BY a.rowid"
            ),
            has_opening: concat!(
                "SELECT EXISTS (SELECT 1 FROM ", $ledger, " o WHERE o.", $party, " = ?1
                   AND o.kind = 'opening'
                   AND NOT EXISTS (SELECT 1 FROM ", $ledger, " u WHERE u.", $party, " = ?1
                                   AND u.ref_type = 'opening_undo' AND u.ref_id = o.id))"
            ),
            balance_before: concat!(
                "SELECT COALESCE(SUM(amount), 0) FROM ", $ledger, " WHERE ", $party, " = ?1
                   AND business_date < ?2"
            ),
            period: concat!(
                " WHERE e.", $party, " = ?1 AND e.business_date BETWEEN ?2 AND ?3
                 ORDER BY e.business_date, e.created_at, e.rowid"
            ),
            // A debt of `?1` or before of a party with a balance then, and what was still open of
            // it at the end of `?1`: only the allocations to credits of that day or before had
            // settled it then.
            aging_debits: concat!(
                "SELECT e.", $party, ", e.business_date,
                        e.amount - COALESCE((SELECT SUM(a.amount) FROM ", $alloc, " a
                                             JOIN ", $ledger, " c ON c.id = a.credit_id
                                             WHERE a.debit_id = e.id AND c.business_date <= ?1), 0)
                 FROM ", $ledger, " e
                 WHERE e.amount > 0 AND e.business_date <= ?1
                   AND e.", $party, " IN (
                     SELECT p.id FROM ", $parties, " p
                     LEFT JOIN (SELECT ", $party, " AS party_id, SUM(amount) AS amount FROM ", $ledger, "
                                WHERE business_date > ?1 GROUP BY ", $party, ") later
                       ON later.party_id = p.id
                     WHERE p.balance - COALESCE(later.amount, 0) <> 0)"
            ),
            // The balance at the end of `?1`: the cached balance (the sum of the whole ledger,
            // kept by the triggers) minus the entries dated after that day.
            aging_parties: concat!(
                "SELECT p.id, p.code, p.name, p.phone, p.balance - COALESCE(later.amount, 0)
                 FROM ", $parties, " p
                 LEFT JOIN (SELECT ", $party, " AS party_id, SUM(amount) AS amount FROM ", $ledger, "
                            WHERE business_date > ?1 GROUP BY ", $party, ") later
                   ON later.party_id = p.id
                 WHERE p.balance - COALESCE(later.amount, 0) <> 0
                 ORDER BY p.name COLLATE NOCASE"
            ),
            rebuild: concat!(
                "UPDATE ", $parties, " SET balance = (SELECT COALESCE(SUM(l.amount), 0) FROM ", $ledger,
                " l WHERE l.", $party, " = ", $parties, ".id)
                 WHERE balance <> (SELECT COALESCE(SUM(l.amount), 0) FROM ", $ledger,
                " l WHERE l.", $party, " = ", $parties, ".id)"
            ),
            parties: concat!("SELECT COUNT(*) FROM ", $parties),
        }
    };
}

const CUSTOMER: Sql = side_sql!(
    "customer_ledger",
    "customer_id",
    "customers",
    "customer_allocations"
);
const SUPPLIER: Sql = side_sql!(
    "supplier_ledger",
    "supplier_id",
    "suppliers",
    "supplier_allocations"
);

const fn sql(side: Side) -> &'static Sql {
    match side {
        Side::Customer => &CUSTOMER,
        Side::Supplier => &SUPPLIER,
    }
}

fn map_entry(r: &Row<'_>) -> rusqlite::Result<LedgerEntry> {
    Ok(LedgerEntry {
        id: r.get(0)?,
        party_id: r.get(1)?,
        kind: r.get(2)?,
        amount: r.get(3)?,
        balance_after: r.get(4)?,
        method: r.get(5)?,
        reference: r.get(6)?,
        cheque_bank: r.get(7)?,
        cheque_date: r.get(8)?,
        stamp_duty: r.get(9)?,
        stamp_charged: r.get(10)?,
        receipt_number: r.get(11)?,
        ref_type: r.get(12)?,
        ref_id: r.get(13)?,
        ref_number: r.get(14)?,
        cash_session_id: r.get(15)?,
        business_date: r.get(16)?,
        note: r.get(17)?,
        created_at: r.get(18)?,
        user_id: r.get(19)?,
        terminal_id: r.get(20)?,
        open: r.get(21)?,
    })
}

fn select_where(
    conn: &Connection,
    side: Side,
    clause: &'static str,
    params: impl rusqlite::Params,
) -> Result<Vec<LedgerEntry>, AppError> {
    let s = sql(side);
    let mut stmt = conn.prepare_cached(&[s.select, clause].concat())?;
    let rows = stmt
        .query_map(params, map_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, side: Side, id: &str) -> Result<LedgerEntry, AppError> {
    select_where(conn, side, sql(side).by_id, [id])?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::not_found("ledger_entry"))
}

pub fn balance(conn: &Connection, side: Side, party_id: &str) -> Result<Money, AppError> {
    conn.query_row(sql(side).party_balance, [party_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| match side {
            Side::Customer => AppError::not_found("customer"),
            Side::Supplier => AppError::not_found("supplier"),
        })
}

/// A page of the party's entries, newest first (`limit` 1..=5000, default 1000), older than
/// `before` when set; `truncated` when older entries remain.
pub fn entries(
    conn: &Connection,
    side: Side,
    party_id: &str,
    q: &EntryQuery,
) -> Result<EntryPage, AppError> {
    let limit = q.limit.unwrap_or(1_000).clamp(1, 5_000);
    let mut entries = select_where(
        conn,
        side,
        sql(side).party_entries,
        params![party_id, q.from, q.to, limit + 1, q.before],
    )?;
    let truncated = entries.len() > usize::try_from(limit).unwrap_or(usize::MAX);
    entries.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    Ok(EntryPage {
        entries,
        truncated,
        has_opening: has_opening(conn, side, party_id)?,
    })
}

/// Debts not fully settled, oldest first (CRD-004 "oldest sales", CRD-006 ageing).
pub fn open_debits(
    conn: &Connection,
    side: Side,
    party_id: &str,
) -> Result<Vec<LedgerEntry>, AppError> {
    Ok(select_where(conn, side, sql(side).open_debits, [party_id])?
        .into_iter()
        .filter(|e| e.open.is_positive())
        .collect())
}

fn open_credits(
    conn: &Connection,
    side: Side,
    party_id: &str,
) -> Result<Vec<LedgerEntry>, AppError> {
    Ok(
        select_where(conn, side, sql(side).open_credits, [party_id])?
            .into_iter()
            .filter(|e| e.open.is_positive())
            .collect(),
    )
}

pub fn allocations_of(
    conn: &Connection,
    side: Side,
    entry_id: &str,
) -> Result<Vec<Allocation>, AppError> {
    let mut stmt = conn.prepare_cached(sql(side).allocations_of)?;
    let rows = stmt
        .query_map([entry_id], |r| {
            Ok(Allocation {
                debit_id: r.get(0)?,
                credit_id: r.get(1)?,
                amount: r.get(2)?,
                debit_number: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// An opening entry exists that no `opening_undo` adjustment cancelled (CRD-003, PUR-007).
pub fn has_opening(conn: &Connection, side: Side, party_id: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(sql(side).has_opening, [party_id], |r| r.get(0))?)
}

/// Appends `entry` (the caller audits the operation) and allocates it. `business_date` is the
/// operator's business day (`TillContext::business_day`).
pub fn append(
    tx: &Transaction<'_>,
    side: Side,
    actor: &Actor,
    entry: &NewEntry,
    business_date: &str,
    now: i64,
) -> Result<LedgerEntry, AppError> {
    if !entry.kind.allowed_on(side) {
        return Err(AppError::validation("kind", "value_invalid"));
    }
    if !entry.kind.sign_ok(entry.amount) {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    if entry.kind == EntryKind::Payment && entry.method.is_none() {
        return Err(AppError::validation("method", "required"));
    }
    if !entry.allocate_to.is_empty() && !entry.amount.is_negative() {
        return Err(AppError::validation("allocateTo", "allocation_invalid"));
    }
    let note = clean_opt(entry.note.as_deref(), "note", 200)?;
    let before = balance(tx, side, &entry.party_id)?;
    let after = before
        .checked_add(entry.amount)
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
    let id = new_id();
    tx.prepare_cached(sql(side).insert)?.execute(params![
        id,
        entry.party_id,
        entry.kind,
        entry.amount,
        after,
        entry.method,
        entry.reference,
        entry.cheque_bank,
        entry.cheque_date,
        entry.stamp_duty,
        entry.stamp_charged,
        entry.receipt_number,
        entry.ref_type,
        entry.ref_id,
        entry.cash_session_id,
        business_date,
        note,
        now,
        actor.user_id,
        actor.terminal_id,
    ])?;
    allocate(tx, side, &entry.party_id, &id, &entry.allocate_to, now)?;
    get(tx, side, &id)
}

fn allocate(
    tx: &Transaction<'_>,
    side: Side,
    party_id: &str,
    entry_id: &str,
    preferred: &[String],
    now: i64,
) -> Result<(), AppError> {
    let entry = get(tx, side, entry_id)?;
    let mut left = entry.open;
    let counterparts: Vec<LedgerEntry> = if entry.amount.is_positive() {
        open_credits(tx, side, party_id)?
    } else {
        let debts = open_debits(tx, side, party_id)?;
        let mut ordered = Vec::with_capacity(debts.len());
        for wanted in preferred {
            // A debt named twice is settled once: its `open` is read before any allocation.
            if ordered.iter().any(|d: &LedgerEntry| &d.id == wanted) {
                continue;
            }
            let debt = debts
                .iter()
                .find(|d| &d.id == wanted)
                .ok_or_else(|| AppError::validation("allocateTo", "allocation_invalid"))?;
            ordered.push(debt.clone());
        }
        ordered.extend(debts.into_iter().filter(|d| !preferred.contains(&d.id)));
        ordered
    };
    for other in counterparts {
        if !left.is_positive() {
            break;
        }
        let take = left.min(other.open);
        if !take.is_positive() {
            continue;
        }
        let (debit_id, credit_id) = if entry.amount.is_positive() {
            (entry.id.as_str(), other.id.as_str())
        } else {
            (other.id.as_str(), entry.id.as_str())
        };
        tx.prepare_cached(sql(side).insert_allocation)?
            .execute(params![new_id(), party_id, debit_id, credit_id, take, now])?;
        left = left.checked_sub(take).unwrap_or(Money::ZERO);
    }
    Ok(())
}

/// The account between two business days (RPT-037, RPT-035).
pub fn statement(
    conn: &Connection,
    side: Side,
    party_id: &str,
    from: &str,
    to: &str,
) -> Result<Statement, AppError> {
    let from = crate::party::clean_day(Some(from), "from")?
        .ok_or_else(|| AppError::validation("from", "required"))?;
    let to = crate::party::clean_day(Some(to), "to")?
        .ok_or_else(|| AppError::validation("to", "required"))?;
    if from > to {
        return Err(AppError::validation("to", "date_invalid"));
    }
    balance(conn, side, party_id)?;
    let opening: Money =
        conn.query_row(sql(side).balance_before, params![party_id, from], |r| {
            r.get(0)
        })?;
    let entries = select_where(conn, side, sql(side).period, params![party_id, from, to])?;
    let sum = |positive: bool| {
        Money::checked_sum(
            entries
                .iter()
                .filter(|e| e.amount.is_positive() == positive)
                .map(|e| {
                    if positive {
                        e.amount
                    } else {
                        Money(-e.amount.0)
                    }
                }),
        )
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))
    };
    let debits = sum(true)?;
    let credits = sum(false)?;
    let closing = opening
        .checked_add(debits)
        .and_then(|m| m.checked_sub(credits))
        .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?;
    Ok(Statement {
        party_id: party_id.to_owned(),
        from,
        to,
        opening,
        entries,
        debits,
        credits,
        closing,
    })
}

/// Ageing of every party with a balance (CRD-006, RPT-035/036) at the end of the `as_of` day,
/// read from the ledger as it stood then: entries dated later are left out of the balance, and a
/// debt is open by what the credits of `as_of` or before had not settled.
pub fn aging(conn: &Connection, side: Side, as_of: &str) -> Result<Vec<AgingRow>, AppError> {
    let as_of_day = NaiveDate::parse_from_str(as_of, "%Y-%m-%d")
        .map_err(|_| AppError::validation("asOf", "date_invalid"))?;
    let mut rows: BTreeMap<String, AgingRow> = BTreeMap::new();
    let mut order = Vec::new();
    {
        let mut stmt = conn.prepare_cached(sql(side).aging_parties)?;
        let parties = stmt.query_map([as_of], |r| {
            Ok(AgingRow {
                party_id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                phone: r.get(3)?,
                balance: r.get(4)?,
                current: Money::ZERO,
                days_31_60: Money::ZERO,
                days_61_90: Money::ZERO,
                over_90: Money::ZERO,
                advance: Money::ZERO,
                oldest_open: None,
            })
        })?;
        for p in parties {
            let p = p?;
            order.push(p.party_id.clone());
            rows.insert(p.party_id.clone(), p);
        }
    }
    let debts: Vec<(String, String, Money)> = {
        let mut stmt = conn.prepare_cached(sql(side).aging_debits)?;
        stmt.query_map([as_of], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (party_id, business_date, open) in debts {
        if !open.is_positive() {
            continue;
        }
        let Some(row) = rows.get_mut(&party_id) else {
            continue;
        };
        let days = NaiveDate::parse_from_str(&business_date, "%Y-%m-%d")
            .map(|d| (as_of_day - d).num_days())
            .unwrap_or(0);
        let bucket = match days {
            ..=30 => &mut row.current,
            31..=60 => &mut row.days_31_60,
            61..=90 => &mut row.days_61_90,
            _ => &mut row.over_90,
        };
        *bucket = bucket.checked_add(open).unwrap_or(*bucket);
        if row
            .oldest_open
            .as_deref()
            .is_none_or(|d| business_date.as_str() < d)
        {
            row.oldest_open = Some(business_date);
        }
    }
    for row in rows.values_mut() {
        let owed = Money::checked_sum([row.current, row.days_31_60, row.days_61_90, row.over_90])
            .unwrap_or(Money::ZERO);
        row.advance = owed
            .checked_sub(row.balance)
            .unwrap_or(Money::ZERO)
            .max(Money::ZERO);
    }
    Ok(order
        .into_iter()
        .filter_map(|id| rows.remove(&id))
        .collect())
}

/// Recomputes every cached balance from the ledger (NFR-013), audited.
pub fn rebuild_balances(
    tx: &Transaction<'_>,
    side: Side,
    actor: &Actor,
) -> Result<LedgerRebuild, AppError> {
    let corrected = i64::try_from(tx.execute(sql(side).rebuild, [])?).unwrap_or(i64::MAX);
    let parties: i64 = tx.query_row(sql(side).parties, [], |r| r.get(0))?;
    audit::append(
        tx,
        &actor.event(
            "ledger.balances_rebuilt",
            "ledger",
            None,
            json!({ "side": side, "parties": parties, "corrected": corrected }),
        ),
    )?;
    Ok(LedgerRebuild { parties, corrected })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::money::Money;
    use proptest::prelude::*;

    const KARIM: &str = "00000000-0000-7000-8000-00000000d001";
    const SIDI: &str = "00000000-0000-7000-8000-00000000d002";

    fn db() -> Db {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            tx.execute_batch(
                "INSERT INTO customers (id, code, name, created_at, updated_at) VALUES
                   ('00000000-0000-7000-8000-00000000d001', 'C00001', 'Karim', 0, 0),
                   ('00000000-0000-7000-8000-00000000d002', 'C00002', 'Sidi', 0, 0);
                 INSERT INTO suppliers (id, code, name, created_at, updated_at) VALUES
                   ('00000000-0000-7000-8000-00000000f001', 'F00001', 'Grossiste Bab Ezzouar', 0, 0);",
            )?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn post(
        db: &Db,
        side: Side,
        party: &str,
        kind: EntryKind,
        amount: i64,
        day: &'static str,
    ) -> LedgerEntry {
        post_to(db, side, party, kind, amount, day, Vec::new()).unwrap()
    }

    fn post_to(
        db: &Db,
        side: Side,
        party: &str,
        kind: EntryKind,
        amount: i64,
        day: &'static str,
        allocate_to: Vec<String>,
    ) -> Result<LedgerEntry, AppError> {
        let mut entry = NewEntry::new(party, kind, Money(amount));
        if kind == EntryKind::Payment {
            entry.method = Some(PaymentMethod::Cash);
        }
        entry.allocate_to = allocate_to;
        db.write(move |tx| append(tx, side, &Actor::default(), &entry, day, 1_000))
    }

    #[test]
    fn debts_and_payments_move_the_balance_and_allocate_oldest_first() {
        let db = db();
        let s1 = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            120_000,
            "2026-07-01",
        );
        let s2 = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            80_000,
            "2026-09-01",
        );
        let pay = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            -150_000,
            "2026-09-20",
        );
        assert_eq!(pay.balance_after, Money(50_000));
        assert_eq!(
            db.read(|c| balance(c, Side::Customer, KARIM)).unwrap(),
            Money(50_000)
        );
        let open = db.read(|c| open_debits(c, Side::Customer, KARIM)).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(
            (open[0].id.clone(), open[0].open),
            (s2.id.clone(), Money(50_000))
        );
        let allocated = db
            .read(move |c| allocations_of(c, Side::Customer, &pay.id))
            .unwrap();
        assert_eq!(
            allocated
                .iter()
                .map(|a| (a.debit_id.clone(), a.amount))
                .collect::<Vec<_>>(),
            vec![(s1.id, Money(120_000)), (s2.id, Money(30_000))]
        );
    }

    #[test]
    fn a_payment_can_settle_chosen_sales_first() {
        let db = db();
        let old = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            100_000,
            "2026-07-01",
        );
        let recent = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            40_000,
            "2026-09-01",
        );
        post_to(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            -40_000,
            "2026-09-02",
            vec![recent.id.clone()],
        )
        .unwrap();
        let open = db.read(|c| open_debits(c, Side::Customer, KARIM)).unwrap();
        assert_eq!(
            open.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            vec![old.id]
        );
        let foreign = post(
            &db,
            Side::Customer,
            SIDI,
            EntryKind::SaleCredit,
            5_000,
            "2026-09-01",
        );
        let err = post_to(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            -1_000,
            "2026-09-03",
            vec![foreign.id],
        )
        .unwrap_err();
        assert_eq!(
            err,
            AppError::validation("allocateTo", "allocation_invalid")
        );
    }

    #[test]
    fn a_debt_chosen_twice_is_settled_once() {
        let db = db();
        let first = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            10_000,
            "2026-09-01",
        );
        let second = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            10_000,
            "2026-09-02",
        );
        let twice = vec![second.id.clone(), second.id.clone()];
        let pay = post_to(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            -15_000,
            "2026-09-03",
            twice,
        )
        .unwrap();
        let allocated = db
            .read(move |c| allocations_of(c, Side::Customer, &pay.id))
            .unwrap();
        assert_eq!(
            allocated
                .iter()
                .map(|a| (a.debit_id.clone(), a.amount))
                .collect::<Vec<_>>(),
            vec![(second.id, Money(10_000)), (first.id, Money(5_000))]
        );
    }

    #[test]
    fn an_advance_waits_for_the_next_debt() {
        let db = db();
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            -30_000,
            "2026-09-01",
        );
        assert_eq!(
            db.read(|c| balance(c, Side::Customer, KARIM)).unwrap(),
            Money(-30_000)
        );
        let sale = post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            50_000,
            "2026-09-05",
        );
        let open = db.read(|c| open_debits(c, Side::Customer, KARIM)).unwrap();
        assert_eq!((open[0].id.clone(), open[0].open), (sale.id, Money(20_000)));
    }

    #[test]
    fn kinds_signs_and_openings_are_checked_per_side() {
        let db = db();
        let wrong_side = post_to(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Purchase,
            1_000,
            "2026-09-01",
            Vec::new(),
        );
        assert_eq!(
            wrong_side.unwrap_err(),
            AppError::validation("kind", "value_invalid")
        );
        let wrong_sign = post_to(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            1_000,
            "2026-09-01",
            Vec::new(),
        );
        assert_eq!(
            wrong_sign.unwrap_err(),
            AppError::validation("amount", "amount_invalid")
        );
        post(
            &db,
            Side::Supplier,
            "00000000-0000-7000-8000-00000000f001",
            EntryKind::Opening,
            900_000,
            "2026-01-01",
        );
        assert!(
            db.read(|c| has_opening(c, Side::Supplier, "00000000-0000-7000-8000-00000000f001"))
                .unwrap()
        );
        assert!(!db.read(|c| has_opening(c, Side::Customer, KARIM)).unwrap());
    }

    /// A regular kridi customer with a long history: the notebook shows the newest entries
    /// first, says older ones exist, and pages back to them by keyset.
    #[test]
    fn the_history_pages_from_the_newest_entry() {
        let db = db();
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Opening,
            1_000,
            "2026-01-02",
        );
        let days = [
            "2026-01-03",
            "2026-01-04",
            "2026-01-05",
            "2026-01-06",
            "2026-01-07",
            "2026-01-08",
            "2026-01-09",
        ];
        for (i, day) in (1_i64..).zip(days) {
            post(
                &db,
                Side::Customer,
                KARIM,
                EntryKind::SaleCredit,
                i * 100,
                day,
            );
        }
        let page = |before: Option<String>| {
            let q = EntryQuery {
                limit: Some(3),
                before,
                ..EntryQuery::default()
            };
            db.read(move |c| entries(c, Side::Customer, KARIM, &q))
                .unwrap()
        };
        let days = |p: &EntryPage| {
            p.entries
                .iter()
                .map(|e| e.business_date.clone())
                .collect::<Vec<_>>()
        };
        let first = page(None);
        assert_eq!(days(&first), ["2026-01-09", "2026-01-08", "2026-01-07"]);
        assert!(first.truncated && first.has_opening);
        let second = page(first.entries.last().map(|e| e.id.clone()));
        assert_eq!(days(&second), ["2026-01-06", "2026-01-05", "2026-01-04"]);
        assert!(second.truncated);
        let third = page(second.entries.last().map(|e| e.id.clone()));
        assert_eq!(days(&third), ["2026-01-03", "2026-01-02"]);
        assert!(
            !third.truncated && third.has_opening,
            "the opening is on the last page"
        );
    }

    #[test]
    fn a_statement_starts_from_the_balance_before_the_period() {
        let db = db();
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Opening,
            200_000,
            "2026-01-15",
        );
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            50_000,
            "2026-09-02",
        );
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            -70_000,
            "2026-09-10",
        );
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            10_000,
            "2026-10-01",
        );
        let s = db
            .read(|c| statement(c, Side::Customer, KARIM, "2026-09-01", "2026-09-30"))
            .unwrap();
        assert_eq!(
            (s.opening, s.debits, s.credits, s.closing),
            (Money(200_000), Money(50_000), Money(70_000), Money(180_000))
        );
        assert_eq!(s.entries.len(), 2);
    }

    #[test]
    fn ageing_buckets_open_debts_by_age() {
        let db = db();
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            10_000,
            "2026-09-20",
        );
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            20_000,
            "2026-08-15",
        );
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            30_000,
            "2026-07-20",
        );
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            40_000,
            "2026-05-01",
        );
        post(
            &db,
            Side::Customer,
            SIDI,
            EntryKind::Payment,
            -5_000,
            "2026-09-01",
        );
        let rows = db.read(|c| aging(c, Side::Customer, "2026-09-28")).unwrap();
        let karim = rows.iter().find(|r| r.party_id == KARIM).unwrap();
        assert_eq!(
            (
                karim.current,
                karim.days_31_60,
                karim.days_61_90,
                karim.over_90,
                karim.balance
            ),
            (
                Money(10_000),
                Money(20_000),
                Money(30_000),
                Money(40_000),
                Money(100_000)
            )
        );
        let sidi = rows.iter().find(|r| r.party_id == SIDI).unwrap();
        assert_eq!((sidi.advance, sidi.balance), (Money(5_000), Money(-5_000)));
    }

    #[test]
    fn ageing_on_a_past_day_reads_the_ledger_as_it_stood_that_day() {
        let db = db();
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            100_000,
            "2026-07-01",
        );
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::Payment,
            -100_000,
            "2026-09-20",
        );
        post(
            &db,
            Side::Customer,
            SIDI,
            EntryKind::Payment,
            -30_000,
            "2026-08-01",
        );
        post(
            &db,
            Side::Customer,
            SIDI,
            EntryKind::SaleCredit,
            50_000,
            "2026-09-10",
        );
        // On 1 September Karim still owed the July debt, and Sidi's payment was an advance.
        let rows = db.read(|c| aging(c, Side::Customer, "2026-09-01")).unwrap();
        let karim = rows.iter().find(|r| r.party_id == KARIM).unwrap();
        assert_eq!(
            (
                karim.balance,
                karim.days_61_90,
                karim.oldest_open.as_deref()
            ),
            (Money(100_000), Money(100_000), Some("2026-07-01"))
        );
        let sidi = rows.iter().find(|r| r.party_id == SIDI).unwrap();
        assert_eq!(
            (sidi.balance, sidi.current, sidi.advance),
            (Money(-30_000), Money::ZERO, Money(30_000))
        );
        // On the 28th Karim is settled and Sidi's advance paid part of the September debt.
        let rows = db.read(|c| aging(c, Side::Customer, "2026-09-28")).unwrap();
        assert!(rows.iter().all(|r| r.party_id != KARIM));
        let sidi = rows.iter().find(|r| r.party_id == SIDI).unwrap();
        assert_eq!(
            (sidi.balance, sidi.current, sidi.advance),
            (Money(20_000), Money(20_000), Money::ZERO)
        );
    }

    #[test]
    fn a_broken_cache_is_rebuilt_from_the_ledger() {
        let db = db();
        post(
            &db,
            Side::Customer,
            KARIM,
            EntryKind::SaleCredit,
            70_000,
            "2026-09-01",
        );
        db.write(|tx| {
            tx.execute("UPDATE customers SET balance = 1 WHERE id = ?1", [KARIM])?;
            Ok(())
        })
        .unwrap();
        let report = db
            .write(|tx| rebuild_balances(tx, Side::Customer, &Actor::default()))
            .unwrap();
        assert_eq!(report.corrected, 1);
        assert_eq!(
            db.read(|c| balance(c, Side::Customer, KARIM)).unwrap(),
            Money(70_000)
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]
        #[test]
        fn the_balance_is_the_sum_of_entries_and_of_open_items(
            moves in proptest::collection::vec((any::<bool>(), 1_i64..200_000), 1..25)
        ) {
            let db = db();
            let mut expected = 0_i64;
            for (debit, amount) in moves {
                let (kind, signed) = if debit { (EntryKind::SaleCredit, amount) } else { (EntryKind::Payment, -amount) };
                expected += signed;
                post(&db, Side::Customer, KARIM, kind, signed, "2026-09-01");
            }
            let cached = db.read(|c| balance(c, Side::Customer, KARIM)).unwrap();
            prop_assert_eq!(cached, Money(expected));
            let entries = db.read(|c| entries(c, Side::Customer, KARIM, &EntryQuery::default())).unwrap().entries;
            let open_debits: i64 = entries.iter().filter(|e| e.amount.is_positive()).map(|e| e.open.0).sum();
            let open_credits: i64 = entries.iter().filter(|e| e.amount.is_negative()).map(|e| e.open.0).sum();
            prop_assert_eq!(open_debits - open_credits, expected);
            prop_assert!(open_debits == 0 || open_credits == 0, "nothing stays open on both sides");
        }

        #[test]
        fn allocations_never_exceed_an_entry(
            moves in proptest::collection::vec((any::<bool>(), 1_i64..50_000), 1..20)
        ) {
            let db = db();
            for (debit, amount) in moves {
                let (kind, signed) = if debit { (EntryKind::SaleCredit, amount) } else { (EntryKind::Payment, -amount) };
                post(&db, Side::Customer, KARIM, kind, signed, "2026-09-01");
            }
            let entries = db.read(|c| entries(c, Side::Customer, KARIM, &EntryQuery::default())).unwrap().entries;
            for e in &entries {
                prop_assert!(e.open.0 >= 0 && e.open.0 <= e.amount.0.abs());
            }
        }
    }
}
