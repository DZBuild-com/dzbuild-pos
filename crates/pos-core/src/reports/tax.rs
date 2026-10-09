//! Tax reports (mvp-backlog RPT-046…053, TAX-009/013): TVA per rate on sales, deductible TVA on
//! purchases with the CTCA art. 30 flag, the monthly timbre statement "sur état", the G50 lines,
//! IFU turnover, the G9 supplier statement, the annual customer statement, the sales and purchase
//! registers. Figures come from the documents and ledgers, never from a cache.

use super::{Built, Grouping, SALES_DOCS, Scope, add, ints, rate_bp, rows};
use crate::datapack::Pack;
use crate::error::AppError;
use crate::money::Money;
use crate::table::{Cell, ColumnKind as K, Table, TagDomain};
use rusqlite::Connection;

/// Output TVA per period and rate (RPT-047): the sales documents of the régime réel (an IFU
/// document never shows TVA), credit notes and returns netted; the exempt rate on its own row.
pub(super) fn vat_by_rate(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let group = s.req.group.unwrap_or(Grouping::Month);
    let period = group.sql("d.business_date");
    let sql = format!(
        "SELECT {period} AS period, t.rate_bp, SUM(t.base_ht), SUM(t.tax_amount), COUNT(DISTINCT d.id)
         FROM document_taxes t JOIN documents d ON d.id = t.document_id
         WHERE {SALES_DOCS} AND d.tax_regime = 'reel'
         GROUP BY period, t.rate_bp ORDER BY period, t.rate_bp DESC"
    );
    let mut table = Table::new(&[
        ("period", group.column_kind()),
        ("rate", K::Percent),
        ("baseHt", K::Money),
        ("tax", K::Money),
        ("totalTtc", K::Money),
        ("documents", K::Int),
    ]);
    let mut total = [0_i64; 3];
    for (period, rate, v) in rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            ints::<3>(r, 2)?,
        ))
    })? {
        let [base, tax, docs] = v;
        let ttc = base.saturating_add(tax);
        add(&mut total, &[base, tax, ttc]);
        table.push(vec![
            Cell::text(period),
            Cell::int(rate),
            Cell::money(Money(base)),
            Cell::money(Money(tax)),
            Cell::money(Money(ttc)),
            Cell::int(docs),
        ])?;
    }
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::money(Money(total[0])),
        Cell::money(Money(total[1])),
        Cell::money(Money(total[2])),
        Cell::NULL,
    ])?;
    Ok(Built::plain(table))
}

/// The cash paid on supplier document `d` (payments allocated to its `purchase` entry).
const CASH_PAID: &str = "COALESCE((SELECT SUM(ABS(a.amount)) FROM supplier_ledger e
       JOIN supplier_allocations a ON a.debit_id = e.id
       JOIN supplier_ledger p ON p.id = a.credit_id
       WHERE e.ref_type = 'document' AND e.ref_id = d.id AND p.method = 'cash'), 0)";

/// A supplier invoice or return of the range with its deductible TVA (TAX-013). A return is
/// stored with positive totals: it is read negative, the TVA it takes back from the deduction.
pub(crate) struct Deductible {
    pub business_date: String,
    pub number: String,
    pub supplier: String,
    pub supplier_ref: Option<String>,
    /// The supplier's identifiers as they stand today (G9).
    pub supplier_ids: SupplierIds,
    pub total_ttc: i64,
    pub tax: i64,
    pub cash_paid: i64,
    pub deductible: i64,
    pub over_limit: bool,
}

pub(crate) struct SupplierIds {
    pub nin: Option<String>,
    pub nif: Option<String>,
    pub address: Option<String>,
    pub rc: Option<String>,
}

/// The deductible rows in date and number order (the TVA statement, the G50).
pub(crate) fn deductible_rows(
    conn: &Connection,
    s: &Scope<'_>,
    pack: &Pack,
) -> Result<Vec<Deductible>, AppError> {
    deductible_rows_by(conn, s, pack, "d.business_date, d.number")
}

/// The deductible rows in the order `order` (a constant fragment over `d` and `sp`).
fn deductible_rows_by(
    conn: &Connection,
    s: &Scope<'_>,
    pack: &Pack,
    order: &str,
) -> Result<Vec<Deductible>, AppError> {
    let sql = format!(
        "SELECT d.business_date, d.number, sp.name, d.source_ref,
                CASE WHEN d.doc_type = 'return_note' THEN -d.total_ttc ELSE d.total_ttc END,
                CASE WHEN d.doc_type = 'return_note' THEN -d.tax_total ELSE d.tax_total END,
                {CASH_PAID}, sp.nin, sp.nif, sp.address, sp.rc
         FROM documents d JOIN suppliers sp ON sp.id = d.supplier_id
         WHERE d.status = 'validated' AND d.business_date BETWEEN :day_from AND :day_to
           AND d.doc_type IN ('supplier_invoice', 'return_note')
           AND (:supplier_id IS NULL OR d.supplier_id = :supplier_id)
         ORDER BY {order}"
    );
    // CTCA art. 30: no deduction on an invoice above the limit (TTC) paid in cash, even in part.
    let limit = pack.fiscal.cash_deduction_limit.0;
    rows(conn, &sql, s, |r| {
        let [total, tax, cash] = ints::<3>(r, 4)?;
        let over_limit = total > limit && cash > 0;
        Ok(Deductible {
            business_date: r.get(0)?,
            number: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            supplier: r.get(2)?,
            supplier_ref: r.get(3)?,
            supplier_ids: SupplierIds {
                nin: r.get(7)?,
                nif: r.get(8)?,
                address: r.get(9)?,
                rc: r.get(10)?,
            },
            total_ttc: total,
            tax,
            cash_paid: cash,
            deductible: if over_limit { 0 } else { tax },
            over_limit,
        })
    })
}

