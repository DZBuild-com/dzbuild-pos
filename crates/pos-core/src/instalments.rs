//! Instalment sales, vente à tempérament (master spec §23): the contract is a validated ticket
//! whose credit part goes to the customer's kridi account (the sale counts at signature, CRD-008);
//! the schedule splits that credit part into monthly dues; payments are M2 versements that settle
//! the contract's sale first (their droit de timbre, receipts and drawer are M2's); a due's state
//! is derived from the allocations, never stored. Guarantees are neutral supporting documents — a
//! closed list without any cheque (TMP-009: a "guarantee cheque" exposes the customer to criminal
//! proceedings). Penalties and scoring are V1/V2 (TMP-004, TMP-008).
//!
//! LAN: the contract is a till sale (its plan rides with its ticket); the versements are M2's.

use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::billing::settings::Trilingual;
use crate::catalog::clean_opt;
use crate::catalog::price_levels::{self, PriceLevelInput};
use crate::customers::credit::{self, CollectInput, Collected};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::Money;
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, SaleExtras, SaleInput, ValidatedSale};
use crate::sqlenum::sql_text_enum;
use crate::tax::timbre::{self, StampTreatment};
use crate::till::TillContext;
use chrono::{Datelike, Days, Months, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The longest schedule the schema accepts (`instalments.max_count` stays within it).
pub const MAX_COUNT: u8 = 60;

/// The code of the instalment price level (TMP-002).
pub const LEVEL_CODE: &str = "instalment";

/// One monthly due of a schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    pub seq: u8,
    /// `YYYY-MM-DD`.
    pub due_on: String,
    pub amount: Money,
}

fn date_invalid() -> AppError {
    AppError::validation("firstDueOn", "date_invalid")
}

fn parse_day(day: &str, field: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| AppError::validation(field, "date_invalid"))
}

fn iso(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

/// The monthly dues of `financed`: whole dinars each, the last one taking the rest; the dates
/// keep the first due's day, clamped to the end of shorter months.
pub fn schedule(financed: Money, count: u8, first_due: NaiveDate) -> Result<Vec<Due>, AppError> {
    if !(1..=MAX_COUNT).contains(&count) {
        return Err(AppError::validation("count", "instalment_count_invalid"));
    }
    if !financed.is_positive() {
        return Err(AppError::validation("down", "instalment_nothing_financed"));
    }
    let n = i64::from(count);
    let mut base = financed.0 / n / 100 * 100;
    if base == 0 {
        // Less than a dinar a month: centimes, so that every due stays positive.
        base = financed.0 / n;
    }
    if base == 0 {
        return Err(AppError::validation("count", "instalment_count_invalid"));
    }
    let last = financed.0 - base * (n - 1);
    let day = first_due.day();
    let first_month = first_due.with_day(1).ok_or_else(date_invalid)?;
    (0..count)
        .map(|i| {
            let month = first_month
                .checked_add_months(Months::new(u32::from(i)))
                .ok_or_else(date_invalid)?;
            let month_end = month
                .checked_add_months(Months::new(1))
                .and_then(|d| d.pred_opt())
                .map_or(28, |d| d.day());
            let date = month
                .with_day(day.min(month_end))
                .ok_or_else(date_invalid)?;
            Ok(Due {
                seq: i + 1,
                due_on: iso(date),
                amount: Money(if i + 1 == count { last } else { base }),
            })
        })
        .collect()
}

/// The supporting documents a shop may keep a copy or a note of. A closed list: never a cheque
/// (TMP-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GuaranteeKind {
    IdDocument,
    WorkCertificate,
    PaySlip,
    CcpAccount,
    ResidenceCertificate,
    Guarantor,
    Other,
}

