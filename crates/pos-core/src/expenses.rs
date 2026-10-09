//! Expenses (spec §5.1 `expenses`; FIN-001, CSH-002): editable categories (each mapped to one of
//! M1's eight cash groups that the drawer and the Z use), expenses paid from the till (an M1
//! `cash::expense` of the open session) or from the back office by any method, reversed — never
//! edited — when entered by mistake.

use crate::audit::{self, Actor};
use crate::cash::{self, ExpenseCategory};
use crate::catalog::{clean_code, clean_names, clean_opt};
use crate::error::AppError;
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::Money;
use crate::payments::PaymentMethod;
use crate::table::{Cell, ColumnKind, Table, TagDomain};
use crate::till::TillContext;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseCategoryInfo {
    pub id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub cash_group: ExpenseCategory,
    pub sort_order: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseCategoryInput {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub cash_group: ExpenseCategory,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewExpense {
    pub category_id: String,
    pub amount: Money,
    pub method: PaymentMethod,
    /// Paid in cash from this terminal's drawer.
    pub from_till: bool,
    pub reference: Option<String>,
    pub supplier_id: Option<String>,
    /// Back office only: a past day of the current year (default today).
    pub business_date: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Expense {
    pub id: String,
    pub category_id: String,
    pub category_code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub amount: Money,
    pub method: PaymentMethod,
    pub from_till: bool,
    pub cash_session_id: Option<String>,
    pub reference: Option<String>,
    pub supplier_id: Option<String>,
    pub note: Option<String>,
    pub reversal_of: Option<String>,
    pub reversed: bool,
    pub business_date: String,
    pub created_at: i64,
    pub user_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub category_id: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category_id: String,
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub total: Money,
}

const CATEGORY: &str =
    "SELECT id, code, name_ar, name_fr, name_en, cash_group, sort_order, archived_at IS NOT NULL
     FROM expense_categories";

fn map_category(r: &Row<'_>) -> rusqlite::Result<ExpenseCategoryInfo> {
    Ok(ExpenseCategoryInfo {
        id: r.get(0)?,
        code: r.get(1)?,
        name_ar: r.get(2)?,
        name_fr: r.get(3)?,
        name_en: r.get(4)?,
        cash_group: r.get(5)?,
        sort_order: r.get(6)?,
        archived: r.get(7)?,
    })
}

pub fn category(conn: &Connection, id: &str) -> Result<ExpenseCategoryInfo, AppError> {
    conn.prepare_cached(&format!("{CATEGORY} WHERE id = ?1"))?
        .query_row([id], map_category)
        .optional()?
        .ok_or_else(|| AppError::not_found("expense_category"))
}

pub fn categories(
    conn: &Connection,
    include_archived: bool,
) -> Result<Vec<ExpenseCategoryInfo>, AppError> {
    let mut stmt = conn.prepare_cached(&format!(
        "{CATEGORY} WHERE ?1 OR archived_at IS NULL ORDER BY sort_order, code"
    ))?;
    let rows = stmt
        .query_map([include_archived], map_category)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn clean_category(
    input: &ExpenseCategoryInput,
) -> Result<(String, (String, String, String)), AppError> {
    Ok((
        clean_code(&input.code, "code")?,
        clean_names(&input.name_ar, &input.name_fr, &input.name_en)?,
    ))
}

pub fn create_category(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: ExpenseCategoryInput,
) -> Result<ExpenseCategoryInfo, AppError> {
    let (code, (ar, fr, en)) = clean_category(&input)?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM expense_categories WHERE code = ?1)",
        [&code],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO expense_categories (id, code, name_ar, name_fr, name_en, cash_group, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![id, code, ar, fr, en, input.cash_group.as_str(), input.sort_order, now],
    )?;
    let created = category(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "expense_category.created",
            "expense_category",
            Some(&id),
            json!({ "after": created }),
        ),
    )?;
    Ok(created)
}

pub fn update_category(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: ExpenseCategoryInput,
) -> Result<ExpenseCategoryInfo, AppError> {
    let before = category(tx, id)?;
    let (code, (ar, fr, en)) = clean_category(&input)?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM expense_categories WHERE code = ?1 AND id <> ?2)",
        params![code, id],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::validation("code", "code_taken"));
    }
    tx.execute(
        "UPDATE expense_categories SET code = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5, cash_group = ?6,
             sort_order = ?7, updated_at = ?8 WHERE id = ?1",
        params![id, code, ar, fr, en, input.cash_group.as_str(), input.sort_order, now_ms()],
    )?;
    let after = category(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "expense_category.updated",
            "expense_category",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

pub fn archive_category(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    category(tx, id)?;
    tx.execute(
        "UPDATE expense_categories SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "expense_category.archived",
            "expense_category",
            Some(id),
            json!({}),
        ),
    )?;
    Ok(())
}

