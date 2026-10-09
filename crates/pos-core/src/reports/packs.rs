//! Reports of the vertical packs (RPT-062, 065, 068, 069, 071, 073, 074 and the pack registers):
//! each reads its pack's tables and the validated documents they point to.

use super::{Built, RETURN_KINDS, SALE_KINDS, SALES_DOCS, Scope, average, rows};
use crate::error::AppError;
use crate::money::{Money, Qty, Rate};
use crate::payments::PaymentMethod;
use crate::table::{Cell, ColumnKind as K, Table, TagDomain};
use chrono::NaiveDate;
use rusqlite::{Connection, params};

/// RPT-065: sales of paid table tickets per dining mode, place and waiter.
pub(super) fn sales_by_table(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT t.mode, COALESCE(dt.code, t.label, ''), u.display_name, COUNT(*), SUM(t.covers), SUM(d.total_ttc)
         FROM dining_tickets t
         JOIN documents d ON d.id = t.document_id
         JOIN users u ON u.id = t.waiter_id
         LEFT JOIN dining_tables dt ON dt.id = t.table_id
         WHERE t.status = 'paid' AND d.status = 'validated'
           AND d.business_date BETWEEN :day_from AND :day_to
           AND (:terminal_id IS NULL OR d.terminal_id = :terminal_id)
           AND (:user_id IS NULL OR t.waiter_id = :user_id)
         GROUP BY t.mode, 2, u.display_name
         ORDER BY 6 DESC
         LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        let (tickets, covers, total): (i64, i64, i64) = (r.get(3)?, r.get(4)?, r.get(5)?);
        Ok(vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::text(r.get::<_, String>(1)?),
            Cell::text(r.get::<_, String>(2)?),
            Cell::int(tickets),
            Cell::int(covers),
            Cell::int(total),
            if covers > 0 {
                Cell::int(average(total, covers))
            } else {
                Cell::NULL
            },
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("diningMode", K::Tag),
        ("place", K::Text),
        ("waiter", K::Text),
        ("tickets", K::Int),
        ("covers", K::Int),
        ("totalTtc", K::Money),
        ("perCover", K::Money),
    ])
    .tag("diningMode", TagDomain::DiningMode);
    for row in found {
        table.push(row)?;
    }
    let sum = |i: usize| {
        table
            .rows
            .iter()
            .filter_map(|r| r[i].as_i64())
            .fold(0_i64, i64::saturating_add)
    };
    let (tickets, covers, total) = (sum(3), sum(4), sum(5));
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::int(tickets),
        Cell::int(covers),
        Cell::int(total),
        if covers > 0 {
            Cell::int(average(total, covers))
        } else {
            Cell::NULL
        },
    ])?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// The controlled-medicine register of the period (pharmacy): M2's report screen lists and
/// exports it, M3's table print gives the paged register.
pub(super) fn controlled_register(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    Ok(Built::plain(crate::pharmacy::register::table(
        conn, &s.from, &s.to, s.lang,
    )?))
}

