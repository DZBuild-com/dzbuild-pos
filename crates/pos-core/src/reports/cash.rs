//! Cash reports: sessions, drawer movements, money received per day and method (sales and kridi
//! collections together), expenses per category.

use super::{Built, ChartKind, SALES_DOCS, Scope, add, ints, name_of, rate_bp, rows};
use crate::error::AppError;
use crate::table::{Cell, ColumnKind as K, Table, TagDomain};
use rusqlite::Connection;
use std::collections::BTreeMap;

pub(super) fn sessions(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT cs.business_date, t.code, COALESCE(u.display_name, cs.user_id), cs.opened_at, cs.closed_at,
                cs.z_number, cs.opening_float, cs.expected_cash, cs.counted_cash, cs.variance
         FROM cash_sessions cs
         LEFT JOIN terminals t ON t.id = cs.terminal_id
         LEFT JOIN users u ON u.id = cs.user_id
         WHERE cs.business_date BETWEEN :day_from AND :day_to
           AND (:terminal_id IS NULL OR cs.terminal_id = :terminal_id)
           AND (:user_id IS NULL OR cs.user_id = :user_id)
         ORDER BY cs.opened_at";
    let opt = |v: Option<i64>| v.map_or(Cell::NULL, Cell::int);
    let found = rows(conn, sql, s, |r| {
        Ok(vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::opt_text(r.get(2)?),
            Cell::int(r.get(3)?),
            opt(r.get(4)?),
            Cell::opt_text(r.get(5)?),
            Cell::int(r.get(6)?),
            opt(r.get(7)?),
            opt(r.get(8)?),
            opt(r.get(9)?),
        ])
    })?;
    let mut table = Table::new(&[
        ("date", K::Date),
        ("terminal", K::Code),
        ("cashier", K::Text),
        ("openedAt", K::DateTime),
        ("closedAt", K::DateTime),
        ("zNumber", K::Code),
        ("openingFloat", K::Money),
        ("expectedCash", K::Money),
        ("countedCash", K::Money),
        ("variance", K::Money),
    ]);
    for row in found {
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL; 9];
    totals.push(Cell::money(table.money_total("variance")));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}