const SELECT: &str = "SELECT e.id, e.category_id, c.code, c.name_ar, c.name_fr, c.name_en, e.amount, e.method, e.from_till,
        e.cash_session_id, e.reference, e.supplier_id, e.note, e.reversal_of,
        EXISTS (SELECT 1 FROM expenses r WHERE r.reversal_of = e.id), e.business_date, e.created_at, e.user_id
     FROM expenses e JOIN expense_categories c ON c.id = e.category_id";

fn map(r: &Row<'_>) -> rusqlite::Result<Expense> {
    Ok(Expense {
        id: r.get(0)?,
        category_id: r.get(1)?,
        category_code: r.get(2)?,
        name_ar: r.get(3)?,
        name_fr: r.get(4)?,
        name_en: r.get(5)?,
        amount: r.get(6)?,
        method: r.get(7)?,
        from_till: r.get(8)?,
        cash_session_id: r.get(9)?,
        reference: r.get(10)?,
        supplier_id: r.get(11)?,
        note: r.get(12)?,
        reversal_of: r.get(13)?,
        reversed: r.get(14)?,
        business_date: r.get(15)?,
        created_at: r.get(16)?,
        user_id: r.get(17)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Expense, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE e.id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("expense"))
}

pub fn record(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: NewExpense,
) -> Result<Expense, AppError> {
    let cat = category(tx, &input.category_id)?;
    if cat.archived {
        return Err(AppError::not_found("expense_category"));
    }
    if !input.amount.is_positive() {
        return Err(AppError::validation("amount", "amount_invalid"));
    }
    let allowed = matches!(
        input.method,
        PaymentMethod::Cash
            | PaymentMethod::CardCib
            | PaymentMethod::Edahabia
            | PaymentMethod::Baridimob
            | PaymentMethod::Cheque
            | PaymentMethod::Transfer
    );
    if !allowed || (input.from_till && input.method != PaymentMethod::Cash) {
        return Err(AppError::validation("method", "payment_method_invalid"));
    }
    if let Some(supplier) = &input.supplier_id {
        crate::suppliers::get(tx, supplier).map_err(|e| match e {
            AppError::NotFound { .. } => AppError::validation("supplierId", "supplier_not_found"),
            other => other,
        })?;
    }
    let note = clean_opt(input.note.as_deref(), "note", 200)?;
    let reference = clean_opt(input.reference.as_deref(), "reference", 48)?;
    let today = ctx.business_day();
    let id = new_id();
    if input.from_till {
        let movement = cash::expense(tx, ctx, cat.cash_group, input.amount, note.as_deref())?;
        tx.execute(
            "INSERT INTO expenses (id, category_id, amount, method, from_till, cash_session_id, cash_movement_id, reference,
                 supplier_id, note, business_date, created_at, user_id, terminal_id)
             VALUES (?1, ?2, ?3, 'cash', 1, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                cat.id,
                input.amount,
                movement.session_id,
                movement.id,
                reference,
                input.supplier_id,
                note,
                today,
                ctx.now,
                ctx.session.user_id,
                ctx.session.terminal_id
            ],
        )?;
    } else {
        let day = crate::party::clean_day(input.business_date.as_deref(), "businessDate")?
            .unwrap_or_else(|| today.clone());
        if day > today || day[..4] != today[..4] {
            return Err(AppError::validation("businessDate", "date_invalid"));
        }
        tx.execute(
            "INSERT INTO expenses (id, category_id, amount, method, from_till, reference, supplier_id, note, business_date,
                 created_at, user_id, terminal_id)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                cat.id,
                input.amount,
                input.method,
                reference,
                input.supplier_id,
                note,
                day,
                ctx.now,
                ctx.session.user_id,
                ctx.session.terminal_id
            ],
        )?;
        audit::append(
            tx,
            &ctx.actor().event(
                "expense.recorded",
                "expense",
                Some(&id),
                json!({ "category": cat.code, "amount": input.amount, "method": input.method, "businessDate": day }),
            ),
        )?;
    }
    get(tx, &id)
}