pub(super) fn vat_deductible(
    conn: &Connection,
    s: &Scope<'_>,
    pack: &Pack,
) -> Result<Built, AppError> {
    let mut table = Table::new(&[
        ("businessDate", K::Date),
        ("number", K::Code),
        ("supplierName", K::Text),
        ("supplierRef", K::Code),
        ("totalTtc", K::Money),
        ("taxTotal", K::Money),
        ("cashPaid", K::Money),
        ("deductibleTax", K::Money),
        ("reason", K::Tag),
    ])
    .tag("reason", TagDomain::DeductionReason);
    let mut total = [0_i64; 3];
    for d in deductible_rows(conn, s, pack)? {
        add(&mut total, &[d.total_ttc, d.tax, d.deductible]);
        table.push(vec![
            Cell::text(d.business_date),
            Cell::text(d.number),
            Cell::text(d.supplier),
            Cell::opt_text(d.supplier_ref),
            Cell::money(Money(d.total_ttc)),
            Cell::money(Money(d.tax)),
            Cell::money(Money(d.cash_paid)),
            Cell::money(Money(d.deductible)),
            if d.over_limit {
                Cell::text("cash_over_limit")
            } else {
                Cell::NULL
            },
        ])?;
    }
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::NULL,
        Cell::money(Money(total[0])),
        Cell::money(Money(total[1])),
        Cell::NULL,
        Cell::money(Money(total[2])),
        Cell::NULL,
    ])?;
    Ok(Built::plain(table))
}

/// Duty per business day and terminal: documents (tickets and invoices not made from a ticket),
/// cash versements (M2 ledger), cash deposits (M8 `deposits`, their `AC…` receipts) and gift
/// vouchers sold for cash (M8 `gift_voucher_ledger` issue rows), charged or absorbed — all of it
/// is due "sur état". The cash is what the customers handed over (the duty they were charged
/// included); versements, deposits and gift vouchers share the versements column.
pub(crate) const STAMP_SQL: &str = "
    WITH docs AS (
      SELECT d.business_date AS day, d.terminal_id AS terminal, COUNT(*) AS n,
             SUM((SELECT COALESCE(SUM(p.amount), 0) FROM payments p
                  WHERE p.document_id = d.id AND p.method IN ('cash', 'change'))) AS cash,
             SUM(CASE WHEN d.stamp_mode = 'charge_customer' THEN d.stamp_duty ELSE 0 END) AS charged,
             SUM(CASE WHEN d.stamp_mode = 'absorb' THEN d.stamp_duty ELSE 0 END) AS absorbed
      FROM documents d
      WHERE d.status = 'validated' AND d.stamp_duty > 0 AND d.supplier_id IS NULL
        AND (d.doc_type = 'ticket' OR (d.doc_type = 'invoice' AND d.origin IS NOT 'ticket'))
        AND d.business_date BETWEEN :day_from AND :day_to
        AND (:terminal_id IS NULL OR d.terminal_id = :terminal_id)
      GROUP BY 1, 2),
    vers AS (
      SELECT e.business_date AS day, e.terminal_id AS terminal, COUNT(*) AS n,
             SUM(e.stamp_charged) - SUM(e.amount) AS cash, SUM(e.stamp_duty) AS duty
      FROM customer_ledger e
      WHERE e.stamp_duty > 0 AND e.business_date BETWEEN :day_from AND :day_to
        AND (:terminal_id IS NULL OR e.terminal_id = :terminal_id)
      GROUP BY 1, 2),
    deps AS (
      SELECT x.business_date AS day, x.terminal_id AS terminal, COUNT(*) AS n,
             SUM(x.amount + x.stamp_charged) AS cash, SUM(x.stamp_duty) AS duty
      FROM deposits x
      WHERE x.kind = 'deposit' AND x.stamp_duty > 0
        AND x.business_date BETWEEN :day_from AND :day_to
        AND (:terminal_id IS NULL OR x.terminal_id = :terminal_id)
      GROUP BY 1, 2),
    gifts AS (
      SELECT g.business_date AS day, g.terminal_id AS terminal, COUNT(*) AS n,
             SUM(g.amount + g.stamp_charged) AS cash, SUM(g.stamp_duty) AS duty
      FROM gift_voucher_ledger g
      WHERE g.kind = 'issue' AND g.stamp_duty > 0
        AND g.business_date BETWEEN :day_from AND :day_to
        AND (:terminal_id IS NULL OR g.terminal_id = :terminal_id)
      GROUP BY 1, 2)
    SELECT day, COALESCE((SELECT code FROM terminals WHERE id = terminal), terminal),
           SUM(n), SUM(cash), SUM(charged), SUM(absorbed), SUM(vduty)
    FROM (SELECT day, terminal, n, cash, charged, absorbed, 0 AS vduty FROM docs
          UNION ALL SELECT day, terminal, n, cash, 0, 0, duty FROM vers
          UNION ALL SELECT day, terminal, n, cash, 0, 0, duty FROM deps
          UNION ALL SELECT day, terminal, n, cash, 0, 0, duty FROM gifts)
    GROUP BY day, terminal ORDER BY day, 2";

