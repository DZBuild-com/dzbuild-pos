//! Margin reports (RPT-013…016) and the profit and loss statement (FIN-005): revenue (HT under
//! the réel, TTC under the IFU), cost of goods at the cost recorded on each line, stock losses and
//! count differences at cost, the kridi adjustments (debts written off), the droit de timbre the
//! shop absorbed, and the expenses per category, including M1 till expenses that have no
//! `expenses` row.

use super::{
    Built, ChartKind, LINE_COST, REVENUE, SALES_DOCS, SALES_LINES, Scope, add, day_of, ints,
    name_of, rate_bp, rows,
};
use crate::error::AppError;
use crate::table::{Cell, ColumnKind as K, Table, TagDomain};
use rusqlite::Connection;

const LINES_FROM: &str = "document_lines l JOIN documents d ON d.id = l.document_id
     LEFT JOIN products p ON p.id = l.product_id";

fn margin_cells(revenue: i64, cost: i64) -> [Cell; 4] {
    let margin = revenue.saturating_sub(cost);
    [
        Cell::int(revenue),
        Cell::int(cost),
        Cell::int(margin),
        rate_bp(margin, revenue),
    ]
}

pub(super) fn by_period(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let group = s.req.group.unwrap_or_default();
    let period = group.sql("d.business_date");
    let sql = format!(
        "SELECT {period} AS period, SUM({REVENUE}), SUM({LINE_COST})
         FROM {LINES_FROM}
         WHERE {SALES_DOCS} AND {SALES_LINES}
         GROUP BY period ORDER BY period"
    );
    let mut table = Table::new(&[
        ("period", group.column_kind()),
        ("revenue", K::Money),
        ("cost", K::Money),
        ("margin", K::Money),
        ("marginRate", K::Percent),
    ]);
    let mut sum = [0_i64; 2];
    for (period, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, ints::<2>(r, 1)?))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::text(period)];
        row.extend(margin_cells(v[0], v[1]));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(margin_cells(sum[0], sum[1]));
    table.set_totals(totals)?;
    Ok(Built::charted(table, ChartKind::Bar, "period", "margin"))
}

pub(super) fn by_product(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let name = name_of("p", s.lang);
    let sql = format!(
        "SELECT MAX(p.reference), COALESCE(MAX({name}), MAX(l.description)), SUM(l.qty), SUM({REVENUE}), SUM({LINE_COST})
         FROM {LINES_FROM}
         WHERE {SALES_DOCS} AND {SALES_LINES}
         GROUP BY COALESCE(l.product_id, l.description)
         ORDER BY SUM({REVENUE}) - SUM({LINE_COST}) DESC, 2"
    );
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("qty", K::Qty),
        ("revenue", K::Money),
        ("cost", K::Money),
        ("margin", K::Money),
        ("marginRate", K::Percent),
    ]);
    let mut sum = [0_i64; 2];
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            ints::<3>(r, 2)?,
        ))
    })?;
    for (reference, name, [qty, revenue, cost]) in found {
        add(&mut sum, &[revenue, cost]);
        let mut row = vec![
            Cell::opt_text(reference),
            Cell::opt_text(name),
            Cell::int(qty),
        ];
        row.extend(margin_cells(revenue, cost));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL, Cell::NULL, Cell::NULL];
    totals.extend(margin_cells(sum[0], sum[1]));
    table.set_totals(totals)?;
    Ok(Built::charted(table, ChartKind::HBar, "name", "margin"))
}

pub(super) fn by_category(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let category = name_of("c", s.lang);
    let sql = format!(
        "SELECT MAX({category}), SUM({REVENUE}), SUM({LINE_COST})
         FROM {LINES_FROM} LEFT JOIN categories c ON c.id = p.category_id
         WHERE {SALES_DOCS} AND {SALES_LINES}
         GROUP BY p.category_id
         ORDER BY SUM({REVENUE}) - SUM({LINE_COST}) DESC"
    );
    let mut table = Table::new(&[
        ("category", K::Text),
        ("revenue", K::Money),
        ("cost", K::Money),
        ("margin", K::Money),
        ("marginRate", K::Percent),
    ]);
    let mut sum = [0_i64; 2];
    for (category, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, Option<String>>(0)?, ints::<2>(r, 1)?))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::opt_text(category)];
        row.extend(margin_cells(v[0], v[1]));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(margin_cells(sum[0], sum[1]));
    table.set_totals(totals)?;
    Ok(Built::charted(table, ChartKind::HBar, "category", "margin"))
}

/// One line of the statement: its `pnl_line` code, a label (the expense category), the signed
/// amount (income positive, costs negative, so the column adds up).
fn line(table: &mut Table, code: &str, label: Option<String>, amount: i64) -> Result<(), AppError> {
    table.push(vec![
        Cell::text(code),
        Cell::opt_text(label),
        Cell::int(amount),
    ])
}

