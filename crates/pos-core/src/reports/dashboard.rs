//! The home dashboard (RPT-060, UX-030): today's figures at a glance. Every figure the viewer
//! may not see is `None`, so the command never has to strip anything.

use super::{LINE_COST, RETURN_KINDS, REVENUE, SALE_KINDS, average, ints, name_of};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentMethod;
use chrono::NaiveDate;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

/// Days of the sales sparkline, today included.
pub const DASHBOARD_DAYS: i64 = 14;
pub const DASHBOARD_TOP: i64 = 5;

/// The documents of `SALES_DOCS` (an invoice made from a ticket is left out, its ticket counts).
const SALES: &str = "d.status = 'validated'
     AND (d.doc_type IN ('ticket', 'return_note', 'credit_note')
          OR (d.doc_type = 'invoice' AND d.origin IS NOT 'ticket'))
     AND d.supplier_id IS NULL";

/// What the viewer may see: costs and margins (`reports.profit`), balances (`credit.view`),
/// supplier balances (`purchases.view`), stock alerts (`stock.view`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DashboardView {
    pub costs: bool,
    pub credit: bool,
    pub purchases: bool,
    pub stock: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DayTotal {
    pub day: String,
    pub net_ttc: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TopLine {
    pub product_id: Option<String>,
    pub name: String,
    pub qty: Qty,
    pub net_ttc: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MethodNet {
    pub method: PaymentMethod,
    pub net: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub business_date: String,
    pub net_ttc: Money,
    pub tickets: i64,
    pub average_ticket: Money,
    /// Negative.
    pub returns_ttc: Money,
    pub yesterday_net_ttc: Money,
    pub month_net_ttc: Money,
    pub margin: Option<Money>,
    /// What customers owe (positive balances).
    pub receivables: Option<Money>,
    /// What the shop owes its suppliers.
    pub payables: Option<Money>,
    pub low_stock: Option<i64>,
    pub expiring: Option<i64>,
    pub open_sessions: i64,
    pub last_days: Vec<DayTotal>,
    pub top_products: Vec<TopLine>,
    pub by_method: Vec<MethodNet>,
}

fn net_between(conn: &Connection, from: &str, to: &str) -> Result<Money, AppError> {
    let sql = format!(
        "SELECT COALESCE(SUM(d.total_ttc), 0) FROM documents d WHERE {SALES} AND d.business_date BETWEEN ?1 AND ?2"
    );
    Ok(Money(
        conn.query_row(&sql, params![from, to], |r| r.get(0))?,
    ))
}

pub fn dashboard(
    conn: &Connection,
    today: &str,
    lang: Lang,
    view: DashboardView,
) -> Result<Dashboard, AppError> {
    let day = NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|_| AppError::validation("today", "date_invalid"))?;
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let first = day - chrono::Duration::days(DASHBOARD_DAYS - 1);
    let yesterday = fmt(day - chrono::Duration::days(1));
    let month_start = day.format("%Y-%m-01").to_string();

    let today_sql = format!(
        "SELECT SUM(d.total_ttc), SUM({SALE_KINDS}),
                SUM(CASE WHEN {RETURN_KINDS} THEN d.total_ttc ELSE 0 END),
                SUM(CASE WHEN {SALE_KINDS} THEN d.total_ttc ELSE 0 END)
         FROM documents d WHERE {SALES} AND d.business_date = ?1"
    );
    let [net, tickets, returns, sales] =
        conn.query_row(&today_sql, [today], |r| ints::<4>(r, 0))?;

    let margin = if view.costs {
        let sql = format!(
            "SELECT SUM({REVENUE}) - SUM({LINE_COST}) FROM document_lines l JOIN documents d ON d.id = l.document_id
             WHERE {SALES} AND l.parent_line_id IS NULL AND d.business_date = ?1"
        );
        Some(Money(
            conn.query_row(&sql, [today], |r| r.get::<_, Option<i64>>(0))?
                .unwrap_or(0),
        ))
    } else {
        None
    };
    let balance_sum = |table: &str| -> Result<Money, AppError> {
        let sql = format!("SELECT COALESCE(SUM(balance), 0) FROM {table} WHERE balance > 0");
        Ok(Money(conn.query_row(&sql, [], |r| r.get(0))?))
    };
    let receivables = if view.credit {
        Some(balance_sum("customers")?)
    } else {
        None
    };
    let payables = if view.purchases {
        Some(balance_sum("suppliers")?)
    } else {
        None
    };
    let (low_stock, expiring) = if view.stock {
        let low = crate::stock::below_minimum(conn, 1_000)?.len();
        let days: i64 = crate::settings::get(conn, "stock.expiry_alert_days")?;
        let soon = crate::stock::lots::expiry_list(conn, today, days)?.len();
        (
            Some(i64::try_from(low).unwrap_or(i64::MAX)),
            Some(i64::try_from(soon).unwrap_or(i64::MAX)),
        )
    } else {
        (None, None)
    };
    let open_sessions: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cash_sessions WHERE status = 'open'",
        [],
        |r| r.get(0),
    )?;

    let per_day_sql = format!(
        "SELECT d.business_date, SUM(d.total_ttc) FROM documents d
         WHERE {SALES} AND d.business_date BETWEEN ?1 AND ?2 GROUP BY d.business_date"
    );
    let mut stmt = conn.prepare(&per_day_sql)?;
    let per_day: BTreeMap<String, i64> = stmt
        .query_map(params![fmt(first), today], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<_, _>>()?;
    let last_days = first
        .iter_days()
        .take_while(|d| *d <= day)
        .map(|d| {
            let key = fmt(d);
            let net_ttc = Money(per_day.get(&key).copied().unwrap_or(0));
            DayTotal { day: key, net_ttc }
        })
        .collect();

    let name = name_of("p", lang);
    let top_sql = format!(
        "SELECT l.product_id, COALESCE(MAX({name}), MAX(l.description)), SUM(l.qty), SUM(l.total_ttc)
         FROM document_lines l JOIN documents d ON d.id = l.document_id LEFT JOIN products p ON p.id = l.product_id
         WHERE {SALES} AND l.parent_line_id IS NULL AND d.business_date = ?1
         GROUP BY COALESCE(l.product_id, l.description)
         ORDER BY SUM(l.total_ttc) DESC, 2 LIMIT ?2"
    );
    let mut stmt = conn.prepare(&top_sql)?;
    let top_products = stmt
        .query_map(params![today, DASHBOARD_TOP], |r| {
            Ok(TopLine {
                product_id: r.get(0)?,
                name: r.get(1)?,
                qty: Qty(r.get(2)?),
                net_ttc: Money(r.get(3)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let method_sql = format!(
        "SELECT CASE pm.method WHEN 'change' THEN 'cash' ELSE pm.method END AS tender, SUM(pm.amount)
         FROM payments pm JOIN documents d ON d.id = pm.document_id
         WHERE {SALES} AND d.business_date = ?1
         GROUP BY tender ORDER BY SUM(pm.amount) DESC, tender"
    );
    let mut stmt = conn.prepare(&method_sql)?;
    let by_method = stmt
        .query_map([today], |r| {
            Ok(MethodNet {
                method: r.get(0)?,
                net: Money(r.get(1)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Dashboard {
        business_date: fmt(day),
        net_ttc: Money(net),
        tickets,
        average_ticket: Money(average(sales, tickets)),
        returns_ttc: Money(returns),
        yesterday_net_ttc: net_between(conn, &yesterday, &yesterday)?,
        month_net_ttc: net_between(conn, &month_start, today)?,
        margin,
        receivables,
        payables,
        low_stock,
        expiring,
        open_sessions,
        last_days,
        top_products,
        by_method,
    })
}
