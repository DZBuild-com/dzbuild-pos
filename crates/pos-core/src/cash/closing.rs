//! X and Z reports and the closing of a cash session (spec §7.3; SAL-002, CSH-003..011,
//! AUD-004/005): the count sheet gives the counted cash, the variance against the expected
//! cash needs a manager above a threshold, the Z number never restarts, and a closure record
//! anchors the audit journal and the terminal's document chains (last seq and hash, checked
//! incrementally since the previous Z) together with the perpetual grand total. Closures are
//! hash-chained per terminal; a closed session can never reopen.

use super::{CashSession, MovementKind, get_session, require_open};
use crate::audit::{self, GENESIS_HASH, canonical_json};
use crate::auth::Permission;
use crate::db::{BackupInfo, BackupKind, Db};
use crate::documents::{self, DocType};
use crate::error::AppError;
use crate::money::Money;
use crate::numbering;
use crate::payments::PaymentMethod;
use crate::tax::tva::RateTotal;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::BTreeSet;

/// DZD notes and coins in circulation, largest first (CSH-006 count helper).
pub const DENOMINATIONS: [Money; 9] = [
    Money(200_000),
    Money(100_000),
    Money(50_000),
    Money(20_000),
    Money(10_000),
    Money(5_000),
    Money(2_000),
    Money(1_000),
    Money(500),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MethodTotal {
    pub method: PaymentMethod,
    pub received: Money,
    /// Change and refunds (negative).
    pub paid_back: Money,
    pub net: Money,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseTotal {
    pub category: String,
    pub amount: Money,
}

/// What a session did (X report; also the body of the Z report).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session: CashSession,
    pub terminal_code: String,
    pub cashier_name: String,
    pub tickets: i64,
    pub returns: i64,
    pub first_ticket: Option<String>,
    pub last_ticket: Option<String>,
    pub sales_ttc: Money,
    /// Negative.
    pub returns_ttc: Money,
    /// Invoices made at this till in the session (M3; not the invoice of a till ticket, whose
    /// ticket is counted): their payments and duty are in `by_method` and the stamp lines, so
    /// their amounts are here. Absent from a Z without any (its stored hash is unchanged).
    #[serde(default, skip_serializing_if = "is_zero")]
    #[specta(optional)]
    pub invoices: i64,
    #[serde(default, skip_serializing_if = "no_money")]
    #[specta(optional)]
    pub invoices_ttc: Money,
    /// Credit notes refunded or booked in the session (M3). Negative.
    #[serde(default, skip_serializing_if = "is_zero")]
    #[specta(optional)]
    pub credit_notes: i64,
    #[serde(default, skip_serializing_if = "no_money")]
    #[specta(optional)]
    pub credit_notes_ttc: Money,
    /// Sales, returns, invoices and credit notes of the session together.
    pub net_ttc: Money,
    /// Tickets and invoices.
    pub discount_total: Money,
    /// Tickets, return notes, invoices and credit notes together, highest rate first.
    pub per_rate: Vec<RateTotal>,
    /// Droit de timbre added to the amounts due (CSH-010): tickets, invoices and kridi
    /// versements (RV) of the session.
    pub stamp_charged: Money,
    /// Droit de timbre the shop paid itself, on the same documents and versements.
    pub stamp_absorbed: Money,
    pub rounding: Money,
    pub by_method: Vec<MethodTotal>,
    pub opening_float: Money,
    pub cash_sales: Money,
    pub cash_refunds: Money,
    pub paid_in: Money,
    pub paid_out: Money,
    /// Kridi versements received in cash, with the duty charged (M2, CRD-004).
    #[serde(default)]
    #[specta(optional)]
    pub credit_payments: Money,
    pub drops: Money,
    pub expenses: Vec<ExpenseTotal>,
    pub expenses_total: Money,
    /// `None` when the viewer may not see it (blind close, CSH-004).
    pub expected_cash: Option<Money>,
    /// A blind summary (CSH-004, [`x_report`] without `reveal_expected`): only the ticket counts
    /// and the non-cash tenders; every drawer figure and the sales totals are zero or empty.
    #[serde(default)]
    pub blind: bool,
    /// Documents under the régime réel: `per_rate` (from those documents only) is printed.
    #[serde(default)]
    pub show_tax: bool,
    /// Documents under the IFU: the report carries the "TVA non applicable" mention.
    #[serde(default)]
    pub ifu_mention: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CountLine {
    pub denomination: Money,
    pub count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloseInput {
    /// The count sheet: one line per note or coin value.
    pub counts: Vec<CountLine>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChainAnchor {
    pub last_seq: i64,
    pub last_hash: String,
    /// The rows since the previous Z verified.
    pub intact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocAnchor {
    pub doc_type: DocType,
    pub series: String,
    pub last_seq: i64,
    pub last_hash: String,
    pub intact: bool,
}

/// The Z report as printed and stored in `cash_closures` (non-editable, CSH-007).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZReport {
    pub z_number: String,
    pub z_seq: i64,
    pub summary: SessionSummary,
    pub counts: Vec<CountLine>,
    pub counted_cash: Money,
    /// Counted minus expected.
    pub variance: Money,
    /// Net sales of the terminal since its first Z, never reset (AUD-005).
    pub grand_total: Money,
    /// The first Z after a journaled restore: the grand total the replaced database had already
    /// printed, higher than the restored copy's, which this Z goes on from (AUD-005: never
    /// reset). Absent otherwise (stored hashes unchanged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub grand_total_carried: Option<Money>,
    pub audit_anchor: ChainAnchor,
    pub document_anchors: Vec<DocAnchor>,
    pub closed_at: i64,
    pub closed_by: String,
    pub closed_by_name: String,
    /// The manager who accepted a variance above the threshold.
    pub approved_by: Option<String>,
    pub notes: Option<String>,
    pub prev_hash: String,
    pub hash: String,
    /// A till closed its Z with this many operations not yet sent to the main PC (M5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub offline_pending: Option<i64>,
}

/// The closure chain of a terminal (AUD-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClosureChainReport {
    /// Closures present for the terminal.
    pub rows: i64,
    /// `None` when the chain is intact. Otherwise the first Z that fails (its report does not
    /// hash to its hash or link to the previous Z, a column disagrees with the report, its Z
    /// number does not follow the previous one, or its grand total is not the previous one plus
    /// the day's net), or else the first Z issued whose closure is gone (the terminal's Z
    /// counter is past the last closure, or a closed session has none).
    pub first_broken_z: Option<String>,
    /// `hash` of the last closure that verified, [`GENESIS_HASH`] when none did.
    pub last_hash: String,
}

/// Z close result: the report, and the backup taken after it (CSH-011); a failed backup does
/// not undo the close, it is reported with its error code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Closed {
    pub z: ZReport,
    pub backup: Option<BackupInfo>,
    pub backup_error: Option<String>,
}

fn sum_err() -> AppError {
    AppError::validation("amount", "amount_invalid")
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` passes a reference
fn is_zero(n: &i64) -> bool {
    *n == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn no_money(m: &Money) -> bool {
    *m == Money::ZERO
}

/// The documents of a session whose amounts the X and Z count: its tickets and return notes, its
/// invoices (not the invoice of a till ticket: the ticket is counted) and its credit notes —
/// every document whose payments `by_method` adds up.
const SESSION_DOCS: &str = "(d.doc_type IN ('ticket', 'return_note', 'credit_note')
     OR (d.doc_type = 'invoice' AND d.origin IS NOT 'ticket'))";

/// The session's figures; `expected_cash` always filled (callers hide it).
fn summarize(conn: &Connection, session_id: &str) -> Result<SessionSummary, AppError> {
    let session = get_session(conn, session_id)?;
    let terminal_code: String = conn
        .query_row(
            "SELECT code FROM terminals WHERE id = ?1",
            [&session.terminal_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| session.terminal_id.clone());
    let cashier_name: String = conn
        .query_row(
            "SELECT display_name FROM users WHERE id = ?1",
            [&session.user_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default();
    let (tickets, first_ticket, last_ticket, sales_ttc): (
        i64,
        Option<String>,
        Option<String>,
        Money,
    ) = conn.query_row(
        "SELECT COUNT(*), MIN(number), MAX(number), COALESCE(SUM(total_ttc), 0)
         FROM documents WHERE cash_session_id = ?1 AND status = 'validated' AND doc_type = 'ticket'",
        [session_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let (discount_total, rounding): (Money, Money) = conn.query_row(
        "SELECT COALESCE(SUM(discount_total), 0), COALESCE(SUM(rounding), 0)
         FROM documents d WHERE d.cash_session_id = ?1 AND d.status = 'validated'
           AND (d.doc_type IN ('ticket', 'credit_note')
                OR (d.doc_type = 'invoice' AND d.origin IS NOT 'ticket'))",
        [session_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let kind_total = |kind_sql: &str| -> Result<(i64, Money), AppError> {
        Ok(conn.query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(SUM(total_ttc), 0) FROM documents d
                 WHERE d.cash_session_id = ?1 AND d.status = 'validated' AND {kind_sql}"
            ),
            [session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    };
    let (invoices, invoices_ttc) =
        kind_total("d.doc_type = 'invoice' AND d.origin IS NOT 'ticket'")?;
    let (credit_notes, credit_notes_ttc) = kind_total("d.doc_type = 'credit_note'")?;
    // The duty of the session (CSH-010): its tickets, its invoices (the cash part; an invoice
    // made from a ticket carries the ticket's duty, counted once with the ticket) and its kridi
    // versements (RV), charged to the customer or absorbed by the shop.
    let (stamp_charged, stamp_absorbed): (Money, Money) = conn.query_row(
        "SELECT COALESCE(SUM(charged), 0), COALESCE(SUM(absorbed), 0) FROM (
           SELECT CASE WHEN stamp_mode = 'charge_customer' THEN stamp_duty ELSE 0 END AS charged,
                  CASE WHEN stamp_mode = 'absorb' THEN stamp_duty ELSE 0 END AS absorbed
           FROM documents WHERE cash_session_id = ?1 AND status = 'validated'
             AND (doc_type = 'ticket' OR (doc_type = 'invoice' AND origin IS NOT 'ticket'))
           UNION ALL
           SELECT stamp_charged, stamp_duty - stamp_charged FROM customer_ledger
           WHERE cash_session_id = ?1 AND kind = 'payment')",
        [session_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (returns, returns_ttc): (i64, Money) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(total_ttc), 0) FROM documents
         WHERE cash_session_id = ?1 AND status = 'validated' AND doc_type = 'return_note'",
        [session_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut stmt = conn.prepare(&format!(
        "SELECT t.rate_bp, SUM(t.base_ht), SUM(t.tax_amount) FROM document_taxes t
         JOIN documents d ON d.id = t.document_id
         WHERE d.cash_session_id = ?1 AND d.status = 'validated' AND {SESSION_DOCS}
           AND d.tax_regime = 'reel'
         GROUP BY t.rate_bp ORDER BY t.rate_bp DESC"
    ))?;
    let per_rate = stmt
        .query_map([session_id], |r| {
            let (base_ht, tax): (Money, Money) = (r.get(1)?, r.get(2)?);
            Ok(RateTotal {
                rate_bp: r.get(0)?,
                base_ht,
                tax,
                total_ttc: Money(base_ht.0 + tax.0),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // An IFU ticket charges no TVA: its taxes never reach the report, its mention does.
    let (reel_docs, ifu_docs): (i64, i64) = conn.query_row(
        &format!(
            "SELECT COALESCE(SUM(d.tax_regime = 'reel'), 0), COALESCE(SUM(d.tax_regime = 'ifu'), 0)
             FROM documents d WHERE d.cash_session_id = ?1 AND d.status = 'validated'
               AND {SESSION_DOCS}"
        ),
        [session_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut stmt = conn.prepare(
        "SELECT p.method, COALESCE(SUM(CASE WHEN p.amount > 0 THEN p.amount END), 0),
                COALESCE(SUM(CASE WHEN p.amount < 0 THEN p.amount END), 0), SUM(p.amount), COUNT(*)
         FROM payments p JOIN documents d ON d.id = p.document_id
         WHERE p.cash_session_id = ?1 AND d.status = 'validated'
         GROUP BY p.method ORDER BY p.method",
    )?;
    let by_method = stmt
        .query_map([session_id], |r| {
            Ok(MethodTotal {
                method: r.get(0)?,
                received: r.get(1)?,
                paid_back: r.get(2)?,
                net: r.get(3)?,
                count: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT kind, category, SUM(amount) FROM cash_movements WHERE session_id = ?1
         GROUP BY kind, category ORDER BY kind, category",
    )?;
    let moves = stmt
        .query_map([session_id], |r| {
            Ok((
                r.get::<_, MovementKind>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Money>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let total_of = |kind: MovementKind| {
        Money::checked_sum(moves.iter().filter(|m| m.0 == kind).map(|m| m.2)).ok_or_else(sum_err)
    };
    let expenses: Vec<ExpenseTotal> = moves
        .iter()
        .filter(|m| m.0 == MovementKind::Expense)
        .map(|m| ExpenseTotal {
            category: m.1.clone().unwrap_or_default(),
            amount: m.2.checked_neg().unwrap_or(Money::ZERO),
        })
        .collect();
    let expected = Money::checked_sum(moves.iter().map(|m| m.2)).ok_or_else(sum_err)?;
    Ok(SessionSummary {
        opening_float: session.opening_float,
        terminal_code,
        cashier_name,
        tickets,
        returns,
        first_ticket,
        last_ticket,
        sales_ttc,
        returns_ttc,
        invoices,
        invoices_ttc,
        credit_notes,
        credit_notes_ttc,
        net_ttc: Money::checked_sum([sales_ttc, returns_ttc, invoices_ttc, credit_notes_ttc])
            .ok_or_else(sum_err)?,
        discount_total,
        per_rate,
        stamp_charged,
        stamp_absorbed,
        rounding,
        by_method,
        cash_sales: total_of(MovementKind::Sale)?,
        cash_refunds: total_of(MovementKind::Refund)?,
        paid_in: total_of(MovementKind::PaidIn)?,
        paid_out: total_of(MovementKind::PaidOut)?,
        credit_payments: total_of(MovementKind::CreditPayment)?,
        drops: total_of(MovementKind::Drop)?,
        expenses_total: total_of(MovementKind::Expense)?,
        expenses,
        expected_cash: Some(expected),
        blind: false,
        show_tax: reel_docs > 0,
        ifu_mention: ifu_docs > 0,
        session,
    })
}

/// What a blind closer may see (CSH-004): the ticket counts and the non-cash tenders. The
/// float, the cash sales and change, refunds, paid in/out, drops, expenses and the sales totals
/// they can be worked back from are all left out. Every field is named (no `..`), so a field
/// added later has to be decided here.
fn blinded(s: SessionSummary) -> SessionSummary {
    SessionSummary {
        session: CashSession {
            opening_float: Money::ZERO,
            expected_cash: None,
            counted_cash: None,
            variance: None,
            ..s.session
        },
        terminal_code: s.terminal_code,
        cashier_name: s.cashier_name,
        tickets: s.tickets,
        returns: s.returns,
        first_ticket: s.first_ticket,
        last_ticket: s.last_ticket,
        sales_ttc: Money::ZERO,
        returns_ttc: Money::ZERO,
        invoices: s.invoices,
        invoices_ttc: Money::ZERO,
        credit_notes: s.credit_notes,
        credit_notes_ttc: Money::ZERO,
        net_ttc: Money::ZERO,
        discount_total: Money::ZERO,
        per_rate: Vec::new(),
        stamp_charged: Money::ZERO,
        stamp_absorbed: Money::ZERO,
        rounding: Money::ZERO,
        by_method: s
            .by_method
            .into_iter()
            .filter(|m| !matches!(m.method, PaymentMethod::Cash | PaymentMethod::Change))
            .collect(),
        opening_float: Money::ZERO,
        cash_sales: Money::ZERO,
        cash_refunds: Money::ZERO,
        paid_in: Money::ZERO,
        paid_out: Money::ZERO,
        credit_payments: Money::ZERO,
        drops: Money::ZERO,
        expenses: Vec::new(),
        expenses_total: Money::ZERO,
        expected_cash: None,
        blind: true,
        show_tax: false,
        ifu_mention: false,
    }
}

/// The X report of a session at any time, without closing anything (CSH-007). The command
/// passes `reveal_expected = has(cash.view_expected) && !cash.blind_close`; without it the
/// report is [`blinded`]: nothing on it adds up to the expected cash (CSH-004).
pub fn x_report(
    conn: &Connection,
    session_id: &str,
    reveal_expected: bool,
) -> Result<SessionSummary, AppError> {
    let summary = summarize(conn, session_id)?;
    Ok(if reveal_expected {
        summary
    } else {
        blinded(summary)
    })
}

/// Journals a close refused for its variance (`cash.close_refused`, CSH-005) in a transaction
/// of its own, the way `auth.override_failed` is: every count the cashier made is on record,
/// recounts included.
/// Closing the open session of `ctx`'s terminal with `input` needs a manager's approval of its
/// variance (CSH-005): it is above `cash.variance_threshold` and `ctx` does not allow
/// `cash.view_expected`. Read only: the close itself checks it again.
pub fn variance_needs_approval(
    conn: &Connection,
    ctx: &TillContext,
    input: &CloseInput,
) -> Result<bool, AppError> {
    let session = require_open(conn, &ctx.session.terminal_id)?;
    let counted_cash = counted(&input.counts)?;
    let expected = summarize(conn, &session.id)?
        .expected_cash
        .unwrap_or(Money::ZERO);
    let variance = counted_cash.checked_sub(expected).ok_or_else(sum_err)?;
    let threshold: i64 = crate::settings::get(conn, "cash.variance_threshold")?;
    Ok(variance.0.unsigned_abs() > threshold.unsigned_abs()
        && !ctx.allows(Permission::CashViewExpected))
}

/// When a count of the open session of `terminal_id` was last refused for its variance
/// (`cash.close_refused`), if one was.
pub fn last_refused_at(conn: &Connection, terminal_id: &str) -> Result<Option<i64>, AppError> {
    let session = require_open(conn, terminal_id)?;
    Ok(conn.query_row(
        "SELECT MAX(at) FROM audit_log WHERE action = 'cash.close_refused'
           AND entity_type = 'cash_session' AND entity_id = ?1",
        [&session.id],
        |r| r.get(0),
    )?)
}

pub fn record_refused_close(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: &CloseInput,
) -> Result<(), AppError> {
    let session = require_open(tx, &ctx.session.terminal_id)?;
    let counted_cash = counted(&input.counts)?;
    let expected = summarize(tx, &session.id)?
        .expected_cash
        .unwrap_or(Money::ZERO);
    let variance = counted_cash.checked_sub(expected).ok_or_else(sum_err)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "cash.close_refused",
            "cash_session",
            Some(&session.id),
            json!({ "counted": counted_cash, "variance": variance, "counts": input.counts }),
        ),
    )?;
    Ok(())
}

fn counted(counts: &[CountLine]) -> Result<Money, AppError> {
    let mut seen = BTreeSet::new();
    let mut total = Money::ZERO;
    for (i, line) in counts.iter().enumerate() {
        if !line.denomination.is_positive() || !seen.insert(line.denomination) {
            return Err(AppError::validation(
                &format!("counts.{i}.denomination"),
                "value_invalid",
            ));
        }
        if line.count < 0 {
            return Err(AppError::validation(
                &format!("counts.{i}.count"),
                "count_invalid",
            ));
        }
        let value = line
            .denomination
            .0
            .checked_mul(line.count)
            .ok_or_else(sum_err)?;
        total = total.checked_add(Money(value)).ok_or_else(sum_err)?;
    }
    Ok(total)
}

/// The previous closure of the terminal: its hash, number, grand total and anchors.
struct Previous {
    hash: String,
    z_seq: i64,
    grand_total: Money,
    audit: (i64, String),
    documents: Vec<DocAnchor>,
}

/// Read from the hashed `report` of the terminal's last closure, never from the plain columns
/// next to it, which no hash covers: resetting `grand_total` or moving an anchor there must not
/// carry into the next Z (AUD-005). [`verify_closures`] reports columns that disagree.
fn previous(conn: &Connection, terminal_id: &str) -> Result<Previous, AppError> {
    let report: Option<String> = conn
        .query_row(
            "SELECT report FROM cash_closures WHERE terminal_id = ?1 ORDER BY z_seq DESC LIMIT 1",
            [terminal_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match report {
        None => Previous {
            hash: GENESIS_HASH.to_owned(),
            z_seq: 0,
            grand_total: Money::ZERO,
            audit: (0, GENESIS_HASH.to_owned()),
            documents: Vec::new(),
        },
        Some(text) => {
            let z: ZReport = serde_json::from_str(&text)?;
            Previous {
                hash: z.hash,
                z_seq: z.z_seq,
                grand_total: z.grand_total,
                audit: (z.audit_anchor.last_seq, z.audit_anchor.last_hash),
                documents: z.document_anchors,
            }
        }
    })
}

/// `hex(SHA-256(prev_hash ␟ canonical_json(report without "hash")))`.
fn closure_hash(prev_hash: &str, report: &Value) -> String {
    let mut body = report.clone();
    if let Some(map) = body.as_object_mut() {
        map.remove("hash");
    }
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update([0x1f_u8]);
    hasher.update(canonical_json(&body).as_bytes());
    hex::encode(hasher.finalize().as_slice())
}

/// Closes the terminal's session (spec §7.3): count sheet → counted cash → variance (a manager
/// accepts it above `cash.variance_threshold`, CSH-005) → Z number → anchors → closure record.
pub fn close_session(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: CloseInput,
) -> Result<ZReport, AppError> {
    close_session_with(tx, ctx, input, None)
}

/// [`close_session`] on a till that still has `offline_pending` operations to send (an offline
/// Z, M5): the count is part of the hashed report and of the audit row.
pub fn close_session_with(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: CloseInput,
    offline_pending: Option<i64>,
) -> Result<ZReport, AppError> {
    let session = require_open(tx, &ctx.session.terminal_id)?;
    let counted_cash = counted(&input.counts)?;
    let notes = crate::catalog::clean_opt(input.notes.as_deref(), "notes", 500)?;
    let summary = summarize(tx, &session.id)?;
    let expected = summary.expected_cash.unwrap_or(Money::ZERO);
    let variance = counted_cash.checked_sub(expected).ok_or_else(sum_err)?;
    let threshold: i64 = crate::settings::get(tx, "cash.variance_threshold")?;
    let approved_by = ctx
        .approved_by(Permission::CashViewExpected)
        .map(str::to_owned);
    if variance.0.unsigned_abs() > threshold.unsigned_abs()
        && !ctx.allows(Permission::CashViewExpected)
    {
        return Err(AppError::conflict("variance_needs_approval"));
    }
    let terminal_code = ctx.session.terminal_code.clone();
    let (z_seq, z_number) = numbering::assign_z(tx, &terminal_code)?;
    for line in &input.counts {
        tx.execute(
            "INSERT INTO cash_counts (session_id, denomination, count) VALUES (?1, ?2, ?3)",
            params![session.id, line.denomination, line.count],
        )?;
    }
    let prev = previous(tx, &ctx.session.terminal_id)?;
    // The day is checked from the previous Z's anchors; an anchor that no longer names a live
    // row (a rewrite, or rows deleted with their counter wound back) breaks the new Z as well.
    let audit_start_live =
        audit_anchor_live(tx, &ctx.session.terminal_id, prev.audit.0, &prev.audit.1)?;
    let audit_report = audit::verify_chain_from(tx, prev.audit.0, &prev.audit.1)?;
    let audit_anchor = ChainAnchor {
        last_seq: audit_report.last_seq,
        last_hash: audit_report.last_hash,
        intact: audit_report.first_broken_seq.is_none() && audit_start_live,
    };
    let mut document_anchors = Vec::new();
    for doc_type in [DocType::Ticket, DocType::ReturnNote] {
        let series = doc_type.series(&terminal_code);
        let start = prev
            .documents
            .iter()
            .find(|a| a.doc_type == doc_type && a.series == series);
        let start_live = match start {
            Some(anchor) => doc_anchor_live(tx, anchor)?,
            None => true,
        };
        let (from_seq, from_hash) = start.map_or((0, GENESIS_HASH.to_owned()), |a| {
            (a.last_seq, a.last_hash.clone())
        });
        let r = documents::verify_chain_from(tx, doc_type, &series, from_seq, &from_hash)?;
        document_anchors.push(DocAnchor {
            doc_type,
            series,
            last_seq: r.last_seq,
            last_hash: r.last_hash,
            intact: r.first_broken_seq.is_none() && start_live,
        });
    }
    // After a journaled restore, the grand total goes on from the replaced database's when it was
    // higher (AUD-005: a restore never resets it).
    let grand_total_carried = carried(
        &replaced_grand_totals(tx, &ctx.session.terminal_id)?,
        prev.z_seq,
        prev.grand_total,
    );
    let grand_total = grand_total_carried
        .unwrap_or(prev.grand_total)
        .checked_add(summary.net_ttc)
        .ok_or_else(sum_err)?;
    let closed_by_name = ctx.session.display_name.clone();
    let mut session_after = summary.session.clone();
    session_after.status = super::SessionStatus::Closed;
    session_after.closed_at = Some(ctx.now);
    session_after.closed_by = Some(ctx.session.user_id.clone());
    session_after.expected_cash = Some(expected);
    session_after.counted_cash = Some(counted_cash);
    session_after.variance = Some(variance);
    session_after.z_number = Some(z_number.clone());
    session_after.notes = notes.clone();
    let mut report = ZReport {
        z_number: z_number.clone(),
        z_seq,
        summary: SessionSummary {
            session: session_after,
            ..summary
        },
        counts: input.counts.clone(),
        counted_cash,
        variance,
        grand_total,
        grand_total_carried,
        audit_anchor,
        document_anchors,
        closed_at: ctx.now,
        closed_by: ctx.session.user_id.clone(),
        closed_by_name,
        approved_by,
        notes: notes.clone(),
        prev_hash: prev.hash.clone(),
        hash: String::new(),
        offline_pending,
    };
    report.hash = closure_hash(&prev.hash, &serde_json::to_value(&report)?);
    tx.execute(
        "UPDATE cash_sessions SET status = 'closed', closed_at = ?2, closed_by = ?3, expected_cash = ?4,
             counted_cash = ?5, variance = ?6, z_number = ?7, notes = ?8 WHERE id = ?1",
        params![session.id, ctx.now, ctx.session.user_id, expected, counted_cash, variance, z_number, notes],
    )?;
    tx.execute(
        "INSERT INTO cash_closures (session_id, terminal_id, z_seq, z_number, closed_at, closed_by, business_date,
             report, grand_total, audit_last_seq, audit_last_hash, audit_intact, documents_anchor, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            session.id,
            ctx.session.terminal_id,
            z_seq,
            z_number,
            ctx.now,
            ctx.session.user_id,
            ctx.business_day(),
            serde_json::to_string(&report)?,
            grand_total,
            report.audit_anchor.last_seq,
            report.audit_anchor.last_hash,
            report.audit_anchor.intact,
            serde_json::to_string(&report.document_anchors)?,
            report.prev_hash,
            report.hash
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "cash.session_closed",
            "cash_session",
            Some(&session.id),
            json!({
                "zNumber": z_number,
                "expected": expected,
                "counted": counted_cash,
                "variance": variance,
                "grandTotal": grand_total,
                "auditLastSeq": report.audit_anchor.last_seq,
                "approvedBy": report.approved_by,
                "offlinePending": report.offline_pending,
            }),
        ),
    )?;
    Ok(report)
}

/// The stored Z report of a closed session (reprint).
pub fn z_report(conn: &Connection, session_id: &str) -> Result<ZReport, AppError> {
    let text: String = conn
        .query_row(
            "SELECT report FROM cash_closures WHERE session_id = ?1",
            [session_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("cash_session"))?;
    Ok(serde_json::from_str(&text)?)
}

/// One `cash_closures` row: the hashed report and the plain columns kept next to it for
/// queries and lists.
struct ClosureRow {
    session_id: String,
    terminal_id: String,
    z_seq: i64,
    z_number: String,
    closed_at: i64,
    closed_by: String,
    business_date: String,
    report: String,
    grand_total: Money,
    audit_last_seq: i64,
    audit_last_hash: String,
    audit_intact: bool,
    documents_anchor: String,
    prev_hash: String,
    hash: String,
}

impl ClosureRow {
    /// Every column says what the report says.
    fn agrees_with(&self, z: &ZReport) -> bool {
        let documents: Option<Vec<DocAnchor>> = serde_json::from_str(&self.documents_anchor).ok();
        let business_date = crate::time::business_date(z.closed_at)
            .format("%Y-%m-%d")
            .to_string();
        self.session_id == z.summary.session.id
            && self.terminal_id == z.summary.session.terminal_id
            && self.z_seq == z.z_seq
            && self.z_number == z.z_number
            && self.closed_at == z.closed_at
            && self.closed_by == z.closed_by
            && self.business_date == business_date
            && self.grand_total == z.grand_total
            && self.audit_last_seq == z.audit_anchor.last_seq
            && self.audit_last_hash == z.audit_anchor.last_hash
            && self.audit_intact == z.audit_anchor.intact
            && documents.as_ref() == Some(&z.document_anchors)
            && self.prev_hash == z.prev_hash
            && self.hash == z.hash
    }
}

/// The last Z (number, grand total) of `terminal_id` in each database a journaled restore
/// replaced (`backup.restored`, `replacedMarks.closures`): this database's journal, or the till's
/// journal the main PC keeps for a till's Z (as [`audit_anchor_live`] reads them).
fn replaced_grand_totals(
    conn: &Connection,
    terminal_id: &str,
) -> Result<Vec<(i64, Money)>, AppError> {
    let payloads: Vec<String> = if crate::sync::journal::holds(conn, terminal_id)? {
        conn.prepare(
            "SELECT payload FROM lan_journal WHERE terminal_id = ?1 AND action = 'backup.restored'",
        )?
        .query_map([terminal_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?
    } else {
        conn.prepare("SELECT payload FROM audit_log WHERE action = 'backup.restored'")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    let mut marks = Vec::new();
    for payload in payloads {
        let closures = serde_json::from_str::<Value>(&payload)
            .ok()
            .and_then(|p| p.pointer("/replacedMarks/closures").cloned())
            .and_then(|c| serde_json::from_value::<Vec<crate::db::ClosureMark>>(c).ok())
            .unwrap_or_default();
        marks.extend(
            closures
                .into_iter()
                .filter(|m| m.terminal_id == terminal_id)
                .map(|m| (m.z_seq, m.grand_total)),
        );
    }
    Ok(marks)
}

/// The grand total a Z following `last_seq` (at `grand_total`) goes on from instead: the highest
/// one a replaced database printed on a Z this one does not have, when it is above `grand_total`.
fn carried(marks: &[(i64, Money)], last_seq: i64, grand_total: Money) -> Option<Money> {
    marks
        .iter()
        .filter(|(seq, _)| *seq > last_seq)
        .map(|(_, total)| *total)
        .max()
        .filter(|total| *total > grand_total)
}

/// Checks the closure chain of a terminal (AUD-004/005, CSH-007): each stored report hashes to
/// its hash and links to the previous one, its columns agree with it, the Z numbers follow each
/// other from 1 (stepping over the journaled skips of a restore only, `numbering::next_after`)
/// and the grand total grows by each day's net (from the replaced database's after a journaled
/// restore, `grand_total_carried`). Closures deleted at the end are caught like the
/// audit journal's: the terminal's Z counter says how many Z were issued, and every closed
/// session names its Z.
pub fn verify_closures(
    conn: &Connection,
    terminal_id: &str,
) -> Result<ClosureChainReport, AppError> {
    let code: Option<String> = conn
        .query_row(
            "SELECT code FROM terminals WHERE id = ?1",
            [terminal_id],
            |r| r.get(0),
        )
        .optional()?;
    let skips = match &code {
        // The Z counter never restarts: its key has no fiscal year (`numbering::assign_z`).
        Some(code) => numbering::journaled_skips(conn, numbering::Z_COUNTER, code)?,
        None => Vec::new(),
    };
    let mut stmt = conn.prepare(
        "SELECT session_id, terminal_id, z_seq, z_number, closed_at, closed_by, business_date, report,
                grand_total, audit_last_seq, audit_last_hash, audit_intact, documents_anchor, prev_hash, hash
         FROM cash_closures WHERE terminal_id = ?1 ORDER BY z_seq",
    )?;
    let rows = stmt
        .query_map([terminal_id], |r| {
            Ok(ClosureRow {
                session_id: r.get(0)?,
                terminal_id: r.get(1)?,
                z_seq: r.get(2)?,
                z_number: r.get(3)?,
                closed_at: r.get(4)?,
                closed_by: r.get(5)?,
                business_date: r.get(6)?,
                report: r.get(7)?,
                grand_total: r.get(8)?,
                audit_last_seq: r.get(9)?,
                audit_last_hash: r.get(10)?,
                audit_intact: r.get(11)?,
                documents_anchor: r.get(12)?,
                prev_hash: r.get(13)?,
                hash: r.get(14)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut report = ClosureChainReport {
        rows: 0,
        first_broken_z: None,
        last_hash: GENESIS_HASH.to_owned(),
    };
    let marks = replaced_grand_totals(conn, terminal_id)?;
    let (mut last_seq, mut grand_total) = (0_i64, Money::ZERO);
    for row in rows {
        report.rows += 1;
        if report.first_broken_z.is_some() {
            continue;
        }
        let verified = serde_json::from_str::<Value>(&row.report)
            .ok()
            .filter(|value| {
                row.prev_hash == report.last_hash && closure_hash(&row.prev_hash, value) == row.hash
            })
            .and_then(|value| serde_json::from_value::<ZReport>(value).ok())
            .filter(|z| {
                row.agrees_with(z)
                    && z.z_seq == numbering::next_after(&skips, last_seq)
                    // A restore's journaled grand total is the only step it may take.
                    && z.grand_total_carried == carried(&marks, last_seq, grand_total)
                    && z
                        .grand_total_carried
                        .unwrap_or(grand_total)
                        .checked_add(z.summary.net_ttc)
                        == Some(z.grand_total)
            });
        let verified = match verified {
            Some(z) if anchors_live(conn, &z)? => Some(z),
            _ => None,
        };
        match verified {
            Some(z) => {
                last_seq = z.z_seq;
                grand_total = z.grand_total;
                report.last_hash = row.hash;
            }
            None => report.first_broken_z = Some(row.z_number),
        }
    }
    if report.first_broken_z.is_none() {
        report.first_broken_z = missing_z(conn, terminal_id, code.as_deref(), &skips, last_seq)?;
    }
    Ok(report)
}

/// The journal row at a Z's audit anchor still carries the anchored hash (seq 0: nothing was
/// anchored yet). A till's Z on the main PC anchors into the till's journal kept there
/// (`lan_journal`, M5); every other Z into this database's `audit_log`.
fn audit_anchor_live(
    conn: &Connection,
    terminal_id: &str,
    seq: i64,
    hash: &str,
) -> Result<bool, AppError> {
    if seq == 0 {
        return Ok(true);
    }
    let live: Option<String> = if crate::sync::journal::holds(conn, terminal_id)? {
        crate::sync::journal::hash_at(conn, terminal_id, seq)?
    } else {
        conn.query_row("SELECT hash FROM audit_log WHERE seq = ?1", [seq], |r| {
            r.get(0)
        })
        .optional()?
    };
    Ok(live.as_deref() == Some(hash))
}

/// A Z's document anchor still names a live row: the document at `last_seq` of its chain
/// carries `last_hash`, and the counters of that series issued at least `last_seq` numbers
/// (journaled skips excepted, `numbering::issued`).
/// Anchored documents deleted together with a counter rewind, or a chain rewritten
/// consistently after the Z, fail here (AUD-004/005).
fn doc_anchor_live(conn: &Connection, anchor: &DocAnchor) -> Result<bool, AppError> {
    if anchor.last_seq == 0 {
        return Ok(true);
    }
    let live: Option<Option<String>> = conn
        .query_row(
            "SELECT hash FROM documents WHERE doc_type = ?1 AND series = ?2 AND chain_seq = ?3",
            params![anchor.doc_type, anchor.series, anchor.last_seq],
            |r| r.get(0),
        )
        .optional()?;
    let issued = numbering::issued(conn, anchor.doc_type.as_str(), &anchor.series)?;
    Ok(live.flatten().as_deref() == Some(anchor.last_hash.as_str()) && issued >= anchor.last_seq)
}

/// Every anchor of a verified Z still names a live row of the chains it anchors.
fn anchors_live(conn: &Connection, z: &ZReport) -> Result<bool, AppError> {
    if !audit_anchor_live(
        conn,
        &z.summary.session.terminal_id,
        z.audit_anchor.last_seq,
        &z.audit_anchor.last_hash,
    )? {
        return Ok(false);
    }
    for anchor in &z.document_anchors {
        if !doc_anchor_live(conn, anchor)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The first Z the terminal issued after `last_seq` whose closure is gone: its Z counter is past
/// the last closure and the journaled `skips` of the terminal code, or a closed session has no
/// closure (the counter wound back as well).
fn missing_z(
    conn: &Connection,
    terminal_id: &str,
    code: Option<&str>,
    skips: &[numbering::Skip],
    last_seq: i64,
) -> Result<Option<String>, AppError> {
    if let Some(code) = code {
        let issued = numbering::last_number(conn, numbering::Z_COUNTER, code, 0)?;
        let next = numbering::next_after(skips, last_seq);
        if issued >= next {
            return Ok(Some(numbering::format_z_number(code, next)));
        }
    }
    Ok(conn
        .query_row(
            "SELECT z_number FROM cash_sessions s
             WHERE s.terminal_id = ?1 AND s.status = 'closed'
               AND NOT EXISTS (SELECT 1 FROM cash_closures c WHERE c.session_id = s.id)
             ORDER BY s.z_number LIMIT 1",
            [terminal_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Closes, then checkpoints the WAL and takes a verified backup (CSH-011). In memory there is
/// nothing to back up.
pub fn close_session_with_backup(
    db: &Db,
    ctx: TillContext,
    input: CloseInput,
) -> Result<Closed, AppError> {
    close_session_with_backup_lan(db, ctx, input, None)
}

/// [`close_session_with_backup`] for a till closing with `offline_pending` operations still to
/// send (an offline Z, M5).
pub fn close_session_with_backup_lan(
    db: &Db,
    ctx: TillContext,
    input: CloseInput,
    offline_pending: Option<i64>,
) -> Result<Closed, AppError> {
    let z = db.write(move |tx| close_session_with(tx, &ctx, input, offline_pending))?;
    if db.path().is_none() {
        return Ok(Closed {
            z,
            backup: None,
            backup_error: None,
        });
    }
    let after = db.checkpoint().and_then(|_| db.backup(BackupKind::Auto));
    Ok(match after {
        Ok(info) => Closed {
            z,
            backup: Some(info),
            backup_error: None,
        },
        Err(e) => {
            tracing::warn!(error = %e, "backup after Z close failed");
            Closed {
                z,
                backup: None,
                backup_error: Some(e.code()),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::session::OverrideGrant;
    use crate::cash::{ExpenseCategory, ManualKind, expense, move_cash};
    use crate::lang::Lang;
    use crate::money::Qty;
    use crate::payments::PaymentInput;
    use crate::returns::{ReturnInput, ReturnLineInput, ReturnReason, create_return};
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::testkit::{NOON, Shop, shop};
    use serde_json::json;

    fn counts(dinars_2000: i64, dinars_100_coins: i64) -> CloseInput {
        CloseInput {
            counts: vec![
                CountLine {
                    denomination: Money(200_000),
                    count: dinars_2000,
                },
                CountLine {
                    denomination: Money(10_000),
                    count: dinars_100_coins,
                },
            ],
            notes: None,
        }
    }

    /// Float 1 000 DA; a 450 DA cash sale (+5 DA duty); a 2 000 DA sale on card; the return of
    /// the milk in cash; 100 DA paid in; a 50 DA expense. Expected cash: 1 055 DA.
    fn busy_day(shop: &Shop) -> String {
        let milk = shop.product("Lait", Money(45_000), &shop.tva19);
        let bread = shop.product("Pain", Money(200_000), &shop.exempt);
        let session = shop.open_till(100_000);
        let ctx = shop.till();
        let (m, b) = (milk.id.clone(), bread.id.clone());
        shop.db
            .write(move |tx| {
                let first = create_sale(
                    tx,
                    &ctx,
                    SaleInput {
                        cart: Cart::new(Lang::Fr, vec![CartLine::product(&m, Qty(1_000))]),
                        payments: vec![PaymentInput::cash(Money(50_000))],
                    },
                )?;
                create_sale(
                    tx,
                    &ctx,
                    SaleInput {
                        cart: Cart::new(Lang::Fr, vec![CartLine::product(&b, Qty(1_000))]),
                        payments: vec![PaymentInput::electronic(
                            PaymentMethod::CardCib,
                            Money(200_000),
                            Some("A-1"),
                        )],
                    },
                )?;
                create_return(
                    tx,
                    &ctx,
                    ReturnInput {
                        original_id: Some(first.document.id.clone()),
                        lines: vec![ReturnLineInput {
                            origin_line_id: Some(first.document.lines[0].id.clone()),
                            product_id: None,
                            qty: Qty(1_000),
                            unit_price: None,
                            restock: true,
                            reason: ReturnReason::Defective,
                            note: None,
                        }],
                        refund_method: PaymentMethod::Cash,
                        refund_reference: None,
                        lang: Lang::Fr,
                        note: None,
                    },
                )?;
                move_cash(tx, &ctx, ManualKind::PaidIn, Money(10_000), Some("Monnaie"))?;
                expense(tx, &ctx, ExpenseCategory::Transport, Money(5_000), None)?;
                Ok(())
            })
            .unwrap();
        session.id
    }

    #[test]
    fn the_x_report_sums_the_day_without_closing_it() {
        let shop = shop();
        let session = busy_day(&shop);
        let id = session.clone();
        let x = shop.db.read(move |c| x_report(c, &id, true)).unwrap();
        assert_eq!(
            (
                x.tickets,
                x.returns,
                x.terminal_code.as_str(),
                x.cashier_name.as_str()
            ),
            (2, 1, "C1", "Yacine")
        );
        assert_eq!(
            (x.first_ticket.as_deref(), x.last_ticket.as_deref()),
            (Some("TK2026-C1-000001"), Some("TK2026-C1-000002"))
        );
        assert_eq!(
            (x.sales_ttc, x.returns_ttc, x.net_ttc),
            (Money(245_000), Money(-45_000), Money(200_000))
        );
        assert_eq!(
            (x.stamp_charged, x.stamp_absorbed),
            (Money(500), Money::ZERO)
        );
        assert_eq!(
            (x.opening_float, x.cash_sales, x.cash_refunds),
            (Money(100_000), Money(45_500), Money(-45_000))
        );
        assert_eq!(
            (x.paid_in, x.expenses_total),
            (Money(10_000), Money(-5_000))
        );
        assert_eq!(
            x.expenses,
            [ExpenseTotal {
                category: "transport".into(),
                amount: Money(5_000)
            }]
        );
        assert_eq!(x.expected_cash, Some(Money(105_500)));
        let card = x
            .by_method
            .iter()
            .find(|m| m.method == PaymentMethod::CardCib)
            .unwrap();
        assert_eq!((card.received, card.count), (Money(200_000), 1));
        // TVA: the milk sold then returned nets to zero; the bread is exempt.
        let tva19 = x.per_rate.iter().find(|r| r.rate_bp.0 == 1_900).unwrap();
        assert_eq!((tva19.base_ht, tva19.tax), (Money::ZERO, Money::ZERO));
        assert!(x.show_tax && !x.ifu_mention && !x.blind);
        // Blind (CSH-004): the counts and the card line, nothing that adds up to the drawer.
        let blind = shop.db.read(move |c| x_report(c, &session, false)).unwrap();
        assert!(blind.blind);
        assert_eq!(blind.expected_cash, None);
        assert_eq!((blind.tickets, blind.returns), (2, 1));
        assert_eq!(
            blind.first_ticket.as_deref(),
            Some("TK2026-C1-000001"),
            "ticket numbers stay"
        );
        assert_eq!(
            blind
                .by_method
                .iter()
                .map(|m| (m.method, m.net))
                .collect::<Vec<_>>(),
            [(PaymentMethod::CardCib, Money(200_000))]
        );
        for figure in [
            blind.opening_float,
            blind.session.opening_float,
            blind.cash_sales,
            blind.cash_refunds,
            blind.paid_in,
            blind.paid_out,
            blind.drops,
            blind.expenses_total,
            blind.sales_ttc,
            blind.returns_ttc,
            blind.net_ttc,
            blind.discount_total,
            blind.stamp_charged,
            blind.stamp_absorbed,
            blind.rounding,
        ] {
            assert_eq!(figure, Money::ZERO);
        }
        assert!(blind.expenses.is_empty() && blind.per_rate.is_empty());
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM cash_sessions WHERE status = 'open'"),
            1
        );
    }

    #[test]
    fn the_x_report_shows_the_cash_versements() {
        let shop = shop();
        let session = cash_versement_day(&shop);
        let x = shop.db.read(move |c| x_report(c, &session, true)).unwrap();
        // 1 000 DA in cash: 10 tranches × 1 DA = 10 DA of duty, charged to the customer.
        assert_eq!(x.credit_payments, Money(101_000));
        assert_eq!(x.expected_cash, Some(Money(601_000)));
    }

    #[test]
    fn the_z_hashes_the_versements_and_an_m1a_z_without_them_still_chains() {
        let shop = shop();
        cash_versement_day(&shop);
        // 6 010 DA expected: three 2 000 DA notes and one 10 DA coin.
        let count = CloseInput {
            counts: vec![
                CountLine {
                    denomination: Money(200_000),
                    count: 3,
                },
                CountLine {
                    denomination: Money(1_000),
                    count: 1,
                },
            ],
            notes: None,
        };
        let z = close(&shop, count);
        assert_eq!(
            (z.summary.credit_payments, z.variance),
            (Money(101_000), Money::ZERO)
        );
        assert_eq!(verify(&shop).first_broken_z, None);
        tamper(
            &shop,
            "DROP TRIGGER cash_closures_no_update;
             UPDATE cash_closures SET report = json_set(report, '$.summary.creditPayments', 0);",
        );
        assert_eq!(verify(&shop).first_broken_z.as_deref(), Some("ZC1-000001"));
        // A Z stored by M1a has no `creditPayments`: it still verifies, reads as 0, and the next
        // Z builds on it.
        let shop = crate::testkit::shop();
        shop.open_till(0);
        let z = close(&shop, CloseInput::default());
        let mut m1a = serde_json::to_value(&z).unwrap();
        m1a["summary"]
            .as_object_mut()
            .unwrap()
            .remove("creditPayments");
        let hash = closure_hash(&z.prev_hash, &m1a);
        m1a["hash"] = json!(hash);
        let text = m1a.to_string();
        shop.db
            .write(move |tx| {
                tx.execute_batch("DROP TRIGGER cash_closures_no_update")?;
                tx.execute(
                    "UPDATE cash_closures SET report = ?1, hash = ?2",
                    params![text, hash],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(verify(&shop).first_broken_z, None);
        let session = z.summary.session.id.clone();
        let stored = shop.db.read(move |c| z_report(c, &session)).unwrap();
        assert_eq!(stored.summary.credit_payments, Money::ZERO);
        shop.open_till(0);
        let z2 = close(&shop, CloseInput::default());
        assert_eq!(z2.prev_hash, stored.hash);
        assert_eq!(verify(&shop).first_broken_z, None);
    }

    /// Karim owes 1 000 DA; the till opens with 5 000 DA; he pays the 1 000 DA in cash (+10 DA
    /// of duty). Returns the session id.
    fn cash_versement_day(shop: &Shop) -> String {
        let karim = shop.customer("Karim");
        let ctx = shop.owner_till();
        let id = karim.id.clone();
        shop.db
            .write(move |tx| {
                crate::customers::credit::set_opening(tx, &ctx, &id, Money(100_000), None)
            })
            .unwrap();
        let session = shop.open_till(500_000);
        let ctx = shop.till();
        let input = crate::customers::credit::CollectInput {
            customer_id: karim.id,
            amount: Money(100_000),
            method: crate::payments::PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            allocate_to: Vec::new(),
            note: None,
        };
        shop.db
            .write(move |tx| crate::customers::credit::collect(tx, &ctx, input))
            .unwrap();
        session.id
    }

    #[test]
    fn the_z_close_counts_numbers_anchors_and_never_reopens() {
        let shop = shop();
        let session = busy_day(&shop);
        let before_close: i64 = shop.count("SELECT MAX(seq) FROM audit_log");
        let ctx = shop.till();
        // 1 055 DA expected: one 1 000 DA note and eleven 5 DA coins.
        let input = CloseInput {
            counts: vec![
                CountLine {
                    denomination: Money(100_000),
                    count: 1,
                },
                CountLine {
                    denomination: Money(500),
                    count: 11,
                },
            ],
            notes: Some("RAS".into()),
        };
        let z = shop
            .db
            .write(move |tx| close_session(tx, &ctx, input))
            .unwrap();
        assert_eq!((z.z_number.as_str(), z.z_seq), ("ZC1-000001", 1));
        assert_eq!(
            (z.counted_cash, z.variance, z.grand_total),
            (Money(105_500), Money::ZERO, Money(200_000))
        );
        assert_eq!(
            (z.audit_anchor.last_seq, z.audit_anchor.intact),
            (before_close, true)
        );
        let tickets = z
            .document_anchors
            .iter()
            .find(|a| a.doc_type == DocType::Ticket)
            .unwrap();
        assert_eq!(
            (tickets.series.as_str(), tickets.last_seq, tickets.intact),
            ("C1", 2, true)
        );
        assert_eq!(z.prev_hash, GENESIS_HASH);
        let stored = shop.db.read(move |c| z_report(c, &session)).unwrap();
        assert_eq!(stored, z);
        assert_eq!(
            stored.summary.session.z_number.as_deref(),
            Some("ZC1-000001")
        );
        // Closed means closed: no sale, no reopening of that session.
        let ctx = shop.till();
        let err = shop
            .db
            .write(move |tx| move_cash(tx, &ctx, ManualKind::PaidIn, Money(100), Some("x")))
            .unwrap_err();
        assert_eq!(err.code(), "cash_session_closed");
        let err = shop
            .db
            .write(|tx| Ok(tx.execute("UPDATE cash_sessions SET status = 'open'", [])?))
            .unwrap_err();
        assert_eq!(err.code(), "constraint_violation");
        // Next day: a new session, the next Z number, the grand total carried on.
        shop.open_till(0);
        let ctx = shop.till();
        let z2 = shop
            .db
            .write(move |tx| close_session(tx, &ctx, CloseInput::default()))
            .unwrap();
        assert_eq!(
            (z2.z_number.as_str(), z2.grand_total, z2.prev_hash.as_str()),
            ("ZC1-000002", Money(200_000), z.hash.as_str())
        );
        let terminal = shop.terminal.id.clone();
        let chain = shop
            .db
            .read(move |c| verify_closures(c, &terminal))
            .unwrap();
        assert_eq!((chain.rows, chain.first_broken_z), (2, None));
    }

    #[test]
    fn a_large_variance_needs_a_manager() {
        let shop = shop();
        busy_day(&shop);
        let ctx = shop.till();
        let short = counts(0, 5);
        let err = shop
            .db
            .write(move |tx| close_session(tx, &ctx, short))
            .unwrap_err();
        assert_eq!(err.code(), "variance_needs_approval");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM counters WHERE doc_type = 'z_report'"),
            0,
            "no Z number used"
        );
        // The command journals the refused count in a transaction of its own (CSH-005).
        let ctx = shop.till();
        shop.db
            .write(move |tx| record_refused_close(tx, &ctx, &counts(0, 5)))
            .unwrap();
        let payload: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'cash.close_refused'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            (payload["counted"].clone(), payload["variance"].clone()),
            (json!(50_000), json!(-55_500))
        );
        let grant = OverrideGrant {
            approver_id: shop.owner.user_id.clone(),
            approver_name: "Amine".into(),
            permission: Permission::CashViewExpected,
            granted_at: NOON,
            discount_limit_bp: 10_000,
        };
        let ctx = shop.till().with_approval(grant);
        let z = shop
            .db
            .write(move |tx| close_session(tx, &ctx, counts(0, 5)))
            .unwrap();
        assert_eq!(
            (z.variance, z.approved_by.as_deref()),
            (Money(-55_500), Some(shop.owner.user_id.as_str()))
        );
        // A difference up to the threshold needs nobody.
        shop.open_till(1_000);
        shop.set("cash.variance_threshold", json!(1_000));
        let ctx = shop.till();
        let z = shop
            .db
            .write(move |tx| close_session(tx, &ctx, CloseInput::default()))
            .unwrap();
        assert_eq!(z.variance, Money(-1_000));
    }

    #[test]
    fn count_sheets_are_checked_and_a_tampered_closure_is_found() {
        let shop = shop();
        shop.open_till(0);
        let ctx = shop.till();
        let bad = CloseInput {
            counts: vec![CountLine {
                denomination: Money(500),
                count: -1,
            }],
            notes: None,
        };
        let err = shop
            .db
            .write(move |tx| close_session(tx, &ctx, bad))
            .unwrap_err();
        assert_eq!(
            (err.code(), err.payload().field),
            ("count_invalid".into(), Some("counts.0.count".into()))
        );
        let ctx = shop.till();
        let twice = CloseInput {
            counts: vec![
                CountLine {
                    denomination: Money(500),
                    count: 1,
                },
                CountLine {
                    denomination: Money(500),
                    count: 1,
                },
            ],
            notes: None,
        };
        let err = shop
            .db
            .write(move |tx| close_session(tx, &ctx, twice))
            .unwrap_err();
        assert_eq!(err.code(), "value_invalid");
        let ctx = shop.till();
        shop.db
            .write(move |tx| close_session(tx, &ctx, CloseInput::default()))
            .unwrap();
        shop.db
            .write(|tx| {
                tx.execute_batch(
                    "DROP TRIGGER cash_closures_no_update;
                     UPDATE cash_closures SET report = json_set(report, '$.grandTotal', 999999);",
                )?;
                Ok(())
            })
            .unwrap();
        let terminal = shop.terminal.id.clone();
        let chain = shop
            .db
            .read(move |c| verify_closures(c, &terminal))
            .unwrap();
        assert_eq!(chain.first_broken_z.as_deref(), Some("ZC1-000001"));
    }

    /// The count of [`busy_day`]'s 1 055 DA: one 1 000 DA note and eleven 5 DA coins.
    fn busy_day_count() -> CloseInput {
        CloseInput {
            counts: vec![
                CountLine {
                    denomination: Money(100_000),
                    count: 1,
                },
                CountLine {
                    denomination: Money(500),
                    count: 11,
                },
            ],
            notes: None,
        }
    }

    fn close(shop: &Shop, input: CloseInput) -> ZReport {
        let ctx = shop.till();
        shop.db
            .write(move |tx| close_session(tx, &ctx, input))
            .unwrap()
    }

    fn verify(shop: &Shop) -> ClosureChainReport {
        let terminal = shop.terminal.id.clone();
        shop.db
            .read(move |c| verify_closures(c, &terminal))
            .unwrap()
    }

    /// Runs `sql` as someone who can drop the append-only triggers.
    fn tamper(shop: &Shop, sql: &str) {
        let sql = sql.to_owned();
        shop.db
            .write(move |tx| {
                tx.execute_batch(&sql)?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn the_next_z_builds_on_the_hashed_report_not_on_the_plain_columns() {
        let shop = shop();
        busy_day(&shop);
        let z1 = close(&shop, busy_day_count());
        // Reset the grand total, and edit the Z's own journal row while moving the audit anchor
        // past it, in the columns that no hash covers.
        tamper(
            &shop,
            "DROP TRIGGER cash_closures_no_update;
             DROP TRIGGER audit_log_no_update;
             UPDATE audit_log SET payload = json_set(payload, '$.grandTotal', 0)
               WHERE seq = (SELECT MAX(seq) FROM audit_log);
             UPDATE cash_closures SET grand_total = 0,
               audit_last_seq = (SELECT MAX(seq) FROM audit_log),
               audit_last_hash = (SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1);",
        );
        assert_eq!(verify(&shop).first_broken_z.as_deref(), Some("ZC1-000001"));
        shop.open_till(0);
        let z2 = close(&shop, CloseInput::default());
        assert_eq!(
            z2.grand_total,
            Money(200_000),
            "the grand total is never reset"
        );
        assert_eq!(
            (z2.audit_anchor.last_seq, z2.audit_anchor.intact),
            (z1.audit_anchor.last_seq, false),
            "the journal rows after Z1 are checked again, and the edited one is found"
        );
    }

    #[test]
    fn every_closure_column_must_agree_with_its_report() {
        for (sql, broken) in [
            (
                "UPDATE cash_closures SET grand_total = grand_total + 1",
                "ZC1-000001",
            ),
            (
                "UPDATE cash_closures SET audit_last_seq = audit_last_seq + 1",
                "ZC1-000001",
            ),
            (
                "UPDATE cash_closures SET audit_last_hash = prev_hash",
                "ZC1-000001",
            ),
            (
                "UPDATE cash_closures SET audit_intact = 1 - audit_intact",
                "ZC1-000001",
            ),
            (
                "UPDATE cash_closures SET documents_anchor = '[]'",
                "ZC1-000001",
            ),
            ("UPDATE cash_closures SET z_seq = 2", "ZC1-000001"),
            (
                "UPDATE cash_closures SET z_number = 'ZC1-000009'",
                "ZC1-000009",
            ),
            (
                "UPDATE cash_closures SET closed_at = closed_at + 1",
                "ZC1-000001",
            ),
            (
                "UPDATE cash_closures SET closed_by = session_id",
                "ZC1-000001",
            ),
            (
                "UPDATE cash_closures SET business_date = '2000-01-01'",
                "ZC1-000001",
            ),
            // Moved to another terminal: this one's Z counter still says Z 1 was issued.
            (
                "UPDATE cash_closures SET terminal_id = session_id",
                "ZC1-000001",
            ),
        ] {
            let shop = shop();
            shop.open_till(0);
            close(&shop, CloseInput::default());
            assert_eq!(verify(&shop).first_broken_z, None);
            tamper(&shop, "DROP TRIGGER cash_closures_no_update");
            tamper(&shop, sql);
            assert_eq!(
                verify(&shop).first_broken_z.as_deref(),
                Some(broken),
                "{sql}"
            );
        }
    }

    #[test]
    fn a_rehashed_grand_total_still_breaks_the_running_sum() {
        let shop = shop();
        shop.open_till(0);
        let mut z = close(&shop, CloseInput::default());
        // Rewrite the last Z, rehash it and align its columns: every hash agrees.
        z.grand_total = Money(-1_000_000);
        z.hash = closure_hash(&z.prev_hash, &serde_json::to_value(&z).unwrap());
        let text = serde_json::to_string(&z).unwrap();
        shop.db
            .write(move |tx| {
                tx.execute_batch("DROP TRIGGER cash_closures_no_update")?;
                tx.execute(
                    "UPDATE cash_closures SET report = ?1, grand_total = ?2, hash = ?3",
                    params![text, z.grand_total, z.hash],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(verify(&shop).first_broken_z.as_deref(), Some("ZC1-000001"));
    }

    #[test]
    fn a_deleted_closure_is_found() {
        let shop = shop();
        shop.open_till(0);
        let z1 = close(&shop, CloseInput::default());
        shop.open_till(0);
        close(&shop, CloseInput::default());
        let chain = verify(&shop);
        assert_eq!((chain.rows, chain.first_broken_z), (2, None));
        tamper(
            &shop,
            "DROP TRIGGER cash_closures_no_delete; DELETE FROM cash_closures WHERE z_seq = 2;",
        );
        // The terminal's Z counter already handed out Z 2.
        let chain = verify(&shop);
        assert_eq!(
            (chain.rows, chain.first_broken_z.as_deref()),
            (1, Some("ZC1-000002"))
        );
        // The next Z links to Z 1, but the hole in the Z numbers stays.
        shop.open_till(0);
        let z3 = close(&shop, CloseInput::default());
        assert_eq!(
            (z3.z_number.as_str(), z3.prev_hash.as_str()),
            ("ZC1-000003", z1.hash.as_str())
        );
        let chain = verify(&shop);
        assert_eq!(
            (chain.rows, chain.first_broken_z.as_deref()),
            (2, Some("ZC1-000003"))
        );
    }

    #[test]
    fn a_closed_session_without_its_closure_is_found() {
        let shop = shop();
        for _ in 0..2 {
            shop.open_till(0);
            close(&shop, CloseInput::default());
        }
        // The Z counter wound back too: the closed session still carries its Z number.
        tamper(
            &shop,
            "DROP TRIGGER cash_closures_no_delete;
             DROP TRIGGER counters_step_by_one;
             DELETE FROM cash_closures WHERE z_seq = 2;
             UPDATE counters SET last_number = 1 WHERE doc_type = 'z_report';",
        );
        let chain = verify(&shop);
        assert_eq!(
            (chain.rows, chain.first_broken_z.as_deref()),
            (1, Some("ZC1-000002"))
        );
    }

    #[test]
    fn closing_on_disk_checkpoints_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pos.db"), crate::db::DbOptions::default()).unwrap();
        let shop = crate::testkit::shop_in(
            db,
            crate::company::TaxRegime::Reel,
            crate::company::PriceMode::Ttc,
        );
        shop.open_till(0);
        let closed =
            close_session_with_backup(&shop.db, shop.till(), CloseInput::default()).unwrap();
        assert_eq!(closed.z.z_number, "ZC1-000001");
        let backup = closed.backup.expect("a backup after the Z");
        assert_eq!(backup.kind, BackupKind::Auto);
        assert!(std::path::Path::new(&backup.path).exists());
        assert_eq!(closed.backup_error, None);
    }

    /// An IFU shop charges no TVA: its Z carries the mention, never a per-rate TVA line.
    #[test]
    fn an_ifu_session_closes_without_tva() {
        let shop = crate::testkit::shop_with(
            crate::company::TaxRegime::Ifu,
            crate::company::PriceMode::Ttc,
        );
        let milk = shop.product("Lait", Money(45_000), &shop.tva19);
        shop.open_till(0);
        let ctx = shop.till();
        let id = milk.id.clone();
        shop.db
            .write(move |tx| {
                create_sale(
                    tx,
                    &ctx,
                    SaleInput {
                        cart: Cart::new(Lang::Fr, vec![CartLine::product(&id, Qty(1_000))]),
                        payments: vec![PaymentInput::electronic(
                            PaymentMethod::CardCib,
                            Money(45_000),
                            Some("A-1"),
                        )],
                    },
                )
            })
            .unwrap();
        let z = close(&shop, CloseInput::default());
        assert_eq!(z.summary.net_ttc, Money(45_000));
        assert!(z.summary.per_rate.is_empty(), "{:?}", z.summary.per_rate);
        assert!(!z.summary.show_tax && z.summary.ifu_mention);
    }

    /// Closes a busy day, rewrites what a Z anchors with `sql`, and returns the chain report.
    fn anchored_then(sql: &str) -> ClosureChainReport {
        let shop = shop();
        busy_day(&shop);
        close(&shop, busy_day_count());
        assert_eq!(verify(&shop).first_broken_z, None);
        tamper(&shop, sql);
        verify(&shop)
    }

    #[test]
    fn a_z_whose_anchored_rows_are_gone_is_broken() {
        // The anchored ticket rewritten (every later check would start from a hash no row has).
        let rewritten = anchored_then(
            "DROP TRIGGER documents_final_once_validated;
             UPDATE documents SET hash = prev_hash WHERE doc_type = 'ticket' AND chain_seq = 2;",
        );
        assert_eq!(rewritten.first_broken_z.as_deref(), Some("ZC1-000001"));
        // The ticket counter wound back below the anchor (the anchored ticket "never issued").
        let rewound = anchored_then(
            "DROP TRIGGER counters_step_by_one;
             UPDATE counters SET last_number = 1 WHERE doc_type = 'ticket';",
        );
        assert_eq!(rewound.first_broken_z.as_deref(), Some("ZC1-000001"));
        // The journal row the Z anchors rewritten.
        let journal = anchored_then(
            "DROP TRIGGER audit_log_no_update;
             UPDATE audit_log SET hash = prev_hash
               WHERE seq = (SELECT audit_last_seq FROM cash_closures);",
        );
        assert_eq!(journal.first_broken_z.as_deref(), Some("ZC1-000001"));
    }

    #[test]
    fn the_next_z_is_broken_when_its_starting_anchor_is_gone() {
        let shop = shop();
        busy_day(&shop);
        close(&shop, busy_day_count());
        tamper(
            &shop,
            "DROP TRIGGER documents_final_once_validated;
             UPDATE documents SET hash = prev_hash WHERE doc_type = 'ticket' AND chain_seq = 2;",
        );
        shop.open_till(0);
        let z2 = close(&shop, CloseInput::default());
        let tickets = z2
            .document_anchors
            .iter()
            .find(|a| a.doc_type == DocType::Ticket)
            .unwrap();
        assert!(
            !tickets.intact,
            "no ticket after Z1, but Z1's anchor is gone"
        );
        let returns = z2
            .document_anchors
            .iter()
            .find(|a| a.doc_type == DocType::ReturnNote)
            .unwrap();
        assert!(returns.intact);
    }

    #[test]
    fn an_offline_z_says_so_and_old_reports_hash_as_before() {
        let shop = shop();
        shop.open_till(0);
        let plain = serde_json::to_value(
            shop.db
                .write({
                    let ctx = shop.till();
                    move |tx| close_session(tx, &ctx, counts(0, 0))
                })
                .unwrap(),
        )
        .unwrap();
        assert!(
            plain.get("offlinePending").is_none(),
            "absent when not offline: M1 hashes unchanged"
        );
        for absent in ["invoices", "invoicesTtc", "creditNotes", "creditNotesTtc"] {
            assert!(
                plain["summary"].get(absent).is_none(),
                "{absent}: absent without invoices, M1 hashes unchanged"
            );
        }
        shop.open_till(0);
        let ctx = shop.till();
        let z = shop
            .db
            .write(move |tx| close_session_with(tx, &ctx, counts(0, 0), Some(3)))
            .unwrap();
        assert_eq!(z.offline_pending, Some(3));
        let report = shop
            .db
            .read({
                let t = shop.terminal.id.clone();
                move |c| verify_closures(c, &t)
            })
            .unwrap();
        assert_eq!(
            report.first_broken_z, None,
            "the flag is inside the hashed report"
        );
    }

    #[test]
    fn the_invoices_and_credit_notes_of_the_session_balance_the_x_and_the_z() {
        use crate::billing::credit_notes::{
            CreditLineInput, CreditNoteInput, CreditReason, create_credit_note,
        };
        use crate::billing::drafts::tests::{cart, fixture, input, save};
        use crate::billing::validate::{ValidateInput, validate};
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        let session = shop.open_till(0).id;
        let (ctx, c) = (shop.till(), cement.id.clone());
        shop.db
            .write(move |tx| {
                create_sale(
                    tx,
                    &ctx,
                    SaleInput {
                        cart: Cart::new(Lang::Fr, vec![CartLine::product(&c, Qty(1_000))]),
                        payments: vec![PaymentInput::cash(Money(121_000))],
                    },
                )
                .map(drop)
            })
            .unwrap();
        let draft = save(
            &shop,
            None,
            input(
                crate::documents::DocType::Invoice,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(10_000))],
                ),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let invoice = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![PaymentInput::cash(Money(1_210_000))],
                    },
                )
            })
            .unwrap()
            .document;
        let ctx = shop.owner_till();
        let back = CreditNoteInput {
            invoice_id: invoice.id.clone(),
            kind: crate::documents::CreditKind::Goods,
            lines: vec![CreditLineInput {
                origin_line_id: invoice.lines[0].id.clone(),
                qty: Qty(1_000),
                restock: true,
            }],
            amount: None,
            reason: CreditReason::Defective,
            refund_method: PaymentMethod::Cash,
            refund_reference: None,
            note: None,
            second_lang: None,
        };
        shop.db
            .write(move |tx| create_credit_note(tx, &ctx, back).map(drop))
            .unwrap();
        let id = session.clone();
        let x = shop.db.read(move |c| x_report(c, &id, true)).unwrap();
        assert_eq!(
            (
                x.tickets,
                x.invoices,
                x.invoices_ttc,
                x.credit_notes,
                x.credit_notes_ttc
            ),
            (1, 1, Money(1_190_000), 1, Money(-119_000))
        );
        assert_eq!(x.net_ttc, Money(119_000 + 1_190_000 - 119_000));
        let paid = Money::checked_sum(x.by_method.iter().map(|m| m.net)).unwrap();
        assert_eq!(
            paid,
            Money(x.net_ttc.0 + x.stamp_charged.0 + x.rounding.0),
            "every tender of the session is a document the report counts"
        );
        assert_eq!(x.stamp_charged, Money(1_200 + 11_900));
        let tva = Money::checked_sum(x.per_rate.iter().map(|r| r.tax)).unwrap();
        assert_eq!(
            tva.0,
            shop.count(
                "SELECT SUM(t.tax_amount) FROM document_taxes t JOIN documents d ON d.id = t.document_id
                 WHERE d.status = 'validated' AND d.doc_type IN ('ticket', 'invoice', 'credit_note')"
            ),
            "the TVA of the session is the G50's"
        );
        let ctx = shop.owner_till();
        let z = shop
            .db
            .write(move |tx| close_session(tx, &ctx, counts(0, 0)))
            .unwrap();
        assert_eq!(
            z.grand_total, x.net_ttc,
            "the invoices are in the grand total"
        );
        let json = serde_json::to_value(&z).unwrap();
        assert_eq!(json["summary"]["invoices"], json!(1));
        assert_eq!(verify(&shop).first_broken_z, None);
    }

    /// AUD-005 after a restore: the first Z goes on from the highest grand total a replaced
    /// database printed on a Z this one lacks, once; a lower one, or one this database already
    /// passed, carries nothing.
    #[test]
    fn a_restore_carries_the_replaced_grand_total_into_one_z_only() {
        let marks = [(1, Money(36_000)), (3, Money(90_000)), (2, Money(50_000))];
        assert_eq!(carried(&marks, 0, Money::ZERO), Some(Money(90_000)));
        assert_eq!(carried(&marks, 2, Money(40_000)), Some(Money(90_000)));
        assert_eq!(carried(&marks, 3, Money(10_000)), None, "already past it");
        assert_eq!(carried(&marks, 0, Money(95_000)), None, "never lowered");
        assert_eq!(carried(&[], 0, Money::ZERO), None);
    }

    #[test]
    fn denominations_are_the_dzd_notes_and_coins() {
        assert_eq!(
            DENOMINATIONS.map(|m| m.0 / 100),
            [2_000, 1_000, 500, 200, 100, 50, 20, 10, 5]
        );
    }
}