/// The droit de timbre statement (RPT-046).
pub(super) fn stamp_duty_statement(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let mut table = Table::new(&[
        ("businessDate", K::Date),
        ("terminal", K::Code),
        ("documents", K::Int),
        ("cashBase", K::Money),
        ("dutyCharged", K::Money),
        ("dutyAbsorbed", K::Money),
        ("versementsDuty", K::Money),
        ("dutyTotal", K::Money),
    ]);
    let mut total = [0_i64; 6];
    for (day, terminal, v) in rows(conn, STAMP_SQL, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            ints::<5>(r, 2)?,
        ))
    })? {
        let [n, cash, charged, absorbed, vduty] = v;
        let due = charged.saturating_add(absorbed).saturating_add(vduty);
        add(&mut total, &[n, cash, charged, absorbed, vduty, due]);
        table.push(vec![
            Cell::text(day),
            Cell::opt_text(terminal),
            Cell::int(n),
            Cell::money(Money(cash)),
            Cell::money(Money(charged)),
            Cell::money(Money(absorbed)),
            Cell::money(Money(vduty)),
            Cell::money(Money(due)),
        ])?;
    }
    table.set_totals(vec![
        Cell::NULL,
        Cell::NULL,
        Cell::int(total[0]),
        Cell::money(Money(total[1])),
        Cell::money(Money(total[2])),
        Cell::money(Money(total[3])),
        Cell::money(Money(total[4])),
        Cell::money(Money(total[5])),
    ])?;
    Ok(Built::plain(table))
}

/// The G50 helper as a table (RPT-048): one row per line with its box on the form.
pub(super) fn g50_table(conn: &Connection, s: &Scope<'_>, pack: &Pack) -> Result<Built, AppError> {
    let g = crate::fiscal::g50::g50(conn, pack, &s.from, &s.to)?;
    let mut table = Table::new(&[("line", K::Tag), ("box", K::Code), ("amount", K::Money)])
        .tag("line", TagDomain::G50Line);
    for l in g.lines {
        table.push(vec![
            Cell::text(l.key),
            Cell::text(l.r#box),
            Cell::money(l.amount),
        ])?;
    }
    Ok(Built::plain(table))
}

/// IFU turnover (RPT-050): the TTC of the IFU sales documents per period, cumulated over the
/// range, and the share of the IFU threshold that cumulative reaches.
pub(super) fn ifu_turnover(
    conn: &Connection,
    s: &Scope<'_>,
    pack: &Pack,
) -> Result<Built, AppError> {
    let group = s.req.group.unwrap_or(Grouping::Month);
    let period = group.sql("d.business_date");
    let sql = format!(
        "SELECT {period} AS period, COUNT(*), SUM(d.total_ttc) FROM documents d
         WHERE {SALES_DOCS} AND d.tax_regime = 'ifu' GROUP BY period ORDER BY period"
    );
    let mut table = Table::new(&[
        ("period", group.column_kind()),
        ("documents", K::Int),
        ("turnoverTtc", K::Money),
        ("cumulative", K::Money),
        ("thresholdShare", K::Percent),
    ]);
    let (mut cumulative, mut docs) = (0_i64, 0_i64);
    for (period, v) in rows(conn, &sql, s, |r| {
        Ok((r.get::<_, String>(0)?, ints::<2>(r, 1)?))
    })? {
        let [n, ttc] = v;
        cumulative = cumulative.saturating_add(ttc);
        docs = docs.saturating_add(n);
        table.push(vec![
            Cell::text(period),
            Cell::int(n),
            Cell::money(Money(ttc)),
            Cell::money(Money(cumulative)),
            rate_bp(cumulative, pack.ifu.threshold.0),
        ])?;
    }
    table.set_totals(vec![
        Cell::NULL,
        Cell::int(docs),
        Cell::money(Money(cumulative)),
        Cell::NULL,
        Cell::NULL,
    ])?;
    Ok(Built::plain(table))
}

/// The supplier statement annexed to the G50 (RPT-049, form G n° 9): one row per supplier invoice
/// or return of the range with the supplier's identifiers, the supplier's own number, the TTC and
/// the TVA deducted. The rows are `deductible_rows` (a return negative, 0 deducted on an invoice
/// caught by CTCA art. 30), so the total deducted is the G50's.
pub(super) fn g9(conn: &Connection, s: &Scope<'_>, pack: &Pack) -> Result<Built, AppError> {
    let mut table = Table::new(&[
        ("nin", K::Code),
        ("nif", K::Code),
        ("name", K::Text),
        ("address", K::Text),
        ("rc", K::Code),
        ("invoiceDate", K::Date),
        ("reference", K::Code),
        ("totalTtc", K::Money),
        ("tvaDeducted", K::Money),
    ]);
    let mut total = [0_i64; 2];
    for d in deductible_rows_by(conn, s, pack, "sp.name, sp.id, d.business_date, d.number")? {
        add(&mut total, &[d.total_ttc, d.deductible]);
        let SupplierIds {
            nin,
            nif,
            address,
            rc,
        } = d.supplier_ids;
        table.push(vec![
            Cell::opt_text(nin),
            Cell::opt_text(nif),
            Cell::text(d.supplier),
            Cell::opt_text(address),
            Cell::opt_text(rc),
            Cell::text(d.business_date),
            Cell::opt_text(d.supplier_ref.or(Some(d.number))),
            Cell::money(Money(d.total_ttc)),
            Cell::money(Money(d.deductible)),
        ])?;
    }
    let mut totals = vec![Cell::NULL; 7];
    totals.extend(total.map(|t| Cell::money(Money(t))));
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}

/// The documents of a customer in the annual statement: invoices and credit notes (the invoice
/// of a ticket included) and the tickets and return notes that were not invoiced.
const CUSTOMER_DOCS: &str =
    "d.status = 'validated' AND d.supplier_id IS NULL AND d.customer_id IS NOT NULL
     AND d.business_date BETWEEN :day_from AND :day_to
     AND (:customer_id IS NULL OR d.customer_id = :customer_id)
     AND (d.doc_type IN ('invoice', 'credit_note')
          OR (d.doc_type IN ('ticket', 'return_note')
              AND NOT EXISTS (SELECT 1 FROM document_links k
                              WHERE k.from_id = d.id AND k.kind = 'invoiced_ticket')))";

/// The annual customer statement (RPT-053, CIDTA art. 183 ter): HT, TVA and TTC per customer who
/// bought in the range, with his identifiers and a flag when his NIF is missing.
pub(super) fn customers_annual(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = format!(
        "SELECT c.name, c.nif, c.rc, c.ai, c.address,
                SUM(d.subtotal_ht), SUM(d.tax_total), SUM(d.total_ttc)
         FROM documents d JOIN customers c ON c.id = d.customer_id
         WHERE {CUSTOMER_DOCS}
         GROUP BY c.id HAVING SUM(d.total_ttc) <> 0 ORDER BY c.name, c.id"
    );
    let mut table = Table::new(&[
        ("name", K::Text),
        ("nif", K::Code),
        ("rc", K::Code),
        ("ai", K::Code),
        ("address", K::Text),
        ("totalHt", K::Money),
        ("tax", K::Money),
        ("totalTtc", K::Money),
        ("nifMissing", K::Tag),
    ])
    .tag("nifMissing", TagDomain::YesNo);
    let mut total = [0_i64; 3];
    let found = rows(conn, &sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            [
                r.get::<_, Option<String>>(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
            ],
            ints::<3>(r, 5)?,
        ))
    })?;
    for (name, [nif, rc, ai, address], v) in found {
        add(&mut total, &v);
        let missing = nif.as_deref().is_none_or(|n| n.trim().is_empty());
        table.push(vec![
            Cell::text(name),
            Cell::opt_text(nif),
            Cell::opt_text(rc),
            Cell::opt_text(ai),
            Cell::opt_text(address),
            Cell::money(Money(v[0])),
            Cell::money(Money(v[1])),
            Cell::money(Money(v[2])),
            Cell::text(if missing { "yes" } else { "no" }),
        ])?;
    }
    let mut totals = vec![Cell::NULL; 5];
    totals.extend(total.map(|t| Cell::money(Money(t))));
    totals.push(Cell::NULL);
    table.set_totals(totals)?;
    Ok(Built::plain(table))
}

