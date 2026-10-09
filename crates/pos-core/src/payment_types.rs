//! Custom payment types (PAY-006, PAY-007): a named tender the shop adds — "Prélèvement CCP"
//! (seeded), "TPE BNA", "Chèque-restaurant entreprise" — that rides one of the base electronic
//! or bank tenders. The sale keeps the base method in `payments` (stamp duty, drawer, Z lines,
//! the M1a/M2 rules stay exact); `payment_type_uses` says which type each tender was, for the
//! payment screen's tiles and the report. Cash and credit have no custom type.

use crate::audit::{self, Actor};
use crate::documents::Document;
use crate::error::AppError;
use crate::ids::new_id;
use crate::payments::{PaymentInput, PaymentMethod};
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

const CODE_MAX: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaymentType {
    pub id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    /// The tender it rides: card, Edahabia, BaridiMob, cheque or transfer.
    pub base_method: PaymentMethod,
    /// The tender must carry its reference (debit number, authorisation…).
    pub requires_reference: bool,
    pub sort_order: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaymentTypeInput {
    /// `a-z`, `0-9` and `_`, 1–24 characters, unique among the live types.
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub base_method: PaymentMethod,
    pub requires_reference: bool,
    pub sort_order: i64,
}

/// Tender `payment` (its index in `SaleInput.payments`) was taken as this custom type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TenderType {
    pub payment: u32,
    pub payment_type_id: String,
}

const SELECT: &str = "SELECT id, code, name_ar, name_fr, name_en, base_method, requires_reference,
        sort_order, archived_at IS NOT NULL
     FROM payment_types";

fn map(row: &Row<'_>) -> rusqlite::Result<PaymentType> {
    Ok(PaymentType {
        id: row.get(0)?,
        code: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        base_method: row.get(5)?,
        requires_reference: row.get(6)?,
        sort_order: row.get(7)?,
        archived: row.get(8)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<PaymentType, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("payment_type"))
}

/// In the payment screen's order (`sort_order`, then the French name).
pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<PaymentType>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE (?1 OR archived_at IS NULL) ORDER BY sort_order, name_fr, code"
    ))?;
    let rows = stmt
        .query_map([include_archived], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The input trimmed and checked: the code, a base method a type can ride, the names.
fn clean(input: &PaymentTypeInput) -> Result<PaymentTypeInput, AppError> {
    let code = input.code.trim().to_lowercase();
    if code.is_empty()
        || code.len() > CODE_MAX
        || !code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(AppError::validation("code", "value_invalid"));
    }
    if !matches!(
        input.base_method,
        PaymentMethod::CardCib
            | PaymentMethod::Edahabia
            | PaymentMethod::Baridimob
            | PaymentMethod::Cheque
            | PaymentMethod::Transfer
    ) {
        return Err(AppError::validation("baseMethod", "payment_method_invalid"));
    }
    let (name_ar, name_fr, name_en) =
        crate::catalog::clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    Ok(PaymentTypeInput {
        code,
        name_ar,
        name_fr,
        name_en,
        ..input.clone()
    })
}

/// The code is free among the live types (other than `except`).
fn ensure_code_free(conn: &Connection, code: &str, except: Option<&str>) -> Result<(), AppError> {
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM payment_types
                        WHERE code = ?1 AND archived_at IS NULL AND (?2 IS NULL OR id <> ?2))",
        params![code, except],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::validation("code", "payment_type_code_taken"));
    }
    Ok(())
}

fn snapshot(p: &PaymentTypeInput) -> serde_json::Value {
    json!({
        "code": p.code,
        "nameFr": p.name_fr,
        "baseMethod": p.base_method,
        "requiresReference": p.requires_reference,
    })
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: PaymentTypeInput,
) -> Result<PaymentType, AppError> {
    let p = clean(&input)?;
    ensure_code_free(tx, &p.code, None)?;
    let now = now_ms();
    let id = new_id();
    tx.execute(
        "INSERT INTO payment_types (id, code, name_ar, name_fr, name_en, base_method,
                                    requires_reference, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            id,
            p.code,
            p.name_ar,
            p.name_fr,
            p.name_en,
            p.base_method,
            p.requires_reference,
            p.sort_order,
            now
        ],
    )?;
    audit::append(
        tx,
        &actor.event(
            "payment_type.created",
            "payment_type",
            Some(&id),
            snapshot(&p),
        ),
    )?;
    get(tx, &id)
}

