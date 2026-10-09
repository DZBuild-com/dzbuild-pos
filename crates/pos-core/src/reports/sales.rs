//! Sales reports: summary, per product, category, customer, cashier and payment method, the
//! period comparison, top products, discounts, returns, the line journal, slow movers and the
//! loss-prevention view.

use super::{
    Built, ChartKind, LINE_COST, RETURN_KINDS, REVENUE, SALE_KINDS, SALES_DOCS, SALES_LINES, Scope,
    add, average, ints, name_of, rate_bp, rows,
};
use crate::error::AppError;
use crate::table::{Cell, ColumnKind as K, Table, TagDomain};
use chrono::NaiveDate;
use rusqlite::Connection;
use std::collections::BTreeMap;

/// Lines of the scope with their document and product (aliases `l`, `d`, `p`).
const LINES_FROM: &str = "document_lines l JOIN documents d ON d.id = l.document_id
     LEFT JOIN products p ON p.id = l.product_id";

/// The summary's duty column is the duty charged to the customers: on the sales documents and on
/// the kridi versements (RV receipts) of the till, cashier and customer, as the X and Z count it.
pub(super) fn summary(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let group = s.req.group.unwrap_or_default();
    let period = group.sql("d.business_date");
    let versement_period = group.sql("e.business_date");
    let sql = format!(
        "SELECT period, SUM(tickets), SUM(returns), SUM(sales), SUM(returned), SUM(discounts),
                SUM(ht), SUM(tax), SUM(ttc), SUM(duty), SUM(credit)
         FROM (
           SELECT {period} AS period,
                  ({SALE_KINDS}) AS tickets, ({RETURN_KINDS}) AS returns,
                  CASE WHEN {SALE_KINDS} THEN d.total_ttc ELSE 0 END AS sales,
                  CASE WHEN {RETURN_KINDS} THEN d.total_ttc ELSE 0 END AS returned,
                  CASE WHEN {SALE_KINDS} THEN d.discount_total ELSE 0 END AS discounts,
                  d.subtotal_ht AS ht, d.tax_total AS tax, d.total_ttc AS ttc,
                  CASE WHEN d.stamp_mode = 'charge_customer' THEN d.stamp_duty ELSE 0 END AS duty,
                  d.credit_amount AS credit
           FROM documents d
           WHERE {SALES_DOCS}
           UNION ALL
           SELECT {versement_period}, 0, 0, 0, 0, 0, 0, 0, 0, e.stamp_charged, 0
           FROM customer_ledger e
           WHERE e.kind = 'payment' AND e.stamp_charged > 0
             AND e.business_date BETWEEN :day_from AND :day_to
             AND (:terminal_id IS NULL OR e.terminal_id = :terminal_id)
             AND (:user_id IS NULL OR e.user_id = :user_id)
             AND (:customer_id IS NULL OR e.customer_id = :customer_id))
         GROUP BY period ORDER BY period"
    );
    let mut table = Table::new(&[
        ("period", group.column_kind()),
        ("tickets", K::Int),
        ("returns", K::Int),
        ("salesTtc", K::Money),
        ("returnsTtc", K::Money),
        ("discounts", K::Money),
        ("netHt", K::Money),
        ("tax", K::Money),
        ("netTtc", K::Money),
        ("stampDuty", K::Money),
        ("onCredit", K::Money),
        ("averageTicket", K::Money),
    ]);
    let summary_row = |first: Cell, v: &[i64; 10]| -> Vec<Cell> {
        let mut out = vec![first];
        out.extend(v.iter().copied().map(Cell::int));
        out.push(Cell::int(average(v[2], v[0])));
        out
    };
    let mut sum = [0_i64; 10];
    for (period, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, ints::<10>(r, 1)?))
    })? {
        add(&mut sum, &v);
        table.push(summary_row(Cell::text(period), &v))?;
    }
    table.set_totals(summary_row(Cell::NULL, &sum))?;
    Ok(Built::charted(table, ChartKind::Bar, "period", "netTtc"))
}

/// Revenue, cost, margin and margin rate cells.
fn margin_cells(revenue: i64, cost: i64) -> [Cell; 3] {
    let margin = revenue.saturating_sub(cost);
    [Cell::int(cost), Cell::int(margin), rate_bp(margin, revenue)]
}