/// Reverses an expense entered by mistake with a negative row. A till expense also puts its
/// cash back in the drawer it left: a `paid_in` of the same session, referencing the reversal,
/// in the same transaction, so the Z's expected cash returns to what it was before the expense
/// and the expense and cash reports agree. Once that session is closed its Z is final
/// (spec §7.3): the reversal is refused (`expense_session_closed`) and the cash comes back as a
/// paid-in of the current session.
pub fn reverse(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    note: &str,
) -> Result<Expense, AppError> {
    let original = get(tx, id)?;
    if original.reversal_of.is_some() {
        return Err(AppError::validation("id", "value_invalid"));
    }
    if original.reversed {
        return Err(AppError::conflict("expense_already_reversed"));
    }
    let note = clean_opt(Some(note), "note", 200)?
        .ok_or_else(|| AppError::validation("note", "required"))?;
    let reversal = new_id();
    let back_in_drawer = match (original.from_till, &original.cash_session_id) {
        (true, Some(session_id)) => {
            let session = cash::get_session(tx, session_id)?;
            if session.status != cash::SessionStatus::Open {
                return Err(AppError::conflict("expense_session_closed"));
            }
            let movement = cash::record(
                tx,
                ctx,
                &session.id,
                cash::MovementKind::PaidIn,
                original.amount,
                None,
                Some(("expense", &reversal)),
                Some(&note),
            )?;
            Some((session.id, movement))
        }
        _ => None,
    };
    let (session_id, movement_id) = match &back_in_drawer {
        Some((session, movement)) => (Some(session.as_str()), Some(movement.as_str())),
        None => (None, None),
    };
    tx.execute(
        "INSERT INTO expenses (id, category_id, amount, method, from_till, cash_session_id, cash_movement_id,
             reference, supplier_id, note, reversal_of, business_date, created_at, user_id, terminal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            reversal,
            original.category_id,
            original.amount.checked_neg().unwrap_or(Money::ZERO),
            original.method,
            back_in_drawer.is_some(),
            session_id,
            movement_id,
            original.reference,
            original.supplier_id,
            note,
            original.id,
            ctx.business_day(),
            ctx.now,
            ctx.session.user_id,
            ctx.session.terminal_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "expense.reversed",
            "expense",
            Some(id),
            json!({ "reversalId": reversal, "amount": original.amount, "note": note,
                    "fromTill": original.from_till, "cashSessionId": session_id,
                    "cashMovementId": movement_id }),
        ),
    )?;
    get(tx, &reversal)
}

pub fn list(conn: &Connection, q: &ExpenseQuery) -> Result<Vec<Expense>, AppError> {
    list_limited(
        conn,
        q,
        i64::from(q.limit.unwrap_or(1_000).clamp(1, 10_000)),
    )
}

