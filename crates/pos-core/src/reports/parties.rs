//! Purchases, payables, receivables and the kridi activity. No contact field (phone, e-mail,
//! address) ever appears here: contact data leaves only through the customer and supplier file
//! exports, under their `*.manage` permission and journaled.
//!
//! A balance is always the one at the end of the `to` day: the cached balance (the sum of the
//! whole ledger, kept by the triggers and rebuilt by `ledger::rebuild_balances`) minus the entries
//! dated after `to`, so a past day reads the ledger as it stood then and today costs nothing more.

use super::{Built, ChartKind, SALE_KINDS, Scope, add, ints, name_of, rows};
use crate::error::AppError;
use crate::ledger::{self, Side};
use crate::table::{Cell, ColumnKind as K, Table};
use rusqlite::Connection;

/// What each customer's ledger moved after the `:day_to` business day (alias it `later`).
const CUSTOMERS_LATER: &str = "(SELECT customer_id, SUM(amount) AS amount FROM customer_ledger
     WHERE business_date > :day_to GROUP BY customer_id)";

/// What each supplier's ledger moved after the `:day_to` business day (alias it `later`).
const SUPPLIERS_LATER: &str = "(SELECT supplier_id, SUM(amount) AS amount FROM supplier_ledger
     WHERE business_date > :day_to GROUP BY supplier_id)";

/// Per supplier over the range (RPT-030): receipts, purchases booked, returns, payments, and the
/// balance at the end of `to`. Purchases are the supplier ledger's `purchase` entries (invoices,
/// or receipts invoiced at once), so a receipt and its invoice are never counted twice.
pub(super) fn purchases_by_supplier(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT su.code, su.name,
                (SELECT COUNT(*) FROM documents d WHERE d.supplier_id = su.id AND d.status = 'validated'
                   AND d.doc_type = 'goods_receipt' AND d.business_date BETWEEN :day_from AND :day_to) AS receipts,
                COALESCE(SUM(CASE WHEN e.kind = 'purchase' THEN e.amount END), 0),
                COALESCE(-SUM(CASE WHEN e.kind = 'return' THEN e.amount END), 0),
                COALESCE(-SUM(CASE WHEN e.kind = 'payment' THEN e.amount END), 0),
                su.balance - COALESCE(MAX(later.amount), 0) AS balance_then
         FROM suppliers su
         LEFT JOIN {SUPPLIERS_LATER} later ON later.supplier_id = su.id
         LEFT JOIN supplier_ledger e ON e.supplier_id = su.id AND e.business_date BETWEEN :day_from AND :day_to
         WHERE (:supplier_id IS NULL OR su.id = :supplier_id)
         GROUP BY su.id
         HAVING COUNT(e.id) > 0 OR receipts > 0 OR balance_then <> 0
         ORDER BY 4 DESC, 2"
    );
    let mut table = Table::new(&[
        ("code", K::Code),
        ("name", K::Text),
        ("receipts", K::Int),
        ("purchases", K::Money),
        ("returns", K::Money),
        ("payments", K::Money),
        ("balance", K::Money),
    ]);
    let mut sum = [0_i64; 5];
    for (code, name, v) in rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            ints::<5>(r, 2)?,
        ))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::text(code), Cell::text(name)];
        row.extend(v.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL, Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::charted(table, ChartKind::HBar, "name", "purchases"))
}

/// Goods received per product (RPT-031): quantity, value HT at cost, average unit cost.
pub(super) fn purchases_by_product(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let name = name_of("p", s.lang);
    let sql = format!(
        "SELECT MAX(p.reference), COALESCE(MAX({name}), MAX(l.description)), SUM(l.qty), SUM(l.total_ht),
                COUNT(DISTINCT d.supplier_id), MAX(d.business_date)
         FROM document_lines l JOIN documents d ON d.id = l.document_id
         LEFT JOIN products p ON p.id = l.product_id
         WHERE d.status = 'validated' AND d.doc_type = 'goods_receipt'
           AND d.business_date BETWEEN :day_from AND :day_to
           AND (:supplier_id IS NULL OR d.supplier_id = :supplier_id)
           AND (:product_id IS NULL OR l.product_id = :product_id)
           AND (:category_id IS NULL OR p.category_id = :category_id)
         GROUP BY COALESCE(l.product_id, l.description)
         ORDER BY SUM(l.total_ht) DESC, 2"
    );
    let mut table = Table::new(&[
        ("reference", K::Code),
        ("name", K::Text),
        ("qty", K::Qty),
        ("purchasedHt", K::Money),
        ("averageCost", K::Money),
        ("suppliers", K::Int),
        ("lastReceipt", K::Date),
    ]);
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
            ints::<3>(r, 2)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut total = 0_i64;
    for (reference, name, [qty, value, suppliers], last) in found {
        total = total.saturating_add(value);
        // Average unit cost = value × 1000 / milli-quantity, rounded half up.
        let average = if qty > 0 {
            i64::try_from((i128::from(value) * 1_000 * 2 + i128::from(qty)) / (2 * i128::from(qty)))
                .unwrap_or(i64::MAX)
        } else {
            0
        };
        table.push(vec![
            Cell::opt_text(reference),
            Cell::opt_text(name),
            Cell::int(qty),
            Cell::int(value),
            Cell::int(average),
            Cell::int(suppliers),
            Cell::opt_text(last),
        ])?;
    }
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::int(total),
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
    ])?;
    Ok(Built::plain(table))
}