pub(super) fn by_product(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let (name, category) = (name_of("p", s.lang), name_of("c", s.lang));
    let sql = format!(
        "SELECT MAX(p.reference), COALESCE(MAX({name}), MAX(l.description)), MAX({category}),
                SUM(l.qty), SUM(l.total_ht), SUM(l.total_ttc), SUM({LINE_COST}), SUM({REVENUE})
         FROM {LINES_FROM} LEFT JOIN categories c ON c.id = p.category_id
         WHERE {SALES_DOCS} AND {SALES_LINES}
         GROUP BY COALESCE(l.product_id, l.description)
         ORDER BY SUM(l.total_ttc) DESC, 2"
    );
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("category", K::Text),
        ("qty", K::Qty),
        ("netHt", K::Money),
        ("netTtc", K::Money),
        ("cost", K::Money),
        ("margin", K::Money),
        ("marginRate", K::Percent),
    ]);
    let mut sum = [0_i64; 5];
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            ints::<5>(r, 3)?,
        ))
    })?;
    for (reference, name, category, v) in found {
        add(&mut sum, &v);
        let [qty, ht, ttc, cost, revenue] = v;
        let mut row = vec![
            Cell::opt_text(reference),
            Cell::opt_text(name),
            Cell::opt_text(category),
            Cell::int(qty),
            Cell::int(ht),
            Cell::int(ttc),
        ];
        row.extend(margin_cells(revenue, cost));
        table.push(row)?;
    }
    let [_, ht, ttc, cost, revenue] = sum;
    let mut totals = vec![
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::int(ht),
        Cell::int(ttc),
    ];
    totals.extend(margin_cells(revenue, cost));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}

pub(super) fn by_category(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let category = name_of("c", s.lang);
    let sql = format!(
        "SELECT MAX({category}), SUM(l.total_ht), SUM(l.total_ttc), SUM({LINE_COST}), SUM({REVENUE})
         FROM {LINES_FROM} LEFT JOIN categories c ON c.id = p.category_id
         WHERE {SALES_DOCS} AND {SALES_LINES}
         GROUP BY p.category_id
         ORDER BY SUM(l.total_ttc) DESC"
    );
    let found = rows(conn, &sql, s, |r| {
        Ok((r.get::<_, Option<String>>(0)?, ints::<4>(r, 1)?))
    })?;
    let mut sum = [0_i64; 4];
    for (_, v) in &found {
        add(&mut sum, v);
    }
    let mut table = Table::new(&[
        ("category", K::Text),
        ("netHt", K::Money),
        ("netTtc", K::Money),
        ("share", K::Percent),
        ("cost", K::Money),
        ("margin", K::Money),
        ("marginRate", K::Percent),
    ]);
    for (category, [ht, ttc, cost, revenue]) in found {
        let mut row = vec![
            Cell::opt_text(category),
            Cell::int(ht),
            Cell::int(ttc),
            rate_bp(ttc, sum[1]),
        ];
        row.extend(margin_cells(revenue, cost));
        table.push(row)?;
    }
    let [ht, ttc, cost, revenue] = sum;
    let mut totals = vec![Cell::NULL, Cell::int(ht), Cell::int(ttc), Cell::NULL];
    totals.extend(margin_cells(revenue, cost));
    table.set_totals(totals)?;
    Ok(Built::charted(table, ChartKind::HBar, "category", "netTtc"))
}

pub(super) fn by_customer(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT MAX(cu.code), MAX(cu.name), SUM({SALE_KINDS}), SUM(d.total_ttc), SUM(d.credit_amount),
                SUM(CASE WHEN {SALE_KINDS} THEN d.total_ttc ELSE 0 END), MAX(d.business_date)
         FROM documents d JOIN customers cu ON cu.id = d.customer_id
         WHERE {SALES_DOCS}
         GROUP BY d.customer_id
         ORDER BY SUM(d.total_ttc) DESC, 2"
    );
    let mut table = Table::new(&[
        ("code", K::Code),
        ("name", K::Text),
        ("tickets", K::Int),
        ("netTtc", K::Money),
        ("onCredit", K::Money),
        ("averageTicket", K::Money),
        ("lastPurchase", K::Date),
    ]);
    let mut sum = [0_i64; 4];
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            ints::<4>(r, 2)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    for (code, name, v, last) in found {
        add(&mut sum, &v);
        let [tickets, net, credit, sales] = v;
        table.push(vec![
            Cell::text(code),
            Cell::text(name),
            Cell::int(tickets),
            Cell::int(net),
            Cell::int(credit),
            Cell::int(average(sales, tickets)),
            Cell::text(last),
        ])?;
    }
    let [tickets, net, credit, sales] = sum;
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::int(tickets),
        Cell::int(net),
        Cell::int(credit),
        Cell::int(average(sales, tickets)),
        Cell::NULL,
    ])?;
    Ok(Built::plain(table))
}