/// STK-026: waste per day, product and reason, at cost.
pub(super) fn waste(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let day = super::day_of("m.created_at");
    let name = super::name_of("p", s.lang);
    let cost = super::value_sql("-m.qty_delta", "COALESCE(m.unit_cost, 0)");
    let sql = format!(
        "SELECT {day}, p.reference, {name}, m.reason_code, -SUM(m.qty_delta), SUM({cost})
         FROM stock_movements m JOIN products p ON p.id = m.product_id
         WHERE m.reason = 'loss' AND m.reason_code IN ('damage', 'expiry', 'loss', 'own_use')
           AND {day} BETWEEN :day_from AND :day_to
           AND (:location_id IS NULL OR m.location_id = :location_id)
           AND (:category_id IS NULL OR p.category_id = :category_id)
         GROUP BY 1, m.product_id, m.reason_code ORDER BY 1, 3 LIMIT :row_limit + 1"
    );
    let found = rows(conn, &sql, s, |r| {
        Ok(vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::text(r.get::<_, String>(2)?),
            Cell::text(r.get::<_, String>(3)?),
            Cell::int(r.get(4)?),
            Cell::int(r.get(5)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("date", K::Date),
        ("reference", K::Code),
        ("product", K::Text),
        ("adjustReason", K::Tag),
        ("qty", K::Qty),
        ("costValue", K::Money),
    ])
    .tag("adjustReason", TagDomain::AdjustReason);
    for row in found {
        table.push(row)?;
    }
    let total = table.money_total("costValue");
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::money(total),
    ])?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// VRT-011: cutting runs of the period.
pub(super) fn yield_runs(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let name = super::name_of("p", s.lang);
    let sql = format!(
        "SELECT y.number, y.business_date, {name}, y.input_qty, y.output_qty, y.loss_qty, y.input_cost
         FROM yield_runs y JOIN products p ON p.id = y.source_product_id
         WHERE y.business_date BETWEEN :day_from AND :day_to ORDER BY y.created_at LIMIT :row_limit + 1"
    );
    let found = rows(conn, &sql, s, |r| {
        let (input, loss): (i64, i64) = (r.get(3)?, r.get(5)?);
        Ok(vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::text(r.get::<_, String>(1)?),
            Cell::text(r.get::<_, String>(2)?),
            Cell::int(input),
            Cell::int(r.get(4)?),
            Cell::int(loss),
            super::rate_bp(loss, input),
            Cell::int(r.get(6)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("runNumber", K::Code),
        ("date", K::Date),
        ("product", K::Text),
        ("inputQty", K::Qty),
        ("outputQty", K::Qty),
        ("lossQty", K::Qty),
        ("lossRate", K::Percent),
        ("cost", K::Money),
    ]);
    for row in found {
        table.push(row)?;
    }
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// RPT-074: top-ups per day and operator, the commission on them, and the float bought.
pub(super) fn flexy_sales(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let settings: Vec<crate::flexy::OperatorSetting> =
        serde_json::from_value(crate::settings::get_value(conn, "flexy.operators")?)?;
    let sql = "SELECT f.business_date, f.operator,
                SUM(CASE WHEN f.kind = 'sale' THEN 1 ELSE 0 END),
                -SUM(CASE WHEN f.kind = 'sale' THEN f.amount ELSE 0 END),
                SUM(CASE WHEN f.kind = 'purchase' THEN f.amount ELSE 0 END),
                SUM(CASE WHEN f.kind = 'purchase' THEN f.cost ELSE 0 END)
         FROM flexy_float f
         WHERE f.business_date BETWEEN :day_from AND :day_to
           AND (:terminal_id IS NULL OR f.terminal_id = :terminal_id)
         GROUP BY 1, 2 ORDER BY 1, 2 LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("date", K::Date),
        ("operator", K::Text),
        ("topUps", K::Int),
        ("amount", K::Money),
        ("commission", K::Money),
        ("floatBought", K::Money),
        ("floatCost", K::Money),
    ]);
    for (day, op, count, amount, bought, cost) in found {
        let bp = settings
            .iter()
            .find(|x| x.code == op)
            .map_or(0, |x| x.commission_bp);
        let commission = Money(amount).apply_rate(Rate(bp)).unwrap_or(Money::ZERO);
        table.push(vec![
            Cell::text(day),
            Cell::text(op),
            Cell::int(count),
            Cell::int(amount),
            Cell::money(commission),
            Cell::int(bought),
            Cell::int(cost),
        ])?;
    }
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

fn report_day(day: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| AppError::validation("to", "date_invalid"))
}

/// RPT-068: the service tickets taken in over the period (or by one technician), with the days
/// they stayed open, the estimate, what the pickup ticket sold and its cost and margin (HT under
/// the réel, TTC under the IFU, like `REVENUE`).
pub(super) fn service_tickets(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let cost = super::LINE_COST;
    let sql = format!(
        "SELECT t.number, t.kind, t.status, c.name, u.display_name, t.business_date, t.closed_at,
                t.estimate, d.total_ttc,
                (SELECT SUM({cost}) FROM document_lines l
                 WHERE l.document_id = d.id AND l.parent_line_id IS NULL),
                CASE d.tax_regime WHEN 'ifu' THEN d.total_ttc ELSE d.subtotal_ht END
         FROM service_tickets t
         JOIN customers c ON c.id = t.customer_id
         LEFT JOIN users u ON u.id = t.technician_id
         LEFT JOIN documents d ON d.id = t.document_id
         WHERE t.business_date BETWEEN :day_from AND :day_to
           AND (:user_id IS NULL OR t.technician_id = :user_id)
         ORDER BY t.created_at, t.rowid LIMIT :row_limit + 1"
    );
    let found = rows(conn, &sql, s, |r| {
        Ok((
            [
                Cell::text(r.get::<_, String>(0)?),
                Cell::text(r.get::<_, String>(1)?),
                Cell::text(r.get::<_, String>(2)?),
                Cell::text(r.get::<_, String>(3)?),
                Cell::opt_text(r.get(4)?),
            ],
            r.get::<_, String>(5)?,
            r.get::<_, Option<i64>>(6)?,
            [
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<i64>>(9)?,
                r.get::<_, Option<i64>>(10)?,
            ],
        ))
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let to = report_day(&s.to)?;
    let mut table = Table::new(&[
        ("ticketNumber", K::Code),
        ("serviceKind", K::Tag),
        ("serviceStatus", K::Tag),
        ("customer", K::Text),
        ("technician", K::Text),
        ("daysOpen", K::Int),
        ("estimate", K::Money),
        ("totalTtc", K::Money),
        ("cost", K::Money),
        ("margin", K::Money),
    ])
    .tag("serviceKind", TagDomain::ServiceKind)
    .tag("serviceStatus", TagDomain::ServiceStatus);
    let opt = |v: Option<i64>| v.map_or(Cell::NULL, Cell::int);
    for (texts, opened, closed_at, [estimate, total, cost, revenue]) in found {
        let end = closed_at.map_or(to, crate::time::business_date);
        let days = (end - report_day(&opened)?).num_days();
        // A delivered ticket has a sale: its cost is 0 when every line was labour.
        let cost = total.map(|_| cost.unwrap_or(0));
        let margin = revenue.zip(cost).map(|(r, c)| r.saturating_sub(c));
        let mut row: Vec<Cell> = texts.into_iter().collect();
        row.extend([
            Cell::int(days),
            opt(estimate),
            opt(total),
            opt(cost),
            opt(margin),
        ]);
        table.push(row)?;
    }
    let totals: Vec<Cell> = ["totalTtc", "cost", "margin"]
        .iter()
        .map(|k| Cell::money(table.money_total(k)))
        .collect();
    let mut row = vec![Cell::NULL; 7];
    row.extend(totals);
    table.set_totals(row)?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// RPT-069: as of `to`, the ready items waiting longer than `repairs.uncollected_days`, oldest
/// first, with the days they waited and the deposits still held.
pub(super) fn uncollected_tickets(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let days: i64 = crate::settings::get(conn, "repairs.uncollected_days")?;
    let to = report_day(&s.to)?;
    let cutoff = (to - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();
    let ready_day = super::day_of("t.ready_at");
    let mut stmt = conn.prepare(&format!(
        "SELECT t.number, c.name, t.item, t.ready_at,
                (SELECT COALESCE(SUM(d.amount), 0) FROM deposits d
                 WHERE d.owner_type = 'service_ticket' AND d.owner_id = t.id)
         FROM service_tickets t JOIN customers c ON c.id = t.customer_id
         WHERE t.status = 'ready' AND t.ready_at IS NOT NULL AND {ready_day} < ?1
         ORDER BY t.ready_at, t.rowid LIMIT ?2 + 1"
    ))?;
    let found = stmt
        .query_map(params![cutoff, s.limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("ticketNumber", K::Code),
        ("customer", K::Text),
        ("item", K::Text),
        ("readySince", K::Date),
        ("daysWaiting", K::Int),
        ("deposits", K::Money),
    ]);
    for (number, customer, item, ready_at, held) in found {
        let since = crate::time::business_date(ready_at);
        table.push(vec![
            Cell::text(number),
            Cell::text(customer),
            Cell::text(item),
            Cell::text(since.format("%Y-%m-%d").to_string()),
            Cell::int((to - since).num_days()),
            Cell::int(held),
        ])?;
    }
    let held = table.money_total("deposits");
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::money(held),
    ])?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// TMP-005: as of `to`, every due of the contracts signed by then that is past its date and not
/// fully paid (the payments of `to` or before fill the oldest dues first; a later payment is
/// left out), oldest first, with what was paid on it by then, what was left and its days late.
pub(super) fn instalments_overdue(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let ids = conn
        .prepare(
            "SELECT p.id FROM instalment_plans p
             WHERE (?1 IS NULL OR p.customer_id = ?1) AND p.business_date <= ?2
               AND EXISTS (SELECT 1 FROM instalment_dues d WHERE d.plan_id = p.id AND d.due_on < ?2)
             ORDER BY p.created_at, p.rowid",
        )?
        .query_map(params![s.req.customer_id, s.to], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut found = Vec::new();
    for id in ids {
        let plan = crate::instalments::get_as_of(conn, &id, &s.to)?;
        for due in plan.dues {
            if due.status == crate::instalments::DueStatus::Overdue {
                found.push((plan.number.clone(), plan.customer_name.clone(), due));
            }
        }
    }
    found.sort_by(|a, b| a.2.due_on.cmp(&b.2.due_on).then_with(|| a.0.cmp(&b.0)));
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("contractNumber", K::Code),
        ("customer", K::Text),
        ("dueDate", K::Date),
        ("amount", K::Money),
        ("paid", K::Money),
        ("left", K::Money),
        ("daysLate", K::Int),
    ]);
    for (number, customer, due) in found {
        let left = due.amount.checked_sub(due.paid).unwrap_or(Money::ZERO);
        table.push(vec![
            Cell::text(number),
            Cell::text(customer),
            Cell::text(due.due_on),
            Cell::money(due.amount),
            Cell::money(due.paid),
            Cell::money(left),
            Cell::int(due.days_late),
        ])?;
    }
    let totals: Vec<Cell> = ["amount", "paid", "left"]
        .iter()
        .map(|k| Cell::money(table.money_total(k)))
        .collect();
    let mut row = vec![Cell::NULL; 3];
    row.extend(totals);
    row.push(Cell::NULL);
    table.set_totals(row)?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// RPT-073: delivery rounds of the period with their stops and collections, filtered by the
/// round's creator. Customers' phones and addresses stay on the route sheet.
pub(super) fn delivery_rounds(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT r.number, r.business_date, r.driver_name, COALESCE(r.vehicle_plate, ''), COUNT(st.seq),
                      SUM(st.status = 'delivered'), SUM(st.status = 'failed'), SUM(st.amount_due), SUM(st.collected),
                      SUM(CASE WHEN st.method = 'cash' THEN st.collected ELSE 0 END), r.status
               FROM delivery_rounds r LEFT JOIN delivery_round_stops st ON st.round_id = r.id
               WHERE r.business_date BETWEEN :day_from AND :day_to
                 AND (:user_id IS NULL OR r.user_id = :user_id)
               GROUP BY r.id ORDER BY r.business_date, r.number LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        let n =
            |i: usize| -> rusqlite::Result<i64> { Ok(r.get::<_, Option<i64>>(i)?.unwrap_or(0)) };
        Ok(vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::text(r.get::<_, String>(1)?),
            Cell::text(r.get::<_, String>(2)?),
            Cell::text(r.get::<_, String>(3)?),
            Cell::int(r.get(4)?),
            Cell::int(n(5)?),
            Cell::int(n(6)?),
            Cell::money(Money(n(7)?)),
            Cell::money(Money(n(8)?)),
            Cell::money(Money(n(9)?)),
            Cell::text(r.get::<_, String>(10)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("roundNumber", K::Code),
        ("date", K::Date),
        ("driver", K::Text),
        ("vehiclePlate", K::Code),
        ("stops", K::Int),
        ("delivered", K::Int),
        ("failed", K::Int),
        ("amountDue", K::Money),
        ("collected", K::Money),
        ("cashCollected", K::Money),
        ("roundStatus", K::Tag),
    ])
    .tag("roundStatus", TagDomain::RoundStatus);
    for row in found {
        table.push(row)?;
    }
    let sum = |i: usize| {
        Cell::int(
            table
                .rows
                .iter()
                .filter_map(|r| r[i].as_i64())
                .fold(0_i64, i64::saturating_add),
        )
    };
    let totals = vec![
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        sum(4),
        sum(5),
        sum(6),
        sum(7),
        sum(8),
        sum(9),
        Cell::NULL,
    ];
    table.set_totals(totals)?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// LOY-002: per customer who holds or held a loyalty card, the points of the period by kind
/// (redeemed, expired and taken back as positive counts, adjustments signed) and the balance at
/// the end of `to`.
pub(super) fn loyalty_points(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT c.name,
                      (SELECT k.code FROM loyalty_cards k WHERE k.customer_id = c.id
                       ORDER BY k.blocked_at IS NULL DESC, k.issued_at DESC LIMIT 1),
                      SUM(CASE WHEN l.kind = 'earn' AND l.business_date >= :day_from THEN l.points ELSE 0 END),
                      -SUM(CASE WHEN l.kind = 'redeem' AND l.business_date >= :day_from THEN l.points ELSE 0 END),
                      -SUM(CASE WHEN l.kind = 'expire' AND l.business_date >= :day_from THEN l.points ELSE 0 END),
                      -SUM(CASE WHEN l.kind = 'reversal' AND l.business_date >= :day_from THEN l.points ELSE 0 END),
                      SUM(CASE WHEN l.kind = 'adjust' AND l.business_date >= :day_from THEN l.points ELSE 0 END),
                      SUM(l.points)
               FROM customers c
               LEFT JOIN loyalty_ledger l ON l.customer_id = c.id AND l.business_date <= :day_to
               WHERE EXISTS (SELECT 1 FROM loyalty_cards k WHERE k.customer_id = c.id)
                 AND (:customer_id IS NULL OR c.id = :customer_id)
               GROUP BY c.id ORDER BY c.name, c.id LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        let mut row = vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::opt_text(r.get(1)?),
        ];
        row.extend(super::ints::<6>(r, 2)?.map(Cell::int));
        Ok(row)
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("customer", K::Text),
        ("card", K::Code),
        ("earned", K::Int),
        ("redeemed", K::Int),
        ("expired", K::Int),
        ("reversed", K::Int),
        ("adjusted", K::Int),
        ("balance", K::Int),
    ]);
    for row in found {
        table.push(row)?;
    }
    let sum = |i: usize| {
        Cell::int(
            table
                .rows
                .iter()
                .filter_map(|r| r[i].as_i64())
                .fold(0_i64, i64::saturating_add),
        )
    };
    let mut totals = vec![Cell::NULL, Cell::NULL];
    totals.extend((2..8).map(sum));
    table.set_totals(totals)?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// LOY-001, PAY-010: the vouchers issued by `to` and still alive at `from` (issued in the period
/// or with money left before it), what was spent of them in the period, their balance and status
/// at the end of `to`. The balance total is what the shop still owes to voucher holders.
pub(super) fn gift_vouchers(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT code, kind, issued_on, expires_on, initial_amount, spent, balance,
                      CASE WHEN expired THEN 'expired' WHEN balance > 0 THEN 'active' ELSE 'spent' END
               FROM (SELECT v.code, v.kind, v.issued_on, v.expires_on, v.initial_amount, v.created_at,
                            (SELECT COALESCE(-SUM(l.amount), 0) FROM gift_voucher_ledger l
                             WHERE l.voucher_id = v.id AND l.kind = 'redeem'
                               AND l.business_date BETWEEN :day_from AND :day_to) AS spent,
                            (SELECT COALESCE(SUM(l.amount), 0) FROM gift_voucher_ledger l
                             WHERE l.voucher_id = v.id AND l.business_date <= :day_to) AS balance,
                            (SELECT COALESCE(SUM(l.amount), 0) FROM gift_voucher_ledger l
                             WHERE l.voucher_id = v.id AND l.business_date < :day_from) AS before,
                            EXISTS (SELECT 1 FROM gift_voucher_ledger l WHERE l.voucher_id = v.id
                                    AND l.kind = 'expire' AND l.business_date <= :day_to) AS expired
                     FROM gift_vouchers v
                     WHERE v.issued_on <= :day_to)
               WHERE issued_on >= :day_from OR before > 0
               ORDER BY issued_on, created_at, code LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        Ok(vec![
            Cell::text(r.get::<_, String>(0)?),
            Cell::text(r.get::<_, String>(1)?),
            Cell::text(r.get::<_, String>(2)?),
            Cell::text(r.get::<_, String>(3)?),
            Cell::int(r.get(4)?),
            Cell::int(r.get(5)?),
            Cell::int(r.get(6)?),
            Cell::text(r.get::<_, String>(7)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("code", K::Code),
        ("voucherKind", K::Tag),
        ("issuedOn", K::Date),
        ("expiresOn", K::Date),
        ("initialAmount", K::Money),
        ("spent", K::Money),
        ("balance", K::Money),
        ("voucherStatus", K::Tag),
    ])
    .tag("voucherKind", TagDomain::VoucherKind)
    .tag("voucherStatus", TagDomain::VoucherStatus);
    for row in found {
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL; 4];
    totals.extend(
        ["initialAmount", "spent", "balance"]
            .iter()
            .map(|k| Cell::money(table.money_total(k))),
    );
    totals.push(Cell::NULL);
    table.set_totals(totals)?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// VRT-141: the jeweller register of the year of `from` (CII art. 359, LF 2026 art. 62), every
/// entry in order with the buyer's identity. M3's table print gives the paged register.
pub(super) fn jeweller_register(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT r.seq, r.business_date, COALESCE(d.number, ''), r.nature, r.pieces,
                      r.weight_mg, r.titre_permille, r.price, r.buyer_name, r.buyer_id_kind,
                      r.buyer_id_number, r.buyer_address
               FROM jeweller_register r JOIN documents d ON d.id = r.document_id
               WHERE r.year = CAST(substr(:day_from, 1, 4) AS INTEGER)
               ORDER BY r.seq LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        Ok(vec![
            Cell::int(r.get(0)?),
            Cell::text(r.get::<_, String>(1)?),
            Cell::text(r.get::<_, String>(2)?),
            Cell::text(r.get::<_, String>(3)?),
            Cell::int(r.get(4)?),
            Cell::qty(Qty(r.get(5)?)),
            Cell::int(r.get(6)?),
            Cell::int(r.get(7)?),
            Cell::text(r.get::<_, String>(8)?),
            Cell::text(r.get::<_, String>(9)?),
            Cell::text(r.get::<_, String>(10)?),
            Cell::text(r.get::<_, String>(11)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("seq", K::Int),
        ("date", K::Date),
        ("ticket", K::Code),
        ("nature", K::Text),
        ("pieces", K::Int),
        ("weightG", K::Qty),
        ("titre", K::Int),
        ("price", K::Money),
        ("buyer", K::Text),
        ("idKind", K::Tag),
        ("idNumber", K::Code),
        ("address", K::Text),
    ])
    .tag("idKind", TagDomain::IdKind);
    for row in found {
        table.push(row)?;
    }
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// RPT-071: the pieces sold over the period per metal and fineness (the register holds every
/// piece sold), their grams, turnover and making charges, and the average price per gram of
/// metal.
pub(super) fn jewellery_sales(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT j.metal, r.titre_permille, SUM(r.pieces), SUM(r.weight_mg), SUM(r.price),
                      SUM(j.facon * r.pieces)
               FROM jeweller_register r JOIN jewellery_pieces j ON j.product_id = r.product_id
               WHERE r.business_date BETWEEN :day_from AND :day_to
               GROUP BY j.metal, r.titre_permille
               ORDER BY j.metal, r.titre_permille DESC LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        Ok((r.get::<_, String>(0)?, super::ints::<5>(r, 1)?))
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("metal", K::Tag),
        ("titre", K::Int),
        ("pieces", K::Int),
        ("weightG", K::Qty),
        ("totalTtc", K::Money),
        ("facon", K::Money),
        ("perGram", K::Money),
    ])
    .tag("metal", TagDomain::Metal);
    let mut sums = [0_i64; 4];
    for (metal, [titre, pieces, weight, total, facon]) in found {
        let per_gram = Money(total.saturating_sub(facon)).mul_div(1_000, weight);
        super::add(&mut sums, &[pieces, weight, total, facon]);
        table.push(vec![
            Cell::text(metal),
            Cell::int(titre),
            Cell::int(pieces),
            Cell::qty(Qty(weight)),
            Cell::int(total),
            Cell::int(facon),
            per_gram.map_or(Cell::NULL, Cell::money),
        ])?;
    }
    let [pieces, weight, total, facon] = sums;
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::int(pieces),
        Cell::qty(Qty(weight)),
        Cell::int(total),
        Cell::int(facon),
        Cell::NULL,
    ])?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// RPT-062 lite: per user who validated tickets or returns in the period, the counts, the net
/// sales HT (after discounts, returns netted) and TTC, the commission rate of their staff profile
/// (0 without one) and the commission on the net HT.
pub(super) fn staff_commissions(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT COALESCE(MAX(u.display_name), d.user_id),
                SUM({SALE_KINDS}), SUM({RETURN_KINDS}), SUM(d.subtotal_ht), SUM(d.total_ttc),
                COALESCE(MAX(p.commission_bp), 0)
         FROM documents d
         LEFT JOIN users u ON u.id = d.user_id
         LEFT JOIN staff_profiles p ON p.user_id = d.user_id
         WHERE {SALES_DOCS}
         GROUP BY d.user_id
         ORDER BY SUM(d.subtotal_ht) DESC, 1
         LIMIT :row_limit + 1"
    );
    let found = rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, super::ints::<5>(r, 1)?))
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("user", K::Text),
        ("tickets", K::Int),
        ("returns", K::Int),
        ("netHt", K::Money),
        ("netTtc", K::Money),
        ("commissionBp", K::Percent),
        ("commission", K::Money),
    ]);
    let mut sums = [0_i64; 5];
    for (name, [tickets, returns, net_ht, net_ttc, bp]) in found {
        let rate = i32::try_from(bp).map_err(|_| AppError::Internal("commission rate".into()))?;
        let commission = Money(net_ht)
            .apply_rate(Rate(rate))
            .ok_or_else(|| AppError::Internal("commission overflow".into()))?;
        super::add(
            &mut sums,
            &[tickets, returns, net_ht, net_ttc, commission.0],
        );
        table.push(vec![
            Cell::text(name),
            Cell::int(tickets),
            Cell::int(returns),
            Cell::money(Money(net_ht)),
            Cell::money(Money(net_ttc)),
            Cell::int(bp),
            Cell::money(commission),
        ])?;
    }
    let [tickets, returns, net_ht, net_ttc, commission] = sums;
    table.set_totals(vec![
        Cell::NULL,
        Cell::int(tickets),
        Cell::int(returns),
        Cell::int(net_ht),
        Cell::int(net_ttc),
        Cell::NULL,
        Cell::int(commission),
    ])?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// EMP-003 lite: per employee with advances by `to`, what was advanced, repaid and kept off
/// salaries in the period (positive amounts) and what they still owe at the end of `to`.
pub(super) fn staff_advances(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT u.display_name,
                      SUM(CASE WHEN l.kind = 'advance' AND l.business_date >= :day_from THEN l.amount ELSE 0 END),
                      -SUM(CASE WHEN l.kind = 'repayment' AND l.business_date >= :day_from THEN l.amount ELSE 0 END),
                      -SUM(CASE WHEN l.kind = 'salary_deduction' AND l.business_date >= :day_from THEN l.amount ELSE 0 END),
                      SUM(l.amount)
               FROM staff_ledger l JOIN users u ON u.id = l.user_id
               WHERE l.business_date <= :day_to AND (:user_id IS NULL OR l.user_id = :user_id)
               GROUP BY l.user_id
               ORDER BY u.display_name COLLATE NOCASE, l.user_id
               LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        let mut row = vec![Cell::text(r.get::<_, String>(0)?)];
        row.extend(super::ints::<4>(r, 1)?.map(Cell::int));
        Ok(row)
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("user", K::Text),
        ("advanced", K::Money),
        ("repaid", K::Money),
        ("deducted", K::Money),
        ("owed", K::Money),
    ]);
    for row in found {
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL];
    totals.extend(
        ["advanced", "repaid", "deducted", "owed"]
            .iter()
            .map(|k| Cell::money(table.money_total(k))),
    );
    table.set_totals(totals)?;
    Ok(Built {
        truncated,
        ..Built::plain(table)
    })
}

