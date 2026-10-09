//! Customers (spec §5.1 `customers`; CRM-001/002/003/004/005/006): a directory keyed by the
//! phone number, with B2B identifiers, a price level and a standing discount (PRC-002/003,
//! PRM-005), a credit limit and the cached kridi balance moved only by `customer_ledger`.

pub mod credit;
pub mod receipts;

use crate::audit::{self, Actor};
use crate::catalog::{clean_opt, normalize, price_levels};
use crate::datapack::Pack;
use crate::documents::DocumentSummary;
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Rate};
use crate::party::{self, NAME_MAX, TEXT_MAX};
use crate::sqlenum::sql_text_enum;
use crate::table::{Cell, ColumnKind, Table, TagDomain};
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
#[cfg(test)]
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CustomerType {
    #[default]
    Individual,
    Reseller,
    Garage,
    Contractor,
    Restaurant,
    Company,
    Other,
}

impl CustomerType {
    pub const ALL: [CustomerType; 7] = [
        CustomerType::Individual,
        CustomerType::Reseller,
        CustomerType::Garage,
        CustomerType::Contractor,
        CustomerType::Restaurant,
        CustomerType::Company,
        CustomerType::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            CustomerType::Individual => "individual",
            CustomerType::Reseller => "reseller",
            CustomerType::Garage => "garage",
            CustomerType::Contractor => "contractor",
            CustomerType::Restaurant => "restaurant",
            CustomerType::Company => "company",
            CustomerType::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<CustomerType> {
        CustomerType::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

sql_text_enum!(CustomerType);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    pub id: String,
    pub code: String,
    pub name: String,
    pub customer_type: CustomerType,
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
    /// Legal form and nature of the activity of a business buyer (décret 05-468 art. 3-2°).
    pub legal_form: Option<String>,
    pub activity: Option<String>,
    /// Exempt from VAT (TAX-004): invoiced at HT prices, rate 0, with its attestation number.
    pub vat_exempt: bool,
    pub vat_exempt_ref: Option<String>,
    pub ids_verified_at: Option<String>,
    pub price_level_id: Option<String>,
    pub discount_bp: Rate,
    pub credit_limit: Option<Money>,
    /// Positive: the customer owes the shop. Negative: store credit or an advance.
    pub balance: Money,
    /// What can still be sold on credit: the limit less the debt, never below 0; `None` without
    /// a limit.
    pub available_credit: Option<Money>,
    /// Store credit the customer can spend: the negative part of `balance`.
    pub store_credit: Money,
    pub lang: Option<Lang>,
    pub notes: Option<String>,
    pub tags: Option<String>,
    pub is_blocked: bool,
    pub is_favourite: bool,
    pub marketing_consent: bool,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// What can still be sold on credit under `limit` (the limit less the debt, never below 0),
/// `None` without a limit.
pub fn available_credit(limit: Option<Money>, balance: Money) -> Option<Money> {
    limit.map(|limit| {
        limit
            .checked_sub(balance.max(Money::ZERO))
            .unwrap_or(Money::ZERO)
            .max(Money::ZERO)
    })
}

/// Store credit the customer can spend (PAY-011): the negative part of the balance.
pub fn store_credit(balance: Money) -> Money {
    if balance.is_negative() {
        balance.checked_neg().unwrap_or(Money::ZERO)
    } else {
        Money::ZERO
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerInput {
    /// Kept when importing a directory; `None` = next `C00001…`.
    pub code: Option<String>,
    pub name: String,
    pub customer_type: CustomerType,
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
    #[serde(default)]
    pub legal_form: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
    /// Needs `vat_exempt_ref`, the number of the exemption attestation.
    #[serde(default)]
    pub vat_exempt: bool,
    #[serde(default)]
    pub vat_exempt_ref: Option<String>,
    pub ids_verified_at: Option<String>,
    pub price_level_id: Option<String>,
    pub discount_bp: Rate,
    pub credit_limit: Option<Money>,
    pub lang: Option<Lang>,
    pub notes: Option<String>,
    pub tags: Option<String>,
    pub is_favourite: bool,
    pub marketing_consent: bool,
}

impl CustomerInput {
    pub fn named(name: &str) -> CustomerInput {
        CustomerInput {
            name: name.to_owned(),
            ..CustomerInput::default()
        }
    }
}

impl From<&Customer> for CustomerInput {
    fn from(c: &Customer) -> CustomerInput {
        CustomerInput {
            code: Some(c.code.clone()),
            name: c.name.clone(),
            customer_type: c.customer_type,
            phone: c.phone.clone(),
            phone2: c.phone2.clone(),
            email: c.email.clone(),
            address: c.address.clone(),
            wilaya: c.wilaya.clone(),
            nif: c.nif.clone(),
            nis: c.nis.clone(),
            rc: c.rc.clone(),
            ai: c.ai.clone(),
            nin: c.nin.clone(),
            legal_form: c.legal_form.clone(),
            activity: c.activity.clone(),
            vat_exempt: c.vat_exempt,
            vat_exempt_ref: c.vat_exempt_ref.clone(),
            ids_verified_at: c.ids_verified_at.clone(),
            price_level_id: c.price_level_id.clone(),
            discount_bp: c.discount_bp,
            credit_limit: c.credit_limit,
            lang: c.lang,
            notes: c.notes.clone(),
            tags: c.tags.clone(),
            is_favourite: c.is_favourite,
            marketing_consent: c.marketing_consent,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerQuery {
    pub text: String,
    pub customer_type: Option<CustomerType>,
    /// Only customers who owe money.
    pub with_debt: bool,
    pub blocked: Option<bool>,
    pub include_archived: bool,
    /// 1..=5000, default 500.
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerHit {
    pub id: String,
    pub code: String,
    pub name: String,
    pub customer_type: CustomerType,
    pub phone: Option<String>,
    pub wilaya: Option<String>,
    pub balance: Money,
    pub credit_limit: Option<Money>,
    /// What can still be sold on credit: the limit less the debt, never below 0; `None` without
    /// a limit.
    pub available_credit: Option<Money>,
    /// Store credit the customer can spend: the negative part of `balance`.
    pub store_credit: Money,
    pub is_blocked: bool,
    pub is_favourite: bool,
    pub archived: bool,
}

const SELECT: &str = "SELECT id, code, name, customer_type, phone, phone2, email, address, wilaya,
        nif, nis, rc, ai, nin, ids_verified_at, price_level_id, discount_bp, credit_limit, balance,
        lang, notes, tags, is_blocked, is_favourite, marketing_consent, archived_at IS NOT NULL,
        created_at, updated_at, legal_form, activity, vat_exempt, vat_exempt_ref FROM customers";

fn map(r: &Row<'_>) -> rusqlite::Result<Customer> {
    let credit_limit: Option<Money> = r.get(17)?;
    let balance: Money = r.get(18)?;
    Ok(Customer {
        id: r.get(0)?,
        code: r.get(1)?,
        name: r.get(2)?,
        customer_type: r.get(3)?,
        phone: r.get(4)?,
        phone2: r.get(5)?,
        email: r.get(6)?,
        address: r.get(7)?,
        wilaya: r.get(8)?,
        nif: r.get(9)?,
        nis: r.get(10)?,
        rc: r.get(11)?,
        ai: r.get(12)?,
        nin: r.get(13)?,
        legal_form: r.get(28)?,
        activity: r.get(29)?,
        vat_exempt: r.get(30)?,
        vat_exempt_ref: r.get(31)?,
        ids_verified_at: r.get(14)?,
        price_level_id: r.get(15)?,
        discount_bp: r.get(16)?,
        credit_limit,
        balance,
        available_credit: available_credit(credit_limit, balance),
        store_credit: store_credit(balance),
        lang: r.get(19)?,
        notes: r.get(20)?,
        tags: r.get(21)?,
        is_blocked: r.get(22)?,
        is_favourite: r.get(23)?,
        marketing_consent: r.get(24)?,
        archived: r.get(25)?,
        created_at: r.get(26)?,
        updated_at: r.get(27)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Customer, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("customer"))
}

/// The customer chosen for a ticket, cart or return note: an unknown or archived one gives
/// `customerId`/`customer_not_found`.
pub(crate) fn selectable(conn: &Connection, id: &str) -> Result<Customer, AppError> {
    match get(conn, id) {
        Ok(c) if !c.archived => Ok(c),
        Ok(_) | Err(AppError::NotFound { .. }) => {
            Err(AppError::validation("customerId", "customer_not_found"))
        }
        Err(e) => Err(e),
    }
}

/// The active customer with this phone number (typed in any form), if any.
pub fn find_by_phone(conn: &Connection, raw: &str) -> Result<Option<Customer>, AppError> {
    let Some(phone) = party::clean_phone(Some(raw), "phone").ok().flatten() else {
        return Ok(None);
    };
    Ok(conn
        .prepare_cached(&format!(
            "{SELECT} WHERE phone = ?1 AND archived_at IS NULL"
        ))?
        .query_row([phone], map)
        .optional()?)
}

fn like(words: &[String], i: usize) -> String {
    words.get(i).map(|w| format!("%{w}%")).unwrap_or_default()
}

/// Directory search (SAL-042, CRM): every typed word must appear in the name, code, phone,
/// NIF or tags, whatever the script; favourites first, then by name.
pub fn search(conn: &Connection, q: &CustomerQuery) -> Result<Vec<CustomerHit>, AppError> {
    hits(conn, q, i64::from(q.limit.unwrap_or(500).clamp(1, 5_000)))
}

/// [`search`] with `limit` rows at most; a negative `limit` has no bound (SQLite's `LIMIT -1`).
fn hits(conn: &Connection, q: &CustomerQuery, limit: i64) -> Result<Vec<CustomerHit>, AppError> {
    let words = normalize::words(&q.text);
    let mut stmt = conn.prepare_cached(
        "SELECT id, code, name, customer_type, phone, wilaya, balance, credit_limit, is_blocked,
                is_favourite, archived_at IS NOT NULL
         FROM customers
         WHERE (?1 OR archived_at IS NULL)
           AND (?2 IS NULL OR customer_type = ?2)
           AND (?3 = 0 OR balance > 0)
           AND (?4 IS NULL OR is_blocked = ?4)
           AND (?5 = '' OR search_text LIKE ?5)
           AND (?6 = '' OR search_text LIKE ?6)
           AND (?7 = '' OR search_text LIKE ?7)
           AND (?8 = '' OR search_text LIKE ?8)
         ORDER BY is_favourite DESC, name COLLATE NOCASE
         LIMIT ?9",
    )?;
    let rows = stmt
        .query_map(
            params![
                q.include_archived,
                q.customer_type,
                q.with_debt,
                q.blocked,
                like(&words, 0),
                like(&words, 1),
                like(&words, 2),
                like(&words, 3),
                limit
            ],
            |r| {
                let balance: Money = r.get(6)?;
                let credit_limit: Option<Money> = r.get(7)?;
                Ok(CustomerHit {
                    id: r.get(0)?,
                    code: r.get(1)?,
                    name: r.get(2)?,
                    customer_type: r.get(3)?,
                    phone: r.get(4)?,
                    wilaya: r.get(5)?,
                    balance,
                    credit_limit,
                    available_credit: available_credit(credit_limit, balance),
                    store_credit: store_credit(balance),
                    is_blocked: r.get(8)?,
                    is_favourite: r.get(9)?,
                    archived: r.get(10)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn clean(conn: &Connection, pack: &Pack, input: CustomerInput) -> Result<CustomerInput, AppError> {
    let name = party::clean_name(&input.name)?;
    let code = match clean_opt(input.code.as_deref(), "code", 24)? {
        Some(code) => Some(crate::catalog::clean_code(&code, "code")?),
        None => None,
    };
    if !(0..=10_000).contains(&input.discount_bp.0) {
        return Err(AppError::validation("discountBp", "value_invalid"));
    }
    if input.credit_limit.is_some_and(Money::is_negative) {
        return Err(AppError::validation("creditLimit", "amount_invalid"));
    }
    if let Some(level) = &input.price_level_id {
        price_levels::active(conn, level)
            .map_err(|_| AppError::validation("priceLevelId", "price_level_not_found"))?;
    }
    let cleaned = CustomerInput {
        code,
        name,
        customer_type: input.customer_type,
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
        legal_form: clean_opt(input.legal_form.as_deref(), "legalForm", 120)?,
        activity: clean_opt(input.activity.as_deref(), "activity", 120)?,
        vat_exempt: input.vat_exempt,
        vat_exempt_ref: clean_opt(input.vat_exempt_ref.as_deref(), "vatExemptRef", 60)?,
        ids_verified_at: party::clean_day(input.ids_verified_at.as_deref(), "idsVerifiedAt")?,
        price_level_id: input.price_level_id,
        discount_bp: input.discount_bp,
        credit_limit: input.credit_limit,
        lang: input.lang,
        notes: clean_opt(input.notes.as_deref(), "notes", 1_000)?,
        tags: clean_opt(input.tags.as_deref(), "tags", NAME_MAX)?,
        is_favourite: input.is_favourite,
        marketing_consent: input.marketing_consent,
    };
    // TAX-004: an exemption is proven by its attestation.
    if cleaned.vat_exempt && cleaned.vat_exempt_ref.is_none() {
        return Err(AppError::validation(
            "vatExemptRef",
            "vat_exempt_ref_required",
        ));
    }
    // CRM-006: a reseller is identified (CIDTA 183 ter).
    if cleaned.customer_type == CustomerType::Reseller {
        for (field, value) in [
            ("nif", &cleaned.nif),
            ("rc", &cleaned.rc),
            ("ai", &cleaned.ai),
            ("address", &cleaned.address),
        ] {
            if value.is_none() {
                return Err(AppError::validation(field, "required"));
            }
        }
    }
    Ok(cleaned)
}

fn search_text(c: &CustomerInput, code: &str) -> String {
    normalize::search_text(
        [
            Some(c.name.as_str()),
            Some(code),
            c.phone.as_deref(),
            c.phone2.as_deref(),
            c.nif.as_deref(),
            c.tags.as_deref(),
        ]
        .into_iter()
        .flatten(),
    )
}

fn check_unique(
    conn: &Connection,
    id: Option<&str>,
    code: &str,
    phone: Option<&str>,
) -> Result<(), AppError> {
    let code_taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM customers WHERE code = ?1 AND id IS NOT ?2)",
        params![code, id],
        |r| r.get(0),
    )?;
    if code_taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    if let Some(phone) = phone {
        let taken: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM customers WHERE phone = ?1 AND archived_at IS NULL
                            AND id IS NOT ?2)",
            params![phone, id],
            |r| r.get(0),
        )?;
        if taken {
            return Err(AppError::validation("phone", "phone_taken"));
        }
    }
    Ok(())
}

/// The next `C…` code no customer holds. Codes kept by an import can sit ahead of the counter;
/// among `count + 1` consecutive codes at least one is free, so the probe always ends, and the
/// counter it moves commits past the imported block with the customer.
fn next_free_code(tx: &Transaction<'_>) -> Result<String, AppError> {
    let customers: i64 = tx.query_row("SELECT COUNT(*) FROM customers", [], |r| r.get(0))?;
    for _ in 0..=customers {
        let code = crate::numbering::next_code(tx, "customer_code", "C", 5)?;
        let taken: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM customers WHERE code = ?1)",
            [&code],
            |r| r.get(0),
        )?;
        if !taken {
            return Ok(code);
        }
    }
    Err(AppError::Internal("no free customer code".into()))
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    pack: &Pack,
    input: CustomerInput,
) -> Result<Customer, AppError> {
    let c = clean(tx, pack, input)?;
    let code = match &c.code {
        Some(code) => code.clone(),
        None => next_free_code(tx)?,
    };
    check_unique(tx, None, &code, c.phone.as_deref())?;
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO customers (id, code, name, search_text, customer_type, phone, phone2, email,
             address, wilaya, nif, nis, rc, ai, nin, ids_verified_at, price_level_id, discount_bp,
             credit_limit, lang, notes, tags, is_favourite, marketing_consent, created_at,
             updated_at, created_by, terminal_id, legal_form, activity, vat_exempt, vat_exempt_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?25, ?26, ?27, ?28, ?29, ?30, ?31)",
        params![
            id,
            code,
            c.name,
            search_text(&c, &code),
            c.customer_type,
            c.phone,
            c.phone2,
            c.email,
            c.address,
            c.wilaya,
            c.nif,
            c.nis,
            c.rc,
            c.ai,
            c.nin,
            c.ids_verified_at,
            c.price_level_id,
            c.discount_bp,
            c.credit_limit,
            c.lang,
            c.notes,
            c.tags,
            c.is_favourite,
            c.marketing_consent,
            now,
            actor.user_id,
            actor.terminal_id,
            c.legal_form,
            c.activity,
            c.vat_exempt,
            c.vat_exempt_ref
        ],
    )?;
    let created = get(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "customer.created",
            "customer",
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
    input: CustomerInput,
) -> Result<Customer, AppError> {
    let before = get(tx, id)?;
    let c = clean(tx, pack, input)?;
    let code = c.code.clone().unwrap_or_else(|| before.code.clone());
    check_unique(tx, Some(id), &code, c.phone.as_deref())?;
    tx.execute(
        "UPDATE customers SET code = ?2, name = ?3, search_text = ?4, customer_type = ?5, phone = ?6,
             phone2 = ?7, email = ?8, address = ?9, wilaya = ?10, nif = ?11, nis = ?12, rc = ?13,
             ai = ?14, nin = ?15, ids_verified_at = ?16, price_level_id = ?17, discount_bp = ?18,
             credit_limit = ?19, lang = ?20, notes = ?21, tags = ?22, is_favourite = ?23,
             marketing_consent = ?24, updated_at = ?25, legal_form = ?26, activity = ?27,
             vat_exempt = ?28, vat_exempt_ref = ?29
         WHERE id = ?1",
        params![
            id,
            code,
            c.name,
            search_text(&c, &code),
            c.customer_type,
            c.phone,
            c.phone2,
            c.email,
            c.address,
            c.wilaya,
            c.nif,
            c.nis,
            c.rc,
            c.ai,
            c.nin,
            c.ids_verified_at,
            c.price_level_id,
            c.discount_bp,
            c.credit_limit,
            c.lang,
            c.notes,
            c.tags,
            c.is_favourite,
            c.marketing_consent,
            now_ms(),
            c.legal_form,
            c.activity,
            c.vat_exempt,
            c.vat_exempt_ref
        ],
    )?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "customer.updated",
            "customer",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

/// CRM-005: a blocked customer may still pay but buys nothing on credit (Task 5).
pub fn set_blocked(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    blocked: bool,
) -> Result<Customer, AppError> {
    get(tx, id)?;
    tx.execute(
        "UPDATE customers SET is_blocked = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, blocked, now_ms()],
    )?;
    let action = if blocked {
        "customer.blocked"
    } else {
        "customer.unblocked"
    };
    audit::append(tx, &actor.event(action, "customer", Some(id), json!({})))?;
    get(tx, id)
}

pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let customer = get(tx, id)?;
    if customer.balance != Money::ZERO {
        return Err(AppError::conflict("balance_not_zero"));
    }
    tx.execute(
        "UPDATE customers SET archived_at = ?2, updated_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event("customer.archived", "customer", Some(id), json!({})),
    )?;
    Ok(())
}

pub fn restore(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let customer = get(tx, id)?;
    check_unique(tx, Some(id), &customer.code, customer.phone.as_deref())?;
    tx.execute(
        "UPDATE customers SET archived_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event("customer.restored", "customer", Some(id), json!({})),
    )?;
    Ok(())
}

/// The customer's validated tickets and return notes, newest first (CRM-002, RET-002).
pub fn documents(
    conn: &Connection,
    customer_id: &str,
    limit: u32,
) -> Result<Vec<DocumentSummary>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, doc_type, number, status, business_date, validated_at, total_due, user_id,
                terminal_id, parent_id
         FROM documents WHERE customer_id = ?1 AND status = 'validated'
         ORDER BY validated_at DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![customer_id, limit.clamp(1, 500)], |r| {
            Ok(DocumentSummary {
                id: r.get(0)?,
                doc_type: r.get(1)?,
                number: r.get(2)?,
                status: r.get(3)?,
                business_date: r.get(4)?,
                validated_at: r.get(5)?,
                total_due: r.get(6)?,
                user_id: r.get(7)?,
                terminal_id: r.get(8)?,
                parent_id: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The directory as a table (IMX-011): the columns the import template reads back.
pub fn export_table(conn: &Connection, q: &CustomerQuery) -> Result<Table, AppError> {
    let mut table = Table::new(&[
        ("code", ColumnKind::Code),
        ("name", ColumnKind::Text),
        ("customerType", ColumnKind::Tag),
        ("phone", ColumnKind::Code),
        ("phone2", ColumnKind::Code),
        ("email", ColumnKind::Text),
        ("address", ColumnKind::Text),
        ("wilaya", ColumnKind::Code),
        ("nif", ColumnKind::Code),
        ("nis", ColumnKind::Code),
        ("rc", ColumnKind::Code),
        ("ai", ColumnKind::Code),
        ("creditLimit", ColumnKind::Money),
        ("discount", ColumnKind::Percent),
        ("balance", ColumnKind::Money),
        ("notes", ColumnKind::Text),
    ])
    .tag("customerType", TagDomain::CustomerType);
    // The whole directory the filters select, never the search screen's first 5 000.
    for hit in hits(conn, q, -1)? {
        let c = get(conn, &hit.id)?;
        table.push(vec![
            Cell::text(c.code),
            Cell::text(c.name),
            Cell::text(c.customer_type.as_str()),
            Cell::opt_text(c.phone),
            Cell::opt_text(c.phone2),
            Cell::opt_text(c.email),
            Cell::opt_text(c.address),
            Cell::opt_text(c.wilaya),
            Cell::opt_text(c.nif),
            Cell::opt_text(c.nis),
            Cell::opt_text(c.rc),
            Cell::opt_text(c.ai),
            c.credit_limit.map_or(Cell::NULL, Cell::money),
            Cell::rate(c.discount_bp),
            Cell::money(c.balance),
            Cell::opt_text(c.notes),
        ])?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    fn create_with(
        shop: &crate::testkit::Shop,
        input: CustomerInput,
    ) -> Result<Customer, AppError> {
        let pack = Arc::clone(&shop.pack);
        let actor = shop.owner.actor();
        shop.db.write(move |tx| create(tx, &actor, &pack, input))
    }

    #[test]
    fn a_customer_gets_a_code_a_clean_phone_and_is_found_in_any_script() {
        let shop = shop();
        let mut input = CustomerInput::named("Karim Bensalah");
        input.phone = Some("+213 555 12 34 56".into());
        input.wilaya = Some("16".into());
        input.tags = Some("voisin, fidèle".into());
        let karim = create_with(&shop, input).unwrap();
        assert_eq!(
            (karim.code.as_str(), karim.phone.as_deref()),
            ("C00001", Some("0555123456"))
        );
        let mut arabic = CustomerInput::named("مصطفى بن علي");
        arabic.phone = Some("0661 22 33 44".into());
        create_with(&shop, arabic).unwrap();
        let q = |text: &str| {
            let q = CustomerQuery {
                text: text.into(),
                ..CustomerQuery::default()
            };
            shop.db.read(move |c| search(c, &q)).unwrap()
        };
        assert_eq!(q("karim").len(), 1);
        assert_eq!(q("0555 12").len(), 1, "by phone digits");
        assert_eq!(q("مصطفي").len(), 1, "ى and ي are the same letter");
        assert_eq!(q("C00002")[0].name, "مصطفى بن علي");
        assert_eq!(q("").len(), 2);
        let found = shop
            .db
            .read(|c| find_by_phone(c, "0555-12-34-56"))
            .unwrap()
            .unwrap();
        assert_eq!(found.id, karim.id);
    }

    #[test]
    fn phones_and_codes_are_unique_and_resellers_carry_their_ids() {
        let shop = shop();
        let mut a = CustomerInput::named("Karim");
        a.phone = Some("0555123456".into());
        create_with(&shop, a.clone()).unwrap();
        a.name = "Karim bis".into();
        assert_eq!(
            create_with(&shop, a).unwrap_err(),
            AppError::validation("phone", "phone_taken")
        );
        let mut coded = CustomerInput::named("Hôtel Safir");
        coded.code = Some("C00001".into());
        assert_eq!(
            create_with(&shop, coded).unwrap_err(),
            AppError::validation("code", "code_taken")
        );
        let mut reseller = CustomerInput::named("Superette Nour");
        reseller.customer_type = CustomerType::Reseller;
        assert_eq!(
            create_with(&shop, reseller.clone()).unwrap_err(),
            AppError::validation("nif", "required")
        );
        reseller.nif = Some("000116001234567".into());
        reseller.rc = Some("16/00-1234567B21".into());
        reseller.ai = Some("16012345678".into());
        reseller.address = Some("Cité 1200 logements, Bab Ezzouar".into());
        reseller.ids_verified_at = Some("2026-09-20".into());
        assert_eq!(
            create_with(&shop, reseller).unwrap().customer_type,
            CustomerType::Reseller
        );
    }

    #[test]
    fn automatic_codes_skip_the_codes_an_import_kept() {
        let shop = shop();
        for code in ["C00001", "C00002", "C00003"] {
            let mut input = CustomerInput::named(code);
            input.code = Some(code.into());
            create_with(&shop, input).unwrap();
        }
        let karim = create_with(&shop, CustomerInput::named("Karim")).unwrap();
        assert_eq!(karim.code, "C00004");
    }

    #[test]
    fn automatic_codes_get_past_an_imported_directory_of_any_size() {
        let shop = shop();
        let pack = Arc::clone(&shop.pack);
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                for n in 1..=1_200 {
                    let mut input = CustomerInput::named(&format!("Client {n}"));
                    input.code = Some(format!("C{n:05}"));
                    create(tx, &actor, &pack, input)?;
                }
                Ok(())
            })
            .unwrap();
        let karim = create_with(&shop, CustomerInput::named("Karim")).unwrap();
        assert_eq!(karim.code, "C01201");
        assert_eq!(
            shop.count(
                "SELECT last_number FROM counters WHERE doc_type = 'customer_code'
                   AND series = '' AND fiscal_year = 0"
            ),
            1_201,
            "the counter is committed past the imported block"
        );
        let nadia = create_with(&shop, CustomerInput::named("Nadia")).unwrap();
        assert_eq!(nadia.code, "C01202");
    }

    #[test]
    fn updates_block_and_archive_are_journaled() {
        let shop = shop();
        let karim = shop.customer("Karim");
        let actor = shop.owner.actor();
        let pack = Arc::clone(&shop.pack);
        let id = karim.id.clone();
        let updated = shop
            .db
            .write(move |tx| {
                let mut input = CustomerInput::named("Karim Bensalah");
                input.credit_limit = Some(Money(5_000_000));
                input.discount_bp = Rate(500);
                update(tx, &actor, &pack, &id, input)
            })
            .unwrap();
        assert_eq!(
            (updated.credit_limit, updated.discount_bp),
            (Some(Money(5_000_000)), Rate(500))
        );
        let actor = shop.owner.actor();
        let id = karim.id.clone();
        assert!(
            shop.db
                .write(move |tx| set_blocked(tx, &actor, &id, true))
                .unwrap()
                .is_blocked
        );
        let actor = shop.owner.actor();
        let id = karim.id.clone();
        shop.db.write(move |tx| archive(tx, &actor, &id)).unwrap();
        assert!(shop.db.read(move |c| get(c, &karim.id)).unwrap().archived);
        assert!(
            shop.db
                .read(|c| search(c, &CustomerQuery::default()))
                .unwrap()
                .is_empty()
        );
        let actions = shop.count(
            "SELECT COUNT(*) FROM audit_log WHERE action IN ('customer.created', 'customer.updated',
               'customer.blocked', 'customer.archived')",
        );
        assert_eq!(actions, 4);
    }

    #[test]
    fn a_customer_who_owes_money_cannot_be_archived() {
        let shop = shop();
        let karim = shop.customer("Karim");
        let ctx = shop.owner_till();
        let id = karim.id.clone();
        shop.db
            .write(move |tx| {
                credit::set_opening(tx, &ctx, &id, Money(250_000), Some("carnet 2025"))
            })
            .unwrap();
        let actor = shop.owner.actor();
        let id = karim.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| archive(tx, &actor, &id))
                .unwrap_err(),
            AppError::conflict("balance_not_zero")
        );
    }

    #[test]
    fn a_vat_exempt_customer_needs_its_attestation_and_keeps_its_b2b_mentions() {
        let shop = shop();
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        let err = shop
            .db
            .write(move |tx| {
                let mut input = CustomerInput::named("Entreprise nationale");
                input.vat_exempt = true;
                create(tx, &actor, &pack, input)
            })
            .unwrap_err();
        assert_eq!(
            (err.payload().field.as_deref(), err.code().as_str()),
            (Some("vatExemptRef"), "vat_exempt_ref_required")
        );
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        let c = shop
            .db
            .write(move |tx| {
                let mut input = CustomerInput::named("Entreprise nationale");
                input.vat_exempt = true;
                input.vat_exempt_ref = Some(" ATT-2026-114 ".into());
                input.legal_form = Some("EPE SPA".into());
                input.activity = Some("Travaux publics".into());
                create(tx, &actor, &pack, input)
            })
            .unwrap();
        assert!(c.vat_exempt);
        assert_eq!(c.vat_exempt_ref.as_deref(), Some("ATT-2026-114"));
        assert_eq!(
            (c.legal_form.as_deref(), c.activity.as_deref()),
            (Some("EPE SPA"), Some("Travaux publics"))
        );
    }

    #[test]
    fn the_export_table_lists_the_directory() {
        let shop = shop();
        let mut input = CustomerInput::named("Karim");
        input.phone = Some("0555123456".into());
        create_with(&shop, input).unwrap();
        let table = shop
            .db
            .read(|c| export_table(c, &CustomerQuery::default()))
            .unwrap();
        assert_eq!(table.columns[0].key, "code");
        assert_eq!(table.rows.len(), 1);
        assert_eq!(
            table.rows[0][table.column("phone").unwrap()],
            Cell::text("0555123456")
        );
    }

    #[test]
    fn the_export_holds_the_whole_directory_past_the_search_limit() {
        let shop = shop();
        let (actor, pack) = (shop.owner.actor(), Arc::clone(&shop.pack));
        shop.db
            .write(move |tx| {
                for i in 0..5_001 {
                    create(
                        tx,
                        &actor,
                        &pack,
                        CustomerInput::named(&format!("Client {i:05}")),
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let all = CustomerQuery::default();
        let table = shop.db.read(move |c| export_table(c, &all)).unwrap();
        assert_eq!(table.rows.len(), 5_001, "never cut at 5 000 rows");
        let found = shop
            .db
            .read(|c| {
                let q = CustomerQuery {
                    limit: Some(10_000),
                    ..CustomerQuery::default()
                };
                search(c, &q)
            })
            .unwrap();
        assert_eq!(found.len(), 5_000, "the search screen keeps its limit");
    }
}