pub(super) fn by_cashier(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT COALESCE(MAX(u.display_name), d.user_id),
                SUM({SALE_KINDS}), SUM({RETURN_KINDS}), SUM(d.total_ttc),
                SUM(CASE WHEN {SALE_KINDS} THEN d.discount_total ELSE 0 END),
                SUM(CASE WHEN {SALE_KINDS} THEN d.total_ttc ELSE 0 END)
         FROM documents d LEFT JOIN users u ON u.id = d.user_id
         WHERE {SALES_DOCS}
         GROUP BY d.user_id
         ORDER BY SUM(d.total_ttc) DESC"
    );
    let mut table = Table::new(&[
        ("cashier", K::Text),
        ("tickets", K::Int),
        ("returns", K::Int),
        ("netTtc", K::Money),
        ("discounts", K::Money),
        ("averageTicket", K::Money),
    ]);
    let cells = |first: Cell, v: &[i64; 5]| {
        vec![
            first,
            Cell::int(v[0]),
            Cell::int(v[1]),
            Cell::int(v[2]),
            Cell::int(v[3]),
            Cell::int(average(v[4], v[0])),
        ]
    };
    let mut sum = [0_i64; 5];
    for (name, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, ints::<5>(r, 1)?))
    })? {
        add(&mut sum, &v);
        table.push(cells(Cell::text(name), &v))?;
    }
    table.set_totals(cells(Cell::NULL, &sum))?;
    Ok(Built::charted(table, ChartKind::HBar, "cashier", "netTtc"))
}

/// Payments of the sales of the scope per method; the change given back is netted into cash, so
/// each method's net is what stayed in the shop. The netted method is aliased `tender`, not
/// `method`: SQLite resolves a `GROUP BY` name to the table column before the result alias.
pub(super) fn by_method(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT CASE pm.method WHEN 'change' THEN 'cash' ELSE pm.method END AS tender,
                SUM(CASE WHEN pm.amount > 0 THEN 1 ELSE 0 END),
                SUM(CASE WHEN pm.amount > 0 THEN pm.amount ELSE 0 END),
                SUM(CASE WHEN pm.amount < 0 THEN pm.amount ELSE 0 END),
                SUM(pm.amount)
         FROM payments pm JOIN documents d ON d.id = pm.document_id
         WHERE {SALES_DOCS}
         GROUP BY tender
         ORDER BY SUM(pm.amount) DESC, tender"
    );
    let mut table = Table::new(&[
        ("method", K::Tag),
        ("payments", K::Int),
        ("received", K::Money),
        ("paidBack", K::Money),
        ("net", K::Money),
    ])
    .tag("method", TagDomain::PaymentMethod);
    let mut sum = [0_i64; 4];
    for (method, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, ints::<4>(r, 1)?))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::text(method)];
        row.extend(v.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::charted(table, ChartKind::HBar, "method", "net"))
}

const METRICS: [&str; 7] = [
    "net_sales",
    "net_ht",
    "tax",
    "discounts",
    "returns",
    "average_ticket",
    "credit_sales",
];

fn figures(conn: &Connection, s: &Scope<'_>) -> Result<[i64; 7], AppError> {
    let sql = format!(
        "SELECT SUM(d.total_ttc), SUM(d.subtotal_ht), SUM(d.tax_total),
                SUM(CASE WHEN {SALE_KINDS} THEN d.discount_total ELSE 0 END),
                SUM(CASE WHEN {RETURN_KINDS} THEN d.total_ttc ELSE 0 END),
                SUM(CASE WHEN {SALE_KINDS} THEN d.total_ttc ELSE 0 END),
                SUM({SALE_KINDS}), SUM(d.credit_amount)
         FROM documents d
         WHERE {SALES_DOCS}"
    );
    let v = rows(conn, &sql, s, |r| ints::<8>(r, 0))?
        .into_iter()
        .next()
        .unwrap_or([0; 8]);
    Ok([v[0], v[1], v[2], v[3], v[4], average(v[5], v[6]), v[7]])
}