/// PAY-006: the payments of the sales of the scope per base method and custom payment type —
/// the plain method first, then each type riding it — with the tenders (positive payments) and
/// the net amount; the change given back is netted into cash, like `sales_by_method`.
pub(super) fn payment_types(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let name = super::name_of("t", s.lang);
    let sql = format!(
        "SELECT CASE pm.method WHEN 'change' THEN 'cash' ELSE pm.method END AS tender,
                t.id IS NOT NULL, COALESCE(MAX(t.sort_order), 0), COALESCE(MAX({name}), ''),
                SUM(CASE WHEN pm.amount > 0 THEN 1 ELSE 0 END), SUM(pm.amount)
         FROM payments pm JOIN documents d ON d.id = pm.document_id
         LEFT JOIN payment_type_uses u ON u.payment_id = pm.id
         LEFT JOIN payment_types t ON t.id = u.payment_type_id
         WHERE {SALES_DOCS}
         GROUP BY tender, t.id"
    );
    let mut found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, bool>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
            super::ints::<2>(r, 4)?,
        ))
    })?;
    let place = |method: &str| {
        PaymentMethod::ALL
            .iter()
            .position(|m| m.as_str() == method)
            .unwrap_or(usize::MAX)
    };
    found.sort_by(|a, b| (place(&a.0), a.1, a.2, &a.3).cmp(&(place(&b.0), b.1, b.2, &b.3)));
    let mut table = Table::new(&[
        ("method", K::Tag),
        ("paymentType", K::Text),
        ("tenders", K::Int),
        ("amount", K::Money),
    ])
    .tag("method", TagDomain::PaymentMethod);
    let mut sums = [0_i64; 2];
    for (method, _, _, type_name, v) in found {
        super::add(&mut sums, &v);
        table.push(vec![
            Cell::text(method),
            Cell::text(type_name),
            Cell::int(v[0]),
            Cell::money(Money(v[1])),
        ])?;
    }
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::int(sums[0]),
        Cell::money(Money(sums[1])),
    ])?;
    Ok(Built::plain(table))
}