/// Names, code, base method, reference rule and place of a live type; the tickets that used it
/// keep their base method (only the report reads the type).
pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: PaymentTypeInput,
) -> Result<PaymentType, AppError> {
    let before = get(tx, id)?;
    if before.archived {
        return Err(AppError::not_found("payment_type"));
    }
    let p = clean(&input)?;
    ensure_code_free(tx, &p.code, Some(id))?;
    tx.execute(
        "UPDATE payment_types SET code = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5,
                base_method = ?6, requires_reference = ?7, sort_order = ?8, updated_at = ?9
         WHERE id = ?1",
        params![
            id,
            p.code,
            p.name_ar,
            p.name_fr,
            p.name_en,
            p.base_method,
            p.requires_reference,
            p.sort_order,
            now_ms()
        ],
    )?;
    audit::append(
        tx,
        &actor.event(
            "payment_type.updated",
            "payment_type",
            Some(id),
            json!({
                "before": {
                    "code": before.code,
                    "nameFr": before.name_fr,
                    "baseMethod": before.base_method,
                    "requiresReference": before.requires_reference,
                },
                "after": snapshot(&p),
            }),
        ),
    )?;
    get(tx, id)
}

/// Hides the type from the payment screen; the tickets that used it keep it (never deleted).
pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let ty = get(tx, id)?;
    if ty.archived {
        return Ok(());
    }
    let now = now_ms();
    tx.execute(
        "UPDATE payment_types SET archived_at = ?2, updated_at = ?2
         WHERE id = ?1 AND archived_at IS NULL",
        params![id, now],
    )?;
    audit::append(
        tx,
        &actor.event(
            "payment_type.archived",
            "payment_type",
            Some(id),
            json!({ "code": ty.code }),
        ),
    )?;
    Ok(())
}

/// The uses of custom types by a validated ticket (`after_sale`): each tender named by `types`
/// must be of the type's base method and carry its reference when the type requires one.
/// M1a writes the ticket's `payments` in tender order (the change row comes after them), so
/// `document.payments[i]` is tender `i`.
pub(crate) fn record_uses(
    tx: &Transaction<'_>,
    document: &Document,
    tenders: &[PaymentInput],
    types: &[TenderType],
) -> Result<(), AppError> {
    let mut seen = Vec::with_capacity(types.len());
    for t in types {
        let i = usize::try_from(t.payment).unwrap_or(usize::MAX);
        if seen.contains(&i) {
            return Err(AppError::validation("paymentTypes", "value_invalid"));
        }
        seen.push(i);
        let tender = tenders
            .get(i)
            .ok_or_else(|| AppError::validation("paymentTypes", "value_invalid"))?;
        let field = format!("payments.{i}.type");
        let ty = get(tx, &t.payment_type_id).map_err(|e| match e {
            AppError::NotFound { .. } => AppError::validation(&field, "payment_type_not_found"),
            other => other,
        })?;
        if ty.archived || ty.base_method != tender.method {
            return Err(AppError::validation(&field, "payment_type_mismatch"));
        }
        if ty.requires_reference
            && tender
                .reference
                .as_deref()
                .is_none_or(|r| r.trim().is_empty())
        {
            return Err(AppError::validation(
                &format!("payments.{i}.reference"),
                "required",
            ));
        }
        let payment = document
            .payments
            .get(i)
            .filter(|p| p.method == tender.method)
            .ok_or_else(|| AppError::Internal(format!("ticket payment {i} is not tender {i}")))?;
        tx.execute(
            "INSERT INTO payment_type_uses (payment_id, payment_type_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![payment.id, ty.id, document.created_at],
        )?;
    }
    Ok(())
}

/// HW-042: the text of the shop's BaridiMob payment QR, shown on the customer screen while the
/// customer pays by BaridiMob; `None` until the owner sets it.
pub fn baridpay_qr(conn: &Connection) -> Result<Option<String>, AppError> {
    crate::settings::get(conn, "payments.baridpay_qr")
}

#[cfg(test)]
impl PaymentTypeInput {
    /// "TPE BNA": a bank terminal riding the CIB card, reference required.
    pub(crate) fn sample() -> PaymentTypeInput {
        PaymentTypeInput {
            code: "bna_tpe".into(),
            name_ar: String::new(),
            name_fr: "TPE BNA".into(),
            name_en: String::new(),
            base_method: PaymentMethod::CardCib,
            requires_reference: true,
            sort_order: 3,
        }
    }
}