/// The range against the same number of days just before it (RPT-002).
pub(super) fn period_comparison(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let day = |d: &str, field: &str| {
        NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| AppError::validation(field, "date_invalid"))
    };
    let (from, to) = (day(&s.from, "from")?, day(&s.to, "to")?);
    let length = (to - from).num_days() + 1;
    let previous_to = from - chrono::Duration::days(1);
    let previous_from = previous_to - chrono::Duration::days(length - 1);
    let current = figures(conn, s)?;
    let previous = figures(
        conn,
        &s.over(
            previous_from.format("%Y-%m-%d").to_string(),
            previous_to.format("%Y-%m-%d").to_string(),
        ),
    )?;
    let mut table = Table::new(&[
        ("metric", K::Tag),
        ("current", K::Money),
        ("previous", K::Money),
        ("change", K::Money),
        ("changeRate", K::Percent),
    ])
    .tag("metric", TagDomain::Metric);
    for (i, metric) in METRICS.iter().enumerate() {
        let change = current[i].saturating_sub(previous[i]);
        table.push(vec![
            Cell::text(*metric),
            Cell::int(current[i]),
            Cell::int(previous[i]),
            Cell::int(change),
            rate_bp(change, previous[i]),
        ])?;
    }
    Ok(Built::plain(table))
}

pub(super) fn top_products(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let name = name_of("p", s.lang);
    let sql = format!(
        "SELECT MAX(p.reference), COALESCE(MAX({name}), MAX(l.description)), SUM(l.qty), SUM(l.total_ttc)
         FROM {LINES_FROM}
         WHERE {SALES_DOCS} AND {SALES_LINES}
         GROUP BY COALESCE(l.product_id, l.description)
         ORDER BY SUM(l.total_ttc) DESC, 2
         LIMIT :row_limit"
    );
    let all =
        format!("SELECT SUM(l.total_ttc) FROM {LINES_FROM} WHERE {SALES_DOCS} AND {SALES_LINES}");
    let whole = rows(conn, &all, s, |r| ints::<1>(r, 0))?
        .into_iter()
        .next()
        .map_or(0, |[v]| v);
    let mut table = Table::new(&[
        ("rank", K::Int),
        ("reference", K::Code),
        ("name", K::Text),
        ("qty", K::Qty),
        ("netTtc", K::Money),
        ("share", K::Percent),
    ]);
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            ints::<2>(r, 2)?,
        ))
    })?;
    for (rank, (reference, name, [qty, ttc])) in (1_i64..).zip(found) {
        table.push(vec![
            Cell::int(rank),
            Cell::opt_text(reference),
            Cell::opt_text(name),
            Cell::int(qty),
            Cell::int(ttc),
            rate_bp(ttc, whole),
        ])?;
    }
    Ok(Built::charted(table, ChartKind::HBar, "name", "netTtc"))
}

/// Discounts per cashier (RPT-009): the automatic part recorded in `sale_line_promotions`
/// (Task 7) split by source, the rest typed at the till.
pub(super) fn discounts(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT COALESCE(MAX(u.display_name), d.user_id),
                SUM(CASE WHEN l.discount_amount <> 0 THEN 1 ELSE 0 END),
                SUM(l.discount_amount - COALESCE(sp.amount, 0)),
                SUM(CASE WHEN sp.source = 'promotion' THEN sp.amount ELSE 0 END),
                SUM(CASE WHEN sp.source = 'customer' THEN sp.amount ELSE 0 END),
                SUM(l.discount_amount),
                SUM(l.price_overridden)
         FROM {LINES_FROM}
         LEFT JOIN sale_line_promotions sp ON sp.line_id = l.id
         LEFT JOIN users u ON u.id = d.user_id
         WHERE {SALES_DOCS} AND {SALES_LINES} AND {SALE_KINDS}
         GROUP BY d.user_id
         ORDER BY SUM(l.discount_amount) DESC"
    );
    let mut table = Table::new(&[
        ("cashier", K::Text),
        ("discountedLines", K::Int),
        ("manualDiscounts", K::Money),
        ("promotionDiscounts", K::Money),
        ("customerDiscounts", K::Money),
        ("discounts", K::Money),
        ("priceOverrides", K::Int),
    ]);
    let mut sum = [0_i64; 6];
    for (name, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, ints::<6>(r, 1)?))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::text(name)];
        row.extend(v.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}