/// The sales register (RPT-051): every sales document in date and number order with its buyer,
/// HT, TVA, TTC, duty and the cumulative TTC; cut at the limit like the journals (no totals then).
/// The invoice of a till ticket is listed too, so the FA series has no gap, with the ticket it
/// was made from (`countedOn`) and no amount: its ticket already counts them, once.
pub(super) fn sales_register(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    // `SALES_DOCS` with every invoice, the invoice of a ticket included.
    let docs = SALES_DOCS.replace(
        "OR (d.doc_type = 'invoice' AND d.origin IS NOT 'ticket')",
        "OR d.doc_type = 'invoice'",
    );
    let sql = format!(
        "SELECT d.business_date, d.number, d.doc_type, COALESCE(bp.name, c.name),
                d.subtotal_ht, d.tax_total, d.total_ttc, d.stamp_duty, t.number
         FROM documents d
         LEFT JOIN customers c ON c.id = d.customer_id
         LEFT JOIN document_parties bp ON bp.document_id = d.id AND bp.role = 'buyer'
         LEFT JOIN document_links k ON k.to_id = d.id AND k.kind = 'invoiced_ticket'
         LEFT JOIN documents t ON t.id = k.from_id
         WHERE {docs}
         ORDER BY d.business_date, d.validated_at, d.number
         LIMIT :row_limit + 1"
    );
    let found = rows(conn, &sql, s, |r| {
        let counted_on =
            if r.get::<_, String>(2)? == "invoice" && r.get::<_, Option<String>>(8)?.is_some() {
                r.get::<_, Option<String>>(8)?
            } else {
                None
            };
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            // The invoice of a ticket adds nothing: the ticket is in the register.
            if counted_on.is_some() {
                [0; 4]
            } else {
                ints::<4>(r, 4)?
            },
            counted_on,
        ))
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("businessDate", K::Date),
        ("number", K::Code),
        ("docType", K::Tag),
        ("customerName", K::Text),
        ("totalHt", K::Money),
        ("tax", K::Money),
        ("totalTtc", K::Money),
        ("stampDuty", K::Money),
        ("cumulative", K::Money),
        ("countedOn", K::Code),
    ])
    .tag("docType", TagDomain::DocType);
    let mut total = [0_i64; 4];
    for (day, number, kind, customer, v, counted_on) in found {
        add(&mut total, &v);
        table.push(vec![
            Cell::text(day),
            Cell::opt_text(number),
            Cell::text(kind),
            Cell::opt_text(customer),
            Cell::money(Money(v[0])),
            Cell::money(Money(v[1])),
            Cell::money(Money(v[2])),
            Cell::money(Money(v[3])),
            Cell::money(Money(total[2])),
            Cell::opt_text(counted_on),
        ])?;
    }
    if !truncated {
        let mut totals = vec![Cell::NULL; 4];
        totals.extend(total.map(|t| Cell::money(Money(t))));
        totals.push(Cell::NULL);
        totals.push(Cell::NULL);
        table.set_totals(totals)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

/// The purchase register (RPT-052): supplier invoices and returns to suppliers in date and number
/// order with the supplier's own number and the cumulative TTC; a return (stored positive) reads
/// negative. Cut at the limit like the journals (no totals then).
pub(super) fn purchases_register(conn: &Connection, s: &Scope<'_>) -> Result<Built, AppError> {
    let sql = "SELECT d.business_date, d.number, d.doc_type, sp.name, d.source_ref,
                CASE WHEN d.doc_type = 'return_note' THEN -d.subtotal_ht ELSE d.subtotal_ht END,
                CASE WHEN d.doc_type = 'return_note' THEN -d.tax_total ELSE d.tax_total END,
                CASE WHEN d.doc_type = 'return_note' THEN -d.total_ttc ELSE d.total_ttc END
         FROM documents d JOIN suppliers sp ON sp.id = d.supplier_id
         WHERE d.status = 'validated' AND d.business_date BETWEEN :day_from AND :day_to
           AND d.doc_type IN ('supplier_invoice', 'return_note')
           AND (:supplier_id IS NULL OR d.supplier_id = :supplier_id)
         ORDER BY d.business_date, d.validated_at, d.number
         LIMIT :row_limit + 1";
    let found = rows(conn, sql, s, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            ints::<3>(r, 5)?,
        ))
    })?;
    let (found, truncated) = Built::cut(found, s.limit);
    let mut table = Table::new(&[
        ("businessDate", K::Date),
        ("number", K::Code),
        ("docType", K::Tag),
        ("supplierName", K::Text),
        ("supplierRef", K::Code),
        ("totalHt", K::Money),
        ("tax", K::Money),
        ("totalTtc", K::Money),
        ("cumulative", K::Money),
    ])
    .tag("docType", TagDomain::DocType);
    let mut total = [0_i64; 3];
    for (day, number, kind, supplier, reference, v) in found {
        add(&mut total, &v);
        table.push(vec![
            Cell::text(day),
            Cell::opt_text(number),
            Cell::text(kind),
            Cell::text(supplier),
            Cell::opt_text(reference),
            Cell::money(Money(v[0])),
            Cell::money(Money(v[1])),
            Cell::money(Money(v[2])),
            Cell::money(Money(total[2])),
        ])?;
    }
    if !truncated {
        let mut totals = vec![Cell::NULL; 5];
        totals.extend(total.map(|t| Cell::money(Money(t))));
        totals.push(Cell::NULL);
        table.set_totals(totals)?;
    }
    Ok(Built {
        table,
        chart: None,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use crate::billing::convert::{TicketInvoiceInput, invoice_ticket};
    use crate::billing::credit_notes::{
        CreditLineInput, CreditNoteInput, CreditReason, create_credit_note,
    };
    use crate::billing::drafts::tests::{cart, fixture, input, save};
    use crate::billing::validate::{ValidateInput, validate};
    use crate::customers::credit::{CollectInput, collect};
    use crate::documents::CreditKind;
    use crate::fiscal::g50::g50;
    use crate::money::{Money, Qty};
    use crate::payments::{PaymentInput, PaymentMethod};
    use crate::reports::{ReportKind, ReportRequest, run_with};
    use crate::sales::cart::{Cart, CartLine};
    use crate::sales::{SaleInput, create_sale};
    use crate::table::Cell;
    use crate::testkit::{Shop, shop};

    /// September: a cash ticket (cement ×2, duty 24.00), its invoice, a typed invoice paid
    /// 4 040.00 cash + the rest on account (cement ×10 + sand ×2), a goods credit note of 3 bags,
    /// a cash versement of 5 000.00 (duty 50.00), and a ticket of an exempt product.
    struct Month {
        shop: Shop,
    }

    fn month() -> Month {
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        let bread = shop.product("Pain", Money(1_500), &shop.exempt);
        for p in [&cement.id, &sand.id, &bread.id] {
            shop.stock_in(p, 1_000_000);
        }
        shop.open_till(0);
        let sell = |lines: Vec<CartLine>, cash: i64| {
            let ctx = shop.till();
            let input = SaleInput {
                cart: Cart::new(crate::lang::Lang::Fr, lines),
                payments: vec![PaymentInput::cash(Money(cash))],
            };
            shop.db
                .write(move |tx| create_sale(tx, &ctx, input))
                .unwrap()
                .document
        };
        let ticket = sell(vec![CartLine::product(&cement.id, Qty(2_000))], 250_000);
        let (ctx, t, c) = (shop.owner_till(), ticket.id.clone(), amine.id.clone());
        shop.db
            .write(move |tx| {
                invoice_ticket(
                    tx,
                    &ctx,
                    TicketInvoiceInput {
                        ticket_id: t,
                        customer_id: c,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap();
        let draft = save(
            &shop,
            None,
            input(
                crate::documents::DocType::Invoice,
                cart(
                    Some(&amine),
                    vec![
                        CartLine::product(&cement.id, Qty(10_000)),
                        CartLine::product(&sand.id, Qty(2_000)),
                    ],
                ),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let invoice = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![
                            PaymentInput::cash(Money(404_000)),
                            PaymentInput {
                                method: PaymentMethod::Credit,
                                amount: Money(1_880_000),
                                reference: None,
                                cheque_bank: None,
                                cheque_date: None,
                            },
                        ],
                    },
                )
            })
            .unwrap()
            .document;
        let ctx = shop.owner_till();
        let av = CreditNoteInput {
            invoice_id: invoice.id.clone(),
            kind: CreditKind::Goods,
            lines: vec![CreditLineInput {
                origin_line_id: invoice.lines[0].id.clone(),
                qty: Qty(3_000),
                restock: true,
            }],
            amount: None,
            reason: CreditReason::Defective,
            refund_method: PaymentMethod::Credit,
            refund_reference: None,
            note: None,
            second_lang: None,
        };
        shop.db
            .write(move |tx| create_credit_note(tx, &ctx, av))
            .unwrap();
        let (ctx, id) = (shop.till(), amine.id.clone());
        shop.db
            .write(move |tx| {
                collect(
                    tx,
                    &ctx,
                    CollectInput {
                        customer_id: id,
                        amount: Money(500_000),
                        method: PaymentMethod::Cash,
                        reference: None,
                        cheque_bank: None,
                        cheque_date: None,
                        allocate_to: Vec::new(),
                        note: None,
                    },
                )
                .map(drop)
            })
            .unwrap();
        sell(vec![CartLine::product(&bread.id, Qty(10_000))], 15_000);
        Month { shop }
    }

    fn report(m: &Month, kind: ReportKind) -> crate::reports::Report {
        let req = ReportRequest::new(kind, "2026-09-01", "2026-09-30");
        let pack = m.shop.pack.clone();
        m.shop.db.read(move |c| run_with(c, &req, &pack)).unwrap()
    }

    fn total(r: &crate::reports::Report, key: &str) -> i64 {
        let i = r.table.column(key).unwrap();
        r.table.totals.as_ref().unwrap()[i].as_i64().unwrap()
    }

    #[test]
    fn sales_reports_count_invoices_and_credit_notes_but_not_invoiced_tickets_twice() {
        let m = month();
        let summary = report(&m, ReportKind::SalesSummary);
        // ticket 2 380.00 + invoice 22 800.00 − credit 3 570.00 + bread 150.00; the invoice of
        // the ticket adds nothing.
        assert_eq!(
            total(&summary, "netTtc"),
            238_000 + 2_280_000 - 357_000 + 15_000
        );
        // The dashboard of the day and the customer's last purchase read the same documents.
        let all = crate::reports::DashboardView {
            costs: true,
            credit: true,
            purchases: true,
            stock: true,
        };
        let d = m
            .shop
            .db
            .read(move |c| crate::reports::dashboard(c, "2026-09-28", crate::lang::Lang::Fr, all))
            .unwrap();
        assert_eq!(d.net_ttc.0, total(&summary, "netTtc"));
        let balances = report(&m, ReportKind::CustomerBalances);
        let last = balances.table.column("lastPurchase").unwrap();
        assert_eq!(balances.table.rows[0][last], Cell::text("2026-09-28"));
    }

    #[test]
    fn vat_by_rate_sums_the_taxes_of_reel_sales() {
        let m = month();
        let r = report(&m, ReportKind::VatByRate);
        let rate = r.table.column("rate").unwrap();
        let tax = r.table.column("tax").unwrap();
        let by_rate: Vec<(i64, i64)> = r
            .table
            .rows
            .iter()
            .map(|row| (row[rate].as_i64().unwrap(), row[tax].as_i64().unwrap()))
            .collect();
        // 19 %: 38 000 (ticket) + 190 000 (invoice) − 57 000 (credit note); 9 %: 90 000; 0 %: 0.
        assert_eq!(by_rate, [(1_900, 171_000), (900, 90_000), (0, 0)]);
        assert_eq!(total(&r, "tax"), 261_000);
    }

    #[test]
    fn the_g50_equals_the_vat_statement_and_the_documents() {
        let m = month();
        let pack = m.shop.pack.clone();
        let g = m
            .shop
            .db
            .read(move |c| g50(c, &pack, "2026-09-01", "2026-09-30"))
            .unwrap();
        let line = |key: &str| g.lines.iter().find(|l| l.key == key).unwrap().amount.0;
        assert_eq!(line("tva19"), 171_000);
        assert_eq!(line("tva9"), 90_000);
        assert_eq!(
            line("tvaTotal"),
            total(&report(&m, ReportKind::VatByRate), "tax")
        );
        assert_eq!(
            line("exempt19"),
            15_000,
            "exempt turnover in the block the pack names"
        );
        assert_eq!(line("caHt19"), line("taxable19") + line("exempt19"));
        assert_eq!(
            g.lines.iter().find(|l| l.key == "caHt9").unwrap().r#box,
            "1"
        );
        assert_eq!(line("stampDuty"), 2_400 + 4_000 + 5_000);
        assert_eq!(
            (g.tva_due, g.tva_credit),
            (g.tva_collected, Money::ZERO),
            "no purchase this month"
        );
    }

    #[test]
    fn the_stamp_statement_equals_the_duty_of_documents_and_versements() {
        let m = month();
        let r = report(&m, ReportKind::StampDutyStatement);
        assert_eq!(total(&r, "dutyCharged"), 2_400 + 4_000);
        assert_eq!(total(&r, "versementsDuty"), 5_000);
        assert_eq!(total(&r, "dutyTotal"), 11_400);
        assert_eq!(
            m.shop.count(
                "SELECT SUM(stamp_duty) FROM documents WHERE doc_type IN ('ticket', 'invoice') AND (origin IS NULL OR origin <> 'ticket')"
            ) + m
                .shop
                .count("SELECT COALESCE(SUM(stamp_duty), 0) FROM customer_ledger"),
            total(&r, "dutyTotal")
        );
    }

    #[test]
    fn ifu_turnover_is_ttc_and_cumulative() {
        let shop = crate::testkit::shop_with(
            crate::company::TaxRegime::Ifu,
            crate::company::PriceMode::Ttc,
        );
        let milk = shop.product("Lait", Money(12_000), &shop.tva9);
        shop.stock_in(&milk.id, 100_000);
        shop.open_till(0);
        for _ in 0..2 {
            let ctx = shop.till();
            let input = SaleInput {
                cart: Cart::new(
                    crate::lang::Lang::Fr,
                    vec![CartLine::product(&milk.id, Qty(10_000))],
                ),
                payments: vec![PaymentInput::cash(Money(125_000))],
            };
            shop.db
                .write(move |tx| create_sale(tx, &ctx, input))
                .unwrap();
        }
        let req = ReportRequest::new(ReportKind::IfuTurnover, "2026-09-01", "2026-09-30");
        let pack = shop.pack.clone();
        let r = shop.db.read(move |c| run_with(c, &req, &pack)).unwrap();
        assert_eq!(total(&r, "turnoverTtc"), 240_000);
        let cumulative = r.table.column("cumulative").unwrap();
        assert_eq!(
            r.table.rows.last().unwrap()[cumulative],
            Cell::money(Money(240_000))
        );
    }

    /// M2 has no `PurchaseLineInput::new`: the line is built literally.
    fn purchase_line(
        product: &str,
        qty: Qty,
        unit_cost: Money,
    ) -> crate::purchases::lines::PurchaseLineInput {
        crate::purchases::lines::PurchaseLineInput {
            product_id: product.to_owned(),
            qty,
            unit_cost,
            pack_qty: None,
            tax_rate_id: None,
            discount: crate::tax::tva::Discount::None,
            origin_line_id: None,
            lot_number: None,
            expiry_date: None,
            serials: Vec::new(),
            note: None,
        }
    }

    #[test]
    fn the_annual_customer_statement_totals_ht_and_tva_per_customer_and_flags_missing_nif() {
        let m = month();
        let (actor, pack) = (m.shop.owner.actor(), m.shop.pack.clone());
        m.shop
            .db
            .write(move |tx| {
                crate::customers::create(
                    tx,
                    &actor,
                    &pack,
                    crate::customers::CustomerInput::named("Client sans achat"),
                )
                .map(drop)
            })
            .unwrap();
        let req = ReportRequest::new(ReportKind::CustomersAnnual, "2026-01-01", "2026-12-31");
        let pack = m.shop.pack.clone();
        let r = m.shop.db.read(move |c| run_with(c, &req, &pack)).unwrap();
        assert_eq!(r.table.rows.len(), 1, "only customers who bought");
        let row = &r.table.rows[0];
        let col = |k: &str| r.table.column(k).unwrap();
        assert_eq!(row[col("name")], Cell::text("SARL Amine"));
        assert_eq!(row[col("nif")], Cell::text("000016001234567"));
        // the invoice of the ticket (2 380.00 TTC), the typed invoice (22 800.00), the credit
        // note (−3 570.00): HT 2 000.00 + 20 000.00 − 3 000.00.
        assert_eq!(
            row[col("totalHt")],
            Cell::money(Money(200_000 + 2_000_000 - 300_000))
        );
        assert_eq!(
            row[col("totalTtc")],
            Cell::money(Money(238_000 + 2_280_000 - 357_000))
        );
        assert_eq!(row[col("nifMissing")], Cell::text("no"));
    }

    #[test]
    fn g9_lists_each_supplier_invoice_with_its_identifiers() {
        let shop = shop();
        let supplier = shop.supplier("Cimenterie Chlef");
        let (actor, pack, id) = (shop.owner.actor(), shop.pack.clone(), supplier.id.clone());
        shop.db
            .write(move |tx| {
                let mut s = crate::suppliers::SupplierInput::named("Cimenterie Chlef");
                s.nif = Some("000002001234567".into());
                s.rc = Some("02/00-0012345B99".into());
                s.address = Some("Zone industrielle, Chlef".into());
                s.nin = Some("109876543210987654".into());
                crate::suppliers::update(tx, &actor, &pack, &id, s).map(drop)
            })
            .unwrap();
        let cement = shop.product("Ciment 25 kg", Money(119_000), &shop.tva19);
        let ctx = shop.owner_till();
        let input = crate::purchases::invoices::InvoiceInput {
            supplier_id: supplier.id.clone(),
            supplier_number: "F-2026-0931".into(),
            receipt_ids: Vec::new(),
            lines: vec![purchase_line(&cement.id, Qty(100_000), Money(90_000))],
            note: None,
            payment: None,
        };
        shop.db
            .write(move |tx| crate::purchases::invoices::record_invoice(tx, &ctx, input).map(drop))
            .unwrap();
        let req = ReportRequest::new(ReportKind::G9Suppliers, "2026-09-01", "2026-09-30");
        let pack = shop.pack.clone();
        let r = shop.db.read(move |c| run_with(c, &req, &pack)).unwrap();
        let col = |k: &str| r.table.column(k).unwrap();
        let row = &r.table.rows[0];
        assert_eq!(
            (
                row[col("nin")].clone(),
                row[col("nif")].clone(),
                row[col("rc")].clone(),
                row[col("reference")].clone()
            ),
            (
                Cell::text("109876543210987654"),
                Cell::text("000002001234567"),
                Cell::text("02/00-0012345B99"),
                Cell::text("F-2026-0931")
            )
        );
        assert_eq!(row[col("tvaDeducted")], Cell::money(Money(1_710_000)));
    }

    #[test]
    fn the_sales_register_is_chronological_and_cumulative() {
        let m = month();
        let r = report(&m, ReportKind::SalesRegister);
        let col = |k: &str| r.table.column(k).unwrap();
        assert_eq!(
            r.table.rows.len(),
            5,
            "ticket, its invoice, typed invoice, credit note, bread ticket"
        );
        let last = r.table.rows.last().unwrap();
        assert_eq!(
            last[col("cumulative")],
            Cell::money(Money(238_000 + 2_280_000 - 357_000 + 15_000)),
            "the invoice of the ticket adds nothing: its ticket counts once"
        );
        // The FA series has no gap in the legal register.
        let invoices: Vec<&Cell> = r
            .table
            .rows
            .iter()
            .filter(|row| row[col("docType")] == Cell::text("invoice"))
            .map(|row| &row[col("number")])
            .collect();
        assert_eq!(
            invoices,
            [&Cell::text("FA2026-000001"), &Cell::text("FA2026-000002")]
        );
        let of_ticket = r
            .table
            .rows
            .iter()
            .find(|row| row[col("number")] == Cell::text("FA2026-000001"))
            .unwrap();
        assert_eq!(
            (
                of_ticket[col("countedOn")].clone(),
                of_ticket[col("totalTtc")].clone()
            ),
            (Cell::text("TK2026-C1-000001"), Cell::money(Money::ZERO))
        );
        assert_eq!(
            r.table.totals.as_ref().unwrap()[col("totalTtc")],
            Cell::money(Money(238_000 + 2_280_000 - 357_000 + 15_000))
        );
        assert!(
            r.table
                .rows
                .iter()
                .any(|row| row[col("docType")] == Cell::text("credit_note"))
        );
        let days: Vec<&Cell> = r
            .table
            .rows
            .iter()
            .map(|row| &row[col("businessDate")])
            .collect();
        assert!(
            days.windows(2).all(|w| w[0].as_str() <= w[1].as_str()),
            "chronological"
        );
    }

    #[test]
    fn an_exported_register_holds_every_row_whatever_the_screen_cut() {
        let m = month();
        let mut req = ReportRequest::new(ReportKind::SalesRegister, "2026-09-01", "2026-09-30");
        req.limit = Some(2);
        let pack = m.shop.pack.clone();
        let (screen, file) = m
            .shop
            .db
            .read(move |c| {
                Ok((
                    run_with(c, &req, &pack)?,
                    crate::reports::run_for_export(c, &req, &pack)?,
                ))
            })
            .unwrap();
        assert!(screen.truncated && screen.table.totals.is_none());
        assert_eq!(screen.table.rows.len(), 2, "the screen shows its cut");
        assert!(!file.truncated);
        assert_eq!(file.table.rows.len(), 5, "the file holds every row");
        assert_eq!(
            total(&file, "totalTtc"),
            238_000 + 2_280_000 - 357_000 + 15_000
        );
        let mut top = ReportRequest::new(ReportKind::TopProducts, "2026-09-01", "2026-09-30");
        top.limit = Some(1);
        let pack = m.shop.pack.clone();
        let top = m
            .shop
            .db
            .read(move |c| crate::reports::run_for_export(c, &top, &pack))
            .unwrap();
        assert_eq!(top.table.rows.len(), 1, "a top 1 stays a top 1");
    }

    /// A return to the supplier is stored with positive totals: the G9 and the purchase register
    /// read it negative, like the deductible TVA, so the totals and the cumulative net it.
    #[test]
    fn a_return_to_the_supplier_reads_negative_in_the_g9_and_the_purchase_register() {
        let shop = shop();
        let supplier = shop.supplier("Cimenterie Chlef");
        let cement = shop.product("Ciment 25 kg", Money(119_000), &shop.tva19);
        let ctx = shop.owner_till();
        // 100 bags at 900.00 HT: 90 000.00 HT, 17 100.00 TVA, 107 100.00 TTC.
        let input = crate::purchases::invoices::InvoiceInput {
            supplier_id: supplier.id.clone(),
            supplier_number: "F-2026-0931".into(),
            receipt_ids: Vec::new(),
            lines: vec![purchase_line(&cement.id, Qty(100_000), Money(90_000))],
            note: None,
            payment: None,
        };
        shop.db
            .write(move |tx| crate::purchases::invoices::record_invoice(tx, &ctx, input).map(drop))
            .unwrap();
        shop.stock_in(&cement.id, 100_000);
        // 10 bags back at 900.00 HT: 9 000.00 HT, 1 710.00 TVA, 10 710.00 TTC.
        let ctx = shop.owner_till();
        let back = crate::purchases::returns::SupplierReturnInput {
            supplier_id: supplier.id.clone(),
            location_id: None,
            lines: vec![crate::purchases::returns::SupplierReturnLine {
                product_id: cement.id.clone(),
                qty: Qty(10_000),
                unit_cost: Some(Money(90_000)),
                lot_number: None,
                serials: Vec::new(),
                note: None,
            }],
            credit_supplier: true,
            note: None,
        };
        shop.db
            .write(move |tx| {
                crate::purchases::returns::return_to_supplier(tx, &ctx, back).map(drop)
            })
            .unwrap();
        let run = |kind: ReportKind| {
            let req = ReportRequest::new(kind, "2026-09-01", "2026-09-30");
            let pack = shop.pack.clone();
            shop.db.read(move |c| run_with(c, &req, &pack)).unwrap()
        };
        let g9 = run(ReportKind::G9Suppliers);
        assert_eq!(g9.table.rows.len(), 2);
        assert_eq!(total(&g9, "totalTtc"), 10_710_000 - 1_071_000);
        assert_eq!(total(&g9, "tvaDeducted"), 1_710_000 - 171_000);
        assert_eq!(
            total(&g9, "tvaDeducted"),
            total(&run(ReportKind::VatDeductible), "deductibleTax"),
            "the G9 deducts what the TVA statement and the G50 deduct"
        );
        let register = run(ReportKind::PurchasesRegister);
        let col = |k: &str| register.table.column(k).unwrap();
        let back = register
            .table
            .rows
            .iter()
            .find(|row| row[col("docType")] == Cell::text("return_note"))
            .unwrap();
        assert_eq!(
            (back[col("totalHt")].clone(), back[col("tax")].clone()),
            (Cell::money(Money(-900_000)), Cell::money(Money(-171_000)))
        );
        assert_eq!(
            register.table.rows.last().unwrap()[col("cumulative")],
            Cell::money(Money(10_710_000 - 1_071_000))
        );
        assert_eq!(total(&register, "totalTtc"), 10_710_000 - 1_071_000);
    }
}