/// RPT-020 (PDP-003): the journal's personal-data rows of the period, oldest first — when, who,
/// the customer's code (the name never enters the report), what (`privacy.accessed`,
/// `exported`, `anonymised`, `consent_set`) and why (the purpose, or the consent's channel).
pub(super) fn personal_data_access(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let on = super::day_of("a.at");
    let sql = format!(
        "SELECT a.at, COALESCE(u.display_name, a.user_id), c.code, a.action,
                COALESCE(json_extract(a.payload, '$.purpose'), json_extract(a.payload, '$.channel'))
         FROM audit_log a
         LEFT JOIN users u ON u.id = a.user_id
         LEFT JOIN customers c ON c.id = a.entity_id
         WHERE a.action IN ('privacy.accessed', 'privacy.exported', 'privacy.anonymised', 'privacy.consent_set')
           AND a.at >= unixepoch(:day_from) * 1000 - {offset}
           AND a.at < unixepoch(:day_to, '+1 day') * 1000 - {offset}
           AND {on} BETWEEN :day_from AND :day_to
           AND (:user_id IS NULL OR a.user_id = :user_id)
           AND (:customer_id IS NULL OR a.entity_id = :customer_id)
         ORDER BY a.seq
         LIMIT :row_limit + 1",
        offset = crate::time::ALGIERS_OFFSET_MS,
    );
    let found = rows(conn, &sql, s, |r| {
        Ok(vec![
            Cell::int(r.get(0)?),
            Cell::opt_text(r.get(1)?),
            Cell::opt_text(r.get(2)?),
            Cell::text(r.get::<_, String>(3)?),
            Cell::opt_text(r.get(4)?),
        ])
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("at", K::DateTime),
        ("user", K::Text),
        ("customerCode", K::Code),
        ("action", K::Tag),
        ("purpose", K::Tag),
    ])
    .tag("action", TagDomain::AuditAction)
    .tag("purpose", TagDomain::PrivacyPurpose);
    for row in found {
        table.push(row)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use crate::money::Money;
    use crate::payments::PaymentInput;
    use crate::reports::{ReportKind, ReportRequest, run};
    use crate::restaurant::bill::{self, PayInput};
    use crate::restaurant::tests::{add, dine_in, restaurant};
    use crate::table::Cell;

    #[test]
    fn the_voucher_report_shows_the_outstanding_liability() {
        let shop = crate::loyalty_tests::club_for_reports();
        let a = crate::loyalty_tests::gift_for_reports(&shop, 500_000);
        crate::loyalty_tests::gift_for_reports(&shop, 200_000);
        let item = shop.product("Parfum", Money(300_000), &shop.tva19).id;
        crate::loyalty_tests::spend_for_reports(&shop, &item, &a.code, 300_000);
        let req = ReportRequest::new(ReportKind::GiftVouchers, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        assert_eq!(report.table.rows.len(), 2);
        let totals = report.table.totals.clone().unwrap();
        assert_eq!(
            (totals[5].as_i64(), totals[6].as_i64()),
            (Some(300_000), Some(400_000)),
            "spent, still owed to holders"
        );
    }

    #[test]
    fn sales_by_table_counts_tickets_covers_and_money_per_table_and_waiter() {
        let (shop, t1, t2) = restaurant();
        let plat = shop.product("Plat", Money(60_000), &shop.tva9).id;
        shop.open_till(100_000);
        for (table, covers) in [(&t1, 2_u8), (&t2, 4), (&t1, 1)] {
            let ticket = dine_in(&shop, table, covers);
            add(&shop, &ticket.id, &plat, 1_000, None, vec![]);
            let ctx = shop.till();
            shop.db
                .write(move |tx| {
                    bill::pay(
                        tx,
                        &ctx,
                        PayInput {
                            ticket_id: ticket.id,
                            // 600 DA TTC + 6 DA droit de timbre on the cash.
                            payments: vec![PaymentInput::cash(Money(60_600))],
                        },
                    )
                })
                .unwrap();
        }
        let req = ReportRequest::new(ReportKind::SalesByTable, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        let t1_row = report
            .table
            .rows
            .iter()
            .find(|r| r[1] == Cell::text("T1"))
            .unwrap();
        assert_eq!(
            &t1_row[3..6],
            &[Cell::int(2), Cell::int(3), Cell::money(Money(120_000))]
        );
        assert_eq!(t1_row[6], Cell::money(Money(40_000)), "TTC per cover");
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| crate::packs::set_modules(tx, &actor, &[]))
            .unwrap();
        let req = ReportRequest::new(ReportKind::SalesByTable, "2026-09-28", "2026-09-28");
        assert_eq!(
            shop.db.read(move |c| run(c, &req)).unwrap_err().code(),
            "module_disabled"
        );
    }

    #[test]
    #[allow(
        clippy::needless_update,
        reason = "`SaleExtras` gains a field per pack (serials, buyer, payment types, location)"
    )]
    fn the_controlled_register_lists_the_entries_of_its_period() {
        use crate::money::Qty;
        use crate::pharmacy::tests::{medicine, pharmacy, prescription, sell};
        use crate::sales::{CartLine, SaleExtras};
        let shop = pharmacy();
        let lexo = medicine(&shop, "Lexomil 6 mg", 45_000, "Bromazépam", true);
        shop.open_till(100_000);
        let extras = SaleExtras {
            prescriptions: vec![prescription(0)],
            ..SaleExtras::default()
        };
        let sale = sell(
            &shop,
            shop.owner_till(),
            vec![CartLine::product(&lexo, Qty(2_000))],
            extras,
        )
        .unwrap();
        assert_eq!(
            ReportKind::ControlledRegister.permission(),
            crate::auth::Permission::PharmacyRegister
        );
        let req = ReportRequest::new(ReportKind::ControlledRegister, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        assert_eq!(report.table.rows.len(), 1);
        let row = &report.table.rows[0];
        assert_eq!(
            (&row[0], &row[2], &row[4]),
            (
                &Cell::text("2026-000001"),
                &Cell::opt_text(sale.document.number.clone()),
                &Cell::int(2_000)
            )
        );
        assert_eq!(
            (&row[5], &row[6], &row[8]),
            (
                &Cell::text("Dr Meriem Haddad (CNOM 16-0452)"),
                &Cell::text("ORD-7781"),
                &Cell::text("Rachid B.")
            )
        );
        let req = ReportRequest::new(ReportKind::ControlledRegister, "2026-09-29", "2026-09-30");
        assert!(
            shop.db
                .read(move |c| run(c, &req))
                .unwrap()
                .table
                .rows
                .is_empty()
        );
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| crate::packs::set_modules(tx, &actor, &[]))
            .unwrap();
        let req = ReportRequest::new(ReportKind::ControlledRegister, "2026-09-28", "2026-09-28");
        assert_eq!(
            shop.db.read(move |c| run(c, &req)).unwrap_err().code(),
            "module_disabled"
        );
    }

    #[test]
    fn waste_and_yield_reports_read_their_movements_and_runs() {
        use crate::butcher::waste::{self, WasteInput, WasteLine, WasteReason};
        use crate::money::Qty;
        let shop = crate::testkit::shop();
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| crate::packs::set_modules(tx, &actor, &[crate::packs::Module::Waste]))
            .unwrap();
        let mut p = crate::catalog::products::ProductInput::simple(
            "Salade",
            crate::catalog::UNIT_PIECE_ID,
            &shop.exempt,
            Money(6_000),
        );
        p.cost_price = Money(3_000);
        let salade = shop.product_with(p).id;
        shop.stock_in(&salade, 10_000);
        let ctx = shop.approved_till(crate::auth::Permission::StockAdjust);
        let input = WasteInput {
            location_id: None,
            lines: vec![WasteLine {
                product_id: salade,
                qty: Qty(4_000),
                reason: WasteReason::Damage,
            }],
            note: None,
        };
        shop.db
            .write(move |tx| waste::record(tx, &ctx, input))
            .unwrap();
        let req = ReportRequest::new(ReportKind::Waste, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        assert_eq!(report.table.rows.len(), 1);
        assert_eq!(
            report.table.totals.as_ref().unwrap()[5],
            Cell::money(Money(12_000))
        );
        assert_eq!(
            ReportKind::Waste.permission(),
            crate::auth::Permission::ReportsStock
        );
        let req = ReportRequest::new(ReportKind::YieldRuns, "2026-09-28", "2026-09-28");
        assert_eq!(
            shop.db.read(move |c| run(c, &req)).unwrap_err().code(),
            "module_disabled"
        );
    }

    #[test]
    fn the_rounds_report_lists_stops_and_collections() {
        use crate::wholesale::rounds::{self, RoundInput, StopReport, StopStatus};
        let shop = crate::wholesale::tests::wholesaler_for_reports();
        let flour = shop.product("Farine 25 kg", Money(250_000), &shop.tva9).id;
        shop.stock_in(&flour, 10_000);
        let a = shop.customer("Épicerie Amine").id;
        let doc = crate::wholesale::tests::bl(&shop, &a, &flour, 2_000);
        let ctx = shop.owner_till();
        let round = shop
            .db
            .write(move |tx| {
                rounds::create(
                    tx,
                    &ctx,
                    RoundInput {
                        driver_name: "Rachid".into(),
                        ..RoundInput::default()
                    },
                )
            })
            .unwrap();
        let (ctx, id) = (shop.owner_till(), round.id.clone());
        shop.db
            .write(move |tx| {
                rounds::set_stops(tx, &ctx, &id, vec![doc.id])?;
                rounds::start(tx, &ctx, &id)?;
                rounds::report_stop(
                    tx,
                    &ctx,
                    &id,
                    StopReport {
                        seq: 1,
                        status: StopStatus::Delivered,
                        collected: Money(500_000),
                        method: Some(crate::payments::PaymentMethod::Cash),
                        reference: None,
                        cheque_bank: None,
                        cheque_date: None,
                        note: None,
                    },
                )?;
                rounds::close(tx, &ctx, &id)
            })
            .unwrap();
        let req = ReportRequest::new(ReportKind::DeliveryRounds, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        assert_eq!(report.table.rows[0][0], Cell::text("TR2026-000001"));
        assert_eq!(
            &report.table.rows[0][4..9],
            &[
                Cell::int(1),
                Cell::int(1),
                Cell::int(0),
                Cell::money(Money(500_000)),
                Cell::money(Money(500_000))
            ]
        );
    }

    #[test]
    fn jewellery_sales_add_grams_and_turnover_per_fineness() {
        let (shop, ring) = crate::jewellery_tests::bijouterie_for_reports();
        crate::jewellery_tests::sell_ring_for_reports(&shop, &ring);
        crate::jewellery_tests::sell_ring_for_reports(&shop, &ring);
        let req = ReportRequest::new(ReportKind::JewellerySales, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        let row = &report.table.rows[0];
        assert_eq!(
            &row[1..6],
            &[
                Cell::int(750),
                Cell::int(2),
                Cell::int(10_400),
                Cell::money(Money(15_680_000)),
                Cell::money(Money(600_000))
            ]
        );
        let req = ReportRequest::new(ReportKind::JewellerRegister, "2026-09-28", "2026-09-28");
        // the buyers' identity numbers are a manager's read, never a cashier's
        assert_eq!(
            ReportKind::JewellerRegister.permission(),
            crate::auth::Permission::CustomersManage
        );
        assert!(
            !crate::auth::roles::cashier_permissions()
                .contains(&ReportKind::JewellerRegister.permission())
        );
        assert_eq!(
            shop.db
                .read(move |c| run(c, &req))
                .unwrap()
                .table
                .rows
                .len(),
            2
        );
    }

    #[test]
    fn commissions_follow_each_cashiers_net_sales() {
        let shop = crate::staff_tests::office_for_reports();
        let actor = shop.owner.actor();
        let cashier = shop.cashier.user_id.clone();
        shop.db
            .write(move |tx| {
                crate::staff::set_profile(
                    tx,
                    &actor,
                    &cashier,
                    crate::staff::ProfileInput {
                        phone: None,
                        hire_date: None,
                        monthly_salary: None,
                        commission_bp: 200,
                        note: None,
                    },
                )
            })
            .unwrap();
        let shirt = shop.product("Chemise", Money(1_190_000), &shop.tva19).id;
        crate::staff_tests::sell_for_reports(&shop, &shirt, 2_000);
        let req = ReportRequest::new(ReportKind::StaffCommissions, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        let row = report
            .table
            .rows
            .iter()
            .find(|r| r[0] == Cell::text("Yacine"))
            .unwrap();
        assert_eq!(
            &row[1..7],
            &[
                Cell::int(1),
                Cell::int(0),
                Cell::money(Money(2_000_000)),
                Cell::money(Money(2_380_000)),
                Cell::int(200),
                Cell::money(Money(40_000))
            ]
        );
    }

    #[test]
    fn the_advances_report_shows_what_each_employee_still_owes() {
        let shop = crate::staff_tests::office_for_reports();
        let (ctx, cashier) = (shop.owner_till(), shop.cashier.user_id.clone());
        shop.db
            .write(move |tx| {
                crate::staff::advance(
                    tx,
                    &ctx,
                    crate::staff::AdvanceInput {
                        user_id: cashier,
                        amount: Money(500_000),
                        method: crate::payments::PaymentMethod::Cash,
                        from_till: true,
                        note: None,
                    },
                )
            })
            .unwrap();
        let (ctx, cashier) = (shop.owner_till(), shop.cashier.user_id.clone());
        shop.db
            .write(move |tx| {
                crate::staff::deduct(tx, &ctx, &cashier, Money(200_000), "2026-09", None)
            })
            .unwrap();
        let req = ReportRequest::new(ReportKind::StaffAdvances, "2026-09-28", "2026-09-28");
        let report = shop.db.read(move |c| run(c, &req)).unwrap();
        assert_eq!(
            report.table.rows,
            vec![vec![
                Cell::text("Yacine"),
                Cell::money(Money(500_000)),
                Cell::money(Money::ZERO),
                Cell::money(Money(200_000)),
                Cell::money(Money(300_000))
            ]]
        );
    }
}