/// [`list`] with `limit` rows at most; a negative `limit` has no bound (SQLite's `LIMIT -1`).
fn list_limited(conn: &Connection, q: &ExpenseQuery, limit: i64) -> Result<Vec<Expense>, AppError> {
    let mut stmt = conn.prepare_cached(&format!(
        "{SELECT} WHERE (?1 IS NULL OR e.business_date >= ?1) AND (?2 IS NULL OR e.business_date <= ?2)
           AND (?3 IS NULL OR e.category_id = ?3)
         ORDER BY e.business_date DESC, e.created_at DESC LIMIT ?4"
    ))?;
    let rows = stmt
        .query_map(params![q.from, q.to, q.category_id, limit], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Net expenses per category (reversals included), highest first; categories netting to zero
/// are left out.
pub fn totals_by_category(
    conn: &Connection,
    from: &str,
    to: &str,
) -> Result<Vec<CategoryTotal>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.code, c.name_ar, c.name_fr, c.name_en, SUM(e.amount) AS total
         FROM expenses e JOIN expense_categories c ON c.id = e.category_id
         WHERE e.business_date BETWEEN ?1 AND ?2
         GROUP BY c.id HAVING total <> 0 ORDER BY total DESC",
    )?;
    let rows = stmt
        .query_map(params![from, to], |r| {
            Ok(CategoryTotal {
                category_id: r.get(0)?,
                code: r.get(1)?,
                name_ar: r.get(2)?,
                name_fr: r.get(3)?,
                name_en: r.get(4)?,
                total: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn export_table(conn: &Connection, q: &ExpenseQuery, lang: Lang) -> Result<Table, AppError> {
    let mut table = Table::new(&[
        ("date", ColumnKind::Date),
        ("category", ColumnKind::Text),
        ("amount", ColumnKind::Money),
        ("method", ColumnKind::Tag),
        ("reference", ColumnKind::Code),
        ("note", ColumnKind::Text),
    ])
    .tag("method", TagDomain::PaymentMethod);
    // Every expense of the period, never the list's first 1 000.
    for e in list_limited(conn, q, -1)? {
        let name = match lang {
            Lang::Ar => e.name_ar,
            Lang::Fr => e.name_fr,
            Lang::En => e.name_en,
        };
        table.push(vec![
            Cell::text(e.business_date),
            Cell::text(name),
            Cell::money(e.amount),
            Cell::text(e.method.as_str()),
            Cell::opt_text(e.reference),
            Cell::opt_text(e.note),
        ])?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    const ELECTRICITY: &str = "00000000-0000-7000-8000-000000000502";
    const TRANSPORT: &str = "00000000-0000-7000-8000-000000000505";

    fn expense(category: &str, amount: i64, method: PaymentMethod, from_till: bool) -> NewExpense {
        NewExpense {
            category_id: category.into(),
            amount: Money(amount),
            method,
            from_till,
            reference: None,
            supplier_id: None,
            business_date: None,
            note: None,
        }
    }

    #[test]
    fn the_export_holds_every_expense_of_the_period_past_the_list_limit() {
        let shop = shop();
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| {
                for _ in 0..1_001 {
                    record(
                        tx,
                        &ctx,
                        expense(TRANSPORT, 10_000, PaymentMethod::Transfer, false),
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let period = ExpenseQuery {
            from: Some("2026-09-01".into()),
            to: Some("2026-09-30".into()),
            category_id: None,
            limit: None,
        };
        let listed = {
            let q = period.clone();
            shop.db.read(move |c| list(c, &q)).unwrap()
        };
        assert_eq!(listed.len(), 1_000, "the list keeps its default limit");
        let table = shop
            .db
            .read(move |c| export_table(c, &period, Lang::Fr))
            .unwrap();
        assert_eq!(table.rows.len(), 1_001, "never cut at the list's limit");
    }

    #[test]
    fn a_till_expense_leaves_the_drawer_and_a_bank_expense_does_not() {
        let shop = shop();
        shop.open_till(500_000);
        let ctx = shop.till();
        let taxi = shop
            .db
            .write(move |tx| {
                record(
                    tx,
                    &ctx,
                    expense(TRANSPORT, 80_000, PaymentMethod::Cash, true),
                )
            })
            .unwrap();
        assert!(taxi.from_till && taxi.cash_session_id.is_some());
        assert_eq!(shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'expense' AND category = 'transport'"), -80_000);
        let ctx = shop.owner_till();
        let mut sonelgaz = expense(ELECTRICITY, 1_250_000, PaymentMethod::Transfer, false);
        sonelgaz.reference = Some("CCP 7894561".into());
        sonelgaz.business_date = Some("2026-09-15".into());
        let bill = shop.db.write(move |tx| record(tx, &ctx, sonelgaz)).unwrap();
        assert_eq!(
            (bill.business_date.as_str(), bill.cash_session_id.clone()),
            ("2026-09-15", None)
        );
        let totals = shop
            .db
            .read(|c| totals_by_category(c, "2026-09-01", "2026-09-30"))
            .unwrap();
        assert_eq!(
            totals
                .iter()
                .map(|t| (t.code.as_str(), t.total.0))
                .collect::<Vec<_>>(),
            vec![("electricity", 1_250_000), ("transport", 80_000)]
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM audit_log WHERE action IN ('cash.expense', 'expense.recorded')"), 2);
    }

    #[test]
    fn expenses_are_reversed_once_never_edited() {
        let shop = shop();
        let ctx = shop.owner_till();
        let e = shop
            .db
            .write(move |tx| {
                record(
                    tx,
                    &ctx,
                    expense(ELECTRICITY, 300_000, PaymentMethod::Transfer, false),
                )
            })
            .unwrap();
        let ctx = shop.owner_till();
        let id = e.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| reverse(tx, &ctx, &id, "  "))
                .unwrap_err(),
            AppError::validation("note", "required")
        );
        let ctx = shop.owner_till();
        let id = e.id.clone();
        let back = shop
            .db
            .write(move |tx| reverse(tx, &ctx, &id, "saisie en double"))
            .unwrap();
        assert_eq!(
            (back.amount, back.reversal_of.as_deref()),
            (Money(-300_000), Some(e.id.as_str()))
        );
        let ctx = shop.owner_till();
        let id = e.id.clone();
        assert_eq!(
            shop.db
                .write(move |tx| reverse(tx, &ctx, &id, "encore"))
                .unwrap_err(),
            AppError::conflict("expense_already_reversed")
        );
        let listed = shop.db.read(|c| list(c, &ExpenseQuery::default())).unwrap();
        assert!(listed.iter().any(|x| x.id == e.id && x.reversed));
        assert_eq!(
            shop.db
                .read(|c| totals_by_category(c, "2026-01-01", "2026-12-31"))
                .unwrap()
                .len(),
            0,
            "a reversed expense counts for nothing"
        );
    }

    /// A till expense entered by mistake: its reversal puts the cash back in the same drawer, so
    /// the Z expects what it expected before the expense; once that session is closed, the
    /// reversal is refused and the cash comes back as a paid-in of the current session.
    #[test]
    fn reversing_a_till_expense_puts_the_cash_back_in_its_drawer() {
        let shop = shop();
        let session = shop.open_till(500_000);
        let expected = |id: &str| {
            let id = id.to_owned();
            shop.db.read(move |c| cash::expected_cash(c, &id)).unwrap()
        };
        let ctx = shop.till();
        let taxi = shop
            .db
            .write(move |tx| {
                record(
                    tx,
                    &ctx,
                    expense(TRANSPORT, 80_000, PaymentMethod::Cash, true),
                )
            })
            .unwrap();
        assert_eq!(expected(&session.id), Money(420_000));
        let (ctx, id) = (shop.owner_till(), taxi.id.clone());
        let back = shop
            .db
            .write(move |tx| reverse(tx, &ctx, &id, "saisie en double"))
            .unwrap();
        assert_eq!(
            expected(&session.id),
            Money(500_000),
            "the Z expects the float again"
        );
        assert!(back.from_till);
        assert_eq!(back.cash_session_id.as_deref(), Some(session.id.as_str()));
        let movement: (String, i64, String) = shop
            .db
            .read({
                let id = back.id.clone();
                move |c| {
                    Ok(c.query_row(
                        "SELECT m.kind, m.amount, m.ref_id FROM expenses e
                         JOIN cash_movements m ON m.id = e.cash_movement_id WHERE e.id = ?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )?)
                }
            })
            .unwrap();
        assert_eq!(movement, ("paid_in".to_owned(), 80_000, back.id.clone()));
        assert_eq!(
            shop.count("SELECT SUM(amount) FROM expenses WHERE from_till = 1"),
            0,
            "the expenses from the till net to nothing, like the drawer"
        );

        // A second taxi, then the session is closed: its Z is final.
        let ctx = shop.till();
        let late = shop
            .db
            .write(move |tx| {
                record(
                    tx,
                    &ctx,
                    expense(TRANSPORT, 30_000, PaymentMethod::Cash, true),
                )
            })
            .unwrap();
        // 4 700 DA expected: four 1 000 DA notes, one 500 and one 200.
        let close = crate::cash::closing::CloseInput {
            counts: [(100_000, 4), (50_000, 1), (20_000, 1)]
                .into_iter()
                .map(|(value, count)| crate::cash::closing::CountLine {
                    denomination: Money(value),
                    count,
                })
                .collect(),
            notes: None,
        };
        let ctx = shop.till();
        shop.db
            .write(move |tx| crate::cash::closing::close_session(tx, &ctx, close))
            .unwrap();
        let (ctx, id) = (shop.owner_till(), late.id.clone());
        assert_eq!(
            shop.db
                .write(move |tx| reverse(tx, &ctx, &id, "erreur"))
                .unwrap_err(),
            AppError::conflict("expense_session_closed")
        );
        assert!(!shop.db.read(move |c| get(c, &late.id)).unwrap().reversed);
    }

    #[test]
    fn categories_are_editable_and_keep_their_cash_group() {
        let shop = shop();
        let actor = shop.owner.actor();
        let internet = shop
            .db
            .write(move |tx| {
                create_category(
                    tx,
                    &actor,
                    ExpenseCategoryInput {
                        code: "internet".into(),
                        name_ar: "الإنترنت".into(),
                        name_fr: "Internet".into(),
                        name_en: "Internet".into(),
                        cash_group: ExpenseCategory::Other,
                        sort_order: 9,
                    },
                )
            })
            .unwrap();
        assert_eq!(shop.db.read(|c| categories(c, false)).unwrap().len(), 9);
        shop.open_till(0);
        let ctx = shop.till();
        let bill = shop
            .db
            .write(move |tx| {
                record(
                    tx,
                    &ctx,
                    expense(&internet.id, 250_000, PaymentMethod::Cash, true),
                )
            })
            .unwrap();
        assert_eq!(bill.category_code, "internet");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM cash_movements WHERE category = 'other'"),
            1,
            "the drawer and the Z see its group"
        );
        let ctx = shop.till();
        let card_from_till = expense(ELECTRICITY, 1_000, PaymentMethod::CardCib, true);
        assert_eq!(
            shop.db
                .write(move |tx| record(tx, &ctx, card_from_till))
                .unwrap_err(),
            AppError::validation("method", "payment_method_invalid")
        );
    }

    #[test]
    fn archived_categories_take_no_expense_and_back_office_days_stay_in_the_year() {
        let shop = shop();
        let taxi = |code: &str| ExpenseCategoryInput {
            code: code.into(),
            name_ar: "طاكسي".into(),
            name_fr: "Taxi".into(),
            name_en: "Taxi".into(),
            cash_group: ExpenseCategory::Transport,
            sort_order: 5,
        };
        let (actor, input) = (shop.owner.actor(), taxi("RENT"));
        assert_eq!(
            shop.db
                .write(move |tx| update_category(tx, &actor, TRANSPORT, input))
                .unwrap_err(),
            AppError::validation("code", "code_taken")
        );
        let (actor, input) = (shop.owner.actor(), taxi("taxi"));
        let renamed = shop
            .db
            .write(move |tx| update_category(tx, &actor, TRANSPORT, input))
            .unwrap();
        assert_eq!(
            (renamed.code.as_str(), renamed.name_fr.as_str()),
            ("taxi", "Taxi")
        );
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| archive_category(tx, &actor, TRANSPORT))
            .unwrap();
        assert!(
            shop.db
                .read(|c| categories(c, false))
                .unwrap()
                .iter()
                .all(|c| c.id != TRANSPORT)
        );
        assert_eq!(shop.db.read(|c| categories(c, true)).unwrap().len(), 8);
        let ctx = shop.owner_till();
        let archived = expense(TRANSPORT, 1_000, PaymentMethod::Cash, false);
        assert_eq!(
            shop.db
                .write(move |tx| record(tx, &ctx, archived))
                .unwrap_err(),
            AppError::not_found("expense_category")
        );
        for day in ["2026-09-29", "2025-12-31"] {
            let ctx = shop.owner_till();
            let mut dated = expense(ELECTRICITY, 1_000, PaymentMethod::Cheque, false);
            dated.business_date = Some(day.into());
            assert_eq!(
                shop.db
                    .write(move |tx| record(tx, &ctx, dated))
                    .unwrap_err(),
                AppError::validation("businessDate", "date_invalid"),
                "{day}"
            );
        }
        let ctx = shop.owner_till();
        let mut unknown = expense(ELECTRICITY, 1_000, PaymentMethod::Cheque, false);
        unknown.supplier_id = Some("00000000-0000-7000-8000-00000000dead".into());
        assert_eq!(
            shop.db
                .write(move |tx| record(tx, &ctx, unknown))
                .unwrap_err(),
            AppError::validation("supplierId", "supplier_not_found")
        );
        let ctx = shop.owner_till();
        let mut bill = expense(ELECTRICITY, 1_250_000, PaymentMethod::Transfer, false);
        bill.reference = Some("CCP 7894561".into());
        shop.db.write(move |tx| record(tx, &ctx, bill)).unwrap();
        let table = shop
            .db
            .read(|c| export_table(c, &ExpenseQuery::default(), Lang::Ar))
            .unwrap();
        assert_eq!(
            table.rows,
            vec![vec![
                Cell::text("2026-09-28"),
                Cell::text("الكهرباء والغاز"),
                Cell::money(Money(1_250_000)),
                Cell::text("transfer"),
                Cell::text("CCP 7894561"),
                Cell::NULL,
            ]]
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action LIKE 'expense%'"),
            3,
            "the rename, the archive and the one expense recorded"
        );
    }
}