impl GuaranteeKind {
    pub const ALL: [GuaranteeKind; 7] = [
        GuaranteeKind::IdDocument,
        GuaranteeKind::WorkCertificate,
        GuaranteeKind::PaySlip,
        GuaranteeKind::CcpAccount,
        GuaranteeKind::ResidenceCertificate,
        GuaranteeKind::Guarantor,
        GuaranteeKind::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            GuaranteeKind::IdDocument => "id_document",
            GuaranteeKind::WorkCertificate => "work_certificate",
            GuaranteeKind::PaySlip => "pay_slip",
            GuaranteeKind::CcpAccount => "ccp_account",
            GuaranteeKind::ResidenceCertificate => "residence_certificate",
            GuaranteeKind::Guarantor => "guarantor",
            GuaranteeKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<GuaranteeKind> {
        GuaranteeKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(GuaranteeKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuaranteeInput {
    pub kind: GuaranteeKind,
    /// The guarantor's or the document holder's name.
    pub holder_name: Option<String>,
    /// The document's number or a short description ("Fiche de paie août 2026").
    pub reference: Option<String>,
    pub phone: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Guarantee {
    pub id: String,
    pub kind: GuaranteeKind,
    pub holder_name: Option<String>,
    pub reference: Option<String>,
    pub phone: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QuoteInput {
    /// The goods, with the contract's customer (and the instalment price level, TMP-002).
    pub cart: Cart,
    /// The down payment, before any stamp duty; 0 for none.
    pub down: Money,
    pub down_method: PaymentMethod,
    pub count: u8,
    /// `YYYY-MM-DD`, after the signature day.
    pub first_due_on: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InstalmentQuote {
    pub total: Money,
    pub down: Money,
    /// The droit de timbre on the cash down payment.
    pub stamp_duty: Money,
    /// What the customer hands over at signature: the down payment and the duty charged.
    pub down_to_receive: Money,
    /// The credit part, split into the dues.
    pub financed: Money,
    pub dues: Vec<Due>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SignInput {
    pub quote: QuoteInput,
    /// The down payment's card authorisation, cheque number or transfer reference.
    pub down_reference: Option<String>,
    pub cheque_bank: Option<String>,
    /// `YYYY-MM-DD`.
    pub cheque_date: Option<String>,
    #[serde(default)]
    pub guarantees: Vec<GuaranteeInput>,
    #[serde(default)]
    pub extras: SaleExtras,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Signed {
    pub plan: Plan,
    /// The contract's ticket.
    pub sale: ValidatedSale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DueStatus {
    Paid,
    /// Partly paid, not late yet.
    Partial,
    /// Due today, nothing paid yet.
    Due,
    /// Past its date and not fully paid.
    Overdue,
    Upcoming,
}

impl DueStatus {
    pub const ALL: [DueStatus; 5] = [
        DueStatus::Paid,
        DueStatus::Partial,
        DueStatus::Due,
        DueStatus::Overdue,
        DueStatus::Upcoming,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            DueStatus::Paid => "paid",
            DueStatus::Partial => "partial",
            DueStatus::Due => "due",
            DueStatus::Overdue => "overdue",
            DueStatus::Upcoming => "upcoming",
        }
    }

    pub fn parse(s: &str) -> Option<DueStatus> {
        DueStatus::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(DueStatus);

/// A due as it stands on a day: what was paid on it (the payments fill the oldest dues first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DueState {
    pub seq: u8,
    /// `YYYY-MM-DD`.
    pub due_on: String,
    pub amount: Money,
    pub paid: Money,
    pub status: DueStatus,
    /// Days past the due date while not fully paid; 0 otherwise.
    pub days_late: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    /// `CT2026-000001`.
    pub number: String,
    pub customer_id: String,
    pub customer_name: String,
    pub document_id: String,
    pub document_number: String,
    /// The contract sale's entry in the kridi notebook.
    pub credit_entry_id: String,
    pub total: Money,
    pub down_payment: Money,
    pub financed: Money,
    pub count: u8,
    pub first_due_on: String,
    pub dues: Vec<DueState>,
    /// Paid on the credit part (the versements allocated to the contract's sale).
    pub paid: Money,
    pub remaining: Money,
    pub guarantees: Vec<Guarantee>,
    pub business_date: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanFilter {
    pub customer_id: Option<String>,
    /// Only the contracts with at least one overdue due.
    pub overdue_only: bool,
    /// Matches the contract number or the customer's name.
    pub text: String,
}

/// A versement on a contract (TMP-003): cash, card, BaridiMob, cheque, CCP transfer or direct
/// debit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanPayment {
    pub amount: Money,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    pub note: Option<String>,
}

/// A due to remind the customer of (TMP-007).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub plan_id: String,
    pub number: String,
    pub customer_name: String,
    pub due: DueState,
}

type CleanGuarantee = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// The checked texts of a guarantee; `prefix` homes its field errors (`guarantees.0.`).
fn clean_guarantee(prefix: &str, g: &GuaranteeInput) -> Result<CleanGuarantee, AppError> {
    let f = |n: &str| format!("{prefix}{n}");
    Ok((
        clean_opt(g.holder_name.as_deref(), &f("holderName"), 120)?,
        clean_opt(g.reference.as_deref(), &f("reference"), 60)?,
        crate::party::clean_phone(g.phone.as_deref(), &f("phone"))?,
        clean_opt(g.note.as_deref(), &f("note"), 200)?,
    ))
}

fn credit_tender(amount: Money) -> PaymentInput {
    PaymentInput {
        method: PaymentMethod::Credit,
        amount,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }
}

/// What signing would do: the price at the cart's level, the down payment's duty (cash only),
/// what to receive now, the credit part and its schedule. Nothing is written.
pub fn quote(
    conn: &Connection,
    ctx: &TillContext,
    input: &QuoteInput,
) -> Result<InstalmentQuote, AppError> {
    packs::require(conn, Module::Instalments)?;
    if input.cart.customer_id.is_none() {
        return Err(AppError::validation("customerId", "customer_required"));
    }
    let max: i64 = crate::settings::get(conn, "instalments.max_count")?;
    if input.count == 0 || i64::from(input.count) > max {
        return Err(AppError::validation("count", "instalment_count_invalid"));
    }
    if input.down.is_negative() {
        return Err(AppError::validation("down", "amount_invalid"));
    }
    if !matches!(
        input.down_method,
        PaymentMethod::Cash
            | PaymentMethod::CardCib
            | PaymentMethod::Edahabia
            | PaymentMethod::Baridimob
            | PaymentMethod::Cheque
            | PaymentMethod::Transfer
    ) {
        return Err(AppError::validation("downMethod", "payment_method_invalid"));
    }
    let first = parse_day(&input.first_due_on, "firstDueOn")?;
    if first <= ctx.business_date() {
        return Err(date_invalid());
    }
    let priced = crate::sales::cart::price_cart(conn, ctx, &input.cart)?;
    let total = priced.totals.total_ttc;
    let financed = total
        .checked_sub(input.down)
        .ok_or_else(|| AppError::validation("down", "amount_invalid"))?;
    let dues = schedule(financed, input.count, first)?;
    // The duty is the sale's own: its settlement on the cash part. A card or mobile down payment
    // is exempt and a cheque or transfer one is signed with its details (the sale refuses it
    // without them), so a non-cash down payment that the data pack does not exempt is refused.
    let (stamp_duty, stamp_charged) = if input.down_method.is_cash() {
        if input.down.is_positive() {
            let settled = crate::payments::preview(
                &ctx.pack,
                total,
                &[PaymentInput::cash(input.down), credit_tender(financed)],
                crate::sales::cart::settle_rules(conn)?,
            )?;
            (settled.stamp_duty, settled.stamp_charged)
        } else {
            (Money::ZERO, Money::ZERO)
        }
    } else if timbre::treatment(&ctx.pack, input.down_method) == StampTreatment::Dutiable {
        return Err(AppError::validation("downMethod", "payment_method_invalid"));
    } else {
        (Money::ZERO, Money::ZERO)
    };
    Ok(InstalmentQuote {
        total,
        down: input.down,
        stamp_duty,
        down_to_receive: input
            .down
            .checked_add(stamp_charged)
            .ok_or_else(|| AppError::validation("down", "amount_invalid"))?,
        financed,
        dues,
    })
}

/// Signs a contract (TMP-001, TMP-002): the goods are sold on one ticket to the customer — the
/// down payment (and its duty) received now, the rest on the kridi account — then the plan, its
/// schedule and its supporting documents are recorded. One `instalments.signed` row (the sale
/// writes its own).
pub fn sign(tx: &Transaction<'_>, ctx: &TillContext, input: SignInput) -> Result<Signed, AppError> {
    let q = quote(tx, ctx, &input.quote)?;
    let customer_id = input.quote.cart.customer_id.clone().unwrap_or_default();
    let note = clean_opt(input.note.as_deref(), "note", 200)?;
    let guarantees = input
        .guarantees
        .iter()
        .enumerate()
        .map(|(i, g)| clean_guarantee(&format!("guarantees.{i}."), g).map(|c| (g.kind, c)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut payments = Vec::new();
    if q.down.is_positive() {
        payments.push(PaymentInput {
            method: input.quote.down_method,
            amount: q.down_to_receive,
            reference: input.down_reference.clone(),
            cheque_bank: input.cheque_bank.clone(),
            cheque_date: input.cheque_date.clone(),
        });
    }
    payments.push(credit_tender(q.financed));
    let sale = crate::sales::create_sale_with(
        tx,
        ctx,
        SaleInput {
            cart: input.quote.cart.clone(),
            payments,
        },
        &input.extras,
    )?;
    let credit_entry_id: String = tx.query_row(
        "SELECT id FROM customer_ledger WHERE ref_type = 'document' AND ref_id = ?1 AND kind = 'sale_credit'",
        [&sale.document.id],
        |r| r.get(0),
    )?;
    let number =
        crate::numbering::assign_custom(tx, "instalment_plan", "CT", ctx.business_date())?.number;
    let first_due_on = q.dues.first().map(|d| d.due_on.clone()).unwrap_or_default();
    let id = new_id();
    tx.execute(
        "INSERT INTO instalment_plans (id, number, customer_id, document_id, credit_entry_id, total, down_payment, financed,
                                       count, first_due_on, note, business_date, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            id,
            number,
            customer_id,
            sale.document.id,
            credit_entry_id,
            q.total,
            q.down,
            q.financed,
            input.quote.count,
            first_due_on,
            note,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    for d in &q.dues {
        tx.execute(
            "INSERT INTO instalment_dues (plan_id, seq, due_on, amount) VALUES (?1, ?2, ?3, ?4)",
            params![id, d.seq, d.due_on, d.amount],
        )?;
    }
    for (kind, cleaned) in &guarantees {
        insert_guarantee(tx, ctx, &id, *kind, cleaned)?;
    }
    audit::append(
        tx,
        &ctx.actor().event(
            "instalments.signed",
            "instalment_plan",
            Some(&id),
            json!({
                "number": number,
                "documentId": sale.document.id,
                "total": q.total,
                "down": q.down,
                "financed": q.financed,
                "count": input.quote.count,
                "guarantees": guarantees.len()
            }),
        ),
    )?;
    Ok(Signed {
        plan: get(tx, &id, &ctx.business_day())?,
        sale,
    })
}

fn insert_guarantee(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    plan_id: &str,
    kind: GuaranteeKind,
    (holder, reference, phone, note): &CleanGuarantee,
) -> Result<String, AppError> {
    let id = new_id();
    tx.execute(
        "INSERT INTO instalment_guarantees (id, plan_id, kind, holder_name, reference, phone, note, created_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, plan_id, kind, holder, reference, phone, note, ctx.now, ctx.session.user_id],
    )?;
    Ok(id)
}

/// Adds a supporting document to a signed contract (TMP-006). One `instalments.guarantee_added`
/// row.
pub fn add_guarantee(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    plan_id: &str,
    g: GuaranteeInput,
) -> Result<Guarantee, AppError> {
    packs::require(tx, Module::Instalments)?;
    plan_exists(tx, plan_id)?;
    let cleaned = clean_guarantee("", &g)?;
    let id = insert_guarantee(tx, ctx, plan_id, g.kind, &cleaned)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "instalments.guarantee_added",
            "instalment_plan",
            Some(plan_id),
            json!({ "kind": g.kind }),
        ),
    )?;
    guarantees(tx, plan_id)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| AppError::not_found("instalment_plan"))
}

fn plan_exists(conn: &Connection, id: &str) -> Result<(), AppError> {
    let found: bool = conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM instalment_plans WHERE id = ?1)")?
        .query_row([id], |r| r.get(0))?;
    if found {
        Ok(())
    } else {
        Err(AppError::not_found("instalment_plan"))
    }
}

/// The supporting documents of a contract, in the order they were given.
pub fn guarantees(conn: &Connection, plan_id: &str) -> Result<Vec<Guarantee>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, kind, holder_name, reference, phone, note, created_at
         FROM instalment_guarantees WHERE plan_id = ?1 ORDER BY created_at, rowid",
    )?;
    let rows = stmt
        .query_map([plan_id], |r| {
            Ok(Guarantee {
                id: r.get(0)?,
                kind: r.get(1)?,
                holder_name: r.get(2)?,
                reference: r.get(3)?,
                phone: r.get(4)?,
                note: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// What the customer paid on the contract: the allocations to its credit entry; with `as_of`
/// (`YYYY-MM-DD`), only those from payments of that day or before, as the ledger stood then.
fn paid_on(
    conn: &Connection,
    credit_entry_id: &str,
    as_of: Option<&str>,
) -> Result<Money, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT COALESCE(SUM(a.amount), 0) FROM customer_allocations a
             JOIN customer_ledger c ON c.id = a.credit_id
             WHERE a.debit_id = ?1 AND (?2 IS NULL OR c.business_date <= ?2)",
        )?
        .query_row(params![credit_entry_id, as_of], |r| r.get(0))?)
}

/// The dues as they stand on `today`: `paid` fills the oldest first.
fn states(dues: &[Due], mut paid: Money, today: NaiveDate) -> Vec<DueState> {
    dues.iter()
        .map(|d| {
            let on_it = paid.min(d.amount).max(Money::ZERO);
            paid = paid.checked_sub(on_it).unwrap_or(Money::ZERO);
            let due_on = NaiveDate::parse_from_str(&d.due_on, "%Y-%m-%d").ok();
            let late = due_on.map_or(0, |dd| (today - dd).num_days().max(0));
            let status = if on_it == d.amount {
                DueStatus::Paid
            } else if late > 0 {
                DueStatus::Overdue
            } else if on_it.is_positive() {
                DueStatus::Partial
            } else if due_on == Some(today) {
                DueStatus::Due
            } else {
                DueStatus::Upcoming
            };
            DueState {
                seq: d.seq,
                due_on: d.due_on.clone(),
                amount: d.amount,
                paid: on_it,
                status,
                days_late: if status == DueStatus::Paid { 0 } else { late },
            }
        })
        .collect()
}

fn map_plan(r: &Row<'_>) -> rusqlite::Result<Plan> {
    Ok(Plan {
        id: r.get(0)?,
        number: r.get(1)?,
        customer_id: r.get(2)?,
        customer_name: r.get(3)?,
        document_id: r.get(4)?,
        document_number: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
        credit_entry_id: r.get(6)?,
        total: r.get(7)?,
        down_payment: r.get(8)?,
        financed: r.get(9)?,
        count: r.get(10)?,
        first_due_on: r.get(11)?,
        business_date: r.get(12)?,
        created_at: r.get(13)?,
        dues: Vec::new(),
        paid: Money::ZERO,
        remaining: Money::ZERO,
        guarantees: Vec::new(),
    })
}

/// A contract with its dues as they stand on `today` (`YYYY-MM-DD`) and its supporting documents.
pub fn get(conn: &Connection, id: &str, today: &str) -> Result<Plan, AppError> {
    load(conn, id, today, false)
}

/// [`get`] as the ledger stood at the end of `day` (`YYYY-MM-DD`): the payments recorded after
/// it are left out, so a past day's dues read as they were then (TMP-005 overdue report).
pub fn get_as_of(conn: &Connection, id: &str, day: &str) -> Result<Plan, AppError> {
    load(conn, id, day, true)
}

fn load(conn: &Connection, id: &str, today: &str, as_of: bool) -> Result<Plan, AppError> {
    let cut_off = as_of.then_some(today);
    let today = parse_day(today, "today")?;
    let mut plan = conn
        .prepare_cached(
            "SELECT p.id, p.number, p.customer_id, c.name, p.document_id, d.number, p.credit_entry_id, p.total,
                    p.down_payment, p.financed, p.count, p.first_due_on, p.business_date, p.created_at
             FROM instalment_plans p
             JOIN customers c ON c.id = p.customer_id
             JOIN documents d ON d.id = p.document_id
             WHERE p.id = ?1",
        )?
        .query_row([id], map_plan)
        .optional()?
        .ok_or_else(|| AppError::not_found("instalment_plan"))?;
    let dues = conn
        .prepare_cached(
            "SELECT seq, due_on, amount FROM instalment_dues WHERE plan_id = ?1 ORDER BY seq",
        )?
        .query_map([id], |r| {
            Ok(Due {
                seq: r.get(0)?,
                due_on: r.get(1)?,
                amount: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    plan.paid = paid_on(conn, &plan.credit_entry_id, cut_off)?.min(plan.financed);
    plan.remaining = plan.financed.checked_sub(plan.paid).unwrap_or(Money::ZERO);
    plan.dues = states(&dues, plan.paid, today);
    plan.guarantees = guarantees(conn, id)?;
    Ok(plan)
}

/// The contracts by customer and text (number, customer's name), newest first, at most 500;
/// with `overdue_only`, those with a due overdue on `today`.
pub fn list(conn: &Connection, filter: &PlanFilter, today: &str) -> Result<Vec<Plan>, AppError> {
    parse_day(today, "today")?;
    let text = filter.text.trim();
    let ids = conn
        .prepare_cached(
            "SELECT p.id FROM instalment_plans p JOIN customers c ON c.id = p.customer_id
             WHERE (?1 IS NULL OR p.customer_id = ?1)
               AND (?2 = '' OR p.number LIKE '%' || ?2 || '%' OR c.name LIKE '%' || ?2 || '%')
             ORDER BY p.created_at DESC, p.rowid DESC
             LIMIT 500",
        )?
        .query_map(params![filter.customer_id, text], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut plans = Vec::with_capacity(ids.len());
    for id in ids {
        let plan = get(conn, &id, today)?;
        if !filter.overdue_only || plan.dues.iter().any(|d| d.status == DueStatus::Overdue) {
            plans.push(plan);
        }
    }
    Ok(plans)
}

/// Every due not fully paid whose date is on or before `today` + `within_days` — the late ones
/// and those coming — oldest first (TMP-005, TMP-007).
pub fn reminders(
    conn: &Connection,
    today: &str,
    within_days: u32,
) -> Result<Vec<Reminder>, AppError> {
    let day = parse_day(today, "today")?;
    let until = iso(day
        .checked_add_days(Days::new(u64::from(within_days)))
        .ok_or_else(|| AppError::validation("today", "date_invalid"))?);
    let ids = conn
        .prepare_cached(
            "SELECT p.id FROM instalment_plans p
             WHERE p.financed > (SELECT COALESCE(SUM(a.amount), 0) FROM customer_allocations a
                                 WHERE a.debit_id = p.credit_entry_id)
               AND EXISTS (SELECT 1 FROM instalment_dues d WHERE d.plan_id = p.id AND d.due_on <= ?1)
             ORDER BY p.created_at, p.rowid",
        )?
        .query_map([&until], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    for id in ids {
        let plan = get(conn, &id, today)?;
        for due in plan.dues {
            if due.status != DueStatus::Paid && due.due_on <= until {
                out.push(Reminder {
                    plan_id: plan.id.clone(),
                    number: plan.number.clone(),
                    customer_name: plan.customer_name.clone(),
                    due,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.due
            .due_on
            .cmp(&b.due.due_on)
            .then_with(|| a.number.cmp(&b.number))
    });
    Ok(out)
}

fn due_of(plan: &Plan, seq: u8) -> Result<&DueState, AppError> {
    plan.dues
        .iter()
        .find(|d| d.seq == seq)
        .ok_or_else(|| AppError::validation("seq", "instalment_due_unknown"))
}

/// The reminder text of a due (SMS/WhatsApp, copied by the cashier: nothing is sent from the
/// app) in `lang`, from `instalments.reminder_message`: what is left on the due on `today`.
pub fn reminder_text(
    conn: &Connection,
    plan_id: &str,
    seq: u8,
    lang: Lang,
    today: &str,
) -> Result<String, AppError> {
    let plan = get(conn, plan_id, today)?;
    let due = due_of(&plan, seq)?;
    let left = due
        .amount
        .checked_sub(due.paid)
        .unwrap_or(Money::ZERO)
        .max(Money::ZERO);
    let templates: Trilingual = crate::settings::get(conn, "instalments.reminder_message")?;
    let shop = crate::company::name_in(conn, lang)?;
    Ok(templates
        .in_lang(lang)
        .unwrap_or_default()
        .replace("{shop}", &shop)
        .replace("{number}", &plan.number)
        .replace("{amount}", &left.to_string())
        .replace("{date}", &due.due_on))
}

/// Notes that the customer was reminded of a due (TMP-007). One `instalments.reminder_noted` row.
pub fn note_reminder(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    plan_id: &str,
    seq: u8,
    note: Option<String>,
) -> Result<(), AppError> {
    packs::require(tx, Module::Instalments)?;
    let plan = get(tx, plan_id, &ctx.business_day())?;
    due_of(&plan, seq)?;
    let note = clean_opt(note.as_deref(), "note", 200)?;
    tx.execute(
        "INSERT INTO instalment_events (id, plan_id, kind, due_seq, note, created_at, user_id)
         VALUES (?1, ?2, 'reminder_given', ?3, ?4, ?5, ?6)",
        params![new_id(), plan_id, seq, note, ctx.now, ctx.session.user_id],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "instalments.reminder_noted",
            "instalment_plan",
            Some(plan_id),
            json!({ "number": plan.number, "seq": seq }),
        ),
    )?;
    Ok(())
}

/// A versement on a contract (TMP-003): an M2 versement of the contract's customer that settles
/// the contract's sale first (any excess goes to the oldest other debts, then to an advance).
/// Never refused while the module is off: a customer may always pay what they owe.
pub fn collect(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    plan_id: &str,
    pay: PlanPayment,
) -> Result<Collected, AppError> {
    if !ctx.allows(Permission::CreditCollect) {
        return Err(AppError::Permission {
            permission: Permission::CreditCollect.as_str().to_owned(),
        });
    }
    let plan = get(tx, plan_id, &ctx.business_day())?;
    credit::collect(
        tx,
        ctx,
        CollectInput {
            customer_id: plan.customer_id,
            amount: pay.amount,
            method: pay.method,
            reference: pay.reference,
            cheque_bank: pay.cheque_bank,
            cheque_date: pay.cheque_date,
            allocate_to: vec![plan.credit_entry_id],
            note: pay.note.or(Some(plan.number)),
        },
    )
}

/// Creates the instalment price level (TMP-002: the instalment price is that level's price; the
/// till selects it for a contract) unless a level has its code; the number created.
pub fn seed_level(tx: &Transaction<'_>, actor: &Actor) -> Result<u32, AppError> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM price_levels WHERE code = ?1)",
        [LEVEL_CODE],
        |r| r.get(0),
    )?;
    if exists {
        return Ok(0);
    }
    let level = price_levels::create(
        tx,
        actor,
        PriceLevelInput {
            code: LEVEL_CODE.into(),
            name_ar: "البيع بالتقسيط".into(),
            name_fr: "Tempérament".into(),
            name_en: "Instalment".into(),
            sort_order: 5,
        },
    )?;
    audit::append(
        tx,
        &actor.event(
            "instalments.level_seeded",
            "price_level",
            Some(&level.id),
            json!({ "code": LEVEL_CODE }),
        ),
    )?;
    Ok(1)
}
