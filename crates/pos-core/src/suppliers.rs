//! Suppliers (spec §5.1 `suppliers`, `supplier_ledger`; PUR-001/005/006/007): a directory with
//! the identifiers the G9 état needs (M3), payment terms, and the payables ledger: supplier
//! invoices (`purchase`), payments, returns, adjustments and the opening balance. A payment in
//! cash from the drawer is a `paid_out` movement of this terminal's session.

use crate::audit::{self, Actor};
use crate::auth::Permission;
use crate::cash::{self, MovementKind};
use crate::catalog::{clean_opt, normalize};
use crate::datapack::Pack;
use crate::error::AppError;
use crate::ids::new_id;
use crate::ledger::{
    self, AgingRow, EntryKind, EntryPage, EntryQuery, LedgerEntry, NewEntry, Side, Statement,
};
use crate::money::Money;
use crate::party::{self, TEXT_MAX};
use crate::payments::{self, PaymentInput, PaymentMethod};
use crate::table::{Cell, ColumnKind, Table};
use crate::till::TillContext;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
#[cfg(test)]
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Supplier {
    pub id: String,
    pub code: String,
    pub name: String,
    pub phone: Option<String>,
    pub phone2: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub wilaya: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub rc: Option<String>,
    pub ai: Option<String>,
    pub nin: Option<String>,
    pub rib: Option<String>,
    pub payment_terms_days: Option<i64>,
    /// What the shop owes the supplier (negative: an advance paid).
    pub balance: Money,
    pub notes: Option<String>,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupplierInput {
    pub code: Option<String>,
    pub name: String,
    pub phone: Option<String>,
    pub phone2: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub wilaya: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub rc: Option<String>,
    pub ai: Option<String>,
    pub nin: Option<String>,
    pub rib: Option<String>,
    pub payment_terms_days: Option<i64>,
    pub notes: Option<String>,
}

impl SupplierInput {
    pub fn named(name: &str) -> SupplierInput {
        SupplierInput {
            name: name.to_owned(),
            ..SupplierInput::default()
        }
    }
}

impl From<&Supplier> for SupplierInput {
    fn from(s: &Supplier) -> SupplierInput {
        SupplierInput {
            code: Some(s.code.clone()),
            name: s.name.clone(),
            phone: s.phone.clone(),
            phone2: s.phone2.clone(),
            email: s.email.clone(),
            address: s.address.clone(),
            wilaya: s.wilaya.clone(),
            nif: s.nif.clone(),
            nis: s.nis.clone(),
            rc: s.rc.clone(),
            ai: s.ai.clone(),
            nin: s.nin.clone(),
            rib: s.rib.clone(),
            payment_terms_days: s.payment_terms_days,
            notes: s.notes.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupplierQuery {
    pub text: String,
    pub with_balance: bool,
    pub include_archived: bool,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupplierHit {
    pub id: String,
    pub code: String,
    pub name: String,
    pub phone: Option<String>,
    pub wilaya: Option<String>,
    pub balance: Money,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupplierPaymentInput {
    pub supplier_id: String,
    pub amount: Money,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
    /// Cash taken out of this terminal's drawer (a `paid_out` movement).
    pub from_till: bool,
    /// Supplier invoices to settle first; empty = the oldest first.
    pub allocate_to: Vec<String>,
    pub note: Option<String>,
}

const SELECT: &str =
    "SELECT id, code, name, phone, phone2, email, address, wilaya, nif, nis, rc, ai, nin,
        rib, payment_terms_days, balance, notes, archived_at IS NOT NULL, created_at, updated_at
     FROM suppliers";

fn map(r: &Row<'_>) -> rusqlite::Result<Supplier> {
    Ok(Supplier {
        id: r.get(0)?,
        code: r.get(1)?,
        name: r.get(2)?,
        phone: r.get(3)?,
        phone2: r.get(4)?,
        email: r.get(5)?,
        address: r.get(6)?,
        wilaya: r.get(7)?,
        nif: r.get(8)?,
        nis: r.get(9)?,
        rc: r.get(10)?,
        ai: r.get(11)?,
        nin: r.get(12)?,
        rib: r.get(13)?,
        payment_terms_days: r.get(14)?,
        balance: r.get(15)?,
        notes: r.get(16)?,
        archived: r.get(17)?,
        created_at: r.get(18)?,
        updated_at: r.get(19)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Supplier, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("supplier"))
}

fn active(conn: &Connection, id: &str) -> Result<Supplier, AppError> {
    let s = get(conn, id)?;
    if s.archived {
        return Err(AppError::not_found("supplier"));
    }
    Ok(s)
}

fn like(words: &[String], i: usize) -> String {
    words.get(i).map(|w| format!("%{w}%")).unwrap_or_default()
}

pub fn search(conn: &Connection, q: &SupplierQuery) -> Result<Vec<SupplierHit>, AppError> {
    hits(conn, q, i64::from(q.limit.unwrap_or(500).clamp(1, 5_000)))
}

/// [`search`] with `limit` rows at most; a negative `limit` has no bound (SQLite's `LIMIT -1`).
fn hits(conn: &Connection, q: &SupplierQuery, limit: i64) -> Result<Vec<SupplierHit>, AppError> {
    let words = normalize::words(&q.text);
    let mut stmt = conn.prepare_cached(
        "SELECT id, code, name, phone, wilaya, balance, archived_at IS NOT NULL FROM suppliers
         WHERE (?1 OR archived_at IS NULL) AND (?2 = 0 OR balance <> 0)
           AND (?3 = '' OR search_text LIKE ?3) AND (?4 = '' OR search_text LIKE ?4)
           AND (?5 = '' OR search_text LIKE ?5)
         ORDER BY name COLLATE NOCASE LIMIT ?6",
    )?;
    let rows = stmt
        .query_map(
            params![
                q.include_archived,
                q.with_balance,
                like(&words, 0),
                like(&words, 1),
                like(&words, 2),
                limit
            ],
            |r| {
                Ok(SupplierHit {
                    id: r.get(0)?,
                    code: r.get(1)?,
                    name: r.get(2)?,
                    phone: r.get(3)?,
                    wilaya: r.get(4)?,
                    balance: r.get(5)?,
                    archived: r.get(6)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn clean(pack: &Pack, input: SupplierInput) -> Result<SupplierInput, AppError> {
    if input
        .payment_terms_days
        .is_some_and(|d| !(0..=365).contains(&d))
    {
        return Err(AppError::validation("paymentTermsDays", "value_invalid"));
    }
    let code = match clean_opt(input.code.as_deref(), "code", 24)? {
        Some(code) => Some(crate::catalog::clean_code(&code, "code")?),
        None => None,
    };
    Ok(SupplierInput {
        code,
        name: party::clean_name(&input.name)?,
        phone: party::clean_phone(input.phone.as_deref(), "phone")?,
        phone2: party::clean_phone(input.phone2.as_deref(), "phone2")?,
        email: party::clean_email(input.email.as_deref(), "email")?,
        address: clean_opt(input.address.as_deref(), "address", TEXT_MAX)?,
        wilaya: party::clean_wilaya(input.wilaya.as_deref(), "wilaya", pack)?,
        nif: party::clean_tax_id(input.nif.as_deref(), "nif", pack, "nif_invalid")?,
        nis: party::clean_tax_id(input.nis.as_deref(), "nis", pack, "nis_invalid")?,
        rc: clean_opt(input.rc.as_deref(), "rc", 40)?,
        ai: clean_opt(input.ai.as_deref(), "ai", 40)?,
        nin: party::clean_nin(input.nin.as_deref(), "nin")?,
        rib: clean_opt(input.rib.as_deref(), "rib", 40)?,
        payment_terms_days: input.payment_terms_days,
        notes: clean_opt(input.notes.as_deref(), "notes", 1_000)?,
    })
}

fn search_text(s: &SupplierInput, code: &str) -> String {
    normalize::search_text(
        [
            Some(s.name.as_str()),
            Some(code),
            s.phone.as_deref(),
            s.phone2.as_deref(),
            s.nif.as_deref(),
        ]
        .into_iter()
        .flatten(),
    )
}

fn code_taken(conn: &Connection, code: &str, id: Option<&str>) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM suppliers WHERE code = ?1 AND id IS NOT ?2)",
        params![code, id],
        |r| r.get(0),
    )?)
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    input: SupplierInput,
) -> Result<Supplier, AppError> {
    let s = clean(pack, input)?;
    let code = match &s.code {
        Some(code) => code.clone(),
        None => {
            let mut code = crate::numbering::next_code(tx, "supplier_code", "F", 5)?;
            while code_taken(tx, &code, None)? {
                code = crate::numbering::next_code(tx, "supplier_code", "F", 5)?;
            }
            code
        }
    };
    if code_taken(tx, &code, None)? {
        return Err(AppError::validation("code", "code_taken"));
    }
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO suppliers (id, code, name, search_text, phone, phone2, email, address, wilaya, nif, nis,
             rc, ai, nin, rib, payment_terms_days, notes, created_at, updated_at, created_by, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18, ?19, ?20)",
        params![
            id, code, s.name, search_text(&s, &code), s.phone, s.phone2, s.email, s.address, s.wilaya, s.nif,
            s.nis, s.rc, s.ai, s.nin, s.rib, s.payment_terms_days, s.notes, now, actor.user_id, actor.terminal_id
        ],
    )?;
    let created = get(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "supplier.created",
            "supplier",
            Some(&id),
            json!({ "after": created }),
        ),
    )?;
    Ok(created)
}

pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    id: &str,
    input: SupplierInput,
) -> Result<Supplier, AppError> {
    let before = get(tx, id)?;
    let s = clean(pack, input)?;
    let code = s.code.clone().unwrap_or_else(|| before.code.clone());
    if code_taken(tx, &code, Some(id))? {
        return Err(AppError::validation("code", "code_taken"));
    }
    tx.execute(
        "UPDATE suppliers SET code = ?2, name = ?3, search_text = ?4, phone = ?5, phone2 = ?6, email = ?7,
             address = ?8, wilaya = ?9, nif = ?10, nis = ?11, rc = ?12, ai = ?13, nin = ?14, rib = ?15,
             payment_terms_days = ?16, notes = ?17, updated_at = ?18
         WHERE id = ?1",
        params![
            id, code, s.name, search_text(&s, &code), s.phone, s.phone2, s.email, s.address, s.wilaya, s.nif,
            s.nis, s.rc, s.ai, s.nin, s.rib, s.payment_terms_days, s.notes, now_ms()
        ],
    )?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "supplier.updated",
            "supplier",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    if get(tx, id)?.balance != Money::ZERO {
        return Err(AppError::conflict("balance_not_zero"));
    }
    tx.execute(
        "UPDATE suppliers SET archived_at = ?2, updated_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event("supplier.archived", "supplier", Some(id), json!({})),
    )?;
    Ok(())
}

pub fn restore(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    get(tx, id)?;
    tx.execute(
        "UPDATE suppliers SET archived_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event("supplier.restored", "supplier", Some(id), json!({})),
    )?;
    Ok(())
}

pub fn export_table(conn: &Connection, q: &SupplierQuery) -> Result<Table, AppError> {
    let mut table = Table::new(&[
        ("code", ColumnKind::Code),
        ("name", ColumnKind::Text),
        ("phone", ColumnKind::Code),
        ("email", ColumnKind::Text),
        ("address", ColumnKind::Text),
        ("wilaya", ColumnKind::Code),
        ("nif", ColumnKind::Code),
        ("nis", ColumnKind::Code),
        ("rc", ColumnKind::Code),
        ("ai", ColumnKind::Code),
        ("rib", ColumnKind::Code),
        ("paymentTermsDays", ColumnKind::Int),
        ("balance", ColumnKind::Money),
    ]);
    // The whole directory the filters select, never the search screen's first 5 000.
    for hit in hits(conn, q, -1)? {
        let s = get(conn, &hit.id)?;
        table.push(vec![
            Cell::text(s.code),
            Cell::text(s.name),
            Cell::opt_text(s.phone),
            Cell::opt_text(s.email),
            Cell::opt_text(s.address),
            Cell::opt_text(s.wilaya),
            Cell::opt_text(s.nif),
            Cell::opt_text(s.nis),
            Cell::opt_text(s.rc),
            Cell::opt_text(s.ai),
            Cell::opt_text(s.rib),
            s.payment_terms_days.map_or(Cell::NULL, Cell::int),
            Cell::money(s.balance),
        ])?;
    }
    Ok(table)
}

fn note(value: Option<&str>, required: bool) -> Result<Option<String>, AppError> {
    let n = clean_opt(value, "note", 200)?;
    if required && n.is_none() {
        return Err(AppError::validation("note", "required"));
    }
    Ok(n)
}

pub fn set_opening(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    supplier_id: &str,
    amount: Money,
    text: Option<&str>,
) -> Result<LedgerEntry, AppError> {
    active(tx, supplier_id)?;
    if amount == Money::ZERO {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    if ledger::has_opening(tx, Side::Supplier, supplier_id)? {
        return Err(AppError::conflict("opening_already_set"));
    }
    let mut entry = NewEntry::new(supplier_id, EntryKind::Opening, amount);
    entry.note = note(text, false)?;
    let actor = ctx.actor();
    let posted = ledger::append(
        tx,
        Side::Supplier,
        &actor,
        &entry,
        &ctx.business_day(),
        ctx.now,
    )?;
    audit::append(
        tx,
        &actor.event(
            "supplier.opening_set",
            "supplier",
            Some(supplier_id),
            json!({ "entryId": posted.id, "amount": amount }),
        ),
    )?;
    Ok(posted)
}

pub fn adjust(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    supplier_id: &str,
    amount: Money,
    text: &str,
) -> Result<LedgerEntry, AppError> {
    active(tx, supplier_id)?;
    if amount == Money::ZERO {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let mut entry = NewEntry::new(supplier_id, EntryKind::Adjustment, amount);
    entry.note = note(Some(text), true)?;
    let actor = ctx.actor();
    let posted = ledger::append(
        tx,
        Side::Supplier,
        &actor,
        &entry,
        &ctx.business_day(),
        ctx.now,
    )?;
    audit::append(
        tx,
        &actor.event(
            "supplier.adjusted",
            "supplier",
            Some(supplier_id),
            json!({ "entryId": posted.id, "amount": amount, "note": entry.note }),
        ),
    )?;
    Ok(posted)
}

/// A supplier invoice (purchase) or a supplier return, referencing its document (no audit: the
/// document's validation audits).
pub(crate) fn book(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    supplier_id: &str,
    kind: EntryKind,
    amount: Money,
    document_id: &str,
) -> Result<LedgerEntry, AppError> {
    active(tx, supplier_id)?;
    let entry = NewEntry::new(supplier_id, kind, amount).referencing("document", document_id);
    ledger::append(
        tx,
        Side::Supplier,
        &ctx.actor(),
        &entry,
        &ctx.business_day(),
        ctx.now,
    )
}

fn rehome(err: AppError) -> AppError {
    match err {
        AppError::Validation { field, code } => {
            AppError::validation(field.strip_prefix("payments.0.").unwrap_or(&field), &code)
        }
        other => other,
    }
}

/// A payment to a supplier (PUR-006): by bank, card, cheque, or cash — from the drawer when
/// `from_till` (needs `cash.paid_in_out` and the open session), else from the safe.
pub fn pay(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: SupplierPaymentInput,
) -> Result<LedgerEntry, AppError> {
    let supplier = active(tx, &input.supplier_id)?;
    if !matches!(
        input.method,
        PaymentMethod::Cash
            | PaymentMethod::CardCib
            | PaymentMethod::Edahabia
            | PaymentMethod::Baridimob
            | PaymentMethod::Cheque
            | PaymentMethod::Transfer
    ) || (input.from_till && input.method != PaymentMethod::Cash)
    {
        return Err(AppError::validation("method", "payment_method_invalid"));
    }
    let tender = payments::clean_tender(
        0,
        &PaymentInput {
            method: input.method,
            amount: input.amount,
            reference: input.reference.clone(),
            cheque_bank: input.cheque_bank.clone(),
            cheque_date: input.cheque_date.clone(),
        },
    )
    .map_err(rehome)?;
    let session = if input.from_till {
        let session = cash::require_open(tx, &ctx.session.terminal_id)?;
        if !ctx.allows(Permission::CashPaidInOut) {
            return Err(AppError::Permission {
                permission: Permission::CashPaidInOut.as_str().to_owned(),
            });
        }
        Some(session)
    } else {
        None
    };
    let mut entry = NewEntry::new(
        &supplier.id,
        EntryKind::Payment,
        tender
            .amount
            .checked_neg()
            .ok_or_else(|| AppError::validation("amount", "amount_invalid"))?,
    );
    entry.method = Some(tender.method);
    entry.reference = tender.reference.clone();
    entry.cheque_bank = tender.cheque_bank.clone();
    entry.cheque_date = tender.cheque_date.clone();
    entry.cash_session_id = session.as_ref().map(|s| s.id.clone());
    entry.note = note(input.note.as_deref(), false)?;
    entry.allocate_to = input.allocate_to;
    let actor = ctx.actor();
    let posted = ledger::append(
        tx,
        Side::Supplier,
        &actor,
        &entry,
        &ctx.business_day(),
        ctx.now,
    )?;
    if let Some(session) = &session {
        cash::record(
            tx,
            ctx,
            &session.id,
            MovementKind::PaidOut,
            posted.amount,
            None,
            Some(("supplier_ledger", &posted.id)),
            entry.note.as_deref(),
        )?;
    }
    audit::append(
        tx,
        &actor.event(
            "supplier.paid",
            "supplier",
            Some(&supplier.id),
            json!({ "entryId": posted.id, "amount": tender.amount, "method": tender.method,
                    "fromTill": input.from_till, "balanceAfter": posted.balance_after }),
        ),
    )?;
    Ok(posted)
}

pub fn entries(
    conn: &Connection,
    supplier_id: &str,
    q: &EntryQuery,
) -> Result<EntryPage, AppError> {
    get(conn, supplier_id)?;
    ledger::entries(conn, Side::Supplier, supplier_id, q)
}

pub fn open_items(conn: &Connection, supplier_id: &str) -> Result<Vec<LedgerEntry>, AppError> {
    get(conn, supplier_id)?;
    ledger::open_debits(conn, Side::Supplier, supplier_id)
}

pub fn statement(
    conn: &Connection,
    supplier_id: &str,
    from: &str,
    to: &str,
) -> Result<Statement, AppError> {
    ledger::statement(conn, Side::Supplier, supplier_id, from, to)
}

pub fn aging(conn: &Connection, as_of: &str) -> Result<Vec<AgingRow>, AppError> {
    ledger::aging(conn, Side::Supplier, as_of)
}

/// What the shop owes its suppliers (dashboard).
pub fn payables(conn: &Connection) -> Result<Money, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(balance), 0) FROM suppliers WHERE balance > 0",
        [],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    fn pay_input(
        supplier: &str,
        amount: i64,
        method: PaymentMethod,
        from_till: bool,
    ) -> SupplierPaymentInput {
        SupplierPaymentInput {
            supplier_id: supplier.into(),
            amount: Money(amount),
            method,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            from_till,
            allocate_to: Vec::new(),
            note: None,
        }
    }

    #[test]
    fn a_supplier_gets_a_code_and_is_found_by_name_or_phone() {
        let shop = shop();
        let mut input = SupplierInput::named("Grossiste Bab Ezzouar");
        input.phone = Some("0550 11 22 33".into());
        input.rib = Some("00400123456789012345".into());
        input.payment_terms_days = Some(30);
        let pack = Arc::clone(&shop.pack);
        let actor = shop.owner.actor();
        let s = shop
            .db
            .write(move |tx| create(tx, &actor, &pack, input))
            .unwrap();
        assert_eq!(
            (s.code.as_str(), s.phone.as_deref()),
            ("F00001", Some("0550112233"))
        );
        let q = SupplierQuery {
            text: "ezzouar".into(),
            ..SupplierQuery::default()
        };
        assert_eq!(shop.db.read(move |c| search(c, &q)).unwrap().len(), 1);
        let q = SupplierQuery {
            text: "0550".into(),
            ..SupplierQuery::default()
        };
        assert_eq!(shop.db.read(move |c| search(c, &q)).unwrap().len(), 1);
    }

    #[test]
    fn paying_from_the_drawer_moves_cash_and_settles_the_oldest_debt() {
        let shop = shop();
        let s = shop.supplier("Grossiste Bab Ezzouar");
        let ctx = shop.owner_till();
        let id = s.id.clone();
        shop.db
            .write(move |tx| set_opening(tx, &ctx, &id, Money(3_000_000), Some("reliquat 2025")))
            .unwrap();
        let ctx = shop.owner_till();
        let payment = pay_input(&s.id, 1_000_000, PaymentMethod::Cash, true);
        assert_eq!(
            shop.db
                .write({
                    let p = payment.clone();
                    move |tx| pay(tx, &ctx, p)
                })
                .unwrap_err()
                .code(),
            "cash_session_closed"
        );
        shop.open_till(2_000_000);
        let ctx = shop.owner_till();
        let paid = shop.db.write(move |tx| pay(tx, &ctx, payment)).unwrap();
        assert_eq!(
            (paid.amount, paid.balance_after),
            (Money(-1_000_000), Money(2_000_000))
        );
        assert_eq!(shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'paid_out' AND ref_type = 'supplier_ledger'"), -1_000_000);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'supplier.paid'"),
            1
        );
    }

    #[test]
    fn bank_payments_need_their_details_and_never_touch_the_drawer() {
        let shop = shop();
        let s = shop.supplier("Cevital Distribution");
        let ctx = shop.owner_till();
        let transfer = pay_input(&s.id, 500_000, PaymentMethod::Transfer, false);
        assert_eq!(
            shop.db
                .write(move |tx| pay(tx, &ctx, transfer))
                .unwrap_err(),
            AppError::validation("reference", "transfer_reference_required")
        );
        let ctx = shop.owner_till();
        let mut transfer = pay_input(&s.id, 500_000, PaymentMethod::Transfer, false);
        transfer.reference = Some("VIR-2026-0931".into());
        let paid = shop.db.write(move |tx| pay(tx, &ctx, transfer)).unwrap();
        assert_eq!(
            paid.balance_after,
            Money(-500_000),
            "an advance to the supplier"
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM cash_movements"), 0);
        let ctx = shop.owner_till();
        let from_till_by_card = pay_input(&s.id, 1_000, PaymentMethod::CardCib, true);
        assert_eq!(
            shop.db
                .write(move |tx| pay(tx, &ctx, from_till_by_card))
                .unwrap_err(),
            AppError::validation("method", "payment_method_invalid")
        );
    }

    #[test]
    fn statements_ageing_and_archive_rules() {
        let shop = shop();
        let s = shop.supplier("Grossiste Bab Ezzouar");
        let ctx = shop.owner_till();
        let id = s.id.clone();
        shop.db
            .write(move |tx| book(tx, &ctx, &id, EntryKind::Purchase, Money(800_000), "doc-1"))
            .unwrap();
        let st = shop
            .db
            .read({
                let id = s.id.clone();
                move |c| statement(c, &id, "2026-09-01", "2026-09-30")
            })
            .unwrap();
        assert_eq!((st.debits, st.closing), (Money(800_000), Money(800_000)));
        assert_eq!(
            shop.db.read(|c| aging(c, "2026-09-28")).unwrap()[0].current,
            Money(800_000)
        );
        assert_eq!(shop.db.read(payables).unwrap(), Money(800_000));
        let actor = shop.owner.actor();
        let id = s.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| archive(tx, &actor, &id))
                .unwrap_err(),
            AppError::conflict("balance_not_zero")
        );
    }

    #[test]
    fn the_export_holds_every_supplier_past_the_search_limit() {
        let shop = shop();
        let (actor, pack) = (shop.owner.actor(), Arc::clone(&shop.pack));
        shop.db
            .write(move |tx| {
                for i in 0..5_001 {
                    create(
                        tx,
                        &actor,
                        &pack,
                        SupplierInput::named(&format!("Grossiste {i:05}")),
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let all = SupplierQuery::default();
        let table = shop.db.read(move |c| export_table(c, &all)).unwrap();
        assert_eq!(table.rows.len(), 5_001, "never cut at 5 000 rows");
    }
}