pub(super) fn profit_and_loss(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sales = format!(
        "SELECT SUM({REVENUE}), SUM({LINE_COST}) FROM {LINES_FROM} WHERE {SALES_DOCS} AND {SALES_LINES}"
    );
    let [revenue, cost] = rows(conn, &sales, s, |r| ints::<2>(r, 0))?
        .into_iter()
        .next()
        .unwrap_or([0; 2]);
    // Negative adjustments for damage, loss, theft and expiry, and count differences, at the cost
    // of the movement (the product's cost when the movement has none): a signed sum, ≤ 0 when the
    // shop lost goods.
    let moved_on = day_of("m.created_at");
    let losses = format!(
        "SELECT SUM((m.qty_delta * COALESCE(m.unit_cost, p.cost_price)
                     + CASE WHEN m.qty_delta < 0 THEN -500 ELSE 500 END) / 1000)
         FROM stock_movements m JOIN products p ON p.id = m.product_id
         WHERE (m.reason_code IN ('damage', 'loss', 'theft', 'expiry') OR m.reason = 'count')
           AND {moved_on} BETWEEN :day_from AND :day_to"
    );
    let [losses] = rows(conn, &losses, s, |r| ints::<1>(r, 0))?
        .into_iter()
        .next()
        .unwrap_or([0]);
    // The droit de timbre the shop paid instead of its customers ("absorb: shop pays; still
    // recorded"): on the absorbing sales documents, and on cash kridi versements and cash
    // deposits, whose rows record the duty and the part the customer handed over.
    let absorbed = format!(
        "SELECT (SELECT COALESCE(SUM(d.stamp_duty), 0) FROM documents d
                 WHERE {SALES_DOCS} AND d.stamp_mode = 'absorb')
              + (SELECT COALESCE(SUM(e.stamp_duty - e.stamp_charged), 0) FROM customer_ledger e
                 WHERE e.kind = 'payment' AND e.business_date BETWEEN :day_from AND :day_to)
              + (SELECT COALESCE(SUM(x.stamp_duty - x.stamp_charged), 0) FROM deposits x
                 WHERE x.kind = 'deposit' AND x.business_date BETWEEN :day_from AND :day_to)
              + (SELECT COALESCE(SUM(g.stamp_duty - g.stamp_charged), 0) FROM gift_voucher_ledger g
                 WHERE g.kind = 'issue' AND g.business_date BETWEEN :day_from AND :day_to)"
    );
    let [absorbed] = rows(conn, &absorbed, s, |r| ints::<1>(r, 0))?
        .into_iter()
        .next()
        .unwrap_or([0]);
    // The kridi adjustments (CRD, "a bad debt written off, a mistake in the notebook"): what the
    // customers owe changed with no sale and no money, a signed sum, ≤ 0 when debts were written
    // off. Opening balances are carried over from before, never income of the period, and so are
    // the counter-entries of an undone opening import (`opening_undo` adjustments).
    let adjusted = "SELECT SUM(e.amount) FROM customer_ledger e
         WHERE e.kind = 'adjustment' AND e.ref_type IS NOT 'opening_undo'
           AND e.business_date BETWEEN :day_from AND :day_to";
    let [credit_adjustments] = rows(conn, adjusted, s, |r| ints::<1>(r, 0))?
        .into_iter()
        .next()
        .unwrap_or([0]);
    // The expenses per category, as the expenses report lists them (M1/M1b till expenses included).
    let spent: Vec<(String, i64)> = super::cash::per_category(conn, s)?
        .into_iter()
        .map(|(label, [_, amount, _])| (label, amount))
        .collect();
    let expenses_total = spent
        .iter()
        .fold(0_i64, |acc, (_, amount)| acc.saturating_add(*amount));

    let gross = revenue.saturating_sub(cost);
    let net = gross
        .saturating_add(losses)
        .saturating_add(credit_adjustments)
        .saturating_sub(absorbed)
        .saturating_sub(expenses_total);
    let mut table = Table::new(&[("line", K::Tag), ("label", K::Text), ("amount", K::Money)])
        .tag("line", TagDomain::PnlLine);
    line(&mut table, "revenue", None, revenue)?;
    line(&mut table, "cost_of_goods", None, cost.saturating_neg())?;
    line(&mut table, "gross_margin", None, gross)?;
    line(&mut table, "stock_losses", None, losses)?;
    line(&mut table, "credit_adjustments", None, credit_adjustments)?;
    line(
        &mut table,
        "stamp_absorbed",
        None,
        absorbed.saturating_neg(),
    )?;
    for (label, amount) in spent {
        line(&mut table, "expense", Some(label), amount.saturating_neg())?;
    }
    line(
        &mut table,
        "expenses_total",
        None,
        expenses_total.saturating_neg(),
    )?;
    line(&mut table, "net_profit", None, net)?;
    Ok(Built::plain(table))
}