/// Returned goods per reason (RPT-010), as positive amounts.
pub(super) fn returns(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT COALESCE(l.reason_code, 'other') AS reason, COUNT(DISTINCT d.id), COUNT(*),
                -SUM(l.total_ttc), -SUM({LINE_COST})
         FROM {LINES_FROM}
         WHERE {SALES_DOCS} AND {SALES_LINES} AND {RETURN_KINDS}
         GROUP BY reason
         ORDER BY -SUM(l.total_ttc) DESC, reason"
    );
    let mut table = Table::new(&[
        ("reason", K::Tag),
        ("returnNotes", K::Int),
        ("lines", K::Int),
        ("returnedTtc", K::Money),
        ("cost", K::Money),
    ])
    .tag("reason", TagDomain::ReturnReason);
    let mut sum = [0_i64; 4];
    for (reason, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, ints::<4>(r, 1)?))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::text(reason)];
        row.extend(v.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::charted(
        table,
        ChartKind::HBar,
        "reason",
        "returnedTtc",
    ))
}

/// The detailed line journal (RPT-012), oldest first, cut at the limit.
pub(super) fn lines(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let name = name_of("p", s.lang);
    let sql = format!(
        "SELECT d.business_date, d.number, d.doc_type, COALESCE(u.display_name, d.user_id), cu.name, p.reference,
                COALESCE({name}, l.description),
                l.qty, l.unit_price, l.discount_amount, l.total_ht, l.tax_amount, l.total_ttc,
                {LINE_COST}, {REVENUE} - {LINE_COST}
         FROM {LINES_FROM}
         LEFT JOIN users u ON u.id = d.user_id
         LEFT JOIN customers cu ON cu.id = d.customer_id
         WHERE {SALES_DOCS} AND {SALES_LINES}
         ORDER BY d.validated_at, d.number, l.line_no
         LIMIT :row_limit + 1"
    );
    let mut table = Table::new(&[
        ("date", K::Date),
        ("number", K::Code),
        ("docType", K::Tag),
        ("cashier", K::Text),
        ("customer", K::Text),
        ("reference", K::Code),
        ("product", K::Text),
        ("qty", K::Qty),
        ("unitPrice", K::Money),
        ("discount", K::Money),
        ("netHt", K::Money),
        ("tax", K::Money),
        ("netTtc", K::Money),
        ("cost", K::Money),
        ("margin", K::Money),
    ])
    .tag("docType", TagDomain::DocType);
    let found = rows(conn, &sql, s, |r| {
        let mut row = vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::text(r.get::<_, String>(2)?),
            Cell::opt_text(r.get(3)?),
            Cell::opt_text(r.get(4)?),
            Cell::opt_text(r.get(5)?),
            Cell::opt_text(r.get(6)?),
        ];
        row.extend(ints::<8>(r, 7)?.map(Cell::int));
        Ok(row)
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    for row in found {
        table.push(row)?;
    }
    if !truncated {
        let mut totals = vec![Cell::NULL; 10];
        for key in ["netHt", "tax", "netTtc", "cost", "margin"] {
            totals.push(Cell::money(table.money_total(key)));
        }
        table.set_totals(totals)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

/// Stock that did not sell in the range (RPT-021), never-sold first, then the oldest last sale.
pub(super) fn slow_movers(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let (name, category) = (name_of("p", s.lang), name_of("c", s.lang));
    let sql = format!(
        "SELECT p.reference, {name}, {category}, oh.qty,
                (SELECT MAX(d.business_date) FROM document_lines l JOIN documents d ON d.id = l.document_id
                  WHERE l.product_id = p.id AND d.status = 'validated' AND {SALE_KINDS}
                    AND d.origin IS NOT 'ticket') AS last_sale,
                (oh.qty * p.cost_price + 500) / 1000 AS cost_value
         FROM products p
         JOIN (SELECT sl.product_id, SUM(sl.qty) AS qty
               FROM stock_levels sl JOIN stock_locations loc ON loc.id = sl.location_id
               WHERE loc.kind = 'storage' AND (:location_id IS NULL OR sl.location_id = :location_id)
               GROUP BY sl.product_id) oh ON oh.product_id = p.id
         LEFT JOIN categories c ON c.id = p.category_id
         WHERE p.archived_at IS NULL AND p.track_stock = 1 AND oh.qty > 0
           AND (:category_id IS NULL OR p.category_id = :category_id)
           AND NOT EXISTS (SELECT 1 FROM document_lines l JOIN documents d ON d.id = l.document_id
                           WHERE l.product_id = p.id AND d.status = 'validated' AND {SALE_KINDS}
                             AND d.origin IS NOT 'ticket'
                             AND d.business_date BETWEEN :day_from AND :day_to)
         ORDER BY last_sale IS NOT NULL, last_sale, cost_value DESC
         LIMIT :row_limit + 1"
    );
    let found = rows(conn, &sql, s, |r| {
        Ok(vec![
            Cell::opt_text(r.get(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::opt_text(r.get(2)?),
            Cell::int(r.get(3)?),
            Cell::opt_text(r.get(4)?),
            Cell::int(r.get(5)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("category", K::Text),
        ("qty", K::Qty),
        ("lastSale", K::Date),
        ("costValue", K::Money),
    ]);
    for row in found {
        table.push(row)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

#[derive(Default)]
struct Loss {
    name: String,
    figures: [i64; 6],
}

/// Per cashier: returns, their value, price overrides, manual discounts, cash paid out and the
/// variance of the sessions they closed (RPT-043).
pub(super) fn loss_prevention(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let mut by_user: BTreeMap<String, Loss> = BTreeMap::new();
    let mut merge = |found: Vec<(String, Vec<i64>)>, at: usize| {
        for (user, values) in found {
            let entry = by_user.entry(user).or_default();
            for (i, v) in values.into_iter().enumerate() {
                if let Some(slot) = entry.figures.get_mut(at + i) {
                    *slot = slot.saturating_add(v);
                }
            }
        }
    };
    let returns = format!(
        "SELECT d.user_id, COUNT(*), -SUM(d.total_ttc) FROM documents d
         WHERE {SALES_DOCS} AND {RETURN_KINDS} GROUP BY d.user_id"
    );
    merge(
        rows(conn, &returns, s, |r| {
            Ok((r.get(0)?, ints::<2>(r, 1)?.to_vec()))
        })?,
        0,
    );
    let till = format!(
        "SELECT d.user_id, SUM(l.price_overridden), SUM(l.discount_amount - COALESCE(sp.amount, 0))
         FROM {LINES_FROM} LEFT JOIN sale_line_promotions sp ON sp.line_id = l.id
         WHERE {SALES_DOCS} AND {SALES_LINES} AND {SALE_KINDS} GROUP BY d.user_id"
    );
    merge(
        rows(conn, &till, s, |r| {
            Ok((r.get(0)?, ints::<2>(r, 1)?.to_vec()))
        })?,
        2,
    );
    let paid_out = "SELECT m.user_id, -SUM(m.amount) FROM cash_movements m JOIN cash_sessions cs ON cs.id = m.session_id
         WHERE m.kind = 'paid_out' AND cs.business_date BETWEEN :day_from AND :day_to
           AND (:user_id IS NULL OR m.user_id = :user_id) GROUP BY m.user_id";
    merge(
        rows(conn, paid_out, s, |r| {
            Ok((r.get(0)?, ints::<1>(r, 1)?.to_vec()))
        })?,
        4,
    );
    let variance = "SELECT cs.user_id, SUM(cs.variance) FROM cash_sessions cs
         WHERE cs.status = 'closed' AND cs.business_date BETWEEN :day_from AND :day_to
           AND (:user_id IS NULL OR cs.user_id = :user_id) GROUP BY cs.user_id";
    merge(
        rows(conn, variance, s, |r| {
            Ok((r.get(0)?, ints::<1>(r, 1)?.to_vec()))
        })?,
        5,
    );
    let names = rows(conn, "SELECT id, display_name FROM users", s, |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for (id, name) in names {
        if let Some(entry) = by_user.get_mut(&id) {
            entry.name = name;
        }
    }
    let mut people: Vec<(String, Loss)> = by_user
        .into_iter()
        .filter(|(_, l)| l.figures.iter().any(|v| *v != 0))
        .collect();
    people.sort_by(|a, b| a.1.name.cmp(&b.1.name).then_with(|| a.0.cmp(&b.0)));
    let mut table = Table::new(&[
        ("cashier", K::Text),
        ("returns", K::Int),
        ("returnsTtc", K::Money),
        ("priceOverrides", K::Int),
        ("manualDiscounts", K::Money),
        ("paidOut", K::Money),
        ("cashVariance", K::Money),
    ]);
    let mut sum = [0_i64; 6];
    for (id, loss) in people {
        add(&mut sum, &loss.figures);
        let mut row = vec![Cell::text(if loss.name.is_empty() {
            id
        } else {
            loss.name
        })];
        row.extend(loss.figures.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}