pub(super) fn movements(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT m.created_at, t.code, COALESCE(u.display_name, m.user_id), m.kind, m.method, m.category,
                m.amount, m.note
         FROM cash_movements m
         JOIN cash_sessions cs ON cs.id = m.session_id
         LEFT JOIN terminals t ON t.id = cs.terminal_id
         LEFT JOIN users u ON u.id = m.user_id
         WHERE cs.business_date BETWEEN :day_from AND :day_to
           AND (:terminal_id IS NULL OR cs.terminal_id = :terminal_id)
           AND (:user_id IS NULL OR m.user_id = :user_id)
         ORDER BY m.created_at, m.id
         LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        Ok(vec![
            Cell::int(r.get(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::opt_text(r.get(2)?),
            Cell::text(r.get::<_, String>(3)?),
            Cell::text(r.get::<_, String>(4)?),
            Cell::opt_text(r.get(5)?),
            Cell::int(r.get(6)?),
            Cell::opt_text(r.get(7)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("at", K::DateTime),
        ("terminal", K::Code),
        ("cashier", K::Text),
        ("kind", K::Tag),
        ("method", K::Tag),
        ("category", K::Tag),
        ("amount", K::Money),
        ("note", K::Text),
    ])
    .tag("kind", TagDomain::CashMovementKind)
    .tag("method", TagDomain::PaymentMethod)
    .tag("category", TagDomain::ExpenseGroup);
    for row in found {
        table.push(row)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

/// Money received per business day and method (RPT-007): sales payments (change netted into
/// cash; kridi and store credit are not money) and kridi versements, with the duty collected on
/// them. The netted method is aliased `tender`: SQLite resolves a `GROUP BY` name to the table
/// column before the result alias.
pub(super) fn daily_by_method(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sales = format!(
        "SELECT d.business_date, CASE pm.method WHEN 'change' THEN 'cash' ELSE pm.method END AS tender,
                SUM(pm.amount)
         FROM payments pm JOIN documents d ON d.id = pm.document_id
         WHERE {SALES_DOCS} AND pm.method NOT IN ('credit', 'voucher')
         GROUP BY d.business_date, tender"
    );
    let collections = "SELECT e.business_date, e.method, -SUM(e.amount), SUM(e.stamp_charged)
         FROM customer_ledger e
         WHERE e.kind = 'payment' AND e.business_date BETWEEN :day_from AND :day_to
           AND (:terminal_id IS NULL OR e.terminal_id = :terminal_id)
         GROUP BY e.business_date, e.method";
    let mut days: BTreeMap<(String, String), [i64; 3]> = BTreeMap::new();
    for (day, method, [amount]) in rows(conn, &sales, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            ints::<1>(r, 2)?,
        ))
    })? {
        let slot = days.entry((day, method)).or_default();
        slot[0] = slot[0].saturating_add(amount);
    }
    for (day, method, [amount, duty]) in rows(conn, collections, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            ints::<2>(r, 2)?,
        ))
    })? {
        let slot = days.entry((day, method)).or_default();
        slot[1] = slot[1].saturating_add(amount);
        slot[2] = slot[2].saturating_add(duty);
    }
    let mut table = Table::new(&[
        ("date", K::Date),
        ("method", K::Tag),
        ("sales", K::Money),
        ("collections", K::Money),
        ("stampDuty", K::Money),
        ("total", K::Money),
    ])
    .tag("method", TagDomain::PaymentMethod);
    let mut sum = [0_i64; 4];
    for ((day, method), [sales, collected, duty]) in days {
        let total = sales.saturating_add(collected).saturating_add(duty);
        let v = [sales, collected, duty, total];
        add(&mut sum, &v);
        let mut row = vec![Cell::text(day), Cell::text(method)];
        row.extend(v.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL, Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}

/// The expenses of the range per category as `(label, [entries, amount, from the till])`,
/// non-zero amounts only, largest first: the `expenses` rows (reversals net them) and M1/M1b
/// till expenses that have no row. Those carry their cash group, not a category code: each group
/// is shown on the first active category of that group (the one the owner may have renamed), or
/// under the group itself when none is left, never dropped. The P&L reads the same list.
pub(super) fn per_category(
    conn: &Connection,
    s: &Scope<'_>,
) -> Result<Vec<(String, [i64; 3])>, AppError> {
    let name = name_of("ec", s.lang);
    let sql = format!(
        "SELECT ec.code, {name}, COUNT(*), SUM(e.amount),
                SUM(CASE WHEN e.from_till = 1 THEN e.amount ELSE 0 END)
         FROM expenses e JOIN expense_categories ec ON ec.id = e.category_id
         WHERE e.business_date BETWEEN :day_from AND :day_to
         GROUP BY ec.id
         UNION ALL
         SELECT COALESCE(MAX(ec.code), m.category), COALESCE(MAX({name}), m.category), COUNT(*),
                -SUM(m.amount), -SUM(m.amount)
         FROM cash_movements m JOIN cash_sessions cs ON cs.id = m.session_id
         LEFT JOIN expense_categories ec ON ec.id = (
             SELECT x.id FROM expense_categories x WHERE x.cash_group = m.category
             ORDER BY x.archived_at IS NOT NULL, x.sort_order, x.id LIMIT 1)
         WHERE m.kind = 'expense' AND cs.business_date BETWEEN :day_from AND :day_to
           AND NOT EXISTS (SELECT 1 FROM expenses e WHERE e.cash_movement_id = m.id)
         GROUP BY m.category"
    );
    let mut per: BTreeMap<String, (String, [i64; 3])> = BTreeMap::new();
    for (code, label, v) in rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            ints::<3>(r, 2)?,
        ))
    })? {
        let entry = per.entry(code).or_insert((label, [0; 3]));
        add(&mut entry.1, &v);
    }
    let mut spent: Vec<(String, [i64; 3])> = per.into_values().filter(|(_, v)| v[1] != 0).collect();
    spent.sort_by(|a, b| b.1[1].cmp(&a.1[1]).then_with(|| a.0.cmp(&b.0)));
    Ok(spent)
}

/// Expenses per category (FIN-002), till and other methods, exactly as the P&L counts them.
pub(super) fn expenses(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let spent = per_category(conn, s)?;
    let whole = spent
        .iter()
        .fold(0_i64, |acc, (_, v)| acc.saturating_add(v[1]));
    let mut table = Table::new(&[
        ("category", K::Text),
        ("entries", K::Int),
        ("amount", K::Money),
        ("fromTill", K::Money),
        ("otherMethods", K::Money),
        ("share", K::Percent),
    ]);
    let mut sum = [0_i64; 4];
    for (label, [entries, amount, till]) in spent {
        let v = [entries, amount, till, amount.saturating_sub(till)];
        add(&mut sum, &v);
        let mut row = vec![Cell::text(label)];
        row.extend(v.map(Cell::int));
        row.push(rate_bp(amount, whole));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(sum.map(Cell::int));
    totals.push(Cell::NULL);
    table.set_totals(totals)?;
    Ok(Built::charted(table, ChartKind::HBar, "category", "amount"))
}