/// Ageing on the `to` day (CRD-007 receivables, PUR-011 payables) from Task 2's ledger.
pub(super) fn aging(conn: &Connection, s: &Scope<'_>, side: Side) -> Result<Built, AppError> {
    let wanted = match side {
        Side::Customer => s.req.customer_id.as_deref(),
        Side::Supplier => s.req.supplier_id.as_deref(),
    };
    let mut table = Table::new(&[
        ("code", K::Code),
        ("name", K::Text),
        ("balance", K::Money),
        ("days0To30", K::Money),
        ("days31To60", K::Money),
        ("days61To90", K::Money),
        ("over90", K::Money),
        ("advance", K::Money),
        ("oldestOpen", K::Date),
    ]);
    let mut sum = [0_i64; 6];
    // `AgingRow` carries a phone: it is not copied into the table.
    for row in ledger::aging(conn, side, &s.to)? {
        if wanted.is_some_and(|id| id != row.party_id) {
            continue;
        }
        let v = [
            row.balance.0,
            row.current.0,
            row.days_31_60.0,
            row.days_61_90.0,
            row.over_90.0,
            row.advance.0,
        ];
        add(&mut sum, &v);
        let mut cells = vec![Cell::text(row.code), Cell::text(row.name)];
        cells.extend(v.map(Cell::int));
        cells.push(Cell::opt_text(row.oldest_open));
        table.push(cells)?;
    }
    let mut totals = vec![Cell::NULL, Cell::NULL];
    totals.extend(sum.map(Cell::int));
    totals.push(Cell::NULL);
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}

/// Customers who owed or were owed at the end of `to` (CRD-006), largest debt first, with their
/// last payment and purchase up to that day. The credit limit is today's.
pub(super) fn customer_balances(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT cu.code, cu.name, cu.balance - COALESCE(later.amount, 0), cu.credit_limit,
                (SELECT MAX(e.business_date) FROM customer_ledger e
                  WHERE e.customer_id = cu.id AND e.kind = 'payment' AND e.business_date <= :day_to),
                (SELECT MAX(d.business_date) FROM documents d
                  WHERE d.customer_id = cu.id AND d.status = 'validated' AND {SALE_KINDS}
                    AND d.business_date <= :day_to)
         FROM customers cu
         LEFT JOIN {CUSTOMERS_LATER} later ON later.customer_id = cu.id
         WHERE cu.balance - COALESCE(later.amount, 0) <> 0
           AND (:customer_id IS NULL OR cu.id = :customer_id)
         ORDER BY 3 DESC, cu.name"
    );
    let mut table = Table::new(&[
        ("code", K::Code),
        ("name", K::Text),
        ("balance", K::Money),
        ("creditLimit", K::Money),
        ("available", K::Money),
        ("lastPayment", K::Date),
        ("lastPurchase", K::Date),
    ]);
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<i64>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut total = 0_i64;
    for (code, name, balance, limit, paid, bought) in found {
        total = total.saturating_add(balance);
        table.push(vec![
            Cell::text(code),
            Cell::text(name),
            Cell::int(balance),
            limit.map_or(Cell::NULL, Cell::int),
            limit.map_or(Cell::NULL, |l| Cell::int(l.saturating_sub(balance))),
            Cell::opt_text(paid),
            Cell::opt_text(bought),
        ])?;
    }
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::int(total),
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
    ])?;
    Ok(Built::plain(table))
}

/// The kridi notebook over the range, per customer (CRD-008), with the balance at the end of `to`.
pub(super) fn credit_activity(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT cu.code, cu.name,
                SUM(CASE WHEN e.kind = 'sale_credit' THEN e.amount ELSE 0 END),
                -SUM(CASE WHEN e.kind = 'payment' THEN e.amount ELSE 0 END),
                SUM(CASE WHEN e.kind = 'refund' THEN e.amount ELSE 0 END),
                SUM(CASE WHEN e.kind = 'voucher_redeemed' THEN e.amount ELSE 0 END),
                SUM(CASE WHEN e.kind IN ('adjustment', 'opening') THEN e.amount ELSE 0 END),
                SUM(e.stamp_charged),
                MAX(cu.balance - COALESCE(later.amount, 0))
         FROM customer_ledger e JOIN customers cu ON cu.id = e.customer_id
         LEFT JOIN {CUSTOMERS_LATER} later ON later.customer_id = e.customer_id
         WHERE e.business_date BETWEEN :day_from AND :day_to
           AND (:customer_id IS NULL OR e.customer_id = :customer_id)
         GROUP BY e.customer_id
         ORDER BY 3 DESC, 2"
    );
    let mut table = Table::new(&[
        ("code", K::Code),
        ("name", K::Text),
        ("soldOnCredit", K::Money),
        ("collected", K::Money),
        ("refunds", K::Money),
        ("storeCreditUsed", K::Money),
        ("adjustments", K::Money),
        ("stampDuty", K::Money),
        ("balance", K::Money),
    ]);
    let mut sum = [0_i64; 7];
    for (code, name, v) in rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            ints::<7>(r, 2)?,
        ))
    })? {
        add(&mut sum, &v);
        let mut row = vec![Cell::text(code), Cell::text(name)];
        row.extend(v.map(Cell::int));
        table.push(row)?;
    }
    let mut totals = vec![Cell::NULL, Cell::NULL];
    totals.extend(sum.map(Cell::int));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}
